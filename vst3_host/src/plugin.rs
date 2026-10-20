use crate::{
	AudioConfig, AudioThread, ComponentHandler, EditorSize, Error, MainThreadMessage, MemoryStream,
	Module, ParamInfo, ParentWindow, PlatformType, PlugFrame, PluginDescriptor, RestartActions,
	RestartFlags, buses::negotiate, host::HostApplication, state, util::utf16_array_to_string,
};
use log::warn;
use std::{
	collections::HashMap,
	ffi::c_void,
	fmt::{Debug, Formatter},
	mem::zeroed,
	ptr::null_mut,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicI32, Ordering::Relaxed},
		mpsc::{Receiver, Sender},
	},
};
use vst3::{
	ComPtr, ComWrapper, Interface,
	Steinberg::{
		FUnknown, IBStream, IPlugFrame, IPlugView, IPlugViewContentScaleSupport,
		IPlugViewContentScaleSupportTrait as _, IPlugViewTrait as _, IPluginBaseTrait as _,
		IPluginFactory, IPluginFactory3, IPluginFactory3Trait as _, IPluginFactoryTrait as _, TUID,
		ViewRect,
		Vst::{
			IAudioProcessor, IAudioProcessorTrait as _, IComponent, IComponentHandler,
			IComponentTrait as _, IConnectionPoint, IConnectionPointTrait as _, IEditController,
			IEditControllerTrait as _, IoModes_, ParameterInfo, ProcessModes_, ProcessSetup,
			String128, SymbolicSampleSizes_, ViewType,
		},
		kInvalidArgument, kNotImplemented, kResultOk,
	},
};

/// Asks `factory` to create the class `cid`, and queries it for `I`.
fn create<I: Interface>(factory: &ComPtr<IPluginFactory>, cid: &[u8; 16]) -> Option<ComPtr<I>> {
	let mut obj = null_mut::<c_void>();

	// SAFETY:
	// `cid` and `I::IID` are 16 bytes each, and `obj` is a valid out pointer.
	let result = unsafe {
		factory.createInstance(cid.as_ptr().cast(), I::IID.as_ptr().cast(), &raw mut obj)
	};

	if result != kResultOk {
		return None;
	}

	// SAFETY:
	// On success `createInstance` hands us an owned, valid pointer to an `I`.
	unsafe { ComPtr::from_raw(obj.cast::<I>()) }
}

/// A plugin's editor: its view, the frame it talks to the host through, and whether it's in a
/// window.
struct Editor {
	view: ComPtr<IPlugView>,
	frame: ComWrapper<PlugFrame>,
	attached: bool,
}

/// A loaded, initialized (but not yet active) VST3 plugin instance.
///
/// All calls have to be made from the main thread.
#[expect(
	clippy::struct_excessive_bools,
	reason = "these are independent steps of the plugin's lifecycle"
)]
pub struct Plugin {
	descriptor: PluginDescriptor,
	component: ComPtr<IComponent>,
	processor: ComPtr<IAudioProcessor>,
	controller: Option<ComPtr<IEditController>>,
	/// Whether the controller is the component itself, instead of a separate object.
	single_component: bool,
	component_initialized: bool,
	controller_initialized: bool,
	connection: Option<(ComPtr<IConnectionPoint>, ComPtr<IConnectionPoint>)>,
	params: Vec<ParamInfo>,
	param_index: HashMap<u32, usize>,
	active: bool,
	editor: Option<Editor>,
	sender: Sender<MainThreadMessage>,
	/// Restart requests the audio thread hasn't seen yet.
	restart_flags: Arc<AtomicI32>,
	/// Set to `false` when the plugin is deactivated, which makes its `AudioThread` inert.
	alive: Arc<AtomicBool>,
	/// Kept alive for as long as the controller may call it.
	handler: ComWrapper<ComponentHandler>,
	_host: ComWrapper<HostApplication>,
	/// Must be last: the library can only be unloaded after everything above is released.
	module: Arc<Module>,
}

impl Debug for Plugin {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Plugin")
			.field("descriptor", &self.descriptor)
			.field("single_component", &self.single_component)
			.field("has_controller", &self.controller.is_some())
			.field("connected", &self.connection.is_some())
			.finish_non_exhaustive()
	}
}

