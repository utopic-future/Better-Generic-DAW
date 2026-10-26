use crate::{
	AudioLayout, Event, Module, RestartFlags, Transport, audio_buffers::AudioBuffers,
	event_list::EventList, param_changes::ParameterChanges,
};
use std::{
	ptr::null_mut,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicI32, Ordering::Relaxed},
	},
};
use vst3::{
	ComPtr, ComWrapper,
	Steinberg::{
		Vst::{
			IAudioProcessor, IAudioProcessorTrait as _, IEventList, IParameterChanges, ProcessData,
			ProcessModes_, SymbolicSampleSizes_,
		},
		kNotImplemented, kResultOk,
	},
};

/// How many events can be waiting for the next block.
const PENDING_EVENTS: usize = 4096;
/// How many events can be handed to the plugin in one block.
const EVENT_LIST_CAPACITY: usize = 4096;
/// How many events from the plugin can be collected per call to `process`.
const OUTPUT_EVENTS: usize = 1024;
const INPUT_PARAM_QUEUES: usize = 64;
const INPUT_PARAM_POINTS: usize = 128;
const OUTPUT_PARAM_QUEUES: usize = 32;
const OUTPUT_PARAM_POINTS: usize = 64;

#[derive(Clone, Copy)]
struct Pending {
	event: Event,
	/// Arrival order, so that events at the same time keep their order.
	seq: u32,
}

/// The tail length a plugin reports if it never stops ringing.
pub const INFINITE_TAIL: u32 = 0xFFFF_FFFF;

/// How a plugin is set up for processing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioConfig {
	pub sample_rate: f64,
	/// The largest block that will be processed at once. Longer blocks are split up.
	pub max_frames: u32,
	/// Whether we're rendering faster than real time, plugins may then use more expensive
	/// algorithms.
	pub offline: bool,
}

/// What happened to a block.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ProcessStatus {
	Processed,
	/// The plugin refused or failed to process the block. Only the dry signal was kept.
	Failed,
	/// The plugin was deactivated or dropped. Only the dry signal was kept.
	Inactive,
}

/// The audio-thread side of an active plugin.
///
/// Created by [`Plugin::activate`](crate::Plugin::activate), and given back to
/// [`Plugin::deactivate`](crate::Plugin::deactivate) to deactivate the plugin.
pub struct AudioThread {
	processor: ComPtr<IAudioProcessor>,
	buffers: AudioBuffers,
	config: AudioConfig,
	layout: AudioLayout,
	processing: bool,
	/// The number of frames processed since the plugin was activated or reset.
	steady_time: i64,
	events_in: ComWrapper<EventList>,
	events_out: ComWrapper<EventList>,
	params_in: ComWrapper<ParameterChanges>,
	params_out: ComWrapper<ParameterChanges>,
	pending: Vec<Pending>,
	seq: u32,
	output_events: Vec<Event>,
	dropped_events: u64,
	/// Cleared when the plugin is deactivated or dropped, which makes this inert.
	alive: Arc<AtomicBool>,
	restart_flags: Arc<AtomicI32>,
	/// Keeps the plugin's code loaded for as long as the processor can be called.
	_module: Arc<Module>,
}

impl std::fmt::Debug for AudioThread {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("AudioThread")
			.field("config", &self.config)
			.field("layout", &self.layout)
			.field("processing", &self.processing)
			.finish_non_exhaustive()
	}
}

#[expect(
	clippy::non_send_fields_in_send_ty,
	reason = "the COM pointer is only used through `&mut self`, see below"
)]
// SAFETY:
// The VST3 spec allows `IAudioProcessor` to be used from a thread other than the one it was
// created on, as long as only one thread uses it at a time. `&mut self` guarantees that.
unsafe impl Send for AudioThread {}

impl AudioThread {
	pub(crate) fn new(
		processor: ComPtr<IAudioProcessor>,
		module: Arc<Module>,
		alive: Arc<AtomicBool>,
		restart_flags: Arc<AtomicI32>,
		config: AudioConfig,
		layout: AudioLayout,
		latency: u32,
	) -> Self {
		Self {
			processor,
			buffers: AudioBuffers::new(layout, config.max_frames as usize, latency as usize),
			config,
			layout,
			processing: false,
			steady_time: 0,
			events_in: ComWrapper::new(EventList::new(EVENT_LIST_CAPACITY)),
			events_out: ComWrapper::new(EventList::new(EVENT_LIST_CAPACITY)),
			params_in: ComWrapper::new(ParameterChanges::new(
				INPUT_PARAM_QUEUES,
				INPUT_PARAM_POINTS,
			)),
			params_out: ComWrapper::new(ParameterChanges::new(
				OUTPUT_PARAM_QUEUES,
				OUTPUT_PARAM_POINTS,
			)),
			pending: Vec::with_capacity(PENDING_EVENTS),
			seq: 0,
			output_events: Vec::with_capacity(OUTPUT_EVENTS),
			dropped_events: 0,
			alive,
			restart_flags,
			_module: module,
		}
	}

