//! Extra test plugins that exercise things the gain plugin can't:
//!
//! - `Probe` reports what the host gave it (transport, setup, buses) through its audio output.
//! - `Mono` only accepts mono buses.
//! - `Synth` has no audio input, only an event input.
//!
//! None of them has a separate controller.

use std::slice;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU64, Ordering::SeqCst};

use vst3::{uid, Class, ComRef, Steinberg::Vst::*, Steinberg::*};

use super::copy_wstring;

fn fill_bus(
    bus: *mut BusInfo,
    media: MediaTypes,
    dir: BusDirections,
    name: &str,
    channels: i32,
    bus_type: BusTypes,
    default_active: bool,
) -> tresult {
    unsafe {
        let bus = &mut *bus;
        bus.mediaType = media as MediaType;
        bus.direction = dir as BusDirection;
        bus.channelCount = channels;
        copy_wstring(name, &mut bus.name);
        bus.busType = bus_type as BusType;
        bus.flags = if default_active {
            BusInfo_::BusFlags_::kDefaultActive as u32
        } else {
            0
        };
    }
    kResultOk
}

/// Returns the channel slices of the bus at `index`, if it has `channels` channels.
unsafe fn bus_channels<'a>(
    buses: *mut AudioBusBuffers,
    count: i32,
    index: usize,
    channels: usize,
    samples: usize,
) -> Option<Vec<&'a mut [f32]>> {
    if index >= count as usize {
        return None;
    }
    let bus = &*buses.add(index);
    if bus.numChannels as usize != channels || bus.__field0.channelBuffers32.is_null() {
        return None;
    }
    let ptrs = slice::from_raw_parts(bus.__field0.channelBuffers32, channels);
    Some(
        ptrs.iter()
            .map(|&p| slice::from_raw_parts_mut(p, samples))
            .collect(),
    )
}

fn put(buf: &mut [f32], index: usize, value: f64) {
    if let Some(slot) = buf.get_mut(index) {
        *slot = value as f32;
    }
}

// ---------------------------------------------------------------------------------------------
// Probe
// ---------------------------------------------------------------------------------------------

pub struct ProbeProcessor {
    active: AtomicBool,
    processing: AtomicBool,
    /// Set when the host calls things in an order the VST3 spec forbids.
    violated: AtomicBool,
    process_mode: AtomicI32,
    max_block: AtomicI32,
    sample_rate: AtomicU64,
    /// bit 0: input 0, bit 1: input 1 (sidechain), bit 2: output 0
    buses_active: AtomicI32,
}

impl ProbeProcessor {
    pub const CID: TUID = uid(0x50524F42, 0x45000000, 0x00000000, 0x00000001);

    pub fn new() -> Self {
        Self {
            active: AtomicBool::new(false),
            processing: AtomicBool::new(false),
            violated: AtomicBool::new(false),
            process_mode: AtomicI32::new(-1),
            max_block: AtomicI32::new(0),
            sample_rate: AtomicU64::new(0),
            buses_active: AtomicI32::new(0b111),
        }
    }
}