impl Plugin {
	/// Loads the plugin's module, creates the component and controller and wires them up.
	pub fn new(
		descriptor: &PluginDescriptor,
	) -> Result<(Self, Receiver<MainThreadMessage>), Error> {
		let module = Arc::new(Module::load(&descriptor.path)?);
		let factory = module.factory().clone();

		let (sender, receiver) = std::sync::mpsc::channel();
		let host = ComWrapper::new(HostApplication::new());
		let restart_flags = Arc::new(AtomicI32::new(0));
		let handler = ComWrapper::new(ComponentHandler::with_restart_flags(
			sender.clone(),
			restart_flags.clone(),
		));

		let host_context = host.to_com_ptr::<FUnknown>().unwrap();

		if let Some(factory3) = factory.cast::<IPluginFactory3>() {
			// SAFETY:
			// `host_context` is a valid `FUnknown` that outlives the call.
			unsafe {
				factory3.setHostContext(host_context.as_ptr());
			}
		}

		let component = create::<IComponent>(&factory, &descriptor.id)
			.ok_or(Error::CreateFailed("component"))?;

		let processor = component
			.cast::<IAudioProcessor>()
			.ok_or(Error::MissingInterface("IAudioProcessor"))?;

		// From here on, `Drop` knows how to take the plugin apart, so early returns are fine.
		let mut this = Self {
			descriptor: descriptor.clone(),
			component,
			processor,
			controller: None,
			single_component: false,
			component_initialized: false,
			controller_initialized: false,
			connection: None,
			params: Vec::new(),
			param_index: HashMap::new(),
			active: false,
			editor: None,
			sender,
			restart_flags,
			alive: Arc::new(AtomicBool::new(false)),
			handler,
			_host: host,
			module,
		};

		// SAFETY:
		// `host_context` is a valid `FUnknown` that outlives the call.
		let result = unsafe { this.component.initialize(host_context.as_ptr()) };
		if result != kResultOk {
			return Err(Error::InitializeFailed("component", result));
		}
		this.component_initialized = true;

		this.create_controller(&factory, &host_context)?;
		this.connect();
		this.sync_controller();
		this.refresh_params();

		Ok((this, receiver))
	}

	fn create_controller(
		&mut self,
		factory: &ComPtr<IPluginFactory>,
		host_context: &ComPtr<FUnknown>,
	) -> Result<(), Error> {
		// Some plugins implement the controller on the component object itself.
		if let Some(controller) = self.component.cast::<IEditController>() {
			self.controller = Some(controller);
			self.single_component = true;
		} else {
			let mut cid = TUID::default();

			// SAFETY:
			// `cid` is a valid out pointer.
			let result = unsafe { self.component.getControllerClassId(&raw mut cid) };

			if result == kResultOk && cid != TUID::default() {
				let cid = cid.map(|b| b as u8);

				let controller = create::<IEditController>(factory, &cid)
					.ok_or(Error::CreateFailed("controller"))?;

				// SAFETY:
				// `host_context` is a valid `FUnknown` that outlives the call.
				let result = unsafe { controller.initialize(host_context.as_ptr()) };

				// Terminate has to be called even if initialization failed, `Drop` takes care of
				// that through the flag below.
				self.controller = Some(controller);
				self.controller_initialized = true;

				if result != kResultOk {
					return Err(Error::InitializeFailed("controller", result));
				}
			}
		}

		if let Some(controller) = &self.controller {
			let handler = self
				.handler
				.as_com_ref::<IComponentHandler>()
				.unwrap()
				.as_ptr();

			// SAFETY:
			// `handler` points to our handler, which outlives the controller's use of it, since
			// `Drop` clears it again.
			unsafe {
				controller.setComponentHandler(handler);
			}
		}

		Ok(())
	}

	/// Links component and controller through `IConnectionPoint`, if both support it.
	fn connect(&mut self) {
		if self.single_component {
			return;
		}

		let Some(controller) = &self.controller else {
			return;
		};

		let (Some(component_point), Some(controller_point)) = (
			self.component.cast::<IConnectionPoint>(),
			controller.cast::<IConnectionPoint>(),
		) else {
			return;
		};

		// SAFETY:
		// Both pointers are valid connection points that outlive the connection.
		let first = unsafe { component_point.connect(controller_point.as_ptr()) };

		// SAFETY:
		// Both pointers are valid connection points that outlive the connection.
		let second = unsafe { controller_point.connect(component_point.as_ptr()) };

		if first == kResultOk && second == kResultOk {
			self.connection = Some((component_point, controller_point));
		}
	}

