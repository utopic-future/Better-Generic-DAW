use crate::{
	api, audio_buffers::AudioBuffers, events::Event, plugin::Shared, transport::Transport,
};
use std::{
	mem::size_of,
	sync::{
		Arc,
		atomic::{AtomicBool, Ordering::Relaxed},
	},
};

/// How many events can be waiting for the next block.
const PENDING_EVENTS: usize = 1024;
/// How many events can be handed to the plugin in one block.
const BLOCK_EVENTS: usize = 1024;

/// How a plugin is set up for processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioConfig {
	pub sample_rate: f64,
	/// The largest block that will be processed at once. Longer blocks are split up.
	pub max_frames: u32,
	/// Whether we're rendering faster than real time, plugins may then use more
	/// expensive algorithms.
	pub offline: bool,
}

#[derive(Clone, Copy)]
struct Pending {
	event: Event,
	/// Arrival order, so that events at the same time keep their order.
	seq: u32,
}

/// The events of a block, in the layout the plugin expects: the header, then a pointer
/// for every event.
struct EventBuffers {
	/// The events themselves.
	events: Vec<api::MidiEvent>,
	/// The header and the pointers to the events, in one allocation, so that they are
	/// next to each other the way the plugin expects.
	memory: Box<[usize]>,
}

impl EventBuffers {
	fn new() -> Self {
		Self {
			events: Vec::with_capacity(BLOCK_EVENTS),
			// the header is four words: the number of events, a reserved word, and two
			// pointers
			memory: vec![0; 4 + BLOCK_EVENTS].into_boxed_slice(),
		}
	}

	fn clear(&mut self) {
		self.events.clear();
		self.memory.fill(0);
	}

	fn push(&mut self, event: api::MidiEvent) {
		self.events.push(event);
	}

	/// Writes the pointers to the events, and returns the header the plugin reads.
	fn finish(&mut self) -> *mut api::Events {
		for (index, event) in self.events.iter_mut().enumerate() {
			self.memory[2 + index] = std::ptr::from_mut(event) as usize;
		}

		self.memory[0] = self.events.len();

		self.memory.as_mut_ptr().cast()
	}
}

/// The audio-thread side of an active plugin.
///
/// Created by [`Plugin::activate`](crate::Plugin::activate), and given back to
/// [`Plugin::deactivate`](crate::Plugin::deactivate) to deactivate the plugin.
pub struct AudioThread {
	shared: Arc<Shared>,
	config: AudioConfig,
	buffers: AudioBuffers,
	pending: Vec<Pending>,
	seq: u32,
	events: EventBuffers,
	output_events: Vec<Event>,
	dropped_events: u64,
	/// Set to `false` when the plugin is deactivated or dropped, which makes this inert.
	alive: Arc<AtomicBool>,
}

impl std::fmt::Debug for AudioThread {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AudioThread")
			.field("config", &self.config)
			.finish_non_exhaustive()
	}
}

#[expect(
	clippy::non_send_fields_in_send_ty,
	reason = "the buffers are only used through `&mut self`, see below"
)]
// SAFETY: the buffers are only used by the thread that has the object, and the plugin
// is only used by that thread while it processes.
unsafe impl Send for AudioThread {}

impl AudioThread {
	pub(crate) fn new(
		shared: Arc<Shared>,
		config: AudioConfig,
		inputs: usize,
		outputs: usize,
		latency: usize,
	) -> Self {
		Self {
			shared,
			config,
			buffers: AudioBuffers::new(inputs, outputs, config.max_frames as usize, latency),
			pending: Vec::with_capacity(PENDING_EVENTS),
			seq: 0,
			events: EventBuffers::new(),
			output_events: Vec::new(),
			dropped_events: 0,
			alive: Arc::new(AtomicBool::new(true)),
		}
	}

	/// How the plugin is set up for processing.
	#[must_use]
	pub fn config(&self) -> AudioConfig {
		self.config
	}

	/// How many samples the plugin delays its output by.
	#[must_use]
	pub fn latency(&self) -> usize {
		self.buffers.latency()
	}

	/// Queues an event for the next call to [`process`](Self::process).
	///
	/// Events are inserted in sorted order (by time, then sequence) to avoid O(n log n)
	/// sort. An event whose time is past the end of the block happens in the last frame
	/// instead. Returns `false`, and drops the event, if too many events are waiting.
	pub fn push_event(&mut self, event: Event) -> bool {
		if self.pending.len() >= PENDING_EVENTS {
			self.dropped_events += 1;
			return false;
		}

		let new_seq = self.seq;
		self.seq = self.seq.wrapping_add(1);

		// Insert in sorted order (insertion sort) - real-time safe
		// We maintain the invariant that self.pending is always sorted
		let insert_idx = self
			.pending
			.iter()
			.position(|p| (p.event.time(), p.seq) > (event.time(), new_seq))
			.unwrap_or(self.pending.len());

		self.pending.insert(
			insert_idx,
			Pending {
				event,
				seq: new_seq,
			},
		);
		true
	}

