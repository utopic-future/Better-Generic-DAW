//! The VST2 API, as the plugins expect it.
//!
//! These are the definitions of the VST2 API, which is what VST2 plugins are built
//! against. A host only needs a small part of it: the plugin's main struct, the events
//! that go in and out, and the numbers of the calls that go back and forth.
//!
//! Everything here is plain old data, so it can be handed to a plugin as it is.

use std::{ffi::c_void, os::raw::c_char};

/// The magic number at the start of every plugin's main struct: `VstP`.
pub const MAGIC: i32 = i32::from_be_bytes(*b"VstP");

/// The flags a plugin can have.
pub mod flags {
	/// The plugin has an editor.
	pub const HAS_EDITOR: i32 = 1;
	/// The plugin can process 32 bit audio.
	pub const CAN_REPLACING: i32 = 1 << 4;
	/// The plugin's state is in formatless chunks.
	pub const PROGRAM_CHUNKS: i32 = 1 << 5;
	/// The plugin is an instrument.
	pub const IS_SYNTH: i32 = 1 << 8;
	/// The plugin is quiet when it isn't processing.
	pub const NO_SOUND_IN_STOP: i32 = 1 << 9;
	/// The plugin can process 64 bit audio.
	pub const CAN_DOUBLE_REPLACING: i32 = 1 << 12;
}

/// The calls the host makes on the plugin.
pub mod opcodes {
	/// The plugin is opened. The host calls this once, after it has told the plugin the
	/// sample rate and the block size.
	pub const OPEN: i32 = 0;
	/// The plugin is closed. The host calls this when it's done with the plugin.
	pub const CLOSE: i32 = 1;
	/// Changes the program (preset). The program is the index.
	pub const SET_PROGRAM: i32 = 2;
	/// The current program (preset).
	pub const GET_PROGRAM: i32 = 3;
	/// The name of the current program (preset).
	pub const GET_PROGRAM_NAME: i32 = 5;
	/// The unit of a parameter, e.g. `dB`. The parameter is the index.
	pub const GET_PARAM_LABEL: i32 = 6;
	/// The value of a parameter, e.g. `-6.0 dB`. The parameter is the index.
	pub const GET_PARAM_DISPLAY: i32 = 7;
	/// The name of a parameter. The parameter is the index.
	pub const GET_PARAM_NAME: i32 = 8;
	/// The sample rate the host uses. It's in `opt`.
	pub const SET_SAMPLE_RATE: i32 = 10;
	/// The largest block the host will process. It's in `value`.
	pub const SET_BLOCK_SIZE: i32 = 11;
	/// The plugin is enabled (1) or bypassed (0). It's in `value`.
	pub const MAINS_CHANGED: i32 = 12;
	/// The size of the editor. The plugin writes a pointer to a rectangle to `ptr`.
	pub const EDIT_GET_RECT: i32 = 13;
	/// Opens the editor in the window the host gives it. The window is in `ptr`.
	pub const EDIT_OPEN: i32 = 14;
	/// Closes the editor.
	pub const EDIT_CLOSE: i32 = 15;
	/// Gives the editor a chance to do something, e.g. redraw itself.
	pub const EDIT_IDLE: i32 = 19;
	/// The plugin's state. The plugin writes a pointer to its data to `ptr`, and the
	/// size of the data is the return value.
	pub const GET_CHUNK: i32 = 23;
	/// Gives the plugin a state to restore. The size of the data is in `value`.
	pub const SET_CHUNK: i32 = 24;
	/// The events for this block. The pointer to them is in `ptr`.
	pub const PROCESS_EVENTS: i32 = 25;
	/// Whether a parameter can be automated. The parameter is the index.
	pub const CAN_BE_AUTOMATED: i32 = 26;
	/// Turns a string into a parameter value. The parameter is the index, and the string
	/// is in `ptr`.
	pub const STRING_TO_PARAMETER: i32 = 27;
	/// The properties of an input channel. The channel is the index.
	pub const GET_INPUT_PROPERTIES: i32 = 33;
	/// The properties of an output channel. The channel is the index.
	pub const GET_OUTPUT_PROPERTIES: i32 = 34;
	/// What kind of plugin this is, e.g. an effect or an instrument.
	pub const GET_PLUG_CATEGORY: i32 = 35;
	/// The plugin is bypassed (1) or not (0). It's in `value`.
	pub const SET_BYPASS: i32 = 44;
	/// The name of the plugin.
	pub const GET_EFFECT_NAME: i32 = 45;
	/// The name of the vendor of the plugin.
	pub const GET_VENDOR_STRING: i32 = 47;
	/// The name of the product, which is often the name of the plugin.
	pub const GET_PRODUCT_STRING: i32 = 48;
	/// The version of the plugin.
	pub const GET_VENDOR_VERSION: i32 = 49;
	/// Whether the plugin can do something. The text is in `ptr`.
	pub const CAN_DO: i32 = 51;
	/// The plugin has nothing to do for a while.
	pub const IDLE: i32 = 53;
	/// Everything there is to know about a parameter. The parameter is the index.
	pub const GET_PARAMETER_PROPERTIES: i32 = 56;
	/// The version of the API the plugin uses, `2400` for VST 2.4.
	pub const GET_VST_VERSION: i32 = 58;
	/// The next plugin in a shell plugin. The name is written to `ptr`.
	pub const SHELL_GET_NEXT_PLUGIN: i32 = 70;
	/// The plugin starts processing.
	pub const START_PROCESS: i32 = 71;
	/// The plugin stops processing.
	pub const STOP_PROCESS: i32 = 72;
}

