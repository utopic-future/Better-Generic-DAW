//! A tiny VST2 plugin used only as a test fixture.
//!
//! It has one parameter, which makes the signal quieter by the amount that's set, and it
//! can save and load its state.

use std::{
	ffi::c_void,
	os::raw::c_char,
	ptr::null_mut,
};
use vst2_host::api::{self, AEffect, HostCallback};

/// The plugin's own data.
struct Plugin {
	/// The value of the gain parameter.
	value: f32,
	/// The state of the plugin, which is the value of the parameter.
	state: [u8; 4],
}

impl Plugin {
	fn new() -> Box<Self> {
		Box::new(Self {
			value: 1.0,
			state: [0; 4],
		})
	}
}

/// The plugin of the effect.
unsafe fn plugin(effect: *mut AEffect) -> &'static mut Plugin {
	// SAFETY: the plugin's data is ours, and it lives as long as the effect.
	unsafe { &mut *(*effect).object.cast::<Plugin>() }
}

/// Copies a string into a buffer that the host gave the plugin.
fn copy_string(ptr: *mut c_void, string: &str, len: usize) {
	if ptr.is_null() {
		return;
	}

	let bytes = string.as_bytes();
	let len = len.min(bytes.len());

	// SAFETY: the host gave us a buffer of at least `len` bytes.
	unsafe {
		std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), ptr.cast::<c_char>(), len);

		if len < 64 {
			ptr.cast::<c_char>().add(len).write(0);
		}
	}
}

/// Sets a parameter.
unsafe extern "C" fn set_parameter(effect: *mut AEffect, index: i32, value: f32) {
	if index == 0 {
		// SAFETY: the effect is the plugin's own.
		unsafe {
			plugin(effect).value = value;
		}
	}
}

/// Reads a parameter.
unsafe extern "C" fn get_parameter(effect: *mut AEffect, index: i32) -> f32 {
	if index == 0 {
		// SAFETY: the effect is the plugin's own.
		unsafe { plugin(effect).value }
	} else {
		0.0
	}
}

/// The old way of processing audio, which the plugin doesn't use.
unsafe extern "C" fn process_deprecated(
	_effect: *mut AEffect,
	inputs: *const *const f32,
	outputs: *mut *mut f32,
	frames: i32,
) {
	// SAFETY: the host gave us the buffers for this block.
	unsafe {
		let input = *inputs.add(0);
		let output = *outputs.add(0);

		for index in 0..frames as usize {
			*output.add(index) = *input.add(index);
		}
	}
}

/// Processes audio, making it quieter by the value of the parameter.
unsafe extern "C" fn process_replacing(
	effect: *mut AEffect,
	inputs: *const *const f32,
	outputs: *mut *mut f32,
	frames: i32,
) {
	// SAFETY: the effect is the plugin's own.
	let gain = unsafe { plugin(effect).value };

	// SAFETY: the host gave us the buffers for this block.
	unsafe {
		let input = *inputs.add(0);
		let output = *outputs.add(0);

		for index in 0..frames as usize {
			*output.add(index) = *input.add(index) * gain;
		}
	}
}

/// Processes audio, in double precision.
unsafe extern "C" fn process_replacing_f64(
	effect: *mut AEffect,
	inputs: *const *const f64,
	outputs: *mut *mut f64,
	frames: i32,
) {
	// SAFETY: the effect is the plugin's own.
	let gain = f64::from(unsafe { plugin(effect).value });

	// SAFETY: the host gave us the buffers for this block.
	unsafe {
		let input = *inputs.add(0);
		let output = *outputs.add(0);

		for index in 0..frames as usize {
			*output.add(index) = *input.add(index) * gain;
		}
	}
}

