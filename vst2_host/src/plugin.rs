use crate::{
	api::{self, AEffect},
	audio_thread::{AudioConfig, AudioThread},
	editor::{EditorSize, ParentWindow},
	error::Error,
	host::{HostState, MainThreadMessage},
	module::Module,
	param::{ParamFlags, ParamInfo},
	plugin_descriptor::PluginDescriptor,
	state::{self, State},
	util::c_array_to_string,
};
use std::{
	ffi::c_void,
	fmt::{Debug, Formatter},
	ptr::null_mut,
	sync::{
		Arc,
		mpsc::{self, Receiver},
	},
};

/// What the main thread and the audio thread share.
pub struct Shared {
	/// The plugin itself.
	///
	/// The audio thread uses it to process audio, and the main thread uses it for
	/// everything else. A VST2 plugin is a single object, so both threads have to use
	/// the same one.
	pub(crate) effect: *mut AEffect,
	/// What the plugin tells the host.
	pub(crate) state: Box<HostState>,
}

// SAFETY: the plugin is used by the audio thread to process audio, and by the main
// thread for everything else. Only one of them uses it at a time, and the parts that
// are shared while it's running are behind a lock.
unsafe impl Send for Shared {}
// SAFETY: see above.
unsafe impl Sync for Shared {}

/// The plugin's editor, and the window it's in.
struct Editor {
	/// The size the editor had.
	size: EditorSize,
}

/// A loaded VST2 plugin.
///
/// All calls have to be made from the main thread, except for the ones the
/// [`AudioThread`] makes.
pub struct Plugin {
	descriptor: PluginDescriptor,
	shared: Arc<Shared>,
	params: Vec<ParamInfo>,
	active: bool,
	opened: bool,
	editor: Option<Editor>,
	/// Must be last: the library can only be unloaded after everything above is
	/// released.
	_module: Module,
}

impl Debug for Plugin {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Plugin")
			.field("descriptor", &self.descriptor)
			.field("active", &self.active)
			.field("has_editor", &self.editor.is_some())
			.finish_non_exhaustive()
	}
}

impl Plugin {
	/// Loads the plugin, and creates an instance of it.
	///
	/// The receiver is where the messages for the main thread come in.
	pub fn new(
		descriptor: &PluginDescriptor,
	) -> Result<(Self, Receiver<MainThreadMessage>), Error> {
		let module = Module::load(&descriptor.path)?;

		// The state is made before the plugin, so that the plugin can talk to the host
		// while it's being made.
		let (sender, receiver) = mpsc::channel();
		let mut state = Box::new(HostState::new(44100.0, 512, false, sender));
		let state_ptr: *mut HostState = &raw mut *state;

		let effect = module
			.create_effect(descriptor.unique_id, state_ptr)
			.ok_or_else(|| Error::NoEffect(descriptor.path.clone()))?;

		// SAFETY: the plugin gave us its main struct, which is valid until it's closed.
		let num_params = unsafe { (*effect).num_params }.max(0) as usize;

		// SAFETY: the plugin gave us its main struct, and the state is ours.
		unsafe {
			(*effect).reserved2 = state_ptr.cast();
		}

		let shared = Arc::new(Shared { effect, state });

		let mut this = Self {
			descriptor: descriptor.clone(),
			shared,
			params: Vec::with_capacity(num_params),
			active: false,
			opened: false,
			editor: None,
			_module: module,
		};

		this.refresh_params();
		this.remember_defaults();

		Ok((this, receiver))
	}

	/// The plugin's descriptor.
	#[must_use]
	pub fn descriptor(&self) -> &PluginDescriptor {
		&self.descriptor
	}

	/// The number of audio inputs the plugin has.
	#[must_use]
	pub fn num_inputs(&self) -> usize {
		// SAFETY: the effect is a valid plugin.
		unsafe { (*self.shared.effect).num_inputs.max(0) as usize }
	}

	/// The number of audio outputs the plugin has.
	#[must_use]
	pub fn num_outputs(&self) -> usize {
		// SAFETY: the effect is a valid plugin.
		unsafe { (*self.shared.effect).num_outputs.max(0) as usize }
	}

	/// The number of samples the plugin delays its output by.
	#[must_use]
	pub fn latency(&self) -> usize {
		// SAFETY: the effect is a valid plugin.
		unsafe { (*self.shared.effect).initial_delay.max(0) as usize }
	}

	/// Whether the plugin has an editor.
	#[must_use]
	pub fn has_editor(&self) -> bool {
		// SAFETY: the effect is a valid plugin.
		let flags = unsafe { (*self.shared.effect).flags };

		flags & api::flags::HAS_EDITOR != 0
	}

	/// Whether the editor is open in a window.
	#[must_use]
	pub fn has_editor_open(&self) -> bool {
		self.editor.is_some()
	}

