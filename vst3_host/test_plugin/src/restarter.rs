//! A plugin whose component and controller are one object, and that asks the host to restart it
//! in all the ways a plugin can. Setting parameter 0 ("Trigger") picks what happens:
//!
//! - `0.1`: the latency becomes 16 (`kLatencyChanged`)
//! - `0.2`: the plugin turns mono (`kIoChanged`)
//! - `0.3`: nothing visible changes (`kReloadComponent`)
//! - `0.4`: parameter 1 is renamed (`kParamTitlesChanged`)
//! - `0.5`: parameter 2 becomes 0.9 (`kParamValuesChanged`)
//!
//! - `0.6`: the editor asks the host for a size of 640 x 480 through `IPlugFrame::resizeView`
//! - `0.7`: the editor can't be resized by the host any more
//!
//! A parameter change to `0.9` in `process` does the same as `0.1`, but from the audio thread.
//!
//! It has an editor, which records what the host does to it. That is reported by parameters
//! 3 to 9: whether it is attached (3), its width (4) and height (5) divided by 1000, the content
//! scale divided by 4 (6), how often it was removed divided by 100 (7), whether it has a frame
//! (8) and whether it was given a parent window (9).

use std::cell::{Cell, RefCell};
use std::ffi::CStr;
use std::rc::Rc;
use std::slice;

use vst3::{uid, Class, ComPtr, ComRef, ComWrapper, Steinberg::Vst::*, Steinberg::*};

use super::{copy_wstring, read_f64s, write_f64s};


pub struct ViewState {
    attached: Cell<bool>,
    frame: RefCell<Option<ComPtr<IPlugFrame>>>,
    width: Cell<i32>,
    height: Cell<i32>,
    scale: Cell<f32>,
    removed: Cell<u32>,
    parent: Cell<usize>,
    can_resize: Cell<bool>,
    alive: Cell<bool>,
    ptr: Cell<*mut IPlugView>,
}

impl ViewState {
    fn new() -> Self {
        Self {
            attached: Cell::new(false),
            frame: RefCell::new(None),
            width: Cell::new(400),
            height: Cell::new(300),
            scale: Cell::new(1.0),
            removed: Cell::new(0),
            parent: Cell::new(0),
            can_resize: Cell::new(true),
            alive: Cell::new(true),
            ptr: Cell::new(std::ptr::null_mut()),
        }
    }
}

const PLATFORM: &str = if cfg!(windows) { "HWND" } else { "X11EmbedWindowID" };

struct PlugView {
    state: Rc<ViewState>,
}

impl Drop for PlugView {
    fn drop(&mut self) {
        self.state.alive.set(false);
    }
}

impl Class for PlugView {
    type Interfaces = (IPlugView, IPlugViewContentScaleSupport);
}

fn supported(kind: FIDString) -> bool {
    !kind.is_null() && unsafe { CStr::from_ptr(kind) }.to_str() == Ok(PLATFORM)
}

impl IPlugViewTrait for PlugView {
    unsafe fn isPlatformTypeSupported(&self, kind: FIDString) -> tresult {
        if supported(kind) { kResultOk } else { kResultFalse }
    }

    unsafe fn attached(&self, parent: *mut std::ffi::c_void, kind: FIDString) -> tresult {
        if parent.is_null() || !supported(kind) {
            return kInvalidArgument;
        }
        self.state.parent.set(parent as usize);
        self.state.attached.set(true);
        kResultOk
    }

    unsafe fn removed(&self) -> tresult {
        self.state.attached.set(false);
        self.state.removed.set(self.state.removed.get() + 1);
        kResultOk
    }

    unsafe fn onWheel(&self, _distance: f32) -> tresult {
        kResultFalse
    }

    unsafe fn onKeyDown(&self, _key: char16, _code: i16, _modifiers: i16) -> tresult {
        kResultFalse
    }

    unsafe fn onKeyUp(&self, _key: char16, _code: i16, _modifiers: i16) -> tresult {
        kResultFalse
    }