	/// Whether the plugin is active, i.e. has an [`AudioThread`] out there.
	#[must_use]
	pub fn is_active(&self) -> bool {
		self.active
	}

	/// Sets up the plugin for processing audio, and hands out the object that does the
	/// processing. This has to be called from the main thread, while the plugin isn't active.
	///
	/// The main audio buses are run as stereo if the plugin allows it, and as mono if that's what
	/// the plugin insists on. Other audio buses, like sidechains, are switched off.
	pub fn activate(&mut self, config: AudioConfig) -> Result<AudioThread, Error> {
		if self.active {
			return Err(Error::AlreadyActive);
		}

		if config.max_frames == 0 || config.sample_rate.is_nan() || config.sample_rate <= 0.0 {
			return Err(Error::SetupFailed("audio configuration", kInvalidArgument));
		}

		// SAFETY:
		// `component` is a valid, initialized component. Plugins may refuse, which is fine.
		unsafe {
			self.component.setIoMode(IoModes_::kSimple as i32);
		}

		let layout = negotiate(&self.component, &self.processor)?;

		// SAFETY:
		// `processor` is a valid, initialized processor.
		let result = unsafe {
			self.processor
				.canProcessSampleSize(SymbolicSampleSizes_::kSample32 as i32)
		};
		if result != kResultOk {
			return Err(Error::SetupFailed("32-bit processing", result));
		}

		let mut setup = ProcessSetup {
			processMode: if config.offline {
				ProcessModes_::kOffline as i32
			} else {
				ProcessModes_::kRealtime as i32
			},
			symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
			maxSamplesPerBlock: config.max_frames as i32,
			sampleRate: config.sample_rate,
		};

		// SAFETY:
		// `setup` is a valid pointer for the duration of the call.
		let result = unsafe { self.processor.setupProcessing(&raw mut setup) };
		if result != kResultOk {
			return Err(Error::SetupFailed("setupProcessing", result));
		}

		// SAFETY:
		// `component` is a valid, initialized component.
		let result = unsafe { self.component.setActive(1) };
		if result != kResultOk {
			return Err(Error::SetupFailed("setActive", result));
		}

		self.active = true;
		self.alive = Arc::new(AtomicBool::new(true));

		// requests from before now are already dealt with, by being activated again
		self.restart_flags.store(0, Relaxed);

		// SAFETY:
		// `processor` is a valid, active processor.
		let latency = unsafe { self.processor.getLatencySamples() };

		Ok(AudioThread::new(
			self.processor.clone(),
			self.module.clone(),
			self.alive.clone(),
			self.restart_flags.clone(),
			config,
			layout,
			latency,
		))
	}

	/// Takes the plugin out of processing again. This has to be called from the main thread, and
	/// the audio thread must not be in the middle of processing a block.
	pub fn deactivate(&mut self, mut audio: AudioThread) {
		audio.stop();
		self.alive.store(false, Relaxed);
		drop(audio);

		if self.active {
			self.active = false;

			// SAFETY:
			// `component` is a valid component, and processing was stopped above.
			unsafe {
				self.component.setActive(0);
			}
		}
	}

	/// Tells the controller what the component's state is, which is how the controller learns
	/// the values of parameters that the component saved. Done once after creation, and after
	/// every load. Failing is fine, plugins without a controller state don't implement it.
	fn sync_controller(&self) {
		let Some(controller) = &self.controller else {
			return;
		};

		let stream = ComWrapper::new(MemoryStream::new());
		let Some(ptr) = stream.to_com_ptr::<IBStream>() else {
			return;
		};

		// SAFETY:
		// `ptr` is a valid stream, and `component` is a valid, initialized component.
		let result = unsafe { self.component.getState(ptr.as_ptr()) };
		if result != kResultOk {
			return;
		}

		if stream.byte_len() == 0 {
			return;
		}

		stream.rewind();

		// SAFETY:
		// `ptr` is a valid stream, and `controller` is a valid, initialized controller.
		let result = unsafe { controller.setComponentState(ptr.as_ptr()) };
		if result != kResultOk && result != kNotImplemented {
			warn!(
				"{}: the controller refused the component's state (tresult {result})",
				self.descriptor.name
			);
		}
	}