	#[must_use]
	pub fn config(&self) -> AudioConfig {
		self.config
	}

	#[must_use]
	pub fn layout(&self) -> AudioLayout {
		self.layout
	}

	/// How many samples the plugin delays its output by.
	#[must_use]
	pub fn latency(&self) -> usize {
		self.buffers.latency()
	}

	/// Asks the plugin for its latency again, and returns the new value. Call this after the
	/// plugin reports that its latency changed.
	pub fn update_latency(&mut self) -> usize {
		// SAFETY:
		// `processor` is a valid, active processor.
		let latency = unsafe { self.processor.getLatencySamples() };

		self.buffers.set_latency(latency as usize);
		latency as usize
	}

	/// How many samples the plugin keeps producing sound after its input goes silent, or
	/// [`INFINITE_TAIL`].
	#[must_use]
	pub fn tail_samples(&self) -> u32 {
		// SAFETY:
		// `processor` is a valid, active processor.
		unsafe { self.processor.getTailSamples() }
	}

	/// Queues an event for the next call to [`process`](Self::process).
	///
	/// Events are inserted in sorted order (by time, then sequence) to avoid O(n log n) sort.
	/// An event whose time is past the end of the block happens in the last frame instead.
	/// Returns `false`, and drops the event, if too many events are waiting.
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