impl Class for ProbeProcessor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl IPluginBaseTrait for ProbeProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for ProbeProcessor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kResultFalse
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kInput) => 2,
            (MediaTypes_::kAudio, BusDirections_::kOutput) => 1,
            (MediaTypes_::kEvent, BusDirections_::kInput | BusDirections_::kOutput) => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        match (media_type as MediaTypes, dir as BusDirections, index) {
            (MediaTypes_::kEvent, BusDirections_::kInput, 0) => fill_bus(
                bus,
                MediaTypes_::kEvent,
                BusDirections_::kInput,
                "Events In",
                16,
                BusTypes_::kMain,
                true,
            ),
            (MediaTypes_::kEvent, BusDirections_::kOutput, 0) => fill_bus(
                bus,
                MediaTypes_::kEvent,
                BusDirections_::kOutput,
                "Events Out",
                16,
                BusTypes_::kMain,
                true,
            ),
            (MediaTypes_::kAudio, BusDirections_::kInput, 0) => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kInput,
                "Input",
                2,
                BusTypes_::kMain,
                true,
            ),
            (MediaTypes_::kAudio, BusDirections_::kInput, 1) => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kInput,
                "Sidechain",
                2,
                BusTypes_::kAux,
                false,
            ),
            (MediaTypes_::kAudio, BusDirections_::kOutput, 0) => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kOutput,
                "Output",
                2,
                BusTypes_::kMain,
                true,
            ),
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(&self, _i: *mut RoutingInfo, _o: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        state: TBool,
    ) -> tresult {
        if media_type as MediaTypes == MediaTypes_::kAudio {
            let bit = match (dir as BusDirections, index) {
                (BusDirections_::kInput, 0) => 0b001,
                (BusDirections_::kInput, 1) => 0b010,
                (BusDirections_::kOutput, 0) => 0b100,
                _ => return kInvalidArgument,
            };
            if state != 0 {
                self.buses_active.fetch_or(bit, SeqCst);
            } else {
                self.buses_active.fetch_and(!bit, SeqCst);
            }
        }
        kResultOk
    }

    unsafe fn setActive(&self, state: TBool) -> tresult {
        // buses must be configured before activation, and processing must be off
        if state == 0 && self.processing.load(SeqCst) {
            self.violated.store(true, SeqCst);
        }
        self.active.store(state != 0, SeqCst);
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
}