	/// Saves everything the plugin needs to come back as it is now: its processor state and its
	/// controller state, packed into one blob that [`load_state`](Self::load_state) takes.
	///
	/// This has to be called from the main thread.
	pub fn save_state(&self) -> Result<Vec<u8>, Error> {
		let component = ComWrapper::new(MemoryStream::new());
		let component_ptr = component.to_com_ptr::<IBStream>().unwrap();

		// SAFETY:
		// `component_ptr` is a valid stream, and `component` is a valid, initialized component.
		let result = unsafe { self.component.getState(component_ptr.as_ptr()) };
		let component_bytes = match result {
			r if r == kResultOk => component.take(),
			r if r == kNotImplemented => Vec::new(),
			r => return Err(Error::StateFailed("saving the component's state", r)),
		};

		let mut controller_bytes = Vec::new();

		// a controller that is the component saves its state with it
		if let Some(controller) = self.controller.as_ref().filter(|_| !self.single_component) {
			let stream = ComWrapper::new(MemoryStream::new());
			let ptr = stream.to_com_ptr::<IBStream>().unwrap();

			// SAFETY:
			// `ptr` is a valid stream, and `controller` is a valid, initialized controller.
			let result = unsafe { controller.getState(ptr.as_ptr()) };
			match result {
				r if r == kResultOk => controller_bytes = stream.take(),
				r if r == kNotImplemented => {}
				r => return Err(Error::StateFailed("saving the controller's state", r)),
			}
		}

		Ok(state::encode(&component_bytes, &controller_bytes))
	}

	/// Restores a state saved by [`save_state`](Self::save_state), of the same plugin.
	///
	/// This has to be called from the main thread. The plugin may have its parameters changed
	/// afterwards, so read them again.
	pub fn load_state(&self, blob: &[u8]) -> Result<(), Error> {
		let (component_bytes, controller_bytes) = state::decode(blob)?;

		if !component_bytes.is_empty() {
			let stream = ComWrapper::new(MemoryStream::from_bytes(component_bytes));
			let ptr = stream.to_com_ptr::<IBStream>().unwrap();

			// SAFETY:
			// `ptr` is a valid stream, and `component` is a valid, initialized component.
			let result = unsafe { self.component.setState(ptr.as_ptr()) };
			if result != kResultOk {
				return Err(Error::StateFailed("loading the component's state", result));
			}

			self.sync_controller();
		}

		if let Some(controller) = self.controller.as_ref().filter(|_| !self.single_component)
			&& !controller_bytes.is_empty()
		{
			let stream = ComWrapper::new(MemoryStream::from_bytes(controller_bytes));
			let ptr = stream.to_com_ptr::<IBStream>().unwrap();

			// SAFETY:
			// `ptr` is a valid stream, and `controller` is a valid, initialized controller.
			let result = unsafe { controller.setState(ptr.as_ptr()) };
			if result != kResultOk {
				return Err(Error::StateFailed("loading the controller's state", result));
			}
		}

		Ok(())
	}

	/// Does what a restart request from the plugin asks for on the plugin object itself, and says
	/// what is left for the caller to do. Call this on the main thread, for every
	/// [`MainThreadMessage::Restart`] that comes in.
	///
	/// Restart requests can arrive while the plugin is processing, so the caller decides when to
	/// do the rest. The plugin keeps working in the meantime, just as it was.
	pub fn handle_restart(&mut self, flags: RestartFlags) -> RestartActions {
		let reactivate = flags.needs_restart();

		// a reloaded plugin can have different parameters, and a reloaded component can have
		// different values that the controller has to learn about
		let params_changed = reactivate || flags.contains(RestartFlags::PARAM_TITLES_CHANGED);
		if params_changed {
			self.refresh_params();
		}

		if flags.contains(RestartFlags::RELOAD_COMPONENT) {
			self.sync_controller();
		}

		RestartActions {
			reactivate,
			// restarting measures the latency again anyway
			update_latency: !reactivate && flags.contains(RestartFlags::LATENCY_CHANGED),
			reread_param_values: flags.contains(RestartFlags::PARAM_VALUES_CHANGED)
				|| flags.contains(RestartFlags::RELOAD_COMPONENT),
			params_changed,
		}
	}

