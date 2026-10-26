use crate::{
	api::{self, AEffect, HostCallback, TimeInfo},
	events::Event,
};
use std::{
	cell::{Cell, UnsafeCell},
	ffi::c_void,
	os::raw::c_char,
	sync::{
		Mutex,
		atomic::{AtomicBool, AtomicI32, AtomicU32, AtomicU64, Ordering},
		mpsc::Sender,
	},
};

/// The name the host tells plugins about.
pub const HOST_NAME: &str = "Generic DAW";

/// The version of the host, as `0x0101`.
const HOST_VERSION: isize = 0x0101;

/// The version of the API the host speaks.
const HOST_API_VERSION: isize = 2400;

/// What the plugin tells the main thread.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MainThreadMessage {
	/// The plugin wants its window to be a different size.
	ResizeEditor { width: i32, height: i32 },
}

/// The id of the plugin that's being created.
///
/// Shell plugins hold more than one plugin, and ask the host which one they should
/// make with this.
static CURRENT_ID: AtomicI32 = AtomicI32::new(0);

/// Sets the id of the plugin that's being created.
pub(crate) fn set_current_id(id: i32) {
	CURRENT_ID.store(id, Ordering::Relaxed);
}

/// What the plugin tells the host while it's processing.
#[derive(Debug)]
pub struct HostState {
	/// The time information the plugin gets to see.
	///
	/// The audio thread writes it before every block, and the plugin reads it while it
	/// processes that block, so both happen on the same thread.
	time_info: UnsafeCell<TimeInfo>,
	/// The parameters the plugin changed on its own.
	param_updates: Mutex<Vec<(u32, f32)>>,
	/// The notes the plugin played.
	output_events: Mutex<Vec<Event>>,
	/// The sample rate of the host.
	sample_rate: AtomicU64,
	/// The block size of the host.
	block_size: AtomicU32,
	/// Whether the host is rendering faster than real time.
	offline: AtomicBool,
	/// Set to `false` when the plugin is deactivated, which makes it inert.
	alive: AtomicBool,
	/// Where the messages for the main thread go.
	sender: Mutex<Option<Sender<MainThreadMessage>>>,
}

// SAFETY: the state is only ever reached through the plugin's own main struct, and the
// plugin only calls back into the host while it's processing or while its editor is
// open. The parts that are written while the plugin is running are behind a lock, and
// the time information is only written by the thread that the plugin is running on.
unsafe impl Send for HostState {}
// SAFETY: see above.
unsafe impl Sync for HostState {}

impl HostState {
	/// Makes the state for a plugin that runs at the given sample rate and block size.
	#[must_use]
	pub fn new(
		sample_rate: f64,
		block_size: u32,
		offline: bool,
		sender: Sender<MainThreadMessage>,
	) -> Self {
		Self {
			time_info: UnsafeCell::new(TimeInfo {
				sample_rate,
				..TimeInfo::default()
			}),
			param_updates: Mutex::new(Vec::new()),
			output_events: Mutex::new(Vec::new()),
			sample_rate: AtomicU64::new(sample_rate.to_bits()),
			block_size: AtomicU32::new(block_size),
			offline: AtomicBool::new(offline),
			alive: AtomicBool::new(true),
			sender: Mutex::new(Some(sender)),
		}
	}

	/// Tells the state how the host is set up.
	pub fn set_config(&self, sample_rate: f64, block_size: u32, offline: bool) {
		self.sample_rate
			.store(sample_rate.to_bits(), Ordering::Relaxed);
		self.block_size.store(block_size, Ordering::Relaxed);
		self.offline.store(offline, Ordering::Relaxed);
		self.alive.store(true, Ordering::Relaxed);
	}

	/// The sample rate of the host.
	#[must_use]
	pub fn sample_rate(&self) -> f64 {
		f64::from_bits(self.sample_rate.load(Ordering::Relaxed))
	}

	/// The block size of the host.
	#[must_use]
	pub fn block_size(&self) -> u32 {
		self.block_size.load(Ordering::Relaxed)
	}

	/// Whether the host is rendering faster than real time.
	#[must_use]
	pub fn is_offline(&self) -> bool {
		self.offline.load(Ordering::Relaxed)
	}

	/// Sets the time information for the next block.
	pub fn set_time_info(&self, time_info: TimeInfo) {
		// SAFETY: the audio thread is the only one that writes this, and it does so
		// before it hands the block to the plugin.
		unsafe {
			*self.time_info.get() = time_info;
		}
	}