/// The calls the plugin makes on the host.
pub mod host_opcodes {
	/// The plugin changed a parameter. The parameter is the index, and the value is in
	/// `opt`.
	pub const AUTOMATE: i32 = 0;
	/// The version of the host's API, `2400` for VST 2.4.
	pub const VERSION: i32 = 1;
	/// The id of the plugin that's being created. This is how shell plugins know which
	/// of their plugins the host wants.
	pub const CURRENT_ID: i32 = 2;
	/// The plugin has nothing to do for a while.
	pub const IDLE: i32 = 3;
	/// The host's time information. The host writes the bits it wants to `value`.
	pub const GET_TIME: i32 = 7;
	/// The events the plugin produced. They're in `ptr`.
	pub const PROCESS_EVENTS: i32 = 8;
	/// The plugin's inputs or outputs changed.
	pub const IO_CHANGED: i32 = 13;
	/// The plugin wants to be called now and then.
	pub const NEED_IDLE: i32 = 14;
	/// The plugin wants its window to be a different size. The width is the index, and
	/// the height is the value.
	pub const SIZE_WINDOW: i32 = 15;
	/// The sample rate of the host.
	pub const GET_SAMPLE_RATE: i32 = 16;
	/// The block size of the host.
	pub const GET_BLOCK_SIZE: i32 = 17;
	/// The kind of processing the host is doing.
	pub const GET_CURRENT_PROCESS_LEVEL: i32 = 23;
	/// Whether the host is recording automation.
	pub const GET_AUTOMATION_STATE: i32 = 24;
	/// The name of the host.
	pub const GET_VENDOR_STRING: i32 = 32;
	/// The name of the host's product.
	pub const GET_PRODUCT_STRING: i32 = 33;
	/// The version of the host.
	pub const GET_VENDOR_VERSION: i32 = 34;
	/// Whether the host can do something. The text is in `ptr`.
	pub const CAN_DO: i32 = 37;
	/// The host's display needs to be updated.
	pub const UPDATE_DISPLAY: i32 = 42;
	/// The user started changing a parameter. The parameter is the index.
	pub const BEGIN_EDIT: i32 = 43;
	/// The user stopped changing a parameter. The parameter is the index.
	pub const END_EDIT: i32 = 44;
}