	/// Tears down audio processing and sets it up again with the same configuration, which is
	/// what [`RestartActions::reactivate`] asks for. Events that were waiting in the old
	/// [`AudioThread`] are lost.
	///
	/// If this fails the plugin is left inactive, and there's no audio thread any more.
	pub fn restart(&mut self, audio: AudioThread) -> Result<AudioThread, Error> {
		let config = audio.config();

		self.deactivate(audio);
		self.activate(config)
	}

	/// Asks the plugin for its editor, but doesn't show it yet. Returns the size the window has
	/// to be. Call [`attach_editor`](Self::attach_editor) once there is a window.
	///
	/// Editors have to be used on the main thread, and one at a time.
	pub fn create_editor(&mut self) -> Result<EditorSize, Error> {
		if self.editor.is_none() {
			let controller = self.controller.as_ref().ok_or(Error::NoEditor)?;

			// SAFETY:
			// `controller` is a valid, initialized controller, and the view type is a valid string.
			let view = unsafe { controller.createView(ViewType::kEditor) };

			// SAFETY:
			// `createView` returns an owned, valid pointer, or null.
			let view = unsafe { ComPtr::from_raw(view) }.ok_or(Error::NoEditor)?;

			// SAFETY:
			// `view` is a valid view, and the platform type is a valid string.
			let supported =
				unsafe { view.isPlatformTypeSupported(PlatformType::native().to_fid()) };
			if supported != kResultOk {
				return Err(Error::UnsupportedEditorPlatform);
			}

			self.editor = Some(Editor {
				view,
				frame: ComWrapper::new(PlugFrame::new(self.sender.clone())),
				attached: false,
			});
		}

		self.editor_size().ok_or(Error::NoEditor)
	}

	/// Puts the editor in a window. Call [`create_editor`](Self::create_editor) first.
	pub fn attach_editor(&mut self, parent: ParentWindow) -> Result<(), Error> {
		let editor = self.editor.as_mut().ok_or(Error::NoEditor)?;

		if editor.attached {
			return Err(Error::EditorAttached);
		}

		let frame = editor.frame.as_com_ref::<IPlugFrame>().unwrap().as_ptr();

		// SAFETY:
		// `view` is a valid view, and `frame` outlives it, since both are in `editor`.
		unsafe {
			editor.view.setFrame(frame);
		}

		// SAFETY:
		// `view` is a valid view. `parent` is up to the caller, as the window it points to.
		let result = unsafe { editor.view.attached(parent.handle, parent.kind.to_fid()) };
		if result != kResultOk {
			// SAFETY:
			// Passing null forgets the frame again.
			unsafe {
				editor.view.setFrame(null_mut());
			}

			return Err(Error::SetupFailed("attaching the editor", result));
		}

		editor.attached = true;
		Ok(())
	}

	/// Whether there is an editor, shown or not.
	#[must_use]
	pub const fn has_editor_open(&self) -> bool {
		self.editor.is_some()
	}

	/// The size the editor is right now.
	#[must_use]
	pub fn editor_size(&self) -> Option<EditorSize> {
		let editor = self.editor.as_ref()?;
		let mut rect = ViewRect {
			left: 0,
			top: 0,
			right: 0,
			bottom: 0,
		};

		// SAFETY:
		// `view` is a valid view and `rect` a valid out pointer.
		let result = unsafe { editor.view.getSize(&raw mut rect) };

		(result == kResultOk).then(|| EditorSize::from_rect(&rect))
	}

	/// Whether the user can change the size of the editor.
	#[must_use]
	pub fn editor_can_resize(&self) -> bool {
		self.editor.as_ref().is_some_and(|editor| {
			// SAFETY:
			// `view` is a valid view.
			let result = unsafe { editor.view.canResize() };

			result == kResultOk
		})
	}