	/// The parameters the plugin changed on its own, and forgets them.
	pub fn take_param_updates(&self) -> Vec<(u32, f32)> {
		let mut updates = self
			.param_updates
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);

		std::mem::take(&mut *updates)
	}

	/// The notes the plugin played, and forgets them.
	pub fn take_output_events(&self) -> Vec<Event> {
		let mut events = self
			.output_events
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);

		std::mem::take(&mut *events)
	}

	/// Whether the plugin may still be used.
	#[must_use]
	pub fn is_alive(&self) -> bool {
		self.alive.load(Ordering::Relaxed)
	}

	/// Makes the plugin inert, so that a leaked audio thread doesn't call into it any
	/// more.
	pub fn kill(&self) {
		self.alive.store(false, Ordering::Relaxed);
	}

	fn automate(&self, index: i32, value: f32) {
		if index < 0 {
			return;
		}

		let mut updates = self
			.param_updates
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);

		updates.push((index as u32, value));
	}

	/// Tells the main thread that the plugin wants a different window size.
	fn resize_editor(&self, width: i32, height: i32) {
		let sender = self
			.sender
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);

		if let Some(sender) = sender.as_ref() {
			let _ = sender.send(MainThreadMessage::ResizeEditor { width, height });
		}
	}

	fn process_events(&self, events: *const api::Events) {
		if events.is_null() {
			return;
		}

		// SAFETY: the plugin gave us a pointer to its events, which are valid until it
		// returns from the call.
		let count = unsafe { (*events).num_events.max(0) as usize };

		// The pointers to the events are right after the header, and there can be more
		// of them than the two that fit in the header.
		// SAFETY: the header is followed by the pointers to the events.
		let pointers = unsafe { events.cast::<*mut api::Event>().add(2) };

		let mut output = self
			.output_events
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);

		for index in 0..count {
			// SAFETY: the plugin says how many events there are, and they are valid
			// until the plugin returns.
			let event = unsafe { pointers.add(index) };

			// SAFETY: the pointer is to an event that is valid until the plugin returns.
			let event = unsafe { *event };

			if event.is_null() {
				continue;
			}

			// SAFETY: the events point to events, which are valid until the plugin returns.
			let event = unsafe { &*event };

			if event.event_type != api::event_types::MIDI {
				continue;
			}

			// SAFETY: the event says that it's a MIDI event, so that's what it is.
			let midi = unsafe { &*std::ptr::from_ref(event).cast::<api::MidiEvent>() };

			let [status, data1, data2, _] = midi.midi_data;

			let event = match status & 0xf0 {
				0x90 if data2 != 0 => Event::NoteOn {
					time: midi.delta_frames.max(0) as u32,
					key: data1,
					velocity: f32::from(data2) / 127.0,
				},
				0x80 | 0x90 => Event::NoteOff {
					time: midi.delta_frames.max(0) as u32,
					key: data1,
					velocity: f32::from(data2) / 127.0,
				},
				_ => continue,
			};

			output.push(event);
		}
	}
}

/// Writes a string into a buffer that the plugin gave the host.
fn copy_string(ptr: *mut c_void, string: &str, len: usize) -> isize {
	if ptr.is_null() {
		return 0;
	}

	let bytes = string.as_bytes();
	let len = len.min(bytes.len());

	// SAFETY: the plugin gave us a buffer of at least `len` bytes.
	unsafe {
		std::ptr::copy_nonoverlapping(bytes.as_ptr().cast::<c_char>(), ptr.cast::<c_char>(), len);
	}

	if len < api::MAX_NAME_LEN {
		// SAFETY: the buffer has room for the string and the null after it.
		let end = unsafe { ptr.cast::<c_char>().add(len) };

		// SAFETY: the null is within the buffer.
		unsafe {
			end.write(0);
		}
	}

	1
}

/// Whether the host can do what the plugin asks for.
fn can_do(ptr: *mut c_void) -> isize {
	if ptr.is_null() {
		return 0;
	}

	// SAFETY: the plugin gave us a null-terminated string.
	let text = unsafe { std::ffi::CStr::from_ptr(ptr.cast::<c_char>()) };

	match text.to_bytes() {
		b"receiveVstMidiEvent" | b"sendVstMidiEvent" => 1,
		_ => 0,
	}
}

