use crate::{
	Channel, Channels, Clip, ClipId, Event, MidiAction, MidiNote, MidiPreview, Node, NodeAction,
	NodeId, TimedMidiAction, Update, Version,
	audio_thread::{Scratch, State},
	midi_clip::VoiceId,
	time::{BeatTime, SecondsTime},
	voice_alloc::VoiceAlloc,
};
use audio_graph::Injector;
use clap_host::{
	RenderMode,
	events::{EventFlags, Match},
};
use log::warn;
use midly::num::{u4, u7};
use rtrb::Producer;
use std::collections::HashMap;
use std::sync::atomic::Ordering;

/// The note ID used for previewed notes. It is outside the ranges used by the voice allocator and
/// by live MIDI input, so that it never collides with them.
const PREVIEW_NOTE_ID: u32 = i32::MAX as u32;

/// Keyboard notes use the unused portion of the live-note ID range.
const KEYBOARD_NOTE_ID_BASE: u32 = (i32::MAX - 32751) as u32;

#[derive(Clone, Copy, Debug, Default)]
enum KeyboardVoice {
	#[default]
	Off,
	PendingOn(u7),
	On(u7),
	PendingOff(u7),
	Retrigger { old_velocity: u7, new_velocity: u7 },
}

impl KeyboardVoice {
	fn take_events(&mut self) -> (Option<u7>, Option<u7>) {
		match *self {
			Self::PendingOn(velocity) => {
				*self = Self::On(velocity);
				(None, Some(velocity))
			}
			Self::PendingOff(velocity) => {
				*self = Self::Off;
				(Some(velocity), None)
			}
			Self::Retrigger {
				old_velocity,
				new_velocity,
			} => {
				*self = Self::On(new_velocity);
				(Some(old_velocity), Some(new_velocity))
			}
			Self::Off | Self::On(_) => (None, None),
		}
	}
}

/// A note that is being previewed through the track's instrument chain.
#[derive(Clone, Copy, Debug)]
struct PreviewVoice {
	preview: MidiPreview,
	/// The number of frames left until the note is turned off.
	remaining: u32,
	/// Whether the note has been turned on already.
	started: bool,
}

impl PreviewVoice {
	fn on(&self, time: u32) -> Event {
		Event::On {
			time,
			key: self.preview.key().0,
			velocity: self.preview.velocity(),
			note_id: Match::Specific(PREVIEW_NOTE_ID),
			flags: EventFlags::empty(),
		}
	}

	fn off(&self, time: u32) -> Event {
		Event::Off {
			time,
			key: self.preview.key().0,
			velocity: self.preview.velocity(),
			note_id: Match::Specific(PREVIEW_NOTE_ID),
			flags: EventFlags::empty(),
		}
	}
}

/// The note that is being previewed through the track's instrument chain.
///
/// The note is turned on in the first block it is processed in, and turned off again as soon as
/// its duration has passed. A preview that is replaced by another one is turned off first, so
/// that no note is left hanging.
#[derive(Debug, Default)]
struct Preview {
	/// The note that is sounding, or that is about to sound.
	voice: Option<PreviewVoice>,
	/// A note that has to be turned off before the next one is turned on.
	pending_stop: Option<PreviewVoice>,
	/// The version of the preview that just ended, waiting to be reported.
	ended: Option<Version>,
}

impl Preview {
	/// Starts previewing a note, turning off the note that was previewed before it.
	fn start(&mut self, preview: MidiPreview, remaining: u32) {
		self.stop();

		self.voice = Some(PreviewVoice {
			preview,
			remaining,
			started: false,
		});
	}

	/// Stops previewing, turning off the note if it is sounding already.
	fn stop(&mut self) {
		if let Some(voice) = self.voice.take()
			&& voice.started
		{
			self.pending_stop = Some(voice);
		}
	}

	/// Plays the note that is being previewed.
	fn process(&mut self, frames: usize, events: &mut Vec<Event>) {
		if let Some(voice) = self.pending_stop.take() {
			events.push(voice.off(0));
		}

		let Some(voice) = &mut self.voice else {
			return;
		};

		if !voice.started {
			voice.started = true;

			events.push(voice.on(0));
		}

		let frames = frames as u32;

		if voice.remaining < frames {
			events.push(voice.off(voice.remaining));

			self.ended = Some(voice.preview.version());
			self.voice = None;
		} else {
			voice.remaining -= frames;
		}
	}