	/// Asks the editor to be `size`, as the user does by dragging the window's edge. The editor
	/// may want something else, and the size it ended up with is returned. The window has to be
	/// made that size. `None` if the editor can't be resized.
	#[must_use]
	pub fn resize_editor(&self, size: EditorSize) -> Option<EditorSize> {
		if !self.editor_can_resize() {
			return None;
		}

		let editor = self.editor.as_ref()?;
		let mut rect = size.to_rect();

		// the plugin is allowed to adjust the size, and has to be told the size that's used
		// SAFETY:
		// `view` is a valid view and `rect` a valid pointer.
		unsafe {
			editor.view.checkSizeConstraint(&raw mut rect);
		}

		// SAFETY:
		// `view` is a valid view and `rect` a valid pointer.
		let result = unsafe { editor.view.onSize(&raw mut rect) };

		(result == kResultOk).then(|| EditorSize::from_rect(&rect))
	}

	/// Tells the editor that its window now has this size. This is the answer to
	/// [`MainThreadMessage::ResizeEditor`], where the editor asked for the size.
	pub fn apply_editor_size(&self, size: EditorSize) {
		if let Some(editor) = &self.editor {
			let mut rect = size.to_rect();

			// SAFETY:
			// `view` is a valid view and `rect` a valid pointer.
			unsafe {
				editor.view.onSize(&raw mut rect);
			}
		}
	}

	/// Tells the editor how many pixels a point is, on `HiDPI` screens. Returns `false` if the
	/// editor doesn't care.
	#[must_use]
	pub fn set_editor_scale(&self, factor: f32) -> bool {
		let Some(support) = self
			.editor
			.as_ref()
			.and_then(|editor| editor.view.cast::<IPlugViewContentScaleSupport>())
		else {
			return false;
		};

		// SAFETY:
		// `support` is a valid interface of the view.
		let result = unsafe { support.setContentScaleFactor(factor) };

		result == kResultOk
	}

	/// Takes the editor out of its window, and forgets it. Does nothing if there's none.
	pub fn close_editor(&mut self) {
		let Some(editor) = self.editor.take() else {
			return;
		};

		if editor.attached {
			// SAFETY:
			// `view` is a valid view that was attached.
			unsafe {
				editor.view.removed();
			}
		}

		// SAFETY:
		// Passing null forgets the frame.
		unsafe {
			editor.view.setFrame(null_mut());
		}
	}

	/// Reads the parameter list from the controller again. Call this after the plugin asks for
	/// its parameters to be rescanned.
	pub fn refresh_params(&mut self) {
		self.params.clear();
		self.param_index.clear();

		let Some(controller) = &self.controller else {
			return;
		};

		// SAFETY:
		// `controller` is a valid, initialized controller.
		let count = unsafe { controller.getParameterCount() };

		for index in 0..count {
			// SAFETY:
			// `ParameterInfo` is plain old data, all zeroes is a valid value.
			let mut info = unsafe { zeroed::<ParameterInfo>() };

			// SAFETY:
			// `info` is a valid out pointer.
			if unsafe { controller.getParameterInfo(index, &raw mut info) } != kResultOk {
				warn!("{}: couldn't read parameter {index}", self.descriptor.name);
				continue;
			}

			let info = ParamInfo::from_raw(&info);

			if self.param_index.contains_key(&info.id) {
				warn!(
					"{}: parameter id {} is used more than once",
					self.descriptor.name, info.id
				);
				continue;
			}

			self.param_index.insert(info.id, self.params.len());
			self.params.push(info);
		}
	}

	/// All parameters, in the order the plugin lists them.
	#[must_use]
	pub fn params(&self) -> &[ParamInfo] {
		&self.params
	}

	#[must_use]
	pub fn param(&self, id: u32) -> Option<&ParamInfo> {
		self.params.get(*self.param_index.get(&id)?)
	}

	/// The controller's current value of the parameter, normalized.
	#[must_use]
	pub fn param_normalized(&self, id: u32) -> Option<f64> {
		self.param(id)?;
		let controller = self.controller.as_ref()?;

		// SAFETY:
		// `controller` is a valid, initialized controller.
		Some(unsafe { controller.getParamNormalized(id) })
	}