	/// Whether the plugin is active, i.e. has an [`AudioThread`] out there.
	#[must_use]
	pub const fn is_active(&self) -> bool {
		self.active
	}

	/// All parameters, in the order the plugin lists them.
	#[must_use]
	pub fn params(&self) -> &[ParamInfo] {
		&self.params
	}

	/// The parameter with the given id.
	#[must_use]
	pub fn param(&self, id: u32) -> Option<&ParamInfo> {
		self.params.iter().find(|param| param.id == id)
	}

	/// The plugin's current value of the parameter, normalized.
	#[must_use]
	pub fn param_normalized(&self, id: u32) -> Option<f64> {
		let index = self.param(id)?.id;

		// SAFETY: the effect is a valid plugin.
		let get_parameter = unsafe { (*self.shared.effect).get_parameter };

		// SAFETY: the effect is a valid plugin, and the parameter is one of its own.
		Some(f64::from(unsafe {
			get_parameter(self.shared.effect, index as i32)
		}))
	}

	/// Sets the value of the parameter.
	///
	/// The value is clamped (and snapped, for discrete parameters) first, and the value
	/// that was actually set is returned.
	#[must_use]
	pub fn set_param_normalized(&self, id: u32, normalized: f64) -> Option<f64> {
		let param = self.param(id)?;
		let normalized = param.snap(normalized);

		// SAFETY: the effect is a valid plugin.
		let set_parameter = unsafe { (*self.shared.effect).set_parameter };

		// SAFETY: the effect is a valid plugin, and the parameter is one of its own.
		unsafe {
			set_parameter(self.shared.effect, param.id as i32, normalized as f32);
		}

		Some(normalized)
	}

	/// The plugin's display string for a value, e.g. `"-6.0 dB"`.
	#[must_use]
	pub fn param_display(&self, id: u32, _value: f64) -> Option<String> {
		let param = self.param(id)?;

		let mut buffer = [0 as std::os::raw::c_char; api::MAX_PARAM_STR_LEN];

		self.dispatch(
			api::opcodes::GET_PARAM_DISPLAY,
			param.id as i32,
			0,
			buffer.as_mut_ptr().cast(),
			0.0,
		);

		Some(c_array_to_string(&buffer))
	}

	/// The number of programs (presets) the plugin has.
	#[must_use]
	pub fn program_count(&self) -> usize {
		// SAFETY: the effect is a valid plugin.
		unsafe { (*self.shared.effect).num_programs.max(0) as usize }
	}

	/// The name of a program (preset).
	#[must_use]
	pub fn program_name(&self, index: usize) -> Option<String> {
		if index >= self.program_count() {
			return None;
		}

		let mut buffer = [0 as std::os::raw::c_char; api::MAX_PROGRAM_NAME_LEN];

		self.dispatch(
			api::opcodes::GET_PROGRAM_NAME,
			index as i32,
			0,
			buffer.as_mut_ptr().cast(),
			0.0,
		);

		Some(c_array_to_string(&buffer))
	}

	/// The program (preset) the plugin is on.
	#[must_use]
	pub fn current_program(&self) -> i32 {
		self.dispatch(api::opcodes::GET_PROGRAM, 0, 0, null_mut(), 0.0) as i32
	}

	/// Changes the program (preset).
	pub fn set_program(&self, index: i32) {
		self.dispatch(api::opcodes::SET_PROGRAM, index, 0, null_mut(), 0.0);
	}

	/// Sets up the plugin for processing audio, and hands out the object that does the
	/// processing. This has to be called from the main thread, while the plugin isn't
	/// active.
	pub fn activate(&mut self, config: AudioConfig) -> Result<AudioThread, Error> {
		if self.active {
			return Err(Error::AlreadyActive);
		}

		if config.max_frames == 0 || config.sample_rate.is_nan() || config.sample_rate <= 0.0 {
			return Err(Error::StateFailed("set up the audio"));
		}

		self.shared
			.state
			.set_config(config.sample_rate, config.max_frames, config.offline);

		// the plugin has to be opened before it's told how it's set up
		if !self.opened {
			self.dispatch(api::opcodes::OPEN, 0, 0, null_mut(), 0.0);
			self.opened = true;
		}

		self.dispatch(
			api::opcodes::SET_SAMPLE_RATE,
			0,
			0,
			null_mut(),
			config.sample_rate as f32,
		);
		self.dispatch(
			api::opcodes::SET_BLOCK_SIZE,
			0,
			config.max_frames as isize,
			null_mut(),
			0.0,
		);
		self.dispatch(api::opcodes::MAINS_CHANGED, 0, 1, null_mut(), 0.0);
		self.dispatch(api::opcodes::START_PROCESS, 0, 0, null_mut(), 0.0);
		self.active = true;

		// The plugin gets as many channels as it has, so that it doesn't read past the
		// end of the buffers. The ones the host has no audio for stay silent.
		let inputs = self.num_inputs();
		let outputs = self.num_outputs();

		Ok(AudioThread::new(
			self.shared.clone(),
			config,
			inputs,
			outputs,
			self.latency(),
		))
	}