	/// Takes the events the plugin produced in the last call to [`process`](Self::process): the
	/// notes it played, and the parameters it changed on its own.
	///
	/// Events that aren't taken are discarded by the next call to `process`.
	pub fn drain_output_events(&mut self) -> impl Iterator<Item = Event> + '_ {
		self.output_events.drain(..)
	}

	/// The restart requests the plugin made since they were last taken, without taking them.
	///
	/// This is how the audio thread finds out quickly. The main thread gets the same requests as
	/// [`MainThreadMessage::Restart`](crate::MainThreadMessage::Restart) and is the one that acts
	/// on them.
	#[must_use]
	pub fn restart_flags(&self) -> RestartFlags {
		RestartFlags(self.restart_flags.load(Relaxed))
	}

	/// Like [`restart_flags`](Self::restart_flags), but forgets them. Activating the plugin
	/// again also forgets them.
	#[must_use]
	pub fn take_restart_flags(&self) -> RestartFlags {
		RestartFlags(self.restart_flags.swap(0, Relaxed))
	}

	/// How many events were dropped because there was no room for them.
	#[must_use]
	pub const fn dropped_events(&self) -> u64 {
		self.dropped_events
	}

	/// Processes `audio` in place: it's the dry input, and gets replaced with the mix of the dry
	/// signal and the plugin's output.
	///
	/// `mix_level` is `0.0` for only the dry signal, and `1.0` for only the plugin's output.
	///
	/// Blocks longer than the configured maximum are processed in pieces. Events queued with
	/// [`push_event`](Self::push_event) are consumed, and the plugin's output events can be taken
	/// with [`drain_output_events`](Self::drain_output_events) afterwards.
	pub fn process(
		&mut self,
		audio: &mut [[f32; 2]],
		transport: Option<&Transport>,
		mix_level: f32,
	) -> ProcessStatus {
		self.output_events.clear();

		if audio.is_empty() {
			return ProcessStatus::Processed;
		}

		// Events are already sorted by push_event (insertion sort) - no allocation!
		let total = audio.len();
		let mut next_event = 0;
		let mut status = ProcessStatus::Processed;
		let mut offset = 0;

		for chunk in audio.chunks_mut(self.config.max_frames as usize) {
			let is_last = offset + chunk.len() == total;
			let transport = transport.map(|t| t.advanced(offset, self.config.sample_rate));

			next_event = self.load_inputs(next_event, offset, chunk.len(), is_last);

			let chunk_status = self.process_chunk(chunk, transport.as_ref(), mix_level, offset);
			if chunk_status != ProcessStatus::Processed {
				status = chunk_status;
			}

			offset += chunk.len();
		}

		self.pending.clear();
		self.seq = 0;

		status
	}

	/// Fills the plugin's input lists with the events that fall in `offset..offset + len`, and
	/// returns the index of the first event after those.
	fn load_inputs(&mut self, first: usize, offset: usize, len: usize, is_last: bool) -> usize {
		self.events_in.clear();
		self.params_in.clear();

		let mut next = first;
		let mut dropped = 0;

		while let Some(pending) = self.pending.get(next) {
			let time = pending.event.time() as usize;

			// everything that's left belongs in the last chunk, even if it's past the end
			if !is_last && time >= offset + len {
				break;
			}

			let local = time.saturating_sub(offset).min(len - 1) as i32;

			match pending.event {
				Event::ParamValue { id, value, .. } => {
					if !self.params_in.add_point(id, local, value) {
						dropped += 1;
					}
				}
				note => {
					// plugins without an event input have nowhere to put notes
					if self.layout.event_inputs > 0
						&& let Some(raw) = note.to_raw(local)
					{
						self.events_in.push(raw);
					}
				}
			}

			next += 1;
		}

		self.dropped_events += dropped;
		next
	}

	/// Moves what the plugin put in its output lists to `output_events`.
	fn collect_outputs(&mut self, offset: usize) {
		let output_events = &mut self.output_events;
		let mut dropped = 0;

		self.events_out.for_each(|raw| {
			let time = (offset as u32).saturating_add(raw.sampleOffset.max(0) as u32);

			if let Some(event) = Event::from_raw(raw, time) {
				if output_events.len() < OUTPUT_EVENTS {
					output_events.push(event);
				} else {
					dropped += 1;
				}
			}
		});

		self.params_out.for_each(|id, sample_offset, value| {
			if output_events.len() < OUTPUT_EVENTS {
				output_events.push(Event::ParamValue {
					time: (offset as u32).saturating_add(sample_offset.max(0) as u32),
					id,
					value,
				});
			} else {
				dropped += 1;
			}
		});

		self.dropped_events += dropped;
	}

	fn process_chunk(
		&mut self,
		audio: &mut [[f32; 2]],
		transport: Option<&Transport>,
		mix_level: f32,
		offset: usize,
	) -> ProcessStatus {
		if !self.alive.load(Relaxed) {
			self.buffers.flush(audio, mix_level);
			return ProcessStatus::Inactive;
		}

		if !self.processing && !self.start() {
			self.buffers.flush(audio, mix_level);
			return ProcessStatus::Failed;
		}

		let frames = audio.len();

		self.buffers.read_in(audio);
		let (inputs, outputs) = self.buffers.prepare(frames);

		self.events_out.clear();
		self.params_out.clear();

		let mut context =
			Transport::to_process_context(transport, self.config.sample_rate, self.steady_time);

		let event_ptr = |list: &ComWrapper<EventList>| {
			list.as_com_ref::<IEventList>()
				.map_or(null_mut(), |list| list.as_ptr())
		};
		let params_ptr = |list: &ComWrapper<ParameterChanges>| {
			list.as_com_ref::<IParameterChanges>()
				.map_or(null_mut(), |list| list.as_ptr())
		};

		let mut data = ProcessData {
			processMode: self.process_mode(),
			symbolicSampleSize: SymbolicSampleSizes_::kSample32 as i32,
			numSamples: frames as i32,
			numInputs: self.layout.input_buses as i32,
			numOutputs: self.layout.output_buses as i32,
			inputs,
			outputs,
			inputParameterChanges: params_ptr(&self.params_in),
			outputParameterChanges: params_ptr(&self.params_out),
			inputEvents: if self.layout.event_inputs > 0 {
				event_ptr(&self.events_in)
			} else {
				null_mut()
			},
			outputEvents: if self.layout.event_outputs > 0 {
				event_ptr(&self.events_out)
			} else {
				null_mut()
			},
			processContext: &raw mut context,
		};

		// SAFETY:
		// `data` points to buffers and lists that are valid for the duration of the call, and the
		// processor is active and processing.
		let result = unsafe { self.processor.process(&raw mut data) };

		self.steady_time += frames as i64;

		if result != kResultOk {
			self.buffers.flush(audio, mix_level);
			return ProcessStatus::Failed;
		}

		self.collect_outputs(offset);
		self.buffers.write_out(audio, mix_level);
		ProcessStatus::Processed
	}

	const fn process_mode(&self) -> i32 {
		if self.config.offline {
			ProcessModes_::kOffline as i32
		} else {
			ProcessModes_::kRealtime as i32
		}
	}

	fn start(&mut self) -> bool {
		// SAFETY:
		// `processor` is a valid, active processor.
		let result = unsafe { self.processor.setProcessing(1) };

		self.processing = result == kResultOk || result == kNotImplemented;
		self.processing
	}

	pub(crate) fn stop(&mut self) {
		if self.processing {
			self.processing = false;

			// SAFETY:
			// `processor` is a valid, active processor.
			unsafe {
				self.processor.setProcessing(0);
			}
		}
	}

	/// Clears the plugin's internal state, like delay lines and filters, and the latency
	/// compensation, as if processing started fresh. Call this when playback jumps.
	pub fn reset(&mut self) {
		// the plugin resets itself when processing is turned on again
		self.stop();
		self.buffers.reset();
		self.steady_time = 0;
	}
}

impl Drop for AudioThread {
	fn drop(&mut self) {
		if self.alive.load(Relaxed) {
			self.stop();
		}
	}
}