    unsafe fn getSize(&self, size: *mut ViewRect) -> tresult {
        *size = ViewRect {
            left: 0,
            top: 0,
            right: self.state.width.get(),
            bottom: self.state.height.get(),
        };
        kResultOk
    }

    unsafe fn onSize(&self, new_size: *mut ViewRect) -> tresult {
        let rect = &*new_size;
        self.state.width.set(rect.right - rect.left);
        self.state.height.set(rect.bottom - rect.top);
        kResultOk
    }

    unsafe fn onFocus(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn setFrame(&self, frame: *mut IPlugFrame) -> tresult {
        *self.state.frame.borrow_mut() = ComRef::from_raw(frame).map(|frame| frame.to_com_ptr());
        kResultOk
    }

    unsafe fn canResize(&self) -> tresult {
        if self.state.can_resize.get() { kResultTrue } else { kResultFalse }
    }

    // sizes are multiples of 10, between 200 x 100 and 800 x 600
    unsafe fn checkSizeConstraint(&self, rect: *mut ViewRect) -> tresult {
        let rect = &mut *rect;
        let snap = |value: i32, min: i32, max: i32| ((value + 5) / 10 * 10).clamp(min, max);
        rect.right = rect.left + snap(rect.right - rect.left, 200, 800);
        rect.bottom = rect.top + snap(rect.bottom - rect.top, 100, 600);
        kResultOk
    }
}

impl IPlugViewContentScaleSupportTrait for PlugView {
    unsafe fn setContentScaleFactor(&self, factor: f32) -> tresult {
        self.state.scale.set(factor);
        kResultOk
    }
}

pub struct Restarter {
    handler: RefCell<Option<ComPtr<IComponentHandler>>>,
    values: [Cell<f64>; 3],
    latency: Cell<u32>,
    arrangement: Cell<SpeakerArrangement>,
    mono_only: Cell<bool>,
    renamed: Cell<bool>,
    view: RefCell<Option<Rc<ViewState>>>,
}

impl Restarter {
    pub const CID: TUID = uid(0x50524F42, 0x45000000, 0x00000000, 0x00000004);

    pub fn new() -> Self {
        Self {
            handler: RefCell::new(None),
            values: [Cell::new(0.0), Cell::new(0.0), Cell::new(0.0)],
            latency: Cell::new(0),
            arrangement: Cell::new(SpeakerArr::kStereo),
            mono_only: Cell::new(false),
            renamed: Cell::new(false),
            view: RefCell::new(None),
        }
    }

    unsafe fn restart(&self, flags: i32) {
        if let Some(handler) = &*self.handler.borrow() {
            handler.restartComponent(flags);
        }
    }

    unsafe fn trigger(&self, value: f64) {
        match (value * 10.0).round() as i32 {
            1 | 9 => {
                self.latency.set(16);
                self.restart(RestartFlags_::kLatencyChanged);
            }
            2 => {
                self.mono_only.set(true);
                self.arrangement.set(SpeakerArr::kMono);
                self.restart(RestartFlags_::kIoChanged);
            }
            3 => self.restart(RestartFlags_::kReloadComponent),
            4 => {
                self.renamed.set(true);
                self.restart(RestartFlags_::kParamTitlesChanged);
            }
            5 => {
                self.values[2].set(0.9);
                self.restart(RestartFlags_::kParamValuesChanged);
            }
            6 => {
                if let Some(view) = &*self.view.borrow() {
                    if view.alive.get() {
                        if let Some(frame) = &*view.frame.borrow() {
                            let mut rect = ViewRect { left: 0, top: 0, right: 640, bottom: 480 };
                            frame.resizeView(view.ptr.get(), &mut rect);
                        }
                    }
                }
            }
            7 => {
                if let Some(view) = &*self.view.borrow() {
                    view.can_resize.set(false);
                }
            }
            _ => {}
        }
    }