/// What kind of plugin a plugin is.
pub mod categories {
	/// The plugin doesn't say what it is.
	pub const UNKNOWN: i32 = 0;
	/// The plugin changes audio.
	pub const EFFECT: i32 = 1;
	/// The plugin makes sound out of notes.
	pub const SYNTH: i32 = 2;
	/// The plugin only looks at audio.
	pub const ANALYSIS: i32 = 3;
	/// The plugin is for mastering.
	pub const MASTERING: i32 = 4;
	/// The plugin puts audio in a space.
	pub const SPACIALIZER: i32 = 5;
	/// The plugin is a room simulation.
	pub const ROOM_FX: i32 = 6;
	/// The plugin is for surround sound.
	pub const SURROUND_FX: i32 = 7;
	/// The plugin repairs audio.
	pub const RESTORATION: i32 = 8;
	/// The plugin processes files instead of playing.
	pub const OFFLINE_PROCESS: i32 = 9;
	/// The plugin holds other plugins.
	pub const SHELL: i32 = 10;
	/// The plugin makes sound out of nothing.
	pub const GENERATOR: i32 = 11;
}

/// The flags of a parameter.
pub mod param_flags {
	/// The parameter is a switch, on or off.
	pub const IS_SWITCH: i32 = 1;
	/// The parameter has a whole number of steps.
	pub const USES_INTEGER_MIN_MAX: i32 = 1 << 1;
	/// The parameter has a step size.
	pub const USES_FLOAT_STEP: i32 = 1 << 2;
	/// The parameter has a whole number of steps.
	pub const USES_INT_STEP: i32 = 1 << 3;
}

/// The flags of a channel.
pub mod pin_flags {
	/// The channel is in use.
	pub const IS_ACTIVE: i32 = 1;
	/// The channel is the left one of a stereo pair.
	pub const IS_STEREO: i32 = 1 << 1;
}

/// The flags of the time information.
pub mod time_flags {
	/// The transport changed.
	pub const TRANSPORT_CHANGED: i32 = 1;
	/// The host is playing.
	pub const TRANSPORT_PLAYING: i32 = 1 << 1;
	/// The host is looping.
	pub const TRANSPORT_CYCLE_ACTIVE: i32 = 1 << 2;
	/// The host is recording.
	pub const TRANSPORT_RECORDING: i32 = 1 << 3;
	/// The host is recording automation.
	pub const AUTOMATION_WRITING: i32 = 1 << 6;
	/// The host is playing automation.
	pub const AUTOMATION_READING: i32 = 1 << 7;
	/// The time in nanoseconds is valid.
	pub const NANOSECONDS_VALID: i32 = 1 << 8;
	/// The position in beats is valid.
	pub const PPQ_POS_VALID: i32 = 1 << 9;
	/// The tempo is valid.
	pub const TEMPO_VALID: i32 = 1 << 10;
	/// The position of the bar is valid.
	pub const BARS_VALID: i32 = 1 << 11;
	/// The loop is valid.
	pub const CYCLE_POS_VALID: i32 = 1 << 12;
	/// The time signature is valid.
	pub const TIME_SIG_VALID: i32 = 1 << 13;
	/// The SMPTE position is valid.
	pub const SMPTE_VALID: i32 = 1 << 14;
}

/// The kind of event.
pub mod event_types {
	/// A MIDI event.
	pub const MIDI: i32 = 1;
	/// A system exclusive event.
	pub const SYSEX: i32 = 6;
}

/// The kind of processing the host is doing.
pub mod process_levels {
	/// The host doesn't say.
	pub const UNKNOWN: i32 = 0;
	/// The host is playing in real time.
	pub const REALTIME: i32 = 2;
	/// The host is rendering faster than real time.
	pub const OFFLINE: i32 = 4;
}

/// The maximum length of a name.
pub const MAX_NAME_LEN: usize = 64;
/// The maximum length of a parameter's label.
pub const MAX_LABEL_LEN: usize = 64;
/// The maximum length of a parameter's short label.
pub const MAX_SHORT_LABEL_LEN: usize = 8;
/// The maximum length of the name of a category of parameters.
pub const MAX_CATEGORY_LABEL_LEN: usize = 24;
/// The maximum length of the name of a program.
pub const MAX_PROGRAM_NAME_LEN: usize = 24;
/// The maximum length of a parameter's value.
pub const MAX_PARAM_STR_LEN: usize = 32;

