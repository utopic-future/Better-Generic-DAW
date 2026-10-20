use crate::{
	Event, Node, PushSlot, Transport,
	audio_thread::{Inject, State},
};
use audio_graph::Injector;
use clap_host::{AudioThread as ClapAudioThread, ClapId, RenderMode, events::EventFlags};
use log::warn;
use rtrb::{Consumer, Producer, RingBuffer};
use std::collections::HashMap;
use vst3_host::{AudioThread as Vst3AudioThread, RestartFlags};

#[derive(Debug)]
pub enum AudioThreadMessage {
	Restart(AudioProcessor),
	Deactivate(AudioProcessor),
	Destroy(AudioProcessor),
}

/// The audio thread of an active plugin, whichever format the plugin is.
#[derive(Debug)]
pub enum AudioBackend {
	Clap(ClapAudioThread),
	Vst3(Vst3AudioThread),
}

#[derive(Debug)]
pub struct AudioProcessor {
	backend: AudioBackend,
	/// Parameter values the plugin changed itself, waiting to be sent to the UI. For VST3 plugins
	/// these are normalized, so `0.0..=1.0`.
	updates: HashMap<ClapId, f32>,
	producer: Producer<(ClapId, f32)>,
}

impl AudioProcessor {
	#[must_use]
	pub fn create(clap: ClapAudioThread) -> (Self, Consumer<(ClapId, f32)>) {
		let param_count = clap.param_count();
		let sample_rate = clap.config().sample_rate as usize;
		let max_frames = clap.config().max_frames_count as usize;

		Self::with_backend(
			AudioBackend::Clap(clap),
			param_count,
			sample_rate,
			max_frames,
		)
	}

	/// `param_count` is the number of parameters the plugin has, to size the queue of parameter
	/// changes that go to the UI.
	#[must_use]
	pub fn create_vst3(
		vst3: Vst3AudioThread,
		param_count: usize,
	) -> (Self, Consumer<(ClapId, f32)>) {
		let sample_rate = vst3.config().sample_rate as usize;
		let max_frames = vst3.config().max_frames as usize;

		Self::with_backend(
			AudioBackend::Vst3(vst3),
			param_count,
			sample_rate,
			max_frames,
		)
	}

	fn with_backend(
		backend: AudioBackend,
		param_count: usize,
		sample_rate: usize,
		max_frames: usize,
	) -> (Self, Consumer<(ClapId, f32)>) {
		let (producer, consumer) =
			RingBuffer::new((param_count * sample_rate).div_ceil(max_frames));

		(
			Self {
				updates: HashMap::with_capacity(param_count),
				backend,
				producer,
			},
			consumer,
		)
	}

	#[must_use]
	pub fn into_backend(self) -> AudioBackend {
		self.backend
	}

	/// # Panics
	///
	/// Panics if this isn't a CLAP plugin, use [`into_backend`](Self::into_backend) if the format
	/// isn't known.
	#[must_use]
	pub fn into_clap(self) -> ClapAudioThread {
		match self.backend {
			AudioBackend::Clap(clap) => clap,
			AudioBackend::Vst3(_) => panic!("not a CLAP plugin"),
		}
	}

	pub(crate) fn sample_rate(&self) -> u32 {
		match &self.backend {
			AudioBackend::Clap(clap) => clap.config().sample_rate as u32,
			AudioBackend::Vst3(vst3) => vst3.config().sample_rate as u32,
		}
	}

	pub(crate) fn max_frames(&self) -> u32 {
		match &self.backend {
			AudioBackend::Clap(clap) => clap.config().max_frames_count,
			AudioBackend::Vst3(vst3) => vst3.config().max_frames,
		}
	}

	/// Whether the plugin asked to have its audio processing set up again.
	pub(crate) fn needs_restart(&self) -> bool {
		match &self.backend {
			AudioBackend::Clap(clap) => clap.needs_restart(),
			AudioBackend::Vst3(vst3) => vst3.restart_flags().needs_restart(),
		}
	}
}

/// Processes a block with a VST3 plugin, and returns the plugin's latency.
///
/// Parameter changes the plugin makes on its own are collected in `updates`, and its other events
/// replace the ones in `events`, for whatever comes after it.
pub fn process_vst3(
	vst3: &mut Vst3AudioThread,
	updates: &mut HashMap<ClapId, f32>,
	transport: &Transport,
	audio: &mut [[f32; 2]],
	events: &mut Vec<Event>,
	mix: f32,
) -> usize {
	for event in events.drain(..) {
		vst3.push_event(event.to_vst3());
	}

	vst3.process(audio, Some(&transport.as_vst3()), mix);

	for event in vst3.drain_output_events() {
		match Event::from_vst3(event) {
			Some(Event::ParamValue {
				param_id, value, ..
			}) => {
				updates.insert(param_id, value);
			}
			Some(event) => events.push(event),
			None => {}
		}
	}

	// Anything else that needs a restart is left for the caller, who has to give the processor
	// back to the main thread.
	let flags = vst3.restart_flags();
	if !flags.needs_restart() && flags.contains(RestartFlags::LATENCY_CHANGED) {
		let _ = vst3.take_restart_flags();
		vst3.update_latency();
	}

	vst3.latency()
}