	/// Sets the value of the parameter on the controller.
	///
	/// The value is clamped (and snapped, for discrete parameters) first, and the value that was
	/// actually set is returned. Unknown and read-only parameters are refused.
	///
	/// This only informs the controller. The audio processor gets new values through parameter
	/// changes in `process`.
	#[must_use]
	pub fn set_param_normalized(&self, id: u32, normalized: f64) -> Option<f64> {
		let param = self.param(id)?;
		if param.is_read_only() {
			return None;
		}

		let normalized = param.snap(normalized);
		let controller = self.controller.as_ref()?;

		// SAFETY:
		// `controller` is a valid, initialized controller.
		(unsafe { controller.setParamNormalized(id, normalized) } == kResultOk)
			.then_some(normalized)
	}

	/// The plugin's display string for a value, e.g. `"-6.0 dB"`.
	#[must_use]
	pub fn param_display(&self, id: u32, normalized: f64) -> Option<String> {
		self.param(id)?;
		let controller = self.controller.as_ref()?;
		let mut string: String128 = [0; 128];

		// SAFETY:
		// `string` is a valid out pointer.
		(unsafe { controller.getParamStringByValue(id, normalized, &raw mut string) } == kResultOk)
			.then(|| utf16_array_to_string(&string))
	}

	/// Parses user input like `"-6 dB"` into a normalized value, using the plugin's own parser.
	#[must_use]
	pub fn param_from_display(&self, id: u32, text: &str) -> Option<f64> {
		self.param(id)?;
		let controller = self.controller.as_ref()?;

		let mut string = text.encode_utf16().chain([0]).collect::<Vec<_>>();
		let mut normalized = 0.0;

		// SAFETY:
		// `string` is null-terminated and `normalized` is a valid out pointer.
		(unsafe { controller.getParamValueByString(id, string.as_mut_ptr(), &raw mut normalized) }
			== kResultOk)
			.then(|| normalized.clamp(0.0, 1.0))
	}

	/// Converts a normalized value to the plugin's "plain" value, e.g. decibels.
	#[must_use]
	pub fn normalized_to_plain(&self, id: u32, normalized: f64) -> Option<f64> {
		self.param(id)?;
		let controller = self.controller.as_ref()?;

		// SAFETY:
		// `controller` is a valid, initialized controller.
		Some(unsafe { controller.normalizedParamToPlain(id, normalized) })
	}

	/// Converts a plain value, e.g. decibels, to a normalized value.
	#[must_use]
	pub fn plain_to_normalized(&self, id: u32, plain: f64) -> Option<f64> {
		self.param(id)?;
		let controller = self.controller.as_ref()?;

		// SAFETY:
		// `controller` is a valid, initialized controller.
		Some(unsafe { controller.plainParamToNormalized(id, plain) })
	}

	#[must_use]
	pub fn descriptor(&self) -> &PluginDescriptor {
		&self.descriptor
	}

	#[must_use]
	pub fn has_controller(&self) -> bool {
		self.controller.is_some()
	}

	/// Whether the component and the controller are one and the same object.
	#[must_use]
	pub fn is_single_component(&self) -> bool {
		self.single_component
	}

	/// Whether component and controller were linked through `IConnectionPoint`.
	#[must_use]
	pub fn is_connected(&self) -> bool {
		self.connection.is_some()
	}
}

impl Drop for Plugin {
	fn drop(&mut self) {
		// This follows the order the VST3 hosting docs prescribe, which has the editor go first.
		self.close_editor();
		self.alive.store(false, Relaxed);

		if self.active {
			// SAFETY:
			// `component` is a valid component. A leaked `AudioThread` is inert by now.
			unsafe {
				self.component.setActive(0);
			}
		}

		if let Some((component_point, controller_point)) = self.connection.take() {
			// SAFETY:
			// Both pointers are valid connection points.
			unsafe {
				component_point.disconnect(controller_point.as_ptr());
			}

			// SAFETY:
			// Both pointers are valid connection points.
			unsafe {
				controller_point.disconnect(component_point.as_ptr());
			}
		}

		if let Some(controller) = &self.controller {
			// SAFETY:
			// Passing null clears the handler.
			unsafe {
				controller.setComponentHandler(null_mut());
			}

			if self.controller_initialized {
				// SAFETY:
				// The controller was initialized and is terminated exactly once.
				unsafe {
					controller.terminate();
				}
			}
		}

		if self.component_initialized {
			// SAFETY:
			// The component was initialized and is terminated exactly once.
			unsafe {
				self.component.terminate();
			}
		}
	}
}