/// The function the plugin uses to call the host.
pub type HostCallback =
	extern "C" fn(effect: *mut AEffect, opcode: i32, index: i32, value: isize, ptr: *mut c_void, opt: f32)
	-> isize;

/// The function the host uses to call the plugin.
pub type Dispatcher = unsafe extern "C" fn(
	effect: *mut AEffect,
	opcode: i32,
	index: i32,
	value: isize,
	ptr: *mut c_void,
	opt: f32,
) -> isize;

/// The function the host uses to have the plugin process audio.
pub type ProcessProc = unsafe extern "C" fn(
	effect: *mut AEffect,
	inputs: *const *const f32,
	outputs: *mut *mut f32,
	frames: i32,
);

/// The function the host uses to have the plugin process audio, in double precision.
pub type ProcessProcF64 = unsafe extern "C" fn(
	effect: *mut AEffect,
	inputs: *const *const f64,
	outputs: *mut *mut f64,
	frames: i32,
);

/// The function the host uses to set a parameter.
pub type SetParameterProc = unsafe extern "C" fn(effect: *mut AEffect, index: i32, value: f32);

/// The function the host uses to read a parameter.
pub type GetParameterProc = unsafe extern "C" fn(effect: *mut AEffect, index: i32) -> f32;

/// The plugin's main struct.
///
/// The plugin fills this in, and the host uses it to call the plugin.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct AEffect {
	/// The magic number, which is [`MAGIC`].
	pub magic: i32,
	/// The function the host calls to ask the plugin to do something.
	pub dispatcher: Dispatcher,
	/// The old way of processing audio, which is only used by plugins that can't do it
	/// any other way.
	pub process: ProcessProc,
	/// Sets a parameter.
	pub set_parameter: SetParameterProc,
	/// Reads a parameter.
	pub get_parameter: GetParameterProc,
	/// The number of programs (presets) the plugin has.
	pub num_programs: i32,
	/// The number of parameters the plugin has.
	pub num_params: i32,
	/// The number of audio inputs the plugin has.
	pub num_inputs: i32,
	/// The number of audio outputs the plugin has.
	pub num_outputs: i32,
	/// The flags of the plugin, see [`flags`].
	pub flags: i32,
	/// For the host to use.
	pub reserved1: *mut c_void,
	/// For the host to use.
	pub reserved2: *mut c_void,
	/// The number of samples the plugin delays its output by.
	pub initial_delay: i32,
	/// Deprecated, and unused.
	pub real_qualities: i32,
	/// Deprecated, and unused.
	pub off_qualities: i32,
	/// Deprecated, and unused.
	pub io_ratio: f32,
	/// The plugin's own data.
	pub object: *mut c_void,
	/// The plugin's own data.
	pub user: *mut c_void,
	/// The id of the plugin, which tells plugins apart.
	pub unique_id: i32,
	/// The version of the plugin, e.g. `1100` for version 1.1.0.
	pub version: i32,
	/// Processes audio, replacing the input with the output.
	pub process_replacing: ProcessProc,
	/// Processes audio, in double precision.
	pub process_replacing_f64: ProcessProcF64,
}

/// A MIDI event.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct MidiEvent {
	/// The kind of event, which is [`event_types::MIDI`].
	pub event_type: i32,
	/// The size of this struct.
	pub byte_size: i32,
	/// The number of frames into the block that the event happens at.
	pub delta_frames: i32,
	/// The flags of the event.
	pub flags: i32,
	/// The length of the note in frames, if the plugin knows it.
	pub note_length: i32,
	/// The offset of the note in frames, if the plugin knows it.
	pub note_offset: i32,
	/// The bytes of the MIDI message.
	pub midi_data: [u8; 4],
	/// How far the note is off the pitch, in cents.
	pub detune: i8,
	/// The velocity of the note off.
	pub note_off_velocity: u8,
	/// Reserved, and zero.
	pub reserved1: u8,
	/// Reserved, and zero.
	pub reserved2: u8,
}