	/// Takes the events the plugin produced in the last call to [`process`](Self::process):
	/// the notes it played, and the parameters it changed on its own.
	///
	/// Events that aren't taken are discarded by the next call to [`process`](Self::process).
	pub fn drain_output_events(&mut self) -> impl Iterator<Item = Event> + '_ {
		self.output_events.drain(..)
	}

	/// How many events were dropped because there was no room for them.
	#[must_use]
	pub const fn dropped_events(&self) -> u64 {
		self.dropped_events
	}

	/// Processes `audio` in place: it's the dry input, and gets replaced with the mix of
	/// the dry signal and the plugin's output.
	///
	/// `mix_level` is `0.0` for only the dry signal, and `1.0` for only the plugin's
	/// output.
	pub fn process(
		&mut self,
		audio: &mut [[f32; 2]],
		transport: Option<&Transport>,
		mix_level: f32,
	) {
		self.output_events.clear();

		if audio.is_empty() {
			return;
		}

		if !self.alive.load(Relaxed) || !self.shared.state.is_alive() {
			self.buffers.flush(audio, mix_level);
			return;
		}

		let frames = audio.len();

		self.buffers.read_in(audio);

		self.update_time_info(transport, frames);
		self.apply_parameters();
		self.send_events(frames);

		let (inputs, outputs) = self.buffers.prepare(frames);

		// SAFETY: the effect is a valid plugin.
		let process = unsafe { (*self.shared.effect).process_replacing };

		// SAFETY: the effect is a valid plugin, and the buffers are valid for the
		// duration of the call.
		unsafe {
			process(self.shared.effect, inputs, outputs, frames as i32);
		}

		self.buffers.write_out(audio, mix_level);

		self.collect_outputs();

		self.pending.clear();
		self.seq = 0;
	}

	/// Tells the plugin where the timeline is.
	fn update_time_info(&self, transport: Option<&Transport>, frames: usize) {
		let sample_rate = self.config.sample_rate;

		let time_info = transport.map_or_else(
			|| api::TimeInfo {
				sample_rate,
				..api::TimeInfo::default()
			},
			|transport| transport.to_time_info(sample_rate, 0),
		);

		let _ = frames;

		self.shared.state.set_time_info(time_info);
	}

	/// Sets the parameters the host changed, which the plugin has no events for.
	fn apply_parameters(&mut self) {
		// SAFETY: the effect is a valid plugin.
		let set_parameter = unsafe { (*self.shared.effect).set_parameter };

		for pending in &self.pending {
			let Event::ParamValue { id, value, .. } = pending.event else {
				continue;
			};

			// SAFETY: the effect is a valid plugin, and the parameter is one of its own.
			unsafe {
				set_parameter(self.shared.effect, id as i32, value);
			}
		}
	}

	/// Hands the notes of this block to the plugin.
	fn send_events(&mut self, frames: usize) {
		self.events.clear();

		for pending in &self.pending {
			let Some(midi) = pending.event.to_midi() else {
				continue;
			};

			if self.events.events.len() >= BLOCK_EVENTS {
				self.dropped_events += 1;
				continue;
			}

			let time = pending.event.time().min(frames.saturating_sub(1) as u32);

			self.events.push(api::MidiEvent {
				event_type: api::event_types::MIDI,
				byte_size: size_of::<api::MidiEvent>() as i32,
				delta_frames: time as i32,
				flags: 0,
				note_length: 0,
				note_offset: 0,
				midi_data: [midi[0], midi[1], midi[2], 0],
				detune: 0,
				note_off_velocity: 0,
				reserved1: 0,
				reserved2: 0,
			});
		}

		if self.events.events.is_empty() {
			return;
		}

		let events = self.events.finish();

		// SAFETY: the effect is a valid plugin.
		let dispatcher = unsafe { (*self.shared.effect).dispatcher };

		// SAFETY: the effect is a valid plugin, and the events are valid for the
		// duration of the call.
		unsafe {
			dispatcher(
				self.shared.effect,
				api::opcodes::PROCESS_EVENTS,
				0,
				0,
				events.cast(),
				0.0,
			);
		}
	}

	/// Takes what the plugin told the host while it was processing.
	fn collect_outputs(&mut self) {
		for (index, value) in self.shared.state.take_param_updates() {
			self.output_events.push(Event::ParamValue {
				time: 0,
				id: index,
				value,
			});
		}

		for event in self.shared.state.take_output_events() {
			self.output_events.push(event);
		}
	}

	/// Clears the plugin's internal state, like delay lines and filters, and the latency
	/// compensation, as if processing started fresh. Call this when playback jumps.
	pub fn reset(&mut self) {
		self.buffers.reset();
	}

	/// Stops the plugin from processing any more.
	pub(crate) fn stop(&self) {
		self.alive.store(false, Relaxed);
	}
}

impl Drop for AudioThread {
	fn drop(&mut self) {
		self.alive.store(false, Relaxed);
	}
}