impl IAudioProcessorTrait for ProbeProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins != 2 || num_outs != 1 {
            return kResultFalse;
        }
        let inputs = slice::from_raw_parts(inputs, 2);
        let outputs = slice::from_raw_parts(outputs, 1);
        if inputs[0] == SpeakerArr::kStereo
            && inputs[1] == SpeakerArr::kStereo
            && outputs[0] == SpeakerArr::kStereo
        {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        _dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if index > 1 {
            return kInvalidArgument;
        }
        *arr = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        64
    }

    unsafe fn setupProcessing(&self, setup: *mut ProcessSetup) -> tresult {
        // setup must happen while inactive
        if self.active.load(SeqCst) {
            self.violated.store(true, SeqCst);
        }
        let setup = &*setup;
        self.process_mode.store(setup.processMode, SeqCst);
        self.max_block.store(setup.maxSamplesPerBlock, SeqCst);
        self.sample_rate.store(setup.sampleRate.to_bits(), SeqCst);
        kResultOk
    }

    unsafe fn setProcessing(&self, state: TBool) -> tresult {
        if state != 0 && !self.active.load(SeqCst) {
            self.violated.store(true, SeqCst);
        }
        self.processing.store(state != 0, SeqCst);
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let d = &*data;

        if !self.active.load(SeqCst)
            || !self.processing.load(SeqCst)
            || self.violated.load(SeqCst)
            || d.numSamples > self.max_block.load(SeqCst)
            || d.numOutputs != 1
        {
            return kResultFalse;
        }

        let n = d.numSamples as usize;
        let Some(mut out) = bus_channels(d.outputs, d.numOutputs, 0, 2, n) else {
            return kResultFalse;
        };
        let (l, r) = out.split_at_mut(1);
        let (l, r) = (&mut *l[0], &mut *r[0]);
        l.fill(0.0);
        r.fill(0.0);

        if d.processContext.is_null() {
            put(l, 0, -1.0);
        } else {
            let c = &*d.processContext;
            put(l, 0, c.tempo);
            put(l, 1, c.state as f64);
            put(l, 2, c.timeSigNumerator as f64);
            put(l, 3, c.timeSigDenominator as f64);
            put(l, 4, c.projectTimeMusic);
            put(l, 5, c.barPositionMusic);
            put(l, 6, c.cycleStartMusic);
            put(l, 7, c.cycleEndMusic);
            put(r, 0, c.projectTimeSamples as f64);
            put(r, 1, c.sampleRate);
            put(r, 13, c.continousTimeSamples as f64);
        }

        put(r, 2, d.numSamples as f64);
        put(r, 3, d.processMode as f64);
        put(r, 4, d.symbolicSampleSize as f64);
        put(r, 5, self.max_block.load(SeqCst) as f64);
        put(r, 6, d.numInputs as f64);
        put(r, 7, d.numOutputs as f64);

        if let Some(input) = bus_channels(d.inputs, d.numInputs, 0, 2, n) {
            put(r, 8, input[0][0] as f64);
            put(r, 9, input[1][0] as f64);
        }
        if d.numInputs > 1 {
            put(r, 10, (*d.inputs.add(1)).numChannels as f64);
        }
        put(r, 11, self.buses_active.load(SeqCst) as f64);
        put(r, 12, f64::from_bits(self.sample_rate.load(SeqCst)));

        // input events: l[8] count, l[9..14] the first note on (pitch, velocity, offset, channel,
        // id), l[14] the number of note offs. Every note on is sent back, an octave higher.
        if let Some(events) = ComRef::from_raw(d.inputEvents) {
            let count = events.getEventCount();
            put(l, 8, count as f64);

            let mut first_on = true;
            let mut note_offs = 0;
            for i in 0..count {
                let mut event = std::mem::zeroed::<Event>();
                if events.getEvent(i, &mut event) != kResultOk {
                    continue;
                }

                match event.r#type as Event_::EventTypes {
                    Event_::EventTypes_::kNoteOnEvent => {
                        let note = event.__field0.noteOn;
                        if first_on {
                            first_on = false;
                            put(l, 9, note.pitch as f64);
                            put(l, 10, note.velocity as f64);
                            put(l, 11, event.sampleOffset as f64);
                            put(l, 12, note.channel as f64);
                            put(l, 13, note.noteId as f64);
                        }

                        if let Some(out) = ComRef::from_raw(d.outputEvents) {
                            let mut echo = event;
                            echo.__field0.noteOn.pitch += 12;
                            out.addEvent(&mut echo);
                        }
                    }
                    Event_::EventTypes_::kNoteOffEvent => note_offs += 1,
                    _ => {}
                }
            }
            put(l, 14, note_offs as f64);
        }

        // input parameter changes: l[15] the number of queues, then for the first two queues
        // (id, points, first offset, first value, last offset, last value)
        let mut param_count = 0;
        if let Some(changes) = ComRef::from_raw(d.inputParameterChanges) {
            param_count = changes.getParameterCount();
            put(l, 15, param_count as f64);

            for q in 0..param_count.min(2) {
                let Some(queue) = ComRef::from_raw(changes.getParameterData(q)) else {
                    continue;
                };
                let points = queue.getPointCount();
                let base = 16 + 6 * q as usize;
                put(l, base, queue.getParameterId() as f64);
                put(l, base + 1, points as f64);

                let (mut offset, mut value) = (0, 0.0);
                if points > 0 && queue.getPoint(0, &mut offset, &mut value) == kResultOk {
                    put(l, base + 2, offset as f64);
                    put(l, base + 3, value);
                }
                if points > 0 && queue.getPoint(points - 1, &mut offset, &mut value) == kResultOk {
                    put(l, base + 4, offset as f64);
                    put(l, base + 5, value);
                }
            }
        }

        // output parameter changes: parameter 100 gets the number of input queues / 10, at 3
        if let Some(changes) = ComRef::from_raw(d.outputParameterChanges) {
            let id: u32 = 100;
            let mut index = 0;
            if let Some(queue) = ComRef::from_raw(changes.addParameterData(&id, &mut index)) {
                queue.addPoint(3, param_count as f64 / 10.0, &mut index);
            }
        }

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        128
    }
}

impl IProcessContextRequirementsTrait for ProbeProcessor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        // tempo, time signature, project time (music), bar position, cycle, transport state
        0xFF
    }
}

// ---------------------------------------------------------------------------------------------
// Mono
// ---------------------------------------------------------------------------------------------

pub struct MonoProcessor;

impl MonoProcessor {
    pub const CID: TUID = uid(0x50524F42, 0x45000000, 0x00000000, 0x00000002);
}

impl Class for MonoProcessor {
    type Interfaces = (IComponent, IAudioProcessor);
}