/// An event, of any kind.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[expect(clippy::struct_field_names, reason = "the names are the ones the API uses")]
pub struct Event {
	/// The kind of event, see [`event_types`].
	pub event_type: i32,
	/// The size of the event.
	pub byte_size: i32,
	/// The number of frames into the block that the event happens at.
	pub delta_frames: i32,
	/// The flags of the event.
	pub flags: i32,
	/// The event itself, which is a [`MidiEvent`] for MIDI events.
	pub data: [u8; 16],
}

/// The events of a block.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
#[expect(clippy::struct_field_names, reason = "the names are the ones the API uses")]
pub struct Events {
	/// The number of events.
	pub num_events: i32,
	/// Reserved, and zero.
	pub reserved: isize,
	/// The events. There are more than two of them if the plugin has more than two, in
	/// which case the plugin's own struct is bigger than this one.
	pub events: [*mut Event; 2],
}

/// A rectangle, in pixels.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct Rect {
	pub left: i16,
	pub top: i16,
	pub right: i16,
	pub bottom: i16,
}

impl Rect {
	/// The width of the rectangle.
	#[must_use]
	pub const fn width(self) -> i32 {
		self.right as i32 - self.left as i32
	}

	/// The height of the rectangle.
	#[must_use]
	pub const fn height(self) -> i32 {
		self.bottom as i32 - self.top as i32
	}
}

/// Everything there is to know about a parameter.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ParameterProperties {
	/// The size of one step.
	pub step_float: f32,
	/// The size of a small step.
	pub small_step_float: f32,
	/// The size of a large step.
	pub large_step_float: f32,
	/// The unit of the parameter.
	pub label: [c_char; MAX_LABEL_LEN],
	/// The flags of the parameter, see [`param_flags`].
	pub flags: i32,
	/// The smallest whole number the parameter can have.
	pub min_integer: i32,
	/// The biggest whole number the parameter can have.
	pub max_integer: i32,
	/// The size of a step, in whole numbers.
	pub step_integer: i32,
	/// The size of a large step, in whole numbers.
	pub large_step_integer: i32,
	/// A short name for the parameter.
	pub short_label: [c_char; MAX_SHORT_LABEL_LEN],
	/// Where the parameter is shown, if the plugin says.
	pub display_index: i16,
	/// The category of the parameter, if the plugin says.
	pub category: i16,
	/// How many parameters are in the category.
	pub num_parameters_in_category: i16,
	/// Reserved, and zero.
	pub reserved: i16,
	/// The name of the category.
	pub category_label: [c_char; MAX_CATEGORY_LABEL_LEN],
	/// Reserved, and zero.
	pub future: [u8; 16],
}

/// Everything there is to know about a channel.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct PinProperties {
	/// The name of the channel.
	pub label: [c_char; MAX_LABEL_LEN],
	/// The flags of the channel, see [`pin_flags`].
	pub flags: i32,
	/// The kind of speakers the channel is part of.
	pub arrangement_type: i32,
	/// A short name for the channel.
	pub short_label: [c_char; MAX_SHORT_LABEL_LEN],
	/// Reserved, and zero.
	pub future: [u8; 48],
}

/// The time information the host gives the plugin.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct TimeInfo {
	/// The position of the first frame of the block, in samples.
	pub sample_pos: f64,
	/// The sample rate of the host.
	pub sample_rate: f64,
	/// The time of the host, in nanoseconds.
	pub nano_seconds: f64,
	/// The position of the first frame of the block, in beats.
	pub ppq_pos: f64,
	/// The tempo of the host, in beats per minute.
	pub tempo: f64,
	/// The position of the start of the bar, in beats.
	pub bar_start_pos: f64,
	/// The start of the loop, in beats.
	pub cycle_start_pos: f64,
	/// The end of the loop, in beats.
	pub cycle_end_pos: f64,
	/// The number of beats in a bar.
	pub time_sig_numerator: i32,
	/// The number of quarter notes in a whole note.
	pub time_sig_denominator: i32,
	/// The SMPTE offset, in frames.
	pub smpte_offset: i32,
	/// The SMPTE frame rate.
	pub smpte_frame_rate: i32,
	/// The number of samples until the next MIDI clock.
	pub samples_to_next_clock: i32,
	/// The flags of the time information, see [`time_flags`].
	pub flags: i32,
}