	/// The version of the preview that just ended, if it ended.
	fn take_ended(&mut self) -> Option<Version> {
		self.ended.take()
	}
}

#[derive(Debug)]
pub struct Track {
	clips: HashMap<ClipId, Clip>,
	voice_alloc: VoiceAlloc<VoiceId, MidiNote>,
	last_polyphony: usize,
	input: Channels,
	audio_producer: Option<Producer<[f32; 2]>>,
	midi_producer: Option<Producer<TimedMidiAction<BeatTime>>>,
	channel: Channel,
	/// The note that is being previewed right now, if any.
	preview: Preview,
	/// Notes held by the computer MIDI keyboard, indexed by MIDI pitch.
	keyboard_notes: [KeyboardVoice; 128],
}

impl Track {
	#[must_use]
	pub fn new(input: Channels, output: Channels) -> Self {
		Self {
			clips: HashMap::new(),
			voice_alloc: VoiceAlloc::default(),
			last_polyphony: 0,
			input,
			audio_producer: None,
			midi_producer: None,
			channel: Channel::new(output),
			preview: Preview::default(),
			keyboard_notes: [KeyboardVoice::Off; 128],
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
		self.voice_alloc.deactivate_all();

		if state.transport.playing {
			for clip in self.clips.values_mut() {
				clip.diff(state, audio, events, &mut self.voice_alloc);
			}
		}

		for voice in self.voice_alloc.drain_inactive() {
			events.push(Event::Off {
				time: 0,
				key: voice.info.key.0,
				velocity: voice.info.velocity,
				note_id: Match::Specific(voice.note_id),
				flags: EventFlags::empty(),
			});
		}

		if state.render_mode == RenderMode::Realtime {
			let iter = state
				.midi_input
				.iter()
				.filter(|action| self.input.midi & (1 << action.action.channel().as_int()) != 0);

			if self.input.enable_midi {
				events.extend(iter.clone().map(|action| match action.action {
					MidiAction::NoteOn(channel, key, velocity) => Event::On {
						time: action.ts as u32,
						key: key.as_int(),
						velocity: f32::from(velocity.as_int()) / 127.0,
						note_id: Match::Specific(
							// Live MIDI notes use high range: i32::MAX - 32767 ..= i32::MAX - 1
							// Channel is 0-15, so we use 32767 + channel
							(i32::MAX - 32767 + i32::from(channel.as_int())) as u32,
						),
						flags: EventFlags::IS_LIVE,
					},
					MidiAction::NoteOff(channel, key, velocity) => Event::Off {
						time: action.ts as u32,
						key: key.as_int(),
						velocity: f32::from(velocity.as_int()) / 127.0,
						note_id: Match::Specific(
							(i32::MAX - 32767 + i32::from(channel.as_int())) as u32,
						),
						flags: EventFlags::IS_LIVE,
					},
				}));
			}

			if let Some(producer) = &mut self.midi_producer
				&& state.transport.playing
			{
				for &TimedMidiAction { ts, action } in iter {
					let ts = (state.transport.position
						+ SecondsTime::from_frames(ts as usize, &state.transport))
					.to_beat_time(&state.transport);

					if producer.push(TimedMidiAction { ts, action }).is_err() {
						warn!("full ring buffer");
						break;
					}
				}
			}

			self.process_keyboard_notes(state, events);

			if self.input.fits_in(state.transport.input_channels) {
				let audio = if self.input.enable_audio {
					&mut *audio
				} else if self.audio_producer.is_some() && state.transport.playing {
					&mut scratch.audio[..audio.len()]
				} else {
					&mut []
				};

				for ([l, r], frame) in audio.iter_mut().zip(
					state
						.audio_input
						.chunks_exact(state.transport.input_channels.into()),
				) {
					*l = frame[usize::from(self.input.left)];
					*r = frame[usize::from(self.input.right)];
				}

				if let Some(producer) = &mut self.audio_producer
					&& state.transport.playing
					&& let (_, rest) = producer.push_partial_slice(audio)
					&& !rest.is_empty()
				{
					warn!("full ring buffer");
				}
			}
		}

		if state.transport.playing {
			for clip in self.clips.values_mut() {
				clip.process(state, audio, events, &mut self.voice_alloc);
			}
		}

		if state.render_mode == RenderMode::Realtime {
			self.preview.process(audio.len(), events);
		}

		self.channel
			.process(state, audio, events, scratch, injector)
	}

	fn process_keyboard_notes(&mut self, state: &State, events: &mut Vec<Event>) {
		let position = state.transport.position.to_beat_time(&state.transport);

		for (key, voice) in self.keyboard_notes.iter_mut().enumerate() {
			let (off, on) = voice.take_events();
			let key = key as u8;
			let note_id = Match::Specific(KEYBOARD_NOTE_ID_BASE + u32::from(key));

			if let Some(velocity) = off {
				events.push(Event::Off {
					time: 0,
					key,
					velocity: f32::from(velocity.as_int()) / 127.0,
					note_id,
					flags: EventFlags::IS_LIVE,
				});

				if state.transport.playing
					&& let Some(producer) = &mut self.midi_producer
					&& producer
						.push(TimedMidiAction {
							ts: position,
							action: MidiAction::NoteOff(u4::new(0), u7::new(key), u7::new(0)),
						})
					.is_err()
				{
					warn!("full ring buffer");
				}
			}

			if let Some(velocity) = on {
				events.push(Event::On {
					time: 0,
					key,
					velocity: f32::from(velocity.as_int()) / 127.0,
					note_id,
					flags: EventFlags::IS_LIVE,
				});

				if state.transport.playing
					&& let Some(producer) = &mut self.midi_producer
					&& producer
						.push(TimedMidiAction {
							ts: position,
							action: MidiAction::NoteOn(u4::new(0), u7::new(key), velocity),
						})
					.is_err()
				{
					warn!("full ring buffer");
				}
			}
		}
	}

	#[must_use]
	pub fn id(&self) -> NodeId {
		self.channel.id()
	}

	pub fn reset(&mut self) {
		self.channel.reset();
	}

	pub fn apply(&mut self, action: NodeAction, state: &mut State, updates: &mut Vec<Update>) {
		match action {
			NodeAction::ClipAdd(clip) => {
				let clip = self.clips.insert(clip.id(), *clip);
				debug_assert!(clip.is_none());
			}
			NodeAction::ClipRemove(id) => {
				let clip = self.clips.remove(&id);
				debug_assert!(clip.is_some());
			}
			NodeAction::ClipReplace(clip) => {
				let old = self.clips.insert(clip.id(), *clip);
				debug_assert!(old.is_some());
			}
			NodeAction::ClipMoveTo(id, pos) => {
				if let Some(clip) = self.clips.get_mut(&id) {
					clip.move_to(pos);
				}
			}
			NodeAction::ClipTrimStartTo(id, pos) => {
				if let Some(clip) = self.clips.get_mut(&id) {
					clip.trim_start_to(pos, &state.transport);
					if let Clip::Audio(audio) = clip {
						audio.fade_start.len = audio.fade_start.len.min(audio.position.len());
						audio.fade_end.len = audio
							.fade_end
							.len
							.min(audio.position.len() - audio.fade_start.len);
					}
				}
			}
			NodeAction::ClipTrimEndTo(id, pos) => {
				if let Some(clip) = self.clips.get_mut(&id) {
					clip.trim_end_to(pos, &state.transport);
					if let Clip::Audio(audio) = clip {
						audio.fade_end.len = audio.fade_end.len.min(audio.position.len());
						audio.fade_start.len = audio
							.fade_start
							.len
							.min(audio.position.len() - audio.fade_end.len);
					}
				}
			}
			NodeAction::ClipVolumeChanged(id, volume) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.volume = volume;
				}
			}
			NodeAction::ClipFadeStartLen(id, len) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_start.len = len;
				}
			}
			NodeAction::ClipFadeStartP(id, p) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_start.p = p;
				}
			}
			NodeAction::ClipFadeStartToggleSymmetric(id) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_start.symmetric ^= true;
				}
			}
			NodeAction::ClipFadeEndLen(id, len) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_end.len = len;
				}
			}
			NodeAction::ClipFadeEndP(id, p) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_end.p = p;
				}
			}
			NodeAction::ClipFadeEndToggleSymmetric(id) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.fade_end.symmetric ^= true;
				}
			}
			NodeAction::ClipStretchStartTo(id, pos) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					let len = clip.position.len() * clip.stretch.abs();
					let end = clip.position.end(&state.transport);
					clip.position.move_to(pos.min(end - BeatTime::TICK));
					clip.position.trim_end_to(end, &state.transport);
					let stretch = clip.stretch;
					clip.stretch = (len / clip.position.len()).copysign(stretch);
					clip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
					clip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
				}
			}
			NodeAction::ClipStretchEndTo(id, pos) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					let len = clip.position.len() * clip.stretch.abs();
					clip.position.trim_end_to(pos, &state.transport);
					let stretch = clip.stretch;
					clip.stretch = (len / clip.position.len()).copysign(clip.stretch);
					clip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
					clip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
				}
			}
			NodeAction::ClipReverse(id) => {
				if let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
					clip.stretch *= -1.0;
					clip.position.reverse(
						state.samples[&clip.sample].len(&state.transport),
						clip.stretch.abs(),
					);
					(clip.fade_start, clip.fade_end) = (clip.fade_end, clip.fade_start);
				}
			}
			NodeAction::ClipSlipTo(id, pos) => {
				if let Some(clip) = self.clips.get_mut(&id) {
					clip.slip_to(pos, &state.transport);
				}
			}
			NodeAction::ClipVelocityChanged(clip_id, note_id, velocity) => {
				if let Some(clip) = self.clips.get_mut(&clip_id) {
					if let Clip::Midi(clip) = clip {
						if let Some(pattern) = state.midi_patterns.get_mut(&clip.pattern) {
							if let Some(note) = pattern.notes.get_mut(&note_id) {
								note.velocity = velocity.clamp(0.0, 1.0);
							}
						}
					}
				}
			}
			NodeAction::InputSetChannels(input) => {
				if (self.input.left != input.left || self.input.right != input.right)
					&& self.audio_producer.is_some()
				{
					updates.push(Update::AudioRecordingInterrupted(self.channel.id()));
				}

				if self.input.midi != input.midi
					&& let Some(producer) = &mut self.midi_producer
				{
					updates.push(Update::MidiRecordingInterrupted(self.channel.id()));

					for (idx, velocity_atomic) in state.playing.iter().enumerate() {
						let velocity = velocity_atomic.load(Ordering::Relaxed);
						if velocity != 0 {
							let channel = u4::new((idx / 128) as u8);
							let key = u7::new((idx % 128) as u8);
							if input.midi & (1 << channel.as_int()) != 0
								&& producer
									.push(TimedMidiAction {
										ts: state.transport.position.to_beat_time(&state.transport),
										action: MidiAction::NoteOn(channel, key, u7::new(velocity)),
									})
									.is_err()
							{
								warn!("full ring buffer");
								break;
							}
						}
					}
				}

				self.input = input;
			}
			NodeAction::InputSetAudioRecording(producer) => {
				self.audio_producer = producer;
				if self.audio_producer.is_none() {
					updates.push(Update::AudioRecordingInterrupted(self.channel.id()));
				}
			}
			NodeAction::InputSetMidiRecording(producer) => {
				self.midi_producer = producer;
				if let Some(producer) = &mut self.midi_producer {
					for (idx, velocity_atomic) in state.playing.iter().enumerate() {
						let velocity = velocity_atomic.load(Ordering::Relaxed);
						if velocity != 0 {
							let channel = u4::new((idx / 128) as u8);
							let key = u7::new((idx % 128) as u8);
							if self.input.midi & (1 << channel.as_int()) != 0
								&& producer
									.push(TimedMidiAction {
										ts: state.transport.position.to_beat_time(&state.transport),
										action: MidiAction::NoteOn(channel, key, u7::new(velocity)),
									})
									.is_err()
							{
								warn!("full ring buffer");
								break;
							}
						}
					}
				} else {
					updates.push(Update::MidiRecordingInterrupted(self.channel.id()));
				}
			}
			NodeAction::MidiKeyboard(action) => {
				let key = action.key().as_int() as usize;
				let current = self.keyboard_notes[key];

				self.keyboard_notes[key] = match action {
					MidiAction::NoteOn(_, _, velocity) if velocity.as_int() != 0 => match current {
						KeyboardVoice::Off => KeyboardVoice::PendingOn(velocity),
						KeyboardVoice::PendingOff(old_velocity) => KeyboardVoice::Retrigger {
							old_velocity,
							new_velocity: velocity,
						},
						voice => voice,
					},
					MidiAction::NoteOn(_, _, _) | MidiAction::NoteOff(_, _, _) => match current {
						KeyboardVoice::PendingOn(_) => KeyboardVoice::Off,
						KeyboardVoice::On(velocity) => KeyboardVoice::PendingOff(velocity),
						KeyboardVoice::Retrigger { old_velocity, .. } => {
							KeyboardVoice::PendingOff(old_velocity)
						}
						voice => voice,
					},
				};
			}
			NodeAction::MidiPreview(preview) => {
				let Some(preview) = preview else {
					self.preview.stop();
					return;
				};

				let remaining = preview.duration().to_frames(&state.transport) as u32;

				self.preview.start(preview, remaining);
			}
			action => self.channel.apply(action),
		}
	}

	pub fn collect_updates(&mut self, updates: &mut Vec<Update>) {
		if let Some(version) = self.preview.take_ended() {
			updates.push(Update::MidiPreviewEnded(version));
		}

		let polyphony = self.voice_alloc.current_polyphony();
		if polyphony != self.last_polyphony {
			self.last_polyphony = polyphony;
			updates.push(Update::Polyphony(self.id(), polyphony));
		}

		self.channel.collect_updates(updates);
	}

	#[must_use]
	pub fn output(&self) -> Channels {
		self.channel.output()
	}

	pub fn restart_all_plugins(&mut self) {
		self.channel.restart_all_plugins();
	}

	#[must_use]
	pub fn from_channel(input: Channels, channel: Channel) -> Self {
		Self {
			clips: HashMap::new(),
			voice_alloc: VoiceAlloc::default(),
			last_polyphony: 0,
			input,
			audio_producer: None,
			midi_producer: None,
			channel,
			preview: Preview::default(),
			keyboard_notes: [KeyboardVoice::Off; 128],
		}
	}

	#[must_use]
	pub fn into_channel(track: Self) -> Channel {
		track.channel
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::MidiKey;

	fn note(key: u8) -> MidiPreview {
		MidiPreview::new(MidiKey(key), 0.8, SecondsTime::new(1, 0))
	}

	#[test]
	fn a_preview_is_turned_on_and_off_again() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);

		// the note is turned on in the first block
		preview.process(50, &mut events);
		assert!(matches!(
			events[0],
			Event::On {
				time: 0,
				key: 60,
				velocity: 0.8,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert_eq!(events.len(), 1);

		// and it keeps sounding until its duration has passed
		events.clear();
		preview.process(50, &mut events);
		assert!(events.is_empty());

		events.clear();
		preview.process(50, &mut events);
		assert!(matches!(
			events[0],
			Event::Off {
				time: 0,
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert_eq!(events.len(), 1);
	}

	#[test]
	fn a_preview_is_turned_off_when_it_is_replaced() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);
		preview.process(50, &mut events);
		events.clear();

		preview.start(note(64), 100);
		preview.process(50, &mut events);

		// the note that was sounding is turned off before the new one is turned on
		assert_eq!(events.len(), 2);
		assert!(matches!(
			events[0],
			Event::Off {
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert!(matches!(
			events[1],
			Event::On {
				key: 64,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
	}

	#[test]
	fn a_preview_that_is_stopped_is_turned_off() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);
		preview.process(50, &mut events);
		events.clear();

		preview.stop();
		preview.process(50, &mut events);

		assert_eq!(events.len(), 1);
		assert!(matches!(
			events[0],
			Event::Off {
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));

		// and nothing is left hanging afterwards
		events.clear();
		preview.process(50, &mut events);
		assert!(events.is_empty());
	}

	#[test]
	fn the_end_of_a_preview_is_reported() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		let preview_note = note(60);
		preview.start(preview_note, 10);

		assert_eq!(preview.take_ended(), None);

		// the note is turned on in the first block, and turned off in the second one, because that
		// is where its duration ends
		preview.process(10, &mut events);
		assert_eq!(preview.take_ended(), None);

		preview.process(10, &mut events);
		assert_eq!(preview.take_ended(), Some(preview_note.version()));
		// and it is only reported once
		assert_eq!(preview.take_ended(), None);
	}
}
				let Some(preview) = preview else {
					self.preview.stop();
					return;
				};

				let remaining = preview.duration().to_frames(&state.transport) as u32;

				self.preview.start(preview, remaining);
			}
			action => self.channel.apply(action),
		}
	}

	pub fn collect_updates(&mut self, updates: &mut Vec<Update>) {
		if let Some(version) = self.preview.take_ended() {
			updates.push(Update::MidiPreviewEnded(version));
		}

		let polyphony = self.voice_alloc.current_polyphony();
		if polyphony != self.last_polyphony {
			self.last_polyphony = polyphony;
			updates.push(Update::Polyphony(self.id(), polyphony));
		}

		self.channel.collect_updates(updates);
	}

	#[must_use]
	pub fn output(&self) -> Channels {
		self.channel.output()
	}

	pub fn restart_all_plugins(&mut self) {
		self.channel.restart_all_plugins();
	}

	#[must_use]
	pub fn from_channel(input: Channels, channel: Channel) -> Self {
		Self {
			clips: HashMap::new(),
			voice_alloc: VoiceAlloc::default(),
			last_polyphony: 0,
			input,
			audio_producer: None,
			midi_producer: None,
			channel,
			preview: Preview::default(),
		}
	}

	#[must_use]
	pub fn into_channel(track: Self) -> Channel {
		track.channel
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::MidiKey;

	fn note(key: u8) -> MidiPreview {
		MidiPreview::new(MidiKey(key), 0.8, SecondsTime::new(1, 0))
	}

	#[test]
	fn a_preview_is_turned_on_and_off_again() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);

		// the note is turned on in the first block
		preview.process(50, &mut events);
		assert!(matches!(
			events[0],
			Event::On {
				time: 0,
				key: 60,
				velocity: 0.8,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert_eq!(events.len(), 1);

		// and it keeps sounding until its duration has passed
		events.clear();
		preview.process(50, &mut events);
		assert!(events.is_empty());

		events.clear();
		preview.process(50, &mut events);
		assert!(matches!(
			events[0],
			Event::Off {
				time: 0,
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert_eq!(events.len(), 1);
	}

	#[test]
	fn a_preview_is_turned_off_when_it_is_replaced() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);
		preview.process(50, &mut events);
		events.clear();

		preview.start(note(64), 100);
		preview.process(50, &mut events);

		// the note that was sounding is turned off before the new one is turned on
		assert_eq!(events.len(), 2);
		assert!(matches!(
			events[0],
			Event::Off {
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
		assert!(matches!(
			events[1],
			Event::On {
				key: 64,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));
	}

	#[test]
	fn a_preview_that_is_stopped_is_turned_off() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		preview.start(note(60), 100);
		preview.process(50, &mut events);
		events.clear();

		preview.stop();
		preview.process(50, &mut events);

		assert_eq!(events.len(), 1);
		assert!(matches!(
			events[0],
			Event::Off {
				key: 60,
				note_id: Match::Specific(PREVIEW_NOTE_ID),
				..
			}
		));

		// and nothing is left hanging afterwards
		events.clear();
		preview.process(50, &mut events);
		assert!(events.is_empty());
	}

	#[test]
	fn the_end_of_a_preview_is_reported() {
		let mut preview = Preview::default();
		let mut events = Vec::new();

		let preview_note = note(60);
		preview.start(preview_note, 10);

		assert_eq!(preview.take_ended(), None);

		// the note is turned on in the first block, and turned off in the second one, because that
		// is where its duration ends
		preview.process(10, &mut events);
		assert_eq!(preview.take_ended(), None);

		preview.process(10, &mut events);
		assert_eq!(preview.take_ended(), Some(preview_note.version()));
		// and it is only reported once
		assert_eq!(preview.take_ended(), None);
	}
}