	/// Takes the plugin out of processing again. This has to be called from the main
	/// thread, and the audio thread must not be in the middle of processing a block.
	pub fn deactivate(&mut self, audio: AudioThread) {
		audio.stop();
		drop(audio);

		if self.active {
			self.active = false;
			self.dispatch(api::opcodes::STOP_PROCESS, 0, 0, null_mut(), 0.0);
			self.dispatch(api::opcodes::MAINS_CHANGED, 0, 0, null_mut(), 0.0);
		}
	}

	/// Tears down audio processing and sets it up again with the same configuration.
	/// Events that were waiting in the old [`AudioThread`] are lost.
	pub fn restart(&mut self, audio: AudioThread) -> Result<AudioThread, Error> {
		let config = audio.config();

		self.deactivate(audio);
		self.activate(config)
	}

	/// The size of the plugin's editor.
	#[must_use]
	pub fn editor_size(&self) -> Option<EditorSize> {
		if !self.has_editor() {
			return None;
		}

		let mut rect: *mut api::Rect = null_mut();

		self.dispatch(
			api::opcodes::EDIT_GET_RECT,
			0,
			0,
			(&raw mut rect).cast(),
			0.0,
		);

		if rect.is_null() {
			return None;
		}

		// SAFETY: the plugin wrote a rectangle there, which is valid until the next call.
		let rect = unsafe { *rect };

		Some(EditorSize {
			width: rect.width().max(1),
			height: rect.height().max(1),
		})
	}

	/// Puts the editor in a window. Call [`editor_size`](Self::editor_size) first to know
	/// how big the window has to be.
	pub fn open_editor(&mut self, parent: ParentWindow) -> Result<(), Error> {
		if self.editor.is_some() {
			return Err(Error::EditorAttached);
		}

		if !self.has_editor() {
			return Err(Error::NoEditor);
		}

		let size = self.editor_size().ok_or(Error::NoEditor)?;

		let result = self.dispatch(api::opcodes::EDIT_OPEN, 0, 0, parent.handle.cast(), 0.0);

		if result == 0 {
			return Err(Error::EditorFailed);
		}

		// the plugin may have changed its mind about the size
		let size = self.editor_size().unwrap_or(size);

		crate::platform::resize_child(parent.handle, size);

		self.editor = Some(Editor { size });

		Ok(())
	}

	/// The size of the editor, if it's open.
	#[must_use]
	pub fn editor_size_if_open(&self) -> Option<EditorSize> {
		self.editor.as_ref().map(|editor| editor.size)
	}

	/// Takes the editor out of its window, and forgets it. Does nothing if there's none.
	pub fn close_editor(&mut self) {
		if self.editor.take().is_some() {
			self.dispatch(api::opcodes::EDIT_CLOSE, 0, 0, null_mut(), 0.0);
		}
	}

	/// Gives the editor a chance to do something, e.g. redraw itself.
	pub fn editor_idle(&self) {
		if self.editor.is_some() {
			self.dispatch(api::opcodes::EDIT_IDLE, 0, 0, null_mut(), 0.0);
		}
	}

	/// Saves everything the plugin needs to come back as it is now.
	///
	/// This has to be called from the main thread.
	pub fn save_state(&self) -> Result<Vec<u8>, Error> {
		let mut state = State {
			program: self.current_program(),
			params: self
				.params
				.iter()
				.map(|param| {
					self.param_normalized(param.id)
						.map_or(0.0, |value| value as f32)
				})
				.collect(),
			chunk: Vec::new(),
		};

		if self.has_chunks() {
			state.chunk = self.get_chunk()?;
		}

		Ok(state::encode(&state))
	}

	/// Restores a state saved by [`save_state`](Self::save_state), of the same plugin.
	///
	/// This has to be called from the main thread. The plugin may have its parameters
	/// changed afterwards, so read them again.
	pub fn load_state(&self, blob: &[u8]) -> Result<(), Error> {
		let state = state::decode(blob)?;

		if !state.chunk.is_empty() && self.has_chunks() {
			self.set_chunk(&state.chunk)?;
		}

		// SAFETY: the effect is a valid plugin.
		let set_parameter = unsafe { (*self.shared.effect).set_parameter };

		for (param, value) in self.params.iter().zip(&state.params) {
			// SAFETY: the effect is a valid plugin, and the parameter is one of its own.
			unsafe {
				set_parameter(self.shared.effect, param.id as i32, *value);
			}
		}

		if state.program >= 0 {
			self.set_program(state.program);
		}

		Ok(())
	}