/// The calls the host answers even before it has a plugin.
fn general_callback(opcode: i32, _index: i32, _value: isize, ptr: *mut c_void, _opt: f32) -> isize {
	match opcode {
		api::host_opcodes::VERSION => HOST_API_VERSION,
		api::host_opcodes::CURRENT_ID => i64::from(CURRENT_ID.load(Ordering::Relaxed)) as isize,
		api::host_opcodes::GET_VENDOR_STRING | api::host_opcodes::GET_PRODUCT_STRING => {
			copy_string(ptr, HOST_NAME, api::MAX_NAME_LEN)
		}
		api::host_opcodes::GET_VENDOR_VERSION => HOST_VERSION,
		api::host_opcodes::CAN_DO => can_do(ptr),
		api::host_opcodes::GET_AUTOMATION_STATE => 1,
		_ => 0,
	}
}

thread_local! {
	/// The state of the plugin that's being made, if there is one.
	///
	/// A plugin can call the host while it's still being made, and then it has no main
	/// struct to point at yet.
	static CREATING: Cell<*mut HostState> = const { Cell::new(std::ptr::null_mut()) };
}

/// Says that a plugin is being made, and that the host's state is this one.
pub(crate) fn set_creating(state: *mut HostState) {
	CREATING.with(|creating| creating.set(state));
}

/// Says that no plugin is being made any more.
pub(crate) fn clear_creating() {
	CREATING.with(|creating| creating.set(std::ptr::null_mut()));
}

/// The function the plugin calls to talk to the host.
pub(crate) extern "C" fn host_callback(
	effect: *mut AEffect,
	opcode: i32,
	index: i32,
	value: isize,
	ptr: *mut c_void,
	opt: f32,
) -> isize {
	host_callback_inner(effect, opcode, index, value, ptr, opt)
}

/// What the plugin asks of the host.
fn host_callback_inner(
	effect: *mut AEffect,
	opcode: i32,
	index: i32,
	value: isize,
	ptr: *mut c_void,
	opt: f32,
) -> isize {
	let creating = CREATING.with(Cell::get);

	let state = if !creating.is_null() {
		// The plugin is still being made, so it has no main struct to point at yet.
		// SAFETY: the state is ours, and it's alive while the plugin is made.
		unsafe { creating.as_ref() }
	} else if effect.is_null() {
		None
	} else {
		// SAFETY: the plugin gives us its own main struct.
		let reserved = unsafe { (*effect).reserved2 };

		// SAFETY: the host's state is in `reserved2`, and it's ours.
		unsafe { reserved.cast::<HostState>().as_ref() }
	};

	let Some(state) = state else {
		return general_callback(opcode, index, value, ptr, opt);
	};

	match opcode {
		api::host_opcodes::AUTOMATE => state.automate(index, opt),
		api::host_opcodes::GET_TIME => {
			// The plugin reads the time information while it processes, which is the
			// same thread that wrote it.
			return state.time_info.get() as isize;
		}
		api::host_opcodes::GET_SAMPLE_RATE => return state.sample_rate() as isize,
		api::host_opcodes::GET_BLOCK_SIZE => {
			return isize::try_from(state.block_size()).unwrap_or(0);
		}
		api::host_opcodes::GET_CURRENT_PROCESS_LEVEL => {
			return if state.is_offline() {
				api::process_levels::OFFLINE as isize
			} else {
				api::process_levels::REALTIME as isize
			};
		}
		api::host_opcodes::PROCESS_EVENTS => state.process_events(ptr.cast_const().cast()),
		api::host_opcodes::VERSION => return HOST_API_VERSION,
		api::host_opcodes::CURRENT_ID => {
			return i64::from(CURRENT_ID.load(Ordering::Relaxed)) as isize;
		}
		api::host_opcodes::GET_VENDOR_STRING | api::host_opcodes::GET_PRODUCT_STRING => {
			return copy_string(ptr, HOST_NAME, api::MAX_NAME_LEN);
		}
		api::host_opcodes::GET_VENDOR_VERSION => return HOST_VERSION,
		api::host_opcodes::CAN_DO => return can_do(ptr),
		api::host_opcodes::BEGIN_EDIT
		| api::host_opcodes::END_EDIT
		| api::host_opcodes::UPDATE_DISPLAY => {}
		api::host_opcodes::SIZE_WINDOW => {
			state.resize_editor(index, value as i32);
			return 1;
		}
		_ => return general_callback(opcode, index, value, ptr, opt),
	}

	0
}

/// The function a plugin gets to call the host with.
pub(crate) const HOST_CALLBACK: HostCallback = host_callback;