    fn channels(&self) -> i32 {
        if self.arrangement.get() == SpeakerArr::kMono { 1 } else { 2 }
    }
}

impl Class for Restarter {
    // the component and the controller are the same object
    type Interfaces = (IComponent, IAudioProcessor, IEditController);
}

impl IPluginBaseTrait for Restarter {
    unsafe fn initialize(&self, _context: *mut FUnknown) -> tresult {
        kResultOk
    }

    unsafe fn terminate(&self) -> tresult {
        self.handler.borrow_mut().take();
        kResultOk
    }
}

impl IComponentTrait for Restarter {
    unsafe fn getControllerClassId(&self, _class_id: *mut TUID) -> tresult {
        kNotImplemented
    }

    unsafe fn setIoMode(&self, _mode: IoMode) -> tresult {
        kResultOk
    }

    unsafe fn getBusCount(&self, media_type: MediaType, _dir: BusDirection) -> i32 {
        if media_type as MediaTypes == MediaTypes_::kAudio { 1 } else { 0 }
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

        let bus = &mut *bus;
        bus.mediaType = MediaTypes_::kAudio as MediaType;
        bus.direction = dir;
        bus.channelCount = self.channels();
        copy_wstring("Audio", &mut bus.name);
        bus.busType = BusTypes_::kMain as BusType;
        bus.flags = BusInfo_::BusFlags_::kDefaultActive as u32;
        kResultOk
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

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        match read_f64s::<3>(state) {
            Some(values) => {
                for (cell, value) in self.values.iter().zip(values) {
                    cell.set(value);
                }
                kResultOk
            }
            None => kResultFalse,
        }
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        write_f64s(state, &[self.values[0].get(), self.values[1].get(), self.values[2].get()])
    }
}

impl IAudioProcessorTrait for Restarter {
    unsafe fn setBusArrangements(
        &self,
        inputs: *mut SpeakerArrangement,
        num_ins: i32,
        outputs: *mut SpeakerArrangement,
        num_outs: i32,
    ) -> tresult {
        if num_ins != 1 || num_outs != 1 || *inputs != *outputs {
            return kResultFalse;
        }

        let wanted = *inputs;
        let allowed = wanted == SpeakerArr::kMono
            || (wanted == SpeakerArr::kStereo && !self.mono_only.get());

        if allowed {
            self.arrangement.set(wanted);
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
        *arr = self.arrangement.get();
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
        self.latency.get()
    }

    unsafe fn setupProcessing(&self, _setup: *mut ProcessSetup) -> tresult {
        kResultOk
    }

    unsafe fn setProcessing(&self, _state: TBool) -> tresult {
        kResultOk
    }

    unsafe fn process(&self, data: *mut ProcessData) -> tresult {
        let d = &*data;

        if let Some(changes) = ComRef::from_raw(d.inputParameterChanges) {
            for q in 0..changes.getParameterCount() {
                let Some(queue) = ComRef::from_raw(changes.getParameterData(q)) else {
                    continue;
                };
                let points = queue.getPointCount();
                let (mut offset, mut value) = (0, 0.0);
                if queue.getParameterId() == 0
                    && points > 0
                    && queue.getPoint(points - 1, &mut offset, &mut value) == kResultOk
                    && (value * 10.0).round() as i32 == 9
                {
                    // from the audio thread
                    self.trigger(value);
                }
            }
        }

        if d.numInputs != 1 || d.numOutputs != 1 {
            return kResultFalse;
        }

        let (input, output) = (&*d.inputs, &*d.outputs);
        let channels = input.numChannels.min(output.numChannels) as usize;
        let n = d.numSamples as usize;

        let inputs = slice::from_raw_parts(input.__field0.channelBuffers32, channels);
        let outputs = slice::from_raw_parts(output.__field0.channelBuffers32, channels);
        for (input, output) in inputs.iter().zip(outputs) {
            slice::from_raw_parts_mut(*output, n).copy_from_slice(slice::from_raw_parts(*input, n));
        }

        kResultOk
    }

    unsafe fn getTailSamples(&self) -> u32 {
        0
    }
}

impl IEditControllerTrait for Restarter {
    unsafe fn setComponentState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn setState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::setState(self, state)
    }