/// Does what the host asks of the plugin.
unsafe extern "C" fn dispatcher(
	effect: *mut AEffect,
	opcode: i32,
	_index: i32,
	_value: isize,
	ptr: *mut c_void,
	_opt: f32,
) -> isize {
	match opcode {
		api::opcodes::OPEN
		| api::opcodes::CLOSE
		| api::opcodes::SET_SAMPLE_RATE
		| api::opcodes::SET_BLOCK_SIZE
		| api::opcodes::MAINS_CHANGED
		| api::opcodes::START_PROCESS
		| api::opcodes::STOP_PROCESS
		| api::opcodes::PROCESS_EVENTS
		| api::opcodes::CAN_BE_AUTOMATED => 1,
		api::opcodes::GET_PARAM_NAME => {
			copy_string(ptr, "Gain", api::MAX_PARAM_STR_LEN);
			1
		}
		api::opcodes::GET_PARAM_DISPLAY => {
			// SAFETY: the effect is the plugin's own.
			let value = unsafe { plugin(effect).value };

			copy_string(ptr, &format!("{value:.2}"), api::MAX_PARAM_STR_LEN);
			1
		}
		api::opcodes::GET_PARAM_LABEL => {
			copy_string(ptr, "", api::MAX_PARAM_STR_LEN);
			1
		}
		api::opcodes::GET_PARAMETER_PROPERTIES => {
			// SAFETY: the host gave us a buffer for the properties.
			unsafe {
				let properties = &mut *ptr.cast::<api::ParameterProperties>();

				properties.step_float = 0.01;
				properties.small_step_float = 0.001;
				properties.large_step_float = 0.1;
			}

			1
		}
		api::opcodes::GET_PRODUCT_STRING | api::opcodes::GET_EFFECT_NAME => {
			copy_string(ptr, "Test Gain", api::MAX_NAME_LEN);
			1
		}
		api::opcodes::GET_VENDOR_STRING => {
			copy_string(ptr, "Generic DAW", api::MAX_NAME_LEN);
			1
		}
		api::opcodes::GET_PROGRAM_NAME => {
			copy_string(ptr, "Default", api::MAX_PROGRAM_NAME_LEN);
			1
		}
		api::opcodes::GET_VENDOR_VERSION => 1000,
		api::opcodes::GET_PLUG_CATEGORY => api::categories::EFFECT as isize,
		api::opcodes::GET_PROGRAM => 0,
		api::opcodes::GET_CHUNK => {
			// SAFETY: the effect is the plugin's own.
			let state = unsafe { &raw const plugin(effect).state };

			// SAFETY: the host gave us a pointer to write the data to.
			unsafe {
				*ptr.cast::<*const u8>() = state.cast::<u8>();
			}

			4
		}
		api::opcodes::SET_CHUNK => {
			// SAFETY: the host gave us the state to restore.
			unsafe {
				let plugin = plugin(effect);

				std::ptr::copy_nonoverlapping(ptr.cast::<u8>(), plugin.state.as_mut_ptr(), 4);

				plugin.value = f32::from_le_bytes(plugin.state);
			}

			1
		}
		_ => 0,
	}
}

/// The entry point of the plugin.
#[unsafe(no_mangle)]
pub extern "C" fn VSTPluginMain(_callback: HostCallback) -> *mut AEffect {
	let plugin = Plugin::new();

	let effect = Box::leak(Box::new(AEffect {
		magic: api::MAGIC,
		dispatcher,
		process: process_deprecated,
		set_parameter: set_parameter,
		get_parameter: get_parameter,
		num_programs: 1,
		num_params: 1,
		num_inputs: 1,
		num_outputs: 1,
		flags: api::flags::CAN_REPLACING,
		reserved1: null_mut(),
		reserved2: null_mut(),
		initial_delay: 0,
		real_qualities: 0,
		off_qualities: 0,
		io_ratio: 0.0,
		object: Box::into_raw(plugin).cast(),
		user: null_mut(),
		unique_id: 0x1234,
		version: 1000,
		process_replacing,
		process_replacing_f64,
	}));

	effect
}