fn process_clap(
	clap: &mut ClapAudioThread,
	updates: &mut HashMap<ClapId, f32>,
	transport: &Transport,
	audio: &mut [[f32; 2]],
	events: &mut Vec<Event>,
	injector: &Injector<Node>,
	mix: f32,
) -> usize {
	clap.push_all(events.drain(..));

	clap.process::<Event>(
		audio,
		|event| {
			if let Event::ParamValue {
				param_id, value, ..
			} = event
			{
				updates.insert(param_id, value);
			} else {
				events.push(event);
			}
		},
		Some(&transport.as_clap()),
		Some(&mut |executor| {
			let task_count = executor.task_count() as usize;
			let executor = Inject(executor);
			injector.inject(&executor, task_count);
		}),
		mix,
	);

	clap.latency()
}

#[derive(Debug)]
pub struct Plugin {
	pub processor: AudioProcessor,
	pub sender: oneshot::Sender<AudioThreadMessage>,
}

#[derive(Debug)]
pub struct PluginSlot {
	plugin: PushSlot<Option<Plugin>>,
	mix: f32,
}

impl Drop for PluginSlot {
	fn drop(&mut self) {
		self.destroy();
	}
}

impl PluginSlot {
	pub fn new(plugin: PushSlot<Option<Plugin>>) -> Self {
		Self { plugin, mix: 1.0 }
	}

	pub fn process(
		&mut self,
		state: &State,
		audio: &mut [[f32; 2]],
		events: &mut Vec<Event>,
		injector: &Injector<Node>,
	) -> usize {
		let Some(plugin) = (if state.render_mode == RenderMode::Realtime {
			self.plugin.try_recv()
		} else {
			self.plugin.recv()
		}) else {
			audio.fill([0.0; 2]);
			events.clear();
			return 0;
		};

		let plugin = match plugin {
			Some(plugin) if state.render_mode == RenderMode::Realtime => plugin,
			Some(plugin) if plugin.processor.needs_restart() => {
				self.restart();

				let Some(plugin) = self.plugin.recv() else {
					audio.fill([0.0; 2]);
					events.clear();
					return 0;
				};

				match plugin {
					Some(plugin) => plugin,
					None => return 0,
				}
			}
			Some(plugin) => plugin,
			None => return 0,
		};

		debug_assert_eq!(
			plugin.processor.sample_rate(),
			state.transport.sample_rate.get()
		);

		debug_assert_eq!(plugin.processor.max_frames(), state.transport.frames.get());

		let processor = &mut plugin.processor;

		let latency = match &mut processor.backend {
			AudioBackend::Clap(clap) => process_clap(
				clap,
				&mut processor.updates,
				&state.transport,
				audio,
				events,
				injector,
				self.mix,
			),
			AudioBackend::Vst3(vst3) => process_vst3(
				vst3,
				&mut processor.updates,
				&state.transport,
				audio,
				events,
				self.mix,
			),
		};

		if state.render_mode == RenderMode::Realtime {
			for (param_id, value) in plugin.processor.updates.extract_if(|_, _| true) {
				if plugin.processor.producer.push((param_id, value)).is_err() {
					warn!("full ring buffer");
					plugin.processor.updates.insert(param_id, value);
					break;
				}
			}
		}

		if plugin.processor.needs_restart() {
			self.restart();
		}

		latency
	}

	pub fn reset(&mut self) {
		if let Some(plugin) = self.plugin.as_mut().and_then(|slot| slot.as_mut()) {
			match &mut plugin.processor.backend {
				AudioBackend::Clap(clap) => clap.reset(),
				AudioBackend::Vst3(vst3) => vst3.reset(),
			}
		}
	}

	pub fn mix_changed(&mut self, mix: f32) {
		self.mix = mix;
	}

	pub fn param_changed(&mut self, param_id: ClapId, value: f32) {
		if let Some(plugin) = self.plugin.as_mut().and_then(|slot| slot.as_mut()) {
			plugin.processor.updates.remove(&param_id);

			let event = Event::ParamValue {
				time: 0,
				param_id,
				value,
				flags: EventFlags::IS_LIVE,
			};

			match &mut plugin.processor.backend {
				AudioBackend::Clap(clap) => clap.push(event),
				AudioBackend::Vst3(vst3) => {
					vst3.push_event(event.to_vst3());
				}
			}
		}
	}

	pub fn activate(&mut self, plugin: Plugin) {
		*self.plugin.as_mut().unwrap() = Some(plugin);
	}

	pub fn deactivate(&mut self) {
		if let Some(plugin) = self.plugin.as_mut().and_then(Option::take) {
			plugin
				.sender
				.send(AudioThreadMessage::Deactivate(plugin.processor))
				.unwrap();
		}
	}

	pub fn restart(&mut self) {
		if self.plugin.as_mut().is_some_and(|plugin| plugin.is_some()) {
			let plugin = self.plugin.take().flatten().unwrap();
			plugin
				.sender
				.send(AudioThreadMessage::Restart(plugin.processor))
				.unwrap();
		}
	}

	pub fn destroy(&mut self) {
		if let Some(plugin) = self.plugin.as_mut().and_then(Option::take) {
			plugin
				.sender
				.send(AudioThreadMessage::Destroy(plugin.processor))
				.unwrap();
		}
	}
}