    unsafe fn getState(&self, state: *mut IBStream) -> tresult {
        IComponentTrait::getState(self, state)
    }

    unsafe fn getParameterCount(&self) -> i32 {
        10
    }

    unsafe fn getParameterInfo(&self, index: i32, info: *mut ParameterInfo) -> tresult {
        let info = &mut *info;
        let title = match index {
            0 => "Trigger",
            1 if self.renamed.get() => "Name B",
            1 => "Name A",
            2 => "Value",
            3 => "ViewAttached",
            4 => "ViewWidth",
            5 => "ViewHeight",
            6 => "ViewScale",
            7 => "ViewRemoved",
            8 => "ViewFrame",
            9 => "ViewParent",
            _ => return kInvalidArgument,
        };

        info.id = index as u32;
        copy_wstring(title, &mut info.title);
        copy_wstring(title, &mut info.shortTitle);
        copy_wstring("", &mut info.units);
        info.stepCount = 0;
        info.defaultNormalizedValue = 0.0;
        info.unitId = 0;
        info.flags = if index < 3 {
            ParameterInfo_::ParameterFlags_::kCanAutomate
        } else {
            ParameterInfo_::ParameterFlags_::kIsReadOnly
        };
        kResultOk
    }

    unsafe fn getParamStringByValue(&self, id: u32, value: f64, string: *mut String128) -> tresult {
        if id > 9 {
            return kInvalidArgument;
        }
        copy_wstring(&format!("{value:.2}"), &mut *string);
        kResultOk
    }

    unsafe fn getParamValueByString(&self, _id: u32, _string: *mut TChar, _value: *mut f64) -> tresult {
        kNotImplemented
    }

    unsafe fn normalizedParamToPlain(&self, _id: u32, value: f64) -> f64 {
        value
    }

    unsafe fn plainParamToNormalized(&self, _id: u32, plain: f64) -> f64 {
        plain
    }

    unsafe fn getParamNormalized(&self, id: u32) -> f64 {
        if let Some(value) = self.values.get(id as usize) {
            return value.get();
        }

        let view = self.view.borrow();
        let Some(view) = view.as_ref() else {
            return 0.0;
        };

        match id {
            3 => f64::from(view.attached.get()),
            4 => f64::from(view.width.get()) / 1000.0,
            5 => f64::from(view.height.get()) / 1000.0,
            6 => f64::from(view.scale.get()) / 4.0,
            7 => f64::from(view.removed.get()) / 100.0,
            8 => f64::from(view.frame.borrow().is_some()),
            9 => f64::from(view.parent.get() != 0),
            _ => 0.0,
        }
    }

    unsafe fn setParamNormalized(&self, id: u32, value: f64) -> tresult {
        let Some(cell) = self.values.get(id as usize) else {
            // the editor's parameters are only reported
            return kResultFalse;
        };
        cell.set(value);

        if id == 0 {
            self.trigger(value);
        }
        kResultOk
    }

    unsafe fn setComponentHandler(&self, handler: *mut IComponentHandler) -> tresult {
        *self.handler.borrow_mut() = ComRef::from_raw(handler).map(|handler| handler.to_com_ptr());
        kResultOk
    }

    unsafe fn createView(&self, name: FIDString) -> *mut IPlugView {
        if name.is_null() || CStr::from_ptr(name).to_str() != Ok("editor") {
            return std::ptr::null_mut();
        }

        let state = Rc::new(ViewState::new());
        let view = ComWrapper::new(PlugView { state: state.clone() });
        let ptr = view.to_com_ptr::<IPlugView>().unwrap();

        state.ptr.set(ptr.as_ptr());
        *self.view.borrow_mut() = Some(state);

        // the host owns this reference
        ptr.into_raw()
    }
}