	/// Whether the plugin's state is in formatless chunks.
	#[must_use]
	pub fn has_chunks(&self) -> bool {
		// SAFETY: the effect is a valid plugin.
		let flags = unsafe { (*self.shared.effect).flags };

		flags & api::flags::PROGRAM_CHUNKS != 0
	}

	fn get_chunk(&self) -> Result<Vec<u8>, Error> {
		let mut data: *mut c_void = null_mut();

		let size = self.dispatch(api::opcodes::GET_CHUNK, 0, 0, (&raw mut data).cast(), 0.0);

		if size <= 0 || data.is_null() {
			return Err(Error::StateFailed("save"));
		}

		// SAFETY: the plugin wrote `size` bytes at `data`, which are valid until the
		// plugin is closed. The memory belongs to the plugin, so it's left alone.
		let chunk =
			unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size as usize) }.to_vec();

		Ok(chunk)
	}

	fn set_chunk(&self, chunk: &[u8]) -> Result<(), Error> {
		let result = self.dispatch(
			api::opcodes::SET_CHUNK,
			0,
			chunk.len() as isize,
			chunk.as_ptr().cast_mut().cast(),
			0.0,
		);

		if result == 0 {
			return Err(Error::StateFailed("load"));
		}

		Ok(())
	}

	/// Reads the parameters of the plugin again.
	fn refresh_params(&mut self) {
		self.params.clear();

		// SAFETY: the effect is a valid plugin.
		let count = unsafe { (*self.shared.effect).num_params }.max(0);

		for index in 0..count {
			let mut name = [0 as std::os::raw::c_char; api::MAX_NAME_LEN];
			self.dispatch(
				api::opcodes::GET_PARAM_NAME,
				index,
				0,
				name.as_mut_ptr().cast(),
				0.0,
			);

			let mut label = [0 as std::os::raw::c_char; api::MAX_LABEL_LEN];
			self.dispatch(
				api::opcodes::GET_PARAM_LABEL,
				index,
				0,
				label.as_mut_ptr().cast(),
				0.0,
			);

			let automatable =
				self.dispatch(api::opcodes::CAN_BE_AUTOMATED, index, 0, null_mut(), 0.0) != 0;

			// SAFETY: the properties are plain old data, and all zeroes is a valid value.
			let mut properties: api::ParameterProperties =
				unsafe { std::mem::zeroed::<api::ParameterProperties>() };

			let has_properties = self.dispatch(
				api::opcodes::GET_PARAMETER_PROPERTIES,
				index,
				0,
				(&raw mut properties).cast(),
				0.0,
			) != 0;

			let mut flags = if automatable {
				ParamFlags::CAN_AUTOMATE
			} else {
				0
			};

			let mut step_count = 0;

			if has_properties {
				if properties.flags & api::param_flags::IS_SWITCH != 0 {
					flags |= ParamFlags::IS_SWITCH;
					step_count = 1;
				} else if properties.flags & api::param_flags::USES_INT_STEP != 0
					&& properties.step_integer > 0
				{
					flags |= ParamFlags::IS_DISCRETE;
					step_count = properties.step_integer;
				} else if properties.flags & api::param_flags::USES_FLOAT_STEP != 0 {
					step_count = 0;
				}
			}

			let short_title = if has_properties {
				c_array_to_string(&properties.short_label)
			} else {
				String::new()
			};

			self.params.push(ParamInfo {
				id: index as u32,
				title: c_array_to_string(&name).into(),
				short_title: short_title.into(),
				units: c_array_to_string(&label).into(),
				step_count,
				default_normalized: 0.0,
				flags: ParamFlags(flags),
			});
		}
	}

	/// Remembers the values the parameters had when the plugin was loaded, which are
	/// the values the host resets them to.
	fn remember_defaults(&mut self) {
		// SAFETY: the effect is a valid plugin.
		let get_parameter = unsafe { (*self.shared.effect).get_parameter };

		for param in &mut self.params {
			// SAFETY: the effect is a valid plugin, and the parameter is one of its own.
			let value = unsafe { get_parameter(self.shared.effect, param.id as i32) };

			param.default_normalized = f64::from(value);
		}
	}

	/// Calls the plugin.
	fn dispatch(&self, opcode: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32) -> isize {
		// SAFETY: the effect is a valid plugin.
		let dispatcher = unsafe { (*self.shared.effect).dispatcher };

		// SAFETY: the effect is a valid plugin.
		unsafe { dispatcher(self.shared.effect, opcode, index, value, ptr, opt) }
	}
}

impl Drop for Plugin {
	fn drop(&mut self) {
		self.close_editor();
		self.shared.state.kill();

		if self.opened {
			self.dispatch(api::opcodes::CLOSE, 0, 0, null_mut(), 0.0);
			self.opened = false;
		}
	}
}