impl IPluginBaseTrait for MonoProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for MonoProcessor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kResultFalse
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kInput | BusDirections_::kOutput) => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        if media_type as MediaTypes != MediaTypes_::kAudio || index != 0 {
            return kInvalidArgument;
        }
        match dir as BusDirections {
            BusDirections_::kInput => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kInput,
                "Mono In",
                1,
                BusTypes_::kMain,
                true,
            ),
            BusDirections_::kOutput => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kOutput,
                "Mono Out",
                1,
                BusTypes_::kMain,
                true,
            ),
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(&self, _i: *mut RoutingInfo, _o: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(&self, _m: MediaType, _d: BusDirection, _i: i32, _s: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
}

impl IAudioProcessorTrait for MonoProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins == 1
            && num_outs == 1
            && *inputs == SpeakerArr::kMono
            && *outputs == SpeakerArr::kMono
        {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        _dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if index != 0 {
            return kInvalidArgument;
        }
        *arr = SpeakerArr::kMono;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let d = &*data;
        let n = d.numSamples as usize;

        let (Some(input), Some(mut output)) = (
            bus_channels(d.inputs, d.numInputs, 0, 1, n),
            bus_channels(d.outputs, d.numOutputs, 0, 1, n),
        ) else {
            return kResultFalse;
        };

        for (out, input) in output[0].iter_mut().zip(input[0].iter()) {
            *out = input * 0.5;
        }
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

// ---------------------------------------------------------------------------------------------
// Synth
// ---------------------------------------------------------------------------------------------

pub struct SynthProcessor;

impl SynthProcessor {
    pub const CID: TUID = uid(0x50524F42, 0x45000000, 0x00000000, 0x00000003);
    /// What the synth writes to both channels when everything is as it should be.
    pub const LEVEL: f32 = 0.25;
}

impl Class for SynthProcessor {
    type Interfaces = (IComponent, IAudioProcessor);
}

impl IPluginBaseTrait for SynthProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for SynthProcessor {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kResultFalse
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, dir: BusDirection) -> i32 {
        match (media_type as MediaTypes, dir as BusDirections) {
            (MediaTypes_::kAudio, BusDirections_::kOutput) => 1,
            (MediaTypes_::kEvent, BusDirections_::kInput) => 1,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        media_type: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        match (media_type as MediaTypes, dir as BusDirections, index) {
            (MediaTypes_::kAudio, BusDirections_::kOutput, 0) => fill_bus(
                bus,
                MediaTypes_::kAudio,
                BusDirections_::kOutput,
                "Output",
                2,
                BusTypes_::kMain,
                true,
            ),
            (MediaTypes_::kEvent, BusDirections_::kInput, 0) => fill_bus(
                bus,
                MediaTypes_::kEvent,
                BusDirections_::kInput,
                "Notes",
                16,
                BusTypes_::kMain,
                true,
            ),
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(&self, _i: *mut RoutingInfo, _o: *mut RoutingInfo) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(&self, _m: MediaType, _d: BusDirection, _i: i32, _s: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }

    unsafe fn getState(&self, _state: *mut IBStream) -> tresult {
        kResultOk
    }
}

impl IAudioProcessorTrait for SynthProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        let _ = inputs;
        if num_ins == 0 && num_outs == 1 && *outputs == SpeakerArr::kStereo {
            kResultTrue
        } else {
            kResultFalse
        }
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        if dir as BusDirections != BusDirections_::kOutput || index != 0 {
            return kInvalidArgument;
        }
        *arr = SpeakerArr::kStereo;
        kResultOk
    }

    unsafe fn canProcessSampleSize(&self, size: i32) -> tresult {
        if size as SymbolicSampleSizes == SymbolicSampleSizes_::kSample32 {
            kResultOk
        } else {
            kNotImplemented
        }
    }

    unsafe fn getLatencySamples(&self) -> u32 {
        0
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let d = &*data;
        let n = d.numSamples as usize;

        let Some(mut output) = bus_channels(d.outputs, d.numOutputs, 0, 2, n) else {
            return kResultFalse;
        };

        // a host that hands an instrument an input bus it doesn't have gets a different level
        let level = if d.numInputs == 0 { Self::LEVEL } else { 0.75 };
        for channel in &mut output {
            channel.fill(level);
        }
        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}
