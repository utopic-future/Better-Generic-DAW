use crate::{
	Channels, Event, Node, NodeAction, NodeId, Update,
	audio_thread::{Scratch, State},
	plugin::PluginSlot,
};
use audio_graph::Injector;
use dsp::Utility;
use utils::ShiftMoveExt as _;

#[cfg(feature = "simd")]
use audio_graph::simd;

#[derive(Debug)]
pub struct Channel {
	id: NodeId,
	plugins: Vec<PluginSlot>,
	utility: Utility,
	enabled: bool,
	bypassed: bool,
	output: Channels,
	peaks: [f32; 2],
	last_peaks: [f32; 2],
}

impl Channel {
	#[must_use]
	pub fn new(output: Channels) -> Self {
		Self {
			plugins: Vec::new(),
			id: NodeId::unique(),
			utility: Utility::default(),
			enabled: true,
			bypassed: false,
			output,
			peaks: [0.0; 2],
			last_peaks: [0.0; 2],
		}
	}

	pub fn process(
		&mut self,
		state: &State,
		audio: &mut [[f32; 2]],
		events: &mut Vec<Event>,
		scratch: &mut Scratch,
		injector: &Injector<Node>,
	) -> usize {
		let mut latency = 0;

		// Bypass: save dry signal before any processing
		if self.bypassed {
			scratch.audio[..audio.len()].copy_from_slice(audio);
			scratch.events.clone_from(events);
		}

		// Process through plugins
		for plugin in &mut self.plugins {
			latency += plugin.process(state, audio, events, injector);
		}

		// Apply utility (volume, pan)
		self.utility.process(audio);

		// If bypassed, restore dry signal WITH utility applied
		if self.bypassed {
			latency = 0;
			// Apply utility to the saved dry signal
			self.utility.process(&mut scratch.audio[..audio.len()]);
			audio.copy_from_slice(&scratch.audio[..audio.len()]);
			events.clone_from(&scratch.events);
		}

		let peaks = max_peaks(audio).map(|x| if x >= f32::EPSILON { x } else { 0.0 });
		self.peaks = [self.peaks[0].max(peaks[0]), self.peaks[1].max(peaks[1])];

		if !self.enabled {
			latency = 0;
			audio.fill([0.0; 2]);
			events.clear();
		}

		latency
	}

	#[must_use]
	pub fn id(&self) -> NodeId {
		self.id
	}

	pub fn reset(&mut self) {
		for plugin in &mut self.plugins {
			plugin.reset();
		}
	}

	pub fn apply(&mut self, action: NodeAction) {
		match action {
			NodeAction::OutputSetChannels(output) => self.output = output,
			NodeAction::ChannelToggleEnabled => self.enabled ^= true,
			NodeAction::ChannelToggleBypassed => self.bypassed ^= true,
			NodeAction::ChannelVolumeChanged(volume) => self.utility.volume = volume,
			NodeAction::ChannelPanChanged(pan) => self.utility.pan = pan,
			NodeAction::PluginInsert(index, plugin) => {
				self.plugins.insert(index, PluginSlot::new(*plugin));
			}
			NodeAction::PluginRemove(index) => _ = self.plugins.remove(index),
			NodeAction::PluginActivate(index, plugin) => self.plugins[index].activate(*plugin),
			NodeAction::PluginDeactivate(index) => self.plugins[index].deactivate(),
			NodeAction::PluginMoveTo(from, to) => self.plugins.shift_move(from, to),
			NodeAction::PluginMixChanged(index, mix) => self.plugins[index].mix_changed(mix),
			NodeAction::PluginParamChanged(index, param_id, value) => {
				self.plugins[index].param_changed(param_id, value);
			}
			// Only tracks have an instrument chain that notes can be previewed through.
			NodeAction::MidiPreview(_) => {}
			_ => panic!("{action:?}"),
		}
	}

	pub fn collect_updates(&mut self, updates: &mut Vec<Update>) {
		let peaks = std::mem::take(&mut self.peaks);
		if peaks != self.last_peaks {
			self.last_peaks = peaks;
			updates.push(Update::Peaks(self.id(), peaks));
		}
	}

	#[must_use]
	pub fn output(&self) -> Channels {
		self.output
	}

	pub fn restart_all_plugins(&mut self) {
		for plugin in &mut self.plugins {
			plugin.restart();
		}
	}
}

fn max_peaks(audio: &[[f32; 2]]) -> [f32; 2] {
	#[cfg(feature = "simd")]
	{
		return simd::peak_stereo_sse2(audio);
	}

	#[cfg(not(feature = "simd"))]
	{
		// Process stereo frames directly - simpler and avoids chunking issues
		let mut max_l = 0.0f32;
		let mut max_r = 0.0f32;
		for &[l, r] in audio {
			let l_abs = l.abs();
			let r_abs = r.abs();
			if l_abs > max_l { max_l = l_abs; }
			if r_abs > max_r { max_r = r_abs; }
		}
		[max_l, max_r]
	}
}
