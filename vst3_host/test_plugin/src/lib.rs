#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

use std::cell::Cell;
use std::ffi::{c_char, c_void, CString};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::{ptr, slice};

mod extra;
mod restarter;

use extra::{MonoProcessor, ProbeProcessor, SynthProcessor};
use restarter::Restarter;
use vst3::{uid, Class, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

fn copy_cstring(src: &str, dst: &mut [c_char]) {
    let c_string = CString::new(src).unwrap_or_else(|_| CString::default());
    let bytes = c_string.as_bytes_with_nul();

    for (src, dst) in bytes.iter().zip(dst.iter_mut()) {
        *dst = *src as c_char;
    }

    if bytes.len() > dst.len() {
        if let Some(last) = dst.last_mut() {
            *last = 0;
        }
    }
}

pub(crate) fn copy_wstring(src: &str, dst: &mut [TChar]) {
    let mut len = 0;
    for (src, dst) in src.encode_utf16().zip(dst.iter_mut()) {
        *dst = src as TChar;
        len += 1;
    }

    if len < dst.len() {
        dst[len] = 0;
    } else if let Some(last) = dst.last_mut() {
        *last = 0;
    }
}

unsafe fn len_wstring(string: *const TChar) -> usize {
    let mut len = 0;

    while *string.offset(len) != 0 {
        len += 1;
    }

    len as usize
}

const PLUGIN_NAME: &'static str = "Gain (vst3-rs example plugin)";


/// Writes `values` to the stream as little-endian doubles.
pub(crate) unsafe fn write_f64s(state: *mut IBStream, values: &[f64]) -> tresult {
    let Some(stream) = ComRef::from_raw(state) else {
        return kInvalidArgument;
    };
    for value in values {
        let mut bytes = value.to_le_bytes();
        let mut written = 0;
        if stream.write(bytes.as_mut_ptr().cast(), 8, &mut written) != kResultOk || written != 8 {
            return kResultFalse;
        }
    }
    kResultOk
}

/// Reads `N` little-endian doubles from the stream.
pub(crate) unsafe fn read_f64s<const N: usize>(state: *mut IBStream) -> Option<[f64; N]> {
    let stream = ComRef::from_raw(state)?;
    let mut values = [0.0; N];
    for value in &mut values {
        let mut bytes = [0u8; 8];
        let mut read = 0;
        if stream.read(bytes.as_mut_ptr().cast(), 8, &mut read) != kResultOk || read != 8 {
            return None;
        }
        *value = f64::from_le_bytes(bytes);
    }
    Some(values)
}

struct GainProcessor {
    gain: AtomicU64,
}

impl Class for GainProcessor {
    type Interfaces = (IComponent, IAudioProcessor, IProcessContextRequirements);
}

impl GainProcessor {
    const CID: TUID = uid(0x6E332252, 0x54224A00, 0xAA69301A, 0xF318797D);

    fn new() -> GainProcessor {
        GainProcessor {
            gain: AtomicU64::new(1.0f64.to_bits()),
        }
    }
}

impl IPluginBaseTrait for GainProcessor {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IComponentTrait for GainProcessor {
    unsafe fn getControllerClassId(&self, class_id: *mut TUID) -> tresult {
        *class_id = GainController::CID;
        kResultOk
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, mediaType: MediaType, dir: BusDirection) -> i32 {
        match mediaType as BusDirections {
            MediaTypes_::kAudio => match dir as BusDirections {
                BusDirections_::kInput => 1,
                BusDirections_::kOutput => 1,
                _ => 0,
            },
            MediaTypes_::kEvent => 0,
            _ => 0,
        }
    }

    unsafe fn getBusInfo(
        &self,
        mediaType: MediaType,
        dir: BusDirection,
        index: i32,
        bus: *mut BusInfo,
    ) -> tresult {
        match mediaType as MediaTypes {
            MediaTypes_::kAudio => match dir as BusDirections {
                BusDirections_::kInput => match index {
                    0 => {
                        let bus = &mut *bus;

                        bus.mediaType = MediaTypes_::kAudio as MediaType;
                        bus.direction = BusDirections_::kInput as BusDirection;
                        bus.channelCount = 2;
                        copy_wstring("Input", &mut bus.name);
                        bus.busType = BusTypes_::kMain as BusType;
                        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;

                        kResultOk
                    }
                    _ => kInvalidArgument,
                },
                BusDirections_::kOutput => match index {
                    0 => {
                        let bus = &mut *bus;

                        bus.mediaType = MediaTypes_::kAudio as MediaType;
                        bus.direction = BusDirections_::kOutput as BusDirection;
                        bus.channelCount = 2;
                        copy_wstring("Output", &mut bus.name);
                        bus.busType = BusTypes_::kMain as BusType;
                        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32 as uint32;

                        kResultOk
                    }
                    _ => kInvalidArgument,
                },
                _ => kInvalidArgument,
            },
            MediaTypes_::kEvent => kInvalidArgument,
            _ => kInvalidArgument,
        }
    }

    unsafe fn getRoutingInfo(
        &self,
        _in_info: *mut RoutingInfo,
        _out_info: *mut RoutingInfo,
    ) -> tresult {
        kNotImplemented
    }

    unsafe fn activateBus(
        &self,
        _media_type: MediaType,
        _dir: BusDirection,
        _index: i32,
        _state: TBool,
    ) -> tresult {
        kResultOk
    }

    unsafe fn setActive(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        match read_f64s::<1>(state) {
            Some([gain]) => {
                self.gain.store(gain.to_bits(), Ordering::Relaxed);
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        write_f64s(state, &[f64::from_bits(self.gain.load(Ordering::Relaxed))])
    }
}

impl IAudioProcessorTrait for GainProcessor {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins != 1 || num_outs != 1 {
            return kResultFalse;
        }

        if *inputs != SpeakerArr::kStereo || *outputs != SpeakerArr::kStereo {
            return kResultFalse;
        }

        kResultTrue
    }

    unsafe fn getBusArrangement(
        &self,
        dir: BusDirection,
        index: i32,
        arr: *mut SpeakerArrangement,
    ) -> tresult {
        match dir as BusDirections {
            BusDirections_::kInput => {
                if index == 0 {
                    *arr = SpeakerArr::kStereo;
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            BusDirections_::kOutput => {
                if index == 0 {
                    *arr = SpeakerArr::kStereo;
                    kResultOk
                } else {
                    kInvalidArgument
                }
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn canProcessSampleSize(&self, symbolic_sample_size: i32) -> tresult {
        match symbolic_sample_size as SymbolicSampleSizes {
            SymbolicSampleSizes_::kSample32 => kResultOk as i32,
            SymbolicSampleSizes_::kSample64 => kNotImplemented as i32,
            _ => kInvalidArgument,
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
        let process_data = &*data;

        if let Some(param_changes) = ComRef::from_raw(process_data.inputParameterChanges) {
            let param_count = param_changes.getParameterCount();
            for param_index in 0..param_count {
                if let Some(param_queue) =
                    ComRef::from_raw(param_changes.getParameterData(param_index))
                {
                    let param_id = param_queue.getParameterId();
                    let point_count = param_queue.getPointCount();

                    match param_id {
                        0 => {
                            let mut sample_offset = 0;
                            let mut value = 0.0;
                            let result = param_queue.getPoint(
                                point_count - 1,
                                &mut sample_offset,
                                &mut value,
                            );

                            if result == kResultTrue {
                                self.gain.store(value.to_bits(), Ordering::Relaxed);
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        let gain = f64::from_bits(self.gain.load(Ordering::Relaxed)) as f32;

        let num_samples = process_data.numSamples as usize;

        if process_data.numInputs != 1 || process_data.numOutputs != 1 {
            return kResultOk;
        }

        let input_buses =
            slice::from_raw_parts(process_data.inputs, process_data.numInputs as usize);
        let output_buses =
            slice::from_raw_parts(process_data.outputs, process_data.numOutputs as usize);

        if input_buses[0].numChannels != 2 || output_buses[0].numChannels != 2 {
            return kResultOk;
        }

        let input_channels = slice::from_raw_parts(
            input_buses[0].__field0.channelBuffers32,
            input_buses[0].numChannels as usize,
        );
        let output_channels = slice::from_raw_parts(
            output_buses[0].__field0.channelBuffers32,
            output_buses[0].numChannels as usize,
        );

        let input_l = slice::from_raw_parts(input_channels[0], num_samples);
        let input_r = slice::from_raw_parts(input_channels[1], num_samples);
        let output_l = slice::from_raw_parts_mut(output_channels[0], num_samples);
        let output_r = slice::from_raw_parts_mut(output_channels[1], num_samples);

        for i in 0..num_samples {
            output_l[i] = gain * input_l[i];
            output_r[i] = gain * input_r[i];
        }

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IProcessContextRequirementsTrait for GainProcessor {
    unsafe fn getProcessContextRequirements(&self) -> u32 {
        0
    }
}

struct GainController {
    gain: Cell<f64>,
    bypass: Cell<f64>,
    mode: Cell<f64>,
    meter: Cell<f64>,
    secret: Cell<f64>,
}

const BYPASS_ID: u32 = 1;
const MODE_ID: u32 = 2;
const METER_ID: u32 = 3;
const SECRET_ID: u32 = 4_000_000_000;
const MODES: [&str; 4] = ["Clean", "Warm", "Hot", "Fuzz"];

impl Class for GainController {
    type Interfaces = (IEditController,);
}

impl GainController {
    const CID: TUID = uid(0x1BA8A477, 0xEE0A4A2D, 0x80F50D14, 0x13D2EAA0);

    fn new() -> GainController {
        GainController {
            gain: Cell::new(1.0),
            bypass: Cell::new(0.0),
            mode: Cell::new(0.0),
            meter: Cell::new(0.5),
            secret: Cell::new(0.0),
        }
    }
}

impl IPluginBaseTrait for GainController {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        kResultOk
    }
}

impl IEditControllerTrait for GainController {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        // the component's state is its gain
        match read_f64s::<1>(state) {
            Some([gain]) => {
                self.gain.set(gain);
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        // the controller's own state is the parameters the component doesn't know about
        match read_f64s::<3>(state) {
            Some([bypass, mode, secret]) => {
                self.bypass.set(bypass);
                self.mode.set(mode);
                self.secret.set(secret);
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        write_f64s(state, &[self.bypass.get(), self.mode.get(), self.secret.get()])
    }

    unsafe fn getParameterCount(&self) -> i32 {
        5
    }

    unsafe fn getParameterInfo(&self, param_index: i32, info: *mut ParameterInfo) -> tresult {
        let info = &mut *info;
        let automate = ParameterInfo_::ParameterFlags_::kCanAutomate as i32;

        let (id, title, units, step_count, default, flags) = match param_index {
            0 => (0, "Gain", "", 0, 1.0, automate),
            1 => (
                BYPASS_ID,
                "Bypass",
                "",
                1,
                0.0,
                automate | ParameterInfo_::ParameterFlags_::kIsBypass as i32,
            ),
            2 => (
                MODE_ID,
                "Mode",
                "",
                3,
                0.0,
                automate | ParameterInfo_::ParameterFlags_::kIsList as i32,
            ),
            3 => (
                METER_ID,
                "Meter",
                "dB",
                0,
                0.5,
                ParameterInfo_::ParameterFlags_::kIsReadOnly as i32,
            ),
            4 => (
                SECRET_ID,
                "Secret",
                "",
                0,
                0.0,
                automate | ParameterInfo_::ParameterFlags_::kIsHidden as i32,
            ),
            _ => return kInvalidArgument,
        };

        info.id = id;
        copy_wstring(title, &mut info.title);
        copy_wstring(title, &mut info.shortTitle);
        copy_wstring(units, &mut info.units);
        info.stepCount = step_count;
        info.defaultNormalizedValue = default;
        info.unitId = 0;
        info.flags = flags;

        kResultOk
    }

    unsafe fn getParamStringByValue(
        &self,
        id: u32,
        value_normalized: f64,
        string: *mut String128,
    ) -> tresult {
        let slice = unsafe { &mut *string };

        let display = match id {
            0 | SECRET_ID => value_normalized.to_string(),
            BYPASS_ID => (if value_normalized >= 0.5 { "On" } else { "Off" }).to_string(),
            MODE_ID => MODES[(value_normalized * 3.0).round().clamp(0.0, 3.0) as usize].to_string(),
            METER_ID => format!("{:.1}", -60.0 + value_normalized * 60.0),
            _ => return kInvalidArgument,
        };

        copy_wstring(&display, slice);
        kResultOk
    }

    unsafe fn getParamValueByString(
        &self,
        id: u32,
        string: *mut TChar,
        value_normalized: *mut f64,
    ) -> tresult {
        let len = len_wstring(string as *const TChar);
        let Ok(string) = String::from_utf16(slice::from_raw_parts(string as *const u16, len))
        else {
            return kInvalidArgument;
        };

        let value = match id {
            0 | SECRET_ID => f64::from_str(&string).ok(),
            BYPASS_ID => match string.as_str() {
                "Off" => Some(0.0),
                "On" => Some(1.0),
                _ => None,
            },
            MODE_ID => MODES.iter().position(|m| *m == string).map(|i| i as f64 / 3.0),
            METER_ID => f64::from_str(&string).ok().map(|db| (db + 60.0) / 60.0),
            _ => None,
        };

        match value {
            Some(value) => {
                *value_normalized = value;
                kResultOk
            }
            None => kInvalidArgument,
        }
    }

    unsafe fn normalizedParamToPlain(&self, id: u32, value_normalized: f64) -> f64 {
        match id {
            0 | SECRET_ID => value_normalized,
            BYPASS_ID => value_normalized.round(),
            MODE_ID => (value_normalized * 3.0).round(),
            METER_ID => -60.0 + value_normalized * 60.0,
            _ => 0.0,
        }
    }

    unsafe fn plainParamToNormalized(&self, id: u32, plain_value: f64) -> f64 {
        match id {
            0 | SECRET_ID => plain_value,
            BYPASS_ID => plain_value,
            MODE_ID => plain_value / 3.0,
            METER_ID => (plain_value + 60.0) / 60.0,
            _ => 0.0,
        }
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        match id {
            0 => self.gain.get(),
            BYPASS_ID => self.bypass.get(),
            MODE_ID => self.mode.get(),
            METER_ID => self.meter.get(),
            SECRET_ID => self.secret.get(),
            _ => 0.0,
        }
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        match id {
            0 => self.gain.set(value),
            BYPASS_ID => self.bypass.set(value),
            MODE_ID => self.mode.set(value),
            METER_ID => self.meter.set(value),
            SECRET_ID => self.secret.set(value),
            _ => return kInvalidArgument,
        }

        kResultOk
    }

    unsafe fn setComponentHandler(&self, _handler: *mut IComponentHandler) -> tresult {
        kResultOk
    }

    unsafe fn createView(&self, _name: *const c_char) -> *mut IPlugView {
        ptr::null_mut()
    }
}

struct Factory {}

impl Class for Factory {
    type Interfaces = (IPluginFactory2,);
}

impl IPluginFactoryTrait for Factory {
    unsafe fn getFactoryInfo(&self, info: *mut PFactoryInfo) -> tresult {
        let info = &mut *info;

        copy_cstring("Vendor", &mut info.vendor);
        copy_cstring("https://example.com", &mut info.url);
        copy_cstring("someone@example.com", &mut info.email);
        info.flags = PFactoryInfo_::FactoryFlags_::kUnicode as int32;

        kResultOk
    }

    unsafe fn countClasses(&self) -> i32 {
        6
    }

    unsafe fn getClassInfo(&self, index: i32, info: *mut PClassInfo) -> tresult {
        match index {
            0 => {
                let info = &mut *info;
                info.cid = GainProcessor::CID;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Audio Module Class", &mut info.category);
                copy_cstring(PLUGIN_NAME, &mut info.name);

                kResultOk
            }
            1 => {
                let info = &mut *info;
                info.cid = GainController::CID;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Component Controller Class", &mut info.category);
                copy_cstring(PLUGIN_NAME, &mut info.name);

                kResultOk
            }
            2..=5 => {
                let (cid, name) = match index {
                    2 => (ProbeProcessor::CID, "Probe"),
                    3 => (MonoProcessor::CID, "Mono"),
                    4 => (SynthProcessor::CID, "Synth"),
                    _ => (Restarter::CID, "Restarter"),
                };
                let info = &mut *info;
                info.cid = cid;
                info.cardinality = PClassInfo_::ClassCardinality_::kManyInstances as int32;
                copy_cstring("Audio Module Class", &mut info.category);
                copy_cstring(name, &mut info.name);

                kResultOk
            }
            _ => kInvalidArgument,
        }
    }

    unsafe fn createInstance(
        &self,
        cid: FIDString,
        iid: FIDString,
        obj: *mut *mut c_void,
    ) -> tresult {
        let instance = match *(cid as *const TUID) {
            GainProcessor::CID => Some(
                ComWrapper::new(GainProcessor::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            GainController::CID => Some(
                ComWrapper::new(GainController::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            ProbeProcessor::CID => Some(
                ComWrapper::new(ProbeProcessor::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            MonoProcessor::CID => Some(
                ComWrapper::new(MonoProcessor)
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            SynthProcessor::CID => Some(
                ComWrapper::new(SynthProcessor)
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            Restarter::CID => Some(
                ComWrapper::new(Restarter::new())
                    .to_com_ptr::<FUnknown>()
                    .unwrap(),
            ),
            _ => None,
        };

        if let Some(instance) = instance {
            let ptr = instance.as_ptr();
            ((*(*ptr).vtbl).queryInterface)(ptr, iid as *mut TUID, obj)
        } else {
            kInvalidArgument
        }
    }
}

impl IPluginFactory2Trait for Factory {
    unsafe fn getClassInfo2(&self, index: i32, info: *mut PClassInfo2) -> tresult {
        let mut base = std::mem::zeroed::<PClassInfo>();
        let result = IPluginFactoryTrait::getClassInfo(self, index, &mut base);
        if result != kResultOk {
            return result;
        }

        let info = &mut *info;
        info.cid = base.cid;
        info.cardinality = base.cardinality;
        info.category = base.category;
        info.name = base.name;
        info.classFlags = 0;
        copy_cstring(
            match index {
                0 | 2 | 3 | 5 => "Fx",
                4 => "Instrument|Synth",
                _ => "",
            },
            &mut info.subCategories,
        );
        copy_cstring("Vendor", &mut info.vendor);
        copy_cstring("1.2.3", &mut info.version);
        copy_cstring("VST 3.7", &mut info.sdkVersion);

        kResultOk
    }
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn InitDll() -> bool {
    true
}

#[cfg(target_os = "windows")]
#[no_mangle]
extern "system" fn ExitDll() -> bool {
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn BundleEntry(_bundle_ref: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "macos")]
#[no_mangle]
extern "system" fn BundleExit() -> bool {
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleEntry(_library_handle: *mut c_void) -> bool {
    true
}

#[cfg(target_os = "linux")]
#[no_mangle]
extern "system" fn ModuleExit() -> bool {
    true
}

#[no_mangle]
extern "system" fn GetPluginFactory() -> *mut IPluginFactory {
    ComWrapper::new(Factory {})
        .to_com_ptr::<IPluginFactory>()
        .unwrap()
        .into_raw()
}
