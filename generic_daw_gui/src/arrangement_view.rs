use crate::{
	action::Action,
	clap_host::{ClapHost, PluginId},
	components::{MenuItem, icon_button, menu, menu_entry, menu_width, text_icon_button},
	config::Config,
	daw::{self, Tab},
	file_tree::FileKind,
	icons::{
		arrow_left_right, arrow_up_down, check, chevron_down, chevron_up,
		chevrons_left_right_ellipsis, circle_ellipsis, clipboard_paste, copy, flip_horizontal,
		gauge, grip_horizontal, grip_vertical, keyboard_music, magnet, midi_port, music_2, plus,
		power, power_off, repeat as repeat_icon, rotate_ccw, sliders_vertical, snowflake,
		square_split_horizontal, trash_2, volume_2, x,
	},
	operation::scroll_into_view,
	plugin_descriptor::PluginDescriptor,
	shortcuts::Action as ShortcutAction,
	state::{DEFAULT_SPLIT_WIDTH, MIN_SPLIT_WIDTH, State},
	stylefns::{
		button_with_radius, container_with_radius, selectable_box, slider_with_radius, split_style,
		sweeten_column_style, sweeten_column_with_radius, sweeten_row_style,
		sweeten_row_with_radius, text_input_transparent, weak_bordered_box, weaker_bordered_box,
		weakest_bordered_box,
	},
	vst2_host::Vst2Host,
	vst3_host::Vst3Host,
	widget::{
		Clip, Delta, LINE_HEIGHT, Note, Piano, PianoRoll, Playlist, Seeker, TEXT_HEIGHT, Track,
		frames_per_px, piano_roll,
		playlist::{self, ContextTarget},
	},
};
use generic_daw_core::{
	AudioThread, Batch, Channels, MidiClip, MidiKey, MidiNote, MidiNoteId, MidiPatternId, NodeId,
	PanMode, Point, PullSlot, SampleId, ThreadPool,
	time::{BeatRange, BeatTime, SecondsTime},
};
use generic_daw_project::proto;
use generic_daw_widget::{
	context_menu::ContextMenu,
	knob::Knob,
	peak_meter::{MAX_VOL, PeakMeter},
	select_area::{self, SelectArea},
};
use iced::{
	Center, Element, Fill, Shrink, Subscription, Task, Vector, border,
	futures::SinkExt as _,
	mouse::Interaction,
	padding, stream,
	time::every,
	widget::{
		button, center, center_x, center_y, column, container, mouse_area, opaque,
		operation::focus, operation::snap_to_end, row, rule, scrollable, slider, space, text,
		text_input, vertical_slider,
	},
	window::frames,
};
use iced_split::{Split, Strategy};
use midi_pattern::MidiPatternPair;
use node::{Node, NodeType};
use rtrb::Consumer;
use sample::SamplePair;
use smol::{Timer, stream::Stream, unblock};
use std::{
	cell::RefCell,
	cmp::{Ordering, Reverse},
	collections::HashMap,
	io::{self, Read},
	iter::repeat,
	path::Path,
	sync::Arc,
	time::{Duration, Instant},
};
use sweeten::widget::drag::DragEvent;
use utils::{NoClone, NoDebug};

mod arrangement;
mod audio_recording;
mod channel;
mod clip;
mod history;
mod midi_pattern;
mod midi_recording;
mod mixer;
mod node;
mod plugin;
mod project;
mod sample;
mod track;

pub use arrangement::Arrangement;
pub use audio_recording::AudioRecording;
pub use clip::{AudioClipRef, MidiClipRef};
pub use midi_recording::MidiRecording;
pub use project::Feedback;

#[derive(Clone, Debug)]
pub enum Message {
	ChangeConfig(NoClone<NoDebug<Box<(AudioThread, ThreadPool)>>>),
	Batch(NoClone<Box<Batch>>),
	DrainQueue,
	RequestUpdate,

	Connect(NodeId, NodeId),
	SetMix(NodeId, NodeId, f32),
	Disconnect(NodeId, NodeId),
	ToggleKind(NodeId),

	AudioPreview(SampleId),

	ChannelAdd,
	ChannelInsert(NodeId),
	ChannelRemove(NodeId),
	ChannelMove(DragEvent),
	ChannelSelect(NodeId),
	ChannelDuplicate(NodeId),
	ChannelNameChanged(NodeId, Arc<str>),
	ChannelRename(NodeId),
	ChannelVolumeChanged(NodeId, f32),
	ChannelPanChanged(NodeId, PanMode),
	ChannelToggleEnabled(NodeId),
	ChannelToggleBypassed(NodeId),

	PluginPicker,
	PluginAdd(Box<PluginDescriptor>),
	PluginAddTo(NodeId, Box<PluginDescriptor>, bool),
	PluginRemove(NodeId, usize),
	PluginMove(NodeId, DragEvent),
	PluginDuplicate(NodeId, usize),
	PluginSetState(NodeId, usize, NoDebug<Box<[u8]>>),
	PluginCopyState(NodeId, usize),
	PluginPasteState(NodeId, usize),
	PluginShow(PluginId),
	PluginMixChanged(NodeId, usize, f32),
	PluginToggleActive(NodeId, usize),

	SampleLoaded(Option<Box<SamplePair>>),
	MidiPatternLoaded(Option<Box<MidiPatternPair>>),
	AudioClipAdd(SampleId, Option<usize>, BeatTime),
	MidiClipAdd(MidiPatternId, Option<usize>, BeatTime),

	TrackAdd,
	TrackInsert(NodeId),
	TrackRemove(NodeId),
	TrackMove(DragEvent),
	TrackDuplicate(NodeId),
	TrackToggleEnabled(NodeId),
	TrackToggleSolo(NodeId),

	SeekTo(BeatTime),
	SetLoopMarker(Option<BeatRange>),

	InputChangeChannels(NodeId, Channels),
	InputToggleAudioRecording(NodeId),
	InputToggleMidiRecording(NodeId),

	OutputChangeChannels(NodeId, Channels),

	Freeze(NodeId),
	FreezeDone(NodeId, Box<SamplePair>, BeatTime),

	PlaylistAction(playlist::Action),
	PianoRollAction(piano_roll::Action),
	/// Something the user wants to do to a clip.
	ClipAction(usize, usize, playlist::ClipAction),
	/// Turns the grid on or off.
	ToggleGrid,
	/// Makes the grid finer.
	NarrowedGrid,
	/// Makes the grid coarser.
	WidenedGrid,
	/// Turns the triplets in the grid on or off.
	ToggleGridTriplets,
	/// Sets the start or the end of the loop.
	SetLoopMarkerStart,
	SetLoopMarkerEnd,

	ArrowUp(Tab),
	ArrowDown(Tab),
	ArrowLeft(Tab),
	ArrowRight(Tab),
	TransposeOctUp(Tab),
	TransposeOctDown(Tab),
	Quantize(Tab),
	Join(Tab),
	SelectAll(Tab),
	SelectInverse(Tab),
	Escape(Tab),
	ToggleEnabled(Tab),
	ToggleSolo(Tab),
	Duplicate(Tab),
	MakeUnique(Tab),
	Delete(Tab),
	Invert(Tab),
	Reverse(Tab),
	Normalize(Tab),

	OnDrag(f32),
	OnDragEnd,
	OnDoubleClick,
}

#[derive(Debug)]
pub struct ArrangementView {
	pub arrangement: Arrangement,

	playlist: RefCell<playlist::State>,
	piano_roll: RefCell<piano_roll::State>,
	midi_clip_id: Option<(usize, usize)>,

	selected: NodeId,
	loading: usize,

	history: history::History,
}

impl ArrangementView {
	pub fn new(
		mut arrangement: Arrangement,
		state: &State,
		view: Option<proto::ViewState>,
	) -> Self {
		let view = view.unwrap_or(proto::ViewState {
			playlist: proto::TabState {
				position: proto::Vector { x: 0.0, y: 0.0 },
				scale: proto::Vector { x: -5.0, y: 85.0 },
			},
			piano_roll: proto::TabState {
				position: proto::Vector {
					x: 0.0,
					y: 43.0 * LINE_HEIGHT,
				},
				scale: proto::Vector {
					x: -7.0,
					y: LINE_HEIGHT,
				},
			},
		});

		if state.metronome {
			arrangement.toggle_metronome();
		}

		let selected = arrangement.master().id;

		Self {
			arrangement,

			playlist: RefCell::new(playlist::State::new(
				Vector::new(view.playlist.position.x, view.playlist.position.y),
				Vector::new(view.playlist.scale.x, view.playlist.scale.y),
			)),
			piano_roll: RefCell::new(piano_roll::State::new(
				Vector::new(view.piano_roll.position.x, view.piano_roll.position.y),
				Vector::new(view.piano_roll.scale.x, view.piano_roll.scale.y),
			)),
			midi_clip_id: None,

			selected,
			loading: 0,

			history: history::History::default(),
		}
	}

	pub fn update(
		&mut self,
		message: Message,
		config: &Config,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		let started = Instant::now();
		let ends_gesture = Self::ends_gesture(&message);

		self.history.begin();
		let action = self.update_inner(message, config, state);
		let commit = ends_gesture || !self.is_dragging();
		self.history.end(&mut self.arrangement, commit);

		let elapsed = started.elapsed();
		if elapsed >= Duration::from_millis(8) {
			log::warn!("arrangement UI update took {elapsed:?}");
		}
		action
	}

	/// Whether a message is a separate edit, even if the mouse is held down.
	fn ends_gesture(message: &Message) -> bool {
		!matches!(
			message,
			Message::Batch(_)
				| Message::DrainQueue
				| Message::RequestUpdate
				| Message::ChangeConfig(_)
				| Message::PlaylistAction(_)
				| Message::PianoRollAction(_)
		)
	}

	/// Whether the user is in the middle of a mouse gesture.
	fn is_dragging(&self) -> bool {
		self.playlist.borrow().status != playlist::Status::None
			|| self.piano_roll.borrow().status != piano_roll::Status::None
	}

	fn update_inner(
		&mut self,
		message: Message,
		config: &Config,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		match message {
			Message::ChangeConfig(NoClone(NoDebug(processor))) => {
				self.arrangement.change_config(*processor, config);
			}
			Message::Batch(NoClone(msg)) => {
				let before = self.arrangement.transport().position;
				self.arrangement.update(*msg);
				let after = self.arrangement.transport().position;
				self.autoscroll(before, after, config, state);
			}
			Message::DrainQueue => _ = self.arrangement.drain_queue(),
			Message::RequestUpdate => self.arrangement.request_update(),
			Message::Connect(from, to) => self.arrangement.connect(from, to),
			Message::SetMix(from, to, mix) => self.arrangement.set_mix(from, to, mix),
			Message::Disconnect(from, to) => self.arrangement.disconnect(from, to),
			Message::ToggleKind(node) => {
				// Turning a track into a channel throws its clips away.
				if self.arrangement.node(node).ty == NodeType::Track {
					self.history.clear(&mut self.arrangement);
				}

				if let Some(track) = self.arrangement.toggle_kind(node) {
					self.update_selection(|c| update_selection_delete_track(c, track));
				}
			}
			Message::AudioPreview(sample) => self.arrangement.set_audio_preview(Some(sample)),
			Message::ChannelAdd => {
				self.selected = self.arrangement.add_channel();
				return Action::batch([
					self.update(
						Message::Connect(self.selected, self.arrangement.master().id),
						config,
						state,
					),
					snap_to_end("mixer").into(),
				]);
			}
			Message::ChannelInsert(node) => {
				let node = self
					.arrangement
					.insert_channel(self.arrangement.channel_of(node).unwrap() + 1);
				return Action::batch([
					self.update(
						Message::Connect(node, self.arrangement.master().id),
						config,
						state,
					),
					self.update(Message::ChannelSelect(node), config, state),
				]);
			}
			Message::ChannelRemove(node) => {
				if self.selected == node {
					self.select_next();
					if self.selected == node {
						self.select_prev();
					}
				}

				self.arrangement.remove_channel(node);
			}
			Message::ChannelMove(event) => {
				if let DragEvent::Dropped {
					index,
					target_index,
				} = event && index != target_index
				{
					self.arrangement.move_channel(index, target_index);
				}
			}
			Message::ChannelSelect(node) => {
				self.selected = node;
				return scroll_into_view(
					"mixer",
					self.arrangement.node(self.selected).widget_id.clone(),
				)
				.into();
			}
			Message::ChannelDuplicate(node) => {
				let (node, instructions) = self.arrangement.duplicate_channel(node);
				return Action::batch(
					instructions
						.into_iter()
						.map(Action::instruction)
						.chain([self.update(Message::ChannelSelect(node), config, state)]),
				);
			}
			Message::ChannelNameChanged(node, name) => {
				self.arrangement.channel_name_changed(node, name);
			}
			Message::ChannelRename(node) => {
				let id = self.arrangement.node(node).name_id.clone();
				return focus(id).into();
			}
			Message::ToggleGrid => {
				state.grid.enabled ^= true;
				state.write();
			}
			Message::NarrowedGrid => {
				return Action::instruction(daw::Instruction::Message(
					daw::Message::ChangedGridSize(state.grid.size - 1.0),
				));
			}
			Message::WidenedGrid => {
				return Action::instruction(daw::Instruction::Message(
					daw::Message::ChangedGridSize(state.grid.size + 1.0),
				));
			}
			Message::ToggleGridTriplets => {
				state.grid.triplets ^= true;
				state.write();
			}
			Message::SetLoopMarkerStart => {
				let transport = self.arrangement.transport();
				let pos = transport.position.to_beat_time(transport);
				let range = transport.loop_range.map_or_else(
					|| BeatRange::new(pos, pos + BeatTime::BEAT * 4.0),
					|range| BeatRange::new(pos, range.end()),
				);

				self.arrangement.set_loop_range(Some(range));
			}
			Message::SetLoopMarkerEnd => {
				let transport = self.arrangement.transport();
				let pos = transport.position.to_beat_time(transport);
				let range = transport.loop_range.map_or_else(
					|| BeatRange::new(pos - BeatTime::BEAT * 4.0, pos),
					|range| BeatRange::new(range.start(), pos),
				);

				self.arrangement.set_loop_range(Some(range));
			}
			Message::ChannelVolumeChanged(node, mut volume) => {
				let db = amp_to_db(volume.abs());
				let nearest = (db / 6.0).round() * 6.0;
				if (db - nearest).abs() < 0.15 {
					volume = db_to_amp(nearest).copysign(volume);
				} else {
					let nearest = (db / 3.0).round() * 3.0;
					if (db - nearest).abs() < 0.05 {
						volume = db_to_amp(nearest).copysign(volume);
					}
				}

				self.arrangement.channel_volume_changed(node, volume);
			}
			Message::ChannelPanChanged(node, mut pan) => {
				let snap = |pan: &mut f32| {
					if pan.abs() < 0.015 {
						*pan = 0.0;
					}
				};

				match &mut pan {
					PanMode::Stereo(pan) => snap(pan),
					PanMode::SplitStereo(l, r) => {
						snap(l);
						snap(r);
					}
				}

				self.arrangement.channel_pan_changed(node, pan);
			}
			Message::ChannelToggleEnabled(node) => self.arrangement.channel_toggle_enabled(node),
			Message::ChannelToggleBypassed(node) => self.arrangement.channel_toggle_bypassed(node),
			Message::PluginPicker => {
				return Action::instruction(daw::Instruction::Message(
					daw::Message::TogglePluginPicker,
				));
			}
			Message::PluginAdd(descriptor) => {
				return self.update(
					Message::PluginAddTo(self.selected, descriptor, true),
					config,
					state,
				);
			}
			Message::PluginAddTo(node, descriptor, show) => {
				if let Some((plugin, instruction)) = self.arrangement.plugin_add(node, *descriptor)
				{
					let mut action = Action::instruction(instruction);
					if show {
						let (activate, open) = {
							let plugin = self
								.arrangement
								.node(node)
								.plugins
								.iter()
								.find(|candidate| candidate.id == plugin)
								.unwrap();

							(plugin.activate_message(), plugin.gui_open_message())
						};

						action = Action::batch([
							action,
							Action::instruction(daw::Instruction::Message(activate)),
							Action::instruction(daw::Instruction::Message(open)),
						]);
					}
					return action;
				}
			}
			Message::PluginRemove(node, i) => {
				let plugin = self.arrangement.plugin_remove(node, i);
				if !plugin.active {
					return Action::instruction(daw::Instruction::Message(
						plugin.destroy_inactive_message(),
					));
				}
			}
			Message::PluginMove(node, event) => {
				if let DragEvent::Dropped {
					index,
					target_index,
				} = event && index != target_index
				{
					self.arrangement.plugin_move(node, index, target_index);
				}
			}
			Message::PluginDuplicate(node, i) => {
				if let Some(instructions) = self.arrangement.plugin_duplicate(node, i) {
					return Action::batch(instructions.into_iter().map(Action::instruction));
				}
			}
			Message::PluginSetState(node, i, state) => {
				let plugin = &self.arrangement.node(node).plugins[i];
				return Action::instruction(daw::Instruction::Message(
					plugin.set_state_message(state.0),
				));
			}
			Message::PluginCopyState(node, i) => {
				let plugin = &self.arrangement.node(node).plugins[i];
				return Action::instruction(daw::Instruction::Message(
					daw::Message::PluginCopyState(plugin.id),
				));
			}
			Message::PluginPasteState(node, i) => {
				let plugin = &self.arrangement.node(node).plugins[i];
				return Action::instruction(daw::Instruction::Message(
					daw::Message::PluginPasteState(plugin.id),
				));
			}
			Message::PluginShow(plugin) => {
				if let Some((node, i)) = self.arrangement.plugin_of(plugin) {
					return Action::instruction(daw::Instruction::Message(
						self.arrangement.node(node).plugins[i].gui_open_message(),
					));
				}
			}
			Message::PluginMixChanged(node, i, mut mix) => {
				if (mix.abs() - 0.5).abs() < 0.005 {
					mix = 0.5f32.copysign(mix);
				}

				self.arrangement.plugin_mix_changed(node, i, mix);
			}
			Message::PluginToggleActive(node, i) => {
				let plugin = &self.arrangement.node(node).plugins[i];
				if plugin.active {
					self.arrangement.plugin_deactivate(node, i);
				} else {
					return Action::instruction(daw::Instruction::Message(
						plugin.activate_message(),
					));
				}
			}
			Message::SampleLoaded(loaded) => {
				self.loading -= 1;
				if let Some(sample) = loaded {
					self.arrangement.add_sample(*sample);
				}
			}
			Message::MidiPatternLoaded(loaded) => {
				self.loading -= 1;
				if let Some(pattern) = loaded {
					self.arrangement.add_midi_pattern(*pattern);
				}
			}
			Message::AudioClipAdd(id, track, pos) => {
				let (track, task) = if let Some(track) = track
					&& track < self.arrangement.tracks().len()
				{
					(track, Action::none())
				} else {
					let track = self.arrangement.tracks().len();
					(track, self.update(Message::TrackAdd, config, state))
				};
				let clip = self.arrangement.add_audio_clip_from_sample(track, pos, id);
				history::record_clip_add(&mut self.arrangement, &mut self.history, track, clip);
				self.playlist.get_mut().primary.insert((track, clip));
				return task;
			}
			Message::MidiClipAdd(id, track, pos) => {
				let (track, task) = if let Some(track) = track
					&& track < self.arrangement.tracks().len()
				{
					(track, Action::none())
				} else {
					let track = self.arrangement.tracks().len();
					(track, self.update(Message::TrackAdd, config, state))
				};
				let clip = self.arrangement.add_midi_clip_from_pattern(track, pos, id);
				history::record_clip_add(&mut self.arrangement, &mut self.history, track, clip);
				self.playlist.get_mut().primary.insert((track, clip));
				return task;
			}
			Message::TrackAdd => {
				self.selected = self.arrangement.add_track();
				return Action::batch([
					self.update(
						Message::Connect(self.selected, self.arrangement.master().id),
						config,
						state,
					),
					scroll_into_view(
						"mixer",
						self.arrangement.node(self.selected).widget_id.clone(),
					)
					.into(),
				]);
			}
			Message::TrackInsert(node) => {
				let node = self
					.arrangement
					.insert_track(self.arrangement.track_of(node).unwrap() + 1);
				return Action::batch([
					self.update(
						Message::Connect(node, self.arrangement.master().id),
						config,
						state,
					),
					self.update(Message::ChannelSelect(node), config, state),
				]);
			}
			Message::TrackRemove(node) => {
				if self.selected == node {
					self.select_next();
					if self.selected == node {
						self.select_prev();
					}
				}

				// Undo steps can point at clips of the track, so they can't stay.
				self.history.clear(&mut self.arrangement);

				let track = self.arrangement.remove_track(node);
				self.update_selection(|c| update_selection_delete_track(c, track));
			}
			Message::TrackMove(event) => {
				if let DragEvent::Dropped {
					index,
					target_index,
				} = event && index != target_index
				{
					self.arrangement.move_track(index, target_index);
					self.update_selection(|c| {
						Some(update_selection_move_track(c, index, target_index))
					});
				}
			}
			Message::TrackDuplicate(node) => {
				let (node, instructions) = self.arrangement.duplicate_track(node);
				return Action::batch(
					instructions
						.into_iter()
						.map(Action::instruction)
						.chain([self.update(Message::ChannelSelect(node), config, state)]),
				);
			}
			Message::TrackToggleEnabled(node) => self.arrangement.track_toggle_enabled(node),
			Message::TrackToggleSolo(node) => self.arrangement.track_toggle_solo(node),
			Message::SeekTo(pos) => self.arrangement.seek_to(pos),
			Message::SetLoopMarker(marker) => self.arrangement.set_loop_range(marker),
			Message::InputChangeChannels(node, channels) => {
				self.arrangement.input_change_channels(node, channels);
			}
			Message::InputToggleAudioRecording(node) => {
				self.arrangement.input_toggle_audio_recording(node);
			}
			Message::InputToggleMidiRecording(node) => {
				self.arrangement.input_toggle_midi_recording(node);
			}
			Message::OutputChangeChannels(node, channels) => {
				self.arrangement.output_change_channels(node, channels);
			}
			Message::Freeze(node) => return Action::instruction(daw::Instruction::Freeze(node)),
			Message::FreezeDone(node, sample, pos) => {
				let id = sample.gui.id;
				self.arrangement.add_sample(*sample);

				let track = self.arrangement.track_of(node).unwrap() + 1;
				let track_id = self.arrangement.insert_track(track);

				self.update_selection(|c| Some(update_selection_insert_track(c, track)));

				let clip = self.arrangement.add_audio_clip_from_sample(track, pos, id);
				history::record_clip_add(&mut self.arrangement, &mut self.history, track, clip);
				self.playlist.get_mut().primary.insert((track, clip));

				for (outgoing, mix) in self.arrangement.node(node).outgoing.clone() {
					self.arrangement.connect(track_id, outgoing);
					self.arrangement.set_mix(track_id, outgoing, mix);
				}

				if self.arrangement.solo().is_some() {
					self.arrangement.track_toggle_solo(track_id);
				} else {
					self.arrangement.channel_toggle_enabled(node);
				}
			}
			Message::PlaylistAction(action) => {
				return self.handle_playlist_action(action, config, state);
			}
			Message::ClipAction(track, clip, action) => {
				return self.handle_clip_action(track, clip, action, config, state);
			}
			Message::PianoRollAction(action) => self.handle_piano_roll_action(action),
			Message::ArrowUp(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					return self.handle_playlist_action(
						playlist::Action::Drag(Delta::Negative(1), Delta::Positive(BeatTime::ZERO)),
						config,
						state,
					);
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Positive(MidiKey(1)),
						Delta::Positive(BeatTime::ZERO),
					));
				}
			},
			Message::ArrowDown(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					return self.handle_playlist_action(
						playlist::Action::Drag(Delta::Positive(1), Delta::Positive(BeatTime::ZERO)),
						config,
						state,
					);
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Negative(MidiKey(1)),
						Delta::Positive(BeatTime::ZERO),
					));
				}
			},
			Message::ArrowLeft(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					let snap_step = state.grid.beats_snap_step(
						self.playlist.get_mut().scale,
						self.arrangement.transport(),
					);
					return self.handle_playlist_action(
						playlist::Action::Drag(Delta::Positive(0), Delta::Negative(snap_step)),
						config,
						state,
					);
				}
				Tab::Mixer => {
					self.select_prev();
					return scroll_into_view(
						"mixer",
						self.arrangement.node(self.selected).widget_id.clone(),
					)
					.into();
				}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					let snap_step = state.grid.beats_snap_step(
						self.playlist.get_mut().scale,
						self.arrangement.transport(),
					);
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Positive(MidiKey(0)),
						Delta::Negative(snap_step),
					));
				}
			},
			Message::ArrowRight(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					let snap_step = state.grid.beats_snap_step(
						self.playlist.get_mut().scale,
						self.arrangement.transport(),
					);
					return self.handle_playlist_action(
						playlist::Action::Drag(Delta::Positive(0), Delta::Positive(snap_step)),
						config,
						state,
					);
				}
				Tab::Mixer => {
					self.select_next();
					return scroll_into_view(
						"mixer",
						self.arrangement.node(self.selected).widget_id.clone(),
					)
					.into();
				}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					let snap_step = state.grid.beats_snap_step(
						self.playlist.get_mut().scale,
						self.arrangement.transport(),
					);
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Positive(MidiKey(0)),
						Delta::Positive(snap_step),
					));
				}
			},
			Message::TransposeOctUp(tab) => match tab {
				Tab::Playlist | Tab::Mixer => {}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Positive(MidiKey(12)),
						Delta::Positive(BeatTime::ZERO),
					));
				}
			},
			Message::TransposeOctDown(tab) => match tab {
				Tab::Playlist | Tab::Mixer => {}
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					self.handle_piano_roll_action(piano_roll::Action::Drag(
						Delta::Negative(MidiKey(12)),
						Delta::Positive(BeatTime::ZERO),
					));
				}
			},
			Message::Quantize(tab) => match tab {
				Tab::Playlist => {
					let playlist = self.playlist.get_mut();
					playlist.finish();
					let snap_step = state
						.grid
						.beats_snap_step(playlist.scale, self.arrangement.transport());
					let clips = playlist.primary.iter().copied().collect::<Vec<_>>();
					for (track, clip) in clips {
						let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
						let pos = self.arrangement.tracks()[track].clips[clip]
							.start()
							.round(snap_step);

						self.arrangement.clip_move_to(track, clip, pos);

						if let Some(before) = before {
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
					}
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					if let Some(clip) = self.midi_clip() {
						let before = self.arrangement.midi_patterns()[&clip.pattern]
							.notes
							.clone();
						let piano_roll = self.piano_roll.get_mut();
						piano_roll.finish();
						let snap_step = state
							.grid
							.beats_snap_step(piano_roll.scale, self.arrangement.transport());
						for &note in &piano_roll.primary {
							let pos = self.arrangement.midi_patterns()[&clip.pattern].notes[note]
								.position
								.round(snap_step);

							self.arrangement
								.note_trim_end_to(clip.pattern, note, pos.end());
							self.arrangement
								.note_trim_start_to(clip.pattern, note, pos.start());
						}

						history::record_pattern_changes(
							&mut self.arrangement,
							&mut self.history,
							clip.pattern,
							&before,
						);
					}
				}
			},
			Message::Join(tab) => match tab {
				Tab::Playlist | Tab::Mixer => {}
				Tab::PianoRoll => {
					if let Some(clip) = self.midi_clip() {
						let before = self.arrangement.midi_patterns()[&clip.pattern]
							.notes
							.clone();
						let piano_roll = self.piano_roll.get_mut();
						piano_roll.finish();
						let mut sorted = piano_roll.primary.drain().collect::<Vec<_>>();
						sorted.sort_unstable();

						let mut i = 0;
						'outer: while i < sorted.len() {
							let note = sorted[i];
							i += 1;

							let pattern = &self.arrangement.midi_patterns()[&clip.pattern];

							let mut j = i;
							while j < sorted.len() {
								let other = sorted[j];
								j += 1;

								if pattern.notes[note].key != pattern.notes[other].key {
									continue;
								}

								let note_pos = pattern.notes[note].position;
								let other_pos = pattern.notes[other].position;

								if note_pos.start().max(other_pos.start())
									> note_pos.end().min(other_pos.end())
								{
									continue;
								}

								self.arrangement.note_trim_start_to(
									clip.pattern,
									note,
									note_pos.start().min(other_pos.start()),
								);

								self.arrangement.note_trim_end_to(
									clip.pattern,
									note,
									note_pos.end().max(other_pos.end()),
								);

								self.arrangement.remove_note(clip.pattern, other);

								i -= 1;
								j -= 1;

								sorted.remove(j);
								for other in &mut sorted[j..] {
									*other -= 1;
								}

								continue 'outer;
							}
						}

						piano_roll.primary.extend(sorted);

						history::record_pattern_changes(
							&mut self.arrangement,
							&mut self.history,
							clip.pattern,
							&before,
						);
					}
				}
			},
			Message::SelectAll(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					self.playlist.get_mut().primary.extend(
						self.arrangement
							.tracks()
							.iter()
							.enumerate()
							.flat_map(|(t, track)| repeat(t).zip(0..track.clips.len())),
					);
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					if let Some(clip) = self.midi_clip() {
						self.piano_roll.get_mut().finish();
						self.piano_roll
							.get_mut()
							.primary
							.extend(0..self.arrangement.midi_patterns()[&clip.pattern].notes.len());
					}
				}
			},
			Message::SelectInverse(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					for (t, track) in self.arrangement.tracks().iter().enumerate() {
						for c in 0..track.clips.len() {
							if !self.playlist.get_mut().primary.insert((t, c)) {
								self.playlist.get_mut().primary.remove(&(t, c));
							}
						}
					}
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					if let Some(clip) = self.midi_clip() {
						self.piano_roll.get_mut().finish();
						for note in 0..self.arrangement.midi_patterns()[&clip.pattern].notes.len() {
							if !self.piano_roll.get_mut().primary.insert(note) {
								self.piano_roll.get_mut().primary.remove(&note);
							}
						}
					}
				}
			},
			Message::Escape(tab) => {
				self.arrangement.set_audio_preview(None);
				self.clear(tab);
			}
			Message::ToggleEnabled(tab) => match tab {
				Tab::Playlist => {
					if self.arrangement.node(self.selected).ty == NodeType::Track {
						return self.update(
							Message::TrackToggleEnabled(self.selected),
							config,
							state,
						);
					}
				}
				Tab::Mixer => match self.arrangement.node(self.selected).ty {
					NodeType::Master | NodeType::Channel => {
						return self.update(
							Message::ChannelToggleEnabled(self.selected),
							config,
							state,
						);
					}
					NodeType::Track => {
						return self.update(
							Message::TrackToggleEnabled(self.selected),
							config,
							state,
						);
					}
				},
				Tab::PianoRoll => {}
			},
			Message::ToggleSolo(tab) => match tab {
				Tab::Playlist | Tab::Mixer => match self.arrangement.node(self.selected).ty {
					NodeType::Master | NodeType::Channel => {}
					NodeType::Track => {
						return self.update(Message::TrackToggleSolo(self.selected), config, state);
					}
				},
				Tab::PianoRoll => {}
			},
			Message::Duplicate(tab) => match tab {
				Tab::Playlist => {
					if let Some(delta) = self
						.playlist
						.get_mut()
						.primary
						.iter()
						.map(|&(track, clip)| self.arrangement.tracks()[track].clips[clip])
						.map(|clip| (clip.start(), clip.end(self.arrangement.transport())))
						.reduce(|old, new| (old.0.min(new.0), old.1.max(new.1)))
					{
						self.playlist.get_mut().finish();
						return Action::batch([
							self.handle_playlist_action(playlist::Action::Clone, config, state),
							self.handle_playlist_action(
								playlist::Action::Drag(
									Delta::Positive(0),
									Delta::Positive(delta.1 - delta.0),
								),
								config,
								state,
							),
						]);
					} else if self.arrangement.node(self.selected).ty == NodeType::Track {
						return self.update(Message::TrackDuplicate(self.selected), config, state);
					}
				}
				Tab::Mixer => match self.arrangement.node(self.selected).ty {
					NodeType::Master => {}
					NodeType::Channel => {
						return self.update(
							Message::ChannelDuplicate(self.selected),
							config,
							state,
						);
					}
					NodeType::Track => {
						return self.update(Message::TrackDuplicate(self.selected), config, state);
					}
				},
				Tab::PianoRoll => {
					if let Some(clip) = self.midi_clip()
						&& let Some(delta) = self
							.piano_roll
							.get_mut()
							.primary
							.iter()
							.map(|&note| {
								self.arrangement.midi_patterns()[&clip.pattern].notes[note].position
							})
							.reduce(|old, new| {
								BeatRange::new(
									old.start().min(new.start()),
									old.end().max(new.end()),
								)
							}) {
						self.piano_roll.get_mut().finish();
						self.handle_piano_roll_action(piano_roll::Action::Clone);
						self.handle_piano_roll_action(piano_roll::Action::Drag(
							Delta::Positive(MidiKey(0)),
							Delta::Positive(delta.len()),
						));
					}
				}
			},
			Message::MakeUnique(tab) => match tab {
				Tab::Playlist => {
					let clips = {
						let playlist = self.playlist.get_mut();
						playlist.finish();
						playlist.primary.iter().copied().collect::<Vec<_>>()
					};

					for (track, clip) in clips {
						let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
						if self.arrangement.make_clip_unique(track, clip)
							&& let Some(before) = before
						{
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
					}
				}
				Tab::Mixer => {}
				Tab::PianoRoll => {
					if let Some((track, clip)) = self.midi_clip_id {
						let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
						if self.arrangement.make_clip_unique(track, clip)
							&& let Some(before) = before
						{
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
					}
				}
			},
			Message::Delete(tab) => match tab {
				Tab::Playlist => {
					self.playlist.get_mut().finish();
					if !self.playlist.get_mut().primary.is_empty() {
						return self.handle_playlist_action(
							playlist::Action::Delete,
							config,
							state,
						);
					} else if self.arrangement.node(self.selected).ty == NodeType::Track {
						return self.update(Message::TrackRemove(self.selected), config, state);
					}
				}
				Tab::Mixer => match self.arrangement.node(self.selected).ty {
					NodeType::Master => {}
					NodeType::Channel => {
						return self.update(Message::ChannelRemove(self.selected), config, state);
					}
					NodeType::Track => {
						return self.update(Message::TrackRemove(self.selected), config, state);
					}
				},
				Tab::PianoRoll => {
					self.piano_roll.get_mut().finish();
					self.handle_piano_roll_action(piano_roll::Action::Delete);
				}
			},
			Message::Invert(tab) => match tab {
				Tab::Playlist => {
					let playlist = self.playlist.get_mut();
					playlist.finish();
					return self.handle_playlist_action(
						playlist::Action::InvertPolarity,
						config,
						state,
					);
				}
				Tab::Mixer => {
					let node = self.arrangement.node(self.selected);
					return self.update(
						Message::ChannelVolumeChanged(self.selected, -node.utility.volume),
						config,
						state,
					);
				}
				Tab::PianoRoll => {}
			},
			Message::Reverse(tab) => match tab {
				Tab::Playlist => {
					let playlist = self.playlist.get_mut();
					playlist.finish();
					let clips = playlist.primary.iter().copied().collect::<Vec<_>>();
					for (track, clip) in clips {
						let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
						self.arrangement.clip_reverse(track, clip);
						if let Some(before) = before {
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
					}
				}
				Tab::Mixer | Tab::PianoRoll => {}
			},
			Message::Normalize(tab) => match tab {
				Tab::Playlist => {
					let playlist = self.playlist.get_mut();
					playlist.finish();
					let clips = playlist.primary.iter().copied().collect::<Vec<_>>();
					for (track, clip) in clips {
						let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
						self.arrangement.clip_normalize(track, clip);
						if let Some(before) = before {
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
					}
				}
				Tab::Mixer | Tab::PianoRoll => {}
			},
			Message::OnDrag(split_at) => state.plugins_pane_split_at = split_at,
			Message::OnDragEnd => state.write(),
			Message::OnDoubleClick => {
				return Action::batch([
					self.update(Message::OnDrag(DEFAULT_SPLIT_WIDTH), config, state),
					self.update(Message::OnDragEnd, config, state),
				]);
			}
		}

		Action::none()
	}

	fn handle_playlist_action(
		&mut self,
		action: playlist::Action,
		config: &Config,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		let playlist::State {
			primary,
			secondary,
			position,
			scale,
			..
		} = self.playlist.get_mut();

		match action {
			playlist::Action::Pan(pos_diff, visible) => {
				let old_position = *position;
				*position += pos_diff;
				position.x = position.x.max(0.0);
				position.y = position
					.y
					.clamp(0.0, 0f32.max(old_position.y + visible - scale.y));
			}
			playlist::Action::Zoom(scale_diff, cursor, visible) => {
				let old_scale = *scale;
				*scale += scale_diff;
				scale.x = scale.x.clamp(-15.0, 0.0);
				scale.y = scale.y.clamp(25.0, 165.0);

				let pos_diff = Vector::new(
					(cursor.x + position.x) * ((old_scale.x - scale.x).exp2() - 1.0),
					(cursor.y + position.y) * ((scale.y / old_scale.y) - 1.0),
				);

				return self.handle_playlist_action(
					playlist::Action::Pan(pos_diff, visible),
					config,
					state,
				);
			}
			playlist::Action::Add(info, track, pos) => {
				return if let Some((path, kind)) = info {
					if kind == FileKind::Midi {
						self.loading += 1;
						let transport = *self.arrangement.transport();
						Task::future(unblock(move || {
							MidiPatternPair::from_midi(&path, &transport)
						}))
						.then(move |pattern| {
							if let Some(pattern) = pattern {
								let id = pattern.gui.id;
								Task::done(Message::MidiPatternLoaded(Some(Box::new(pattern))))
									.chain(Task::done(Message::MidiClipAdd(id, track, pos)))
							} else {
								Task::done(Message::MidiPatternLoaded(None))
							}
						})
						.into()
					} else if let Some(sample) = self
						.arrangement
						.samples()
						.values()
						.find(|sample| sample.path == path)
					{
						self.update(Message::AudioClipAdd(sample.id, track, pos), config, state)
					} else {
						self.loading += 1;
						Task::future(unblock(|| SamplePair::new(path)))
							.then(move |sample| {
								if let Some(sample) = sample {
									let id = sample.gui.id;
									Task::done(Message::SampleLoaded(Some(Box::new(sample))))
										.chain(Task::done(Message::AudioClipAdd(id, track, pos)))
								} else {
									Task::done(Message::SampleLoaded(None))
								}
							})
							.into()
					}
				} else {
					self.loading += 1;
					let pattern = MidiPatternPair::from_notes(Vec::new(), "MIDI Pattern");
					let id = pattern.gui.id;
					Action::batch([
						self.update(
							Message::MidiPatternLoaded(Some(Box::new(pattern))),
							config,
							state,
						),
						self.update(Message::MidiClipAdd(id, track, pos), config, state),
					])
				};
			}
			playlist::Action::Open(track, clip) => {
				if self.midi_clip_id != Some((track, clip)) {
					self.midi_clip_id = Some((track, clip));
					self.piano_roll.get_mut().clear();
				}
				return Action::instruction(daw::Instruction::Message(daw::Message::TopPane(
					Tab::PianoRoll,
				)));
			}
			playlist::Action::Clone => {
				let mut sorted = primary.drain().collect::<Vec<_>>();
				sorted.sort_unstable_by_key(|&(_, c)| c);
				for (track, clip) in sorted {
					let new_clip = self.arrangement.duplicate_clip(track, clip);
					history::record_clip_add(
						&mut self.arrangement,
						&mut self.history,
						track,
						new_clip,
					);
					primary.insert((track, new_clip));
				}
			}
			playlist::Action::Drag(track_diff, pos_diff) => {
				let mut sorted = primary.iter().copied().collect::<Vec<_>>();
				match track_diff {
					Delta::Positive(0) | Delta::Negative(0) => {}
					Delta::Negative(..) => sorted.sort_unstable_by_key(|&(t, c)| (t, Reverse(c))),
					Delta::Positive(..) => {
						sorted.sort_unstable_by_key(|&(t, c)| (Reverse(t), Reverse(c)));
					}
				}

				for (track, clip) in sorted {
					let before = history::ClipSnapshot::of(&self.arrangement, track, clip);
					let pos = self.arrangement.tracks()[track].clips[clip].start();
					self.arrangement.clip_move_to(track, clip, pos + pos_diff);

					let new_track = (track + track_diff).min(self.arrangement.tracks().len() - 1);
					let new_clip = self.arrangement.clip_switch_track(track, clip, new_track);

					if let Some(before) = before {
						history::record_clip_change(
							&mut self.arrangement,
							&mut self.history,
							before,
						);
					}

					self.update_selection(|c| {
						Some(update_selection_move_clip(
							c, track, clip, new_track, new_clip,
						))
					});
				}
			}
			playlist::Action::TrimStart(pos_diff) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						let pos = arrangement.tracks()[track].clips[clip].start();
						arrangement.clip_trim_start_to(track, clip, pos + pos_diff);
					},
				);
			}
			playlist::Action::TrimEnd(pos_diff) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						let pos =
							arrangement.tracks()[track].clips[clip].end(arrangement.transport());
						arrangement.clip_trim_end_to(track, clip, pos + pos_diff);
					},
				);
			}
			playlist::Action::InvertPolarity => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						if let generic_daw_core::Clip::Audio(audio) =
							&arrangement.tracks()[track].clips[clip]
						{
							arrangement.clip_volume_changed(track, clip, -audio.volume);
						}
					},
				);
			}
			playlist::Action::DragVolume(vol_diff) => {
				let vol_diff = db_to_amp(vol_diff);
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						if let generic_daw_core::Clip::Audio(audio) =
							&arrangement.tracks()[track].clips[clip]
						{
							arrangement.clip_volume_changed(track, clip, audio.volume * vol_diff);
						}
					},
				);
			}
			playlist::Action::FadeStartLen(pos_diff) => {
				let pos_diff =
					pos_diff.map(|pos_diff| pos_diff.to_seconds_time(self.arrangement.transport()));
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						if let generic_daw_core::Clip::Audio(audio) =
							&arrangement.tracks()[track].clips[clip]
						{
							arrangement.clip_fade_start_len(
								track,
								clip,
								audio.fade_start.len + pos_diff,
							);
						}
					},
				);
			}
			playlist::Action::FadeStartP(p) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						arrangement.clip_fade_start_p(track, clip, Point { x: p.x, y: p.y });
					},
				);
			}
			playlist::Action::FadeStartToggleSymmetric => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						arrangement.clip_fade_start_toggle_symmetric(track, clip);
					},
				);
			}
			playlist::Action::FadeEndLen(pos_diff) => {
				let pos_diff =
					pos_diff.map(|pos_diff| pos_diff.to_seconds_time(self.arrangement.transport()));
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						if let generic_daw_core::Clip::Audio(audio) =
							&arrangement.tracks()[track].clips[clip]
						{
							arrangement.clip_fade_end_len(
								track,
								clip,
								audio.fade_end.len + pos_diff,
							);
						}
					},
				);
			}
			playlist::Action::FadeEndP(p) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						arrangement.clip_fade_end_p(track, clip, Point { x: p.x, y: p.y });
					},
				);
			}
			playlist::Action::FadeEndToggleSymmetric => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						arrangement.clip_fade_end_toggle_symmetric(track, clip);
					},
				);
			}
			playlist::Action::StretchStart(pos_diff) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						let pos = arrangement.tracks()[track].clips[clip].start();
						arrangement.clip_stretch_start_to(track, clip, pos + pos_diff);
					},
				);
			}
			playlist::Action::StretchEnd(pos_diff) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						let pos =
							arrangement.tracks()[track].clips[clip].end(arrangement.transport());
						arrangement.clip_stretch_end_to(track, clip, pos + pos_diff);
					},
				);
			}
			playlist::Action::SplitAt(mut pos) => {
				let mut sorted = primary
					.drain()
					.filter(|&(track, clip)| {
						(self.arrangement.tracks()[track].clips[clip].start()
							..=self.arrangement.tracks()[track].clips[clip]
								.end(self.arrangement.transport()))
							.contains(&pos)
					})
					.collect::<Vec<_>>();
				sorted.sort_unstable_by_key(|&(_, c)| c);

				for (track, lhs) in sorted {
					let before = history::ClipSnapshot::of(&self.arrangement, track, lhs);
					let clip = self.arrangement.tracks()[track].clips[lhs];
					if clip.end(self.arrangement.transport()) == pos {
						primary.insert((track, lhs));
					} else if clip.start() == pos {
						secondary.insert((track, lhs));
					} else if clip.len(self.arrangement.transport()) > BeatTime::TICK {
						let start = clip.start() + BeatTime::TICK;
						let end = clip.end(self.arrangement.transport()) - BeatTime::TICK;
						pos = pos.clamp(start, end);
						let rhs = self.arrangement.duplicate_clip(track, lhs);
						self.arrangement
							.clip_fade_end_len(track, lhs, SecondsTime::ZERO);
						self.arrangement.clip_trim_end_to(track, lhs, pos);
						self.arrangement
							.clip_fade_start_len(track, rhs, SecondsTime::ZERO);
						self.arrangement.clip_trim_start_to(track, rhs, pos);
						history::record_clip_add(
							&mut self.arrangement,
							&mut self.history,
							track,
							rhs,
						);
						if let Some(before) = before {
							history::record_clip_change(
								&mut self.arrangement,
								&mut self.history,
								before,
							);
						}
						primary.insert((track, lhs));
						secondary.insert((track, rhs));
					}
				}
			}
			playlist::Action::DragSplit(pos) => {
				let mut clamped = HashMap::new();

				for &(track, lhs) in &*primary {
					let new = self.arrangement.tracks()[track].clips[lhs].start() + BeatTime::TICK;

					clamped
						.entry(track)
						.and_modify(|old| *old = new.max(*old))
						.or_insert_with(|| new.max(pos));
				}

				for &(track, rhs) in &*secondary {
					let new = self.arrangement.tracks()[track].clips[rhs]
						.end(self.arrangement.transport())
						- BeatTime::TICK;

					clamped
						.entry(track)
						.and_modify(|old| *old = new.min(*old))
						.or_insert_with(|| new.min(pos));
				}

				let lhs = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					lhs,
					|arrangement, track, clip| {
						arrangement.clip_trim_end_to(track, clip, clamped[&track]);
					},
				);

				let rhs = secondary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					rhs,
					|arrangement, track, clip| {
						arrangement.clip_trim_start_to(track, clip, clamped[&track]);
					},
				);
			}
			playlist::Action::DragSlip(pos_diff) => {
				let clips = primary.iter().copied().collect::<Vec<_>>();
				edit_clips(
					&mut self.arrangement,
					&mut self.history,
					clips,
					|arrangement, track, clip| {
						let pos =
							arrangement.tracks()[track].clips[clip].offset(arrangement.transport());
						arrangement.clip_slip_to(track, clip, pos + pos_diff);
					},
				);
			}
			playlist::Action::Delete => {
				let mut sorted = primary.drain().collect::<Vec<_>>();
				sorted.sort_unstable_by_key(|&(_, c)| Reverse(c));
				for (track, clip) in sorted {
					self.update_selection(|c| update_selection_delete_clip(c, track, clip));

					let track_id = self.arrangement.tracks()[track].id;
					let removed = self.arrangement.detach_clip(track, clip);
					history::record_clip_remove(
						&mut self.arrangement,
						&mut self.history,
						track_id,
						clip,
						removed,
					);
				}
			}
			playlist::Action::Finished => {}
		}

		Action::none()
	}

	/// Does something to a clip of the playlist.
	fn handle_clip_action(
		&mut self,
		track: usize,
		clip: usize,
		action: playlist::ClipAction,
		config: &Config,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		if track >= self.arrangement.tracks().len()
			|| clip >= self.arrangement.tracks()[track].clips.len()
		{
			return Action::none();
		}

		// the clip the user picked is the one the action is about
		{
			let playlist = self.playlist.get_mut();
			playlist.primary.clear();
			playlist.primary.insert((track, clip));
		}

		match action {
			playlist::ClipAction::Duplicate => {
				self.handle_playlist_action(playlist::Action::Clone, config, state)
			}
			playlist::ClipAction::MakeUnique => {
				self.update(Message::MakeUnique(Tab::Playlist), config, state)
			}
			playlist::ClipAction::Delete => {
				self.handle_playlist_action(playlist::Action::Delete, config, state)
			}
			playlist::ClipAction::Reverse => {
				self.update(Message::Reverse(Tab::Playlist), config, state)
			}
			playlist::ClipAction::Normalize => {
				self.update(Message::Normalize(Tab::Playlist), config, state)
			}
			playlist::ClipAction::InvertPolarity => {
				self.update(Message::Invert(Tab::Playlist), config, state)
			}
			playlist::ClipAction::Split => {
				let transport = self.arrangement.transport();
				let pos = transport.position.to_beat_time(transport);

				self.handle_playlist_action(playlist::Action::SplitAt(pos), config, state)
			}
			playlist::ClipAction::OpenInPianoRoll => {
				self.handle_playlist_action(playlist::Action::Open(track, clip), config, state)
			}
			playlist::ClipAction::Quantize => {
				// the notes of the clip are quantized in the piano roll
				self.midi_clip_id = Some((track, clip));
				self.piano_roll.get_mut().clear();

				if let Some(clip) = self.midi_clip() {
					let notes = self.arrangement.midi_patterns()[&clip.pattern].notes.len();
					self.piano_roll.get_mut().primary.extend(0..notes);
				}

				self.update(Message::Quantize(Tab::PianoRoll), config, state)
			}
		}
	}

	fn handle_piano_roll_action(&mut self, action: piano_roll::Action) {
		let Some(clip) = self.midi_clip() else {
			return;
		};

		let piano_roll::State {
			primary,
			secondary,
			position,
			scale,
			..
		} = self.piano_roll.get_mut();

		match action {
			piano_roll::Action::Pan(pos_diff, height, visible) => {
				let old_position = *position;
				*position += pos_diff;
				position.x = position.x.max(0.0);
				position.y = position.y.clamp(0.0, old_position.y + visible - height);
			}
			piano_roll::Action::Zoom(scale_diff, cursor, height, visible) => {
				let old_scale = *scale;
				*scale += scale_diff;
				scale.x = scale.x.clamp(-15.0, 0.0);
				scale.y = scale.y.clamp(LINE_HEIGHT, 2.0 * LINE_HEIGHT);

				let pos_diff = Vector::new(
					(cursor.x + position.x) * ((old_scale.x - scale.x).exp2() - 1.0),
					(cursor.y + position.y) * ((scale.y / old_scale.y) - 1.0),
				);

				self.handle_piano_roll_action(piano_roll::Action::Pan(pos_diff, height, visible));
			}
			piano_roll::Action::Add(key, pos) => {
				let note = self.arrangement.add_note(
					clip.pattern,
					MidiNote {
						key,
						velocity: 1.0,
						position: BeatRange::new(pos, pos + BeatTime::BEAT),
						id: MidiNoteId::unique(),
					},
				);
				self.piano_roll.get_mut().primary.insert(note);
				history::record_note_add(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					note,
				);
				self.preview_midi_note(key, 1.0);
			}
			piano_roll::Action::Clone => {
				let mut sorted = primary.drain().collect::<Vec<_>>();
				sorted.sort_unstable();
				for note in sorted {
					let new_note = self.arrangement.duplicate_note(clip.pattern, note);
					history::record_note_add(
						&mut self.arrangement,
						&mut self.history,
						clip.pattern,
						new_note,
					);
					primary.insert(new_note);
				}
			}
			piano_roll::Action::Drag(key_diff, pos_diff) => {
				let mut preview = None;

				for &note in &*primary {
					let before = self.arrangement.midi_patterns()[&clip.pattern].notes[note];
					let pos = before.position.start();
					self.arrangement
						.note_move_to(clip.pattern, note, pos + pos_diff);

					let key = self.arrangement.midi_patterns()[&clip.pattern].notes[note].key;
					let velocity =
						self.arrangement.midi_patterns()[&clip.pattern].notes[note].velocity;
					let new_key = key + key_diff;

					self.arrangement
						.note_change_key(clip.pattern, note, new_key);

					history::record_note_change(
						&mut self.arrangement,
						&mut self.history,
						clip.pattern,
						before,
					);

					preview.get_or_insert((new_key, velocity));
				}

				// Only a new key is worth hearing, moving a note doesn't change its pitch.
				let key_changed = match key_diff {
					Delta::Positive(diff) | Delta::Negative(diff) => diff.0 != 0,
				};

				if key_changed && let Some((key, velocity)) = preview {
					self.preview_midi_note(key, velocity);
				}
			}
			piano_roll::Action::TrimStart(pos_diff) => {
				let notes = primary.iter().copied().collect::<Vec<_>>();
				edit_notes(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					notes,
					|arrangement, note| {
						let pos = arrangement.midi_patterns()[&clip.pattern].notes[note]
							.position
							.start();
						arrangement.note_trim_start_to(clip.pattern, note, pos + pos_diff);
					},
				);
			}
			piano_roll::Action::TrimEnd(pos_diff) => {
				let notes = primary.iter().copied().collect::<Vec<_>>();
				edit_notes(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					notes,
					|arrangement, note| {
						let pos = arrangement.midi_patterns()[&clip.pattern].notes[note]
							.position
							.end();
						arrangement.note_trim_end_to(clip.pattern, note, pos + pos_diff);
					},
				);
			}
			piano_roll::Action::SplitAt(mut pos) => {
				let mut sorted = primary
					.drain()
					.filter(|&lhs| {
						let note = self.arrangement.midi_patterns()[&clip.pattern].notes[lhs];
						(note.position.start()..=note.position.end()).contains(&pos)
					})
					.collect::<Vec<_>>();
				sorted.sort_unstable();

				for lhs in sorted {
					let before = self.arrangement.midi_patterns()[&clip.pattern].notes[lhs];
					let note = before;
					if note.position.end() == pos {
						primary.insert(lhs);
					} else if note.position.start() == pos {
						secondary.insert(lhs);
					} else if note.position.len() > BeatTime::TICK {
						let start = note.position.start() + BeatTime::TICK;
						let end = note.position.end() - BeatTime::TICK;
						pos = pos.clamp(start, end);
						let rhs = self.arrangement.duplicate_note(clip.pattern, lhs);
						self.arrangement.note_trim_end_to(clip.pattern, lhs, pos);
						self.arrangement.note_trim_start_to(clip.pattern, rhs, pos);
						history::record_note_add(
							&mut self.arrangement,
							&mut self.history,
							clip.pattern,
							rhs,
						);
						history::record_note_change(
							&mut self.arrangement,
							&mut self.history,
							clip.pattern,
							before,
						);
						primary.insert(lhs);
						secondary.insert(rhs);
					}
				}
			}
			piano_roll::Action::DragSplit(pos) => {
				let mut clamped = HashMap::new();

				for &lhs in &*primary {
					let note = self.arrangement.midi_patterns()[&clip.pattern].notes[lhs];
					let new = note.position.start() + BeatTime::TICK;

					clamped
						.entry(note.key)
						.and_modify(|old| *old = new.max(*old))
						.or_insert_with(|| new.max(pos));
				}

				for &rhs in &*secondary {
					let note = self.arrangement.midi_patterns()[&clip.pattern].notes[rhs];
					let new = note.position.end() - BeatTime::TICK;

					clamped
						.entry(note.key)
						.and_modify(|old| *old = new.min(*old))
						.or_insert_with(|| new.min(pos));
				}

				let notes = primary.iter().copied().collect::<Vec<_>>();
				edit_notes(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					notes,
					|arrangement, note| {
						let key = arrangement.midi_patterns()[&clip.pattern].notes[note].key;
						arrangement.note_trim_end_to(clip.pattern, note, clamped[&key]);
					},
				);

				let notes = secondary.iter().copied().collect::<Vec<_>>();
				edit_notes(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					notes,
					|arrangement, note| {
						let key = arrangement.midi_patterns()[&clip.pattern].notes[note].key;
						arrangement.note_trim_start_to(clip.pattern, note, clamped[&key]);
					},
				);
			}
			piano_roll::Action::DragVelocity(val) => {
				let notes = primary.iter().copied().collect::<Vec<_>>();
				edit_notes(
					&mut self.arrangement,
					&mut self.history,
					clip.pattern,
					notes,
					|arrangement, note| {
						arrangement.note_change_velocity(clip.pattern, note, val);
					},
				);
			}
			piano_roll::Action::Delete => {
				let mut sorted = primary.drain().collect::<Vec<_>>();
				sorted.sort_unstable_by_key(|&n| Reverse(n));
				for note in sorted {
					let removed = self.arrangement.midi_patterns()[&clip.pattern].notes[note];
					self.arrangement.remove_note(clip.pattern, note);
					history::record_note_remove(
						&mut self.arrangement,
						&mut self.history,
						clip.pattern,
						note,
						removed,
					);
				}
			}
			piano_roll::Action::Finished => {}
		}
	}

	pub fn view<'a>(&'a self, tab: Tab, state: &'a State) -> Element<'a, Message> {
		match tab {
			Tab::Playlist => self.view_playlist(state),
			Tab::Mixer => self.view_mixer(state),
			Tab::PianoRoll => self.view_piano_roll(state),
		}
	}

	/// The menu of the playlist, of the clip the user right-clicked on, or of the
	/// background.
	fn playlist_context_menu<'a>(&'a self, state: &'a State) -> Element<'a, Message> {
		match self.playlist.borrow().context_menu {
			Some(ContextTarget::AudioClip(track, clip)) => {
				Self::clip_context_menu(track, clip, false, state)
			}
			Some(ContextTarget::MidiClip(track, clip)) => {
				Self::clip_context_menu(track, clip, true, state)
			}
			Some(ContextTarget::EmptyTrack { .. } | ContextTarget::EmptyArea { .. }) | None => {
				Self::background_context_menu(Tab::Playlist, state)
			}
		}
	}

	/// The menu of a clip of the playlist.
	fn clip_context_menu(
		track: usize,
		clip: usize,
		midi: bool,
		state: &State,
	) -> Element<'_, Message> {
		let shortcuts = &state.shortcuts;

		let mut items = vec![
			MenuItem::Action {
				icon: Some(copy()),
				label: "Duplicate".into(),
				shortcut: shortcuts.label(ShortcutAction::Duplicate),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Duplicate),
			},
			MenuItem::Action {
				icon: Some(copy()),
				label: "Make unique".into(),
				shortcut: shortcuts.label(ShortcutAction::MakeUnique),
				message: Message::ClipAction(track, clip, playlist::ClipAction::MakeUnique),
			},
			MenuItem::Action {
				icon: Some(trash_2()),
				label: "Delete".into(),
				shortcut: shortcuts.label(ShortcutAction::Delete),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Delete),
			},
		];

		if midi {
			items.insert(
				0,
				MenuItem::Action {
					icon: Some(keyboard_music()),
					label: "Open in piano roll".into(),
					shortcut: String::new(),
					message: Message::ClipAction(
						track,
						clip,
						playlist::ClipAction::OpenInPianoRoll,
					),
				},
			);

			items.push(MenuItem::Separator);

			items.push(MenuItem::Action {
				icon: Some(magnet()),
				label: "Quantize".into(),
				shortcut: shortcuts.label(ShortcutAction::Quantize),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Quantize),
			});
		} else {
			items.push(MenuItem::Separator);

			items.push(MenuItem::Action {
				icon: Some(flip_horizontal()),
				label: "Reverse".into(),
				shortcut: shortcuts.label(ShortcutAction::Reverse),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Reverse),
			});

			items.push(MenuItem::Action {
				icon: Some(gauge()),
				label: "Normalize".into(),
				shortcut: shortcuts.label(ShortcutAction::Normalize),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Normalize),
			});

			items.push(MenuItem::Action {
				icon: Some(arrow_up_down()),
				label: "Invert polarity".into(),
				shortcut: shortcuts.label(ShortcutAction::Invert),
				message: Message::ClipAction(track, clip, playlist::ClipAction::InvertPolarity),
			});

			items.push(MenuItem::Separator);

			items.push(MenuItem::Action {
				icon: Some(square_split_horizontal()),
				label: "Split at playhead".into(),
				shortcut: String::new(),
				message: Message::ClipAction(track, clip, playlist::ClipAction::Split),
			});
		}

		menu(items)
	}

	/// The menu of the background of a tab.
	fn background_context_menu(tab: Tab, state: &State) -> Element<'_, Message> {
		let shortcuts = &state.shortcuts;

		let mut items = vec![
			MenuItem::Action {
				icon: Some(check()),
				label: "Select all".into(),
				shortcut: shortcuts.label(ShortcutAction::SelectAll),
				message: Message::SelectAll(tab),
			},
			MenuItem::Action {
				icon: Some(x()),
				label: "Deselect".into(),
				shortcut: shortcuts.label(ShortcutAction::Escape),
				message: Message::Escape(tab),
			},
		];

		if tab == Tab::Playlist {
			items.insert(
				0,
				MenuItem::Action {
					icon: Some(plus()),
					label: "Add track".into(),
					shortcut: String::new(),
					message: Message::TrackAdd,
				},
			);

			items.push(MenuItem::Separator);

			items.push(MenuItem::Action {
				icon: Some(magnet()),
				label: "Quantize".into(),
				shortcut: shortcuts.label(ShortcutAction::Quantize),
				message: Message::Quantize(tab),
			});

			items.push(MenuItem::Action {
				icon: Some(repeat_icon()),
				label: "Set loop start".into(),
				shortcut: String::new(),
				message: Message::SetLoopMarkerStart,
			});

			items.push(MenuItem::Action {
				icon: Some(repeat_icon()),
				label: "Set loop end".into(),
				shortcut: String::new(),
				message: Message::SetLoopMarkerEnd,
			});
		}

		if tab == Tab::PianoRoll {
			items.push(MenuItem::Separator);

			items.push(MenuItem::Action {
				icon: Some(magnet()),
				label: "Quantize".into(),
				shortcut: shortcuts.label(ShortcutAction::Quantize),
				message: Message::Quantize(tab),
			});

			items.push(MenuItem::Action {
				icon: Some(arrow_left_right()),
				label: "Legatify".into(),
				shortcut: shortcuts.label(ShortcutAction::Join),
				message: Message::Join(tab),
			});

			items.push(MenuItem::Action {
				icon: Some(keyboard_music()),
				label: "Transpose up an octave".into(),
				shortcut: shortcuts.label(ShortcutAction::TransposeOctaveUp),
				message: Message::TransposeOctUp(tab),
			});

			items.push(MenuItem::Action {
				icon: Some(keyboard_music()),
				label: "Transpose down an octave".into(),
				shortcut: shortcuts.label(ShortcutAction::TransposeOctaveDown),
				message: Message::TransposeOctDown(tab),
			});
		}

		items.push(MenuItem::Separator);

		items.push(MenuItem::Submenu {
			icon: Some(magnet()),
			label: "Grid".into(),
			items: vec![
				MenuItem::Action {
					icon: Some(plus()),
					label: "Finer".into(),
					shortcut: shortcuts.label(ShortcutAction::NarrowedGrid),
					message: Message::NarrowedGrid,
				},
				MenuItem::Action {
					icon: Some(x()),
					label: "Coarser".into(),
					shortcut: shortcuts.label(ShortcutAction::WidenedGrid),
					message: Message::WidenedGrid,
				},
				MenuItem::Separator,
				MenuItem::Toggle {
					icon: Some(music_2()),
					label: "Triplets".into(),
					shortcut: shortcuts.label(ShortcutAction::ToggleGridTriplets),
					is_on: state.grid.triplets,
					message: Message::ToggleGridTriplets,
				},
				MenuItem::Toggle {
					icon: Some(magnet()),
					label: "On".into(),
					shortcut: shortcuts.label(ShortcutAction::ToggleGrid),
					is_on: state.grid.enabled,
					message: Message::ToggleGrid,
				},
			],
		});

		menu(items)
	}

	/// The menu of the piano roll.
	fn piano_roll_context_menu(state: &State) -> Element<'_, Message> {
		Self::background_context_menu(Tab::PianoRoll, state)
	}

	/// The items that connect a node to the other nodes.
	fn route_items(&self, node: &Node) -> Vec<MenuItem<Message>> {
		let mut items = Vec::new();

		for channel in self.arrangement.channels() {
			let other = self.arrangement.node(channel.id);
			let connected = node.outgoing.contains_key(&other.id);

			items.push(MenuItem::Toggle {
				icon: None,
				label: other.name.to_string(),
				shortcut: String::new(),
				is_on: connected,
				message: if connected {
					Message::Disconnect(node.id, other.id)
				} else {
					Message::Connect(node.id, other.id)
				},
			});
		}

		for track in self.arrangement.tracks() {
			let other = self.arrangement.node(track.id);
			let connected = node.outgoing.contains_key(&other.id);

			items.push(MenuItem::Toggle {
				icon: None,
				label: other.name.to_string(),
				shortcut: String::new(),
				is_on: connected,
				message: if connected {
					Message::Disconnect(node.id, other.id)
				} else {
					Message::Connect(node.id, other.id)
				},
			});
		}

		items
	}

	fn view_playlist<'a>(&'a self, state: &'a State) -> Element<'a, Message> {
		Seeker::new(
			self.arrangement.transport(),
			&state.grid,
			self.playlist.borrow().position,
			self.playlist.borrow().scale,
			column![
				sweeten::column(
					self.arrangement
						.tracks()
						.iter()
						.enumerate()
						.map(|(i, track)| {
							let node = self.arrangement.node(track.id);

							let enabled = node.enabled
								&& self.arrangement.solo().is_none_or(|solo| solo == node.id);

							let soloed = self.arrangement.solo() == Some(node.id);

							let button_style = |cond: bool| {
								if !enabled {
									button::secondary
								} else if cond {
									button::warning
								} else {
									button::primary
								}
							};

							SelectArea::new(
								container(row![
									mouse_area(center_y(grip_vertical()))
										.interaction(Interaction::Grab),
									opaque(
										mouse_area(ContextMenu::new(
											row![
												column![
													row![
														PeakMeter::new(&node.peaks[0])
															.enabled(enabled),
														PeakMeter::new(&node.peaks[1])
															.enabled(enabled),
													]
													.spacing(2),
													container(space().width(28).height(2)).style(
														container_with_radius(
															if node.polyphony > 0 {
																container::primary
															} else {
																container::secondary
															},
															border::bottom(f32::INFINITY),
														)
													)
												]
												.spacing(2),
												row![
													row![
														rule::vertical(1),
														column![
															center_y(
																text_input(
																	format!("Track {}", i + 1),
																	&*node.name
																)
																.id(node.name_id.clone())
																.on_input(|name| {
																	Message::ChannelNameChanged(
																		node.id,
																		name.into(),
																	)
																})
																.width(55)
																.padding(0)
																.size(13)
																.align_x(Center)
																.style(text_input_transparent)
															)
															.height(35),
															(self.playlist.borrow().scale.y
																>= 85.0)
																.then(|| track.inputs_toolbar(
																	enabled,
																	self.arrangement.transport()
																)),
														]
														.spacing(5),
														rule::vertical(1),
													]
													.spacing(5)
													.height(Shrink),
													(self.playlist.borrow().scale.y >= 45.0).then(
														|| column![
															ContextMenu::new(
																Knob::new(
																	0.0..=MAX_VOL,
																	node.utility
																		.volume
																		.abs()
																		.cbrt(),
																	|v| {
																		Message::ChannelVolumeChanged(
																		node.id,
																		v.powi(3).copysign(
																			node.utility.volume,
																		),
																	)
																	}
																)
																.default(1.0)
																.radius(19.44444)
																.enabled(enabled)
																.tooltip(
																	format_db(node.utility.volume)
																),
																|| node.volume_context_menu(
																	Tab::Playlist
																)
															),
															node.pan_knob(19.44444, enabled),
														]
														.align_x(Center)
														.spacing(5)
														.wrap()
													),
													column![
														column![
															icon_button(
																x(),
																if enabled {
																	button::danger
																} else {
																	button::secondary
																}
															)
															.on_press(
																Message::TrackRemove(node.id)
															),
															text_icon_button(
																"M",
																button_style(soloed)
															)
															.on_press(
																Message::TrackToggleEnabled(
																	node.id
																)
															)
														]
														.spacing(5)
														.wrap(),
														column![
															text_icon_button(
																"S",
																button_style(soloed)
															)
															.on_press(
																Message::TrackToggleSolo(node.id)
															),
															icon_button(
																snowflake(),
																button_style(false)
															)
															.on_press_maybe(
																enabled.then_some(Message::Freeze(
																	node.id
																))
															),
														]
														.spacing(5)
														.wrap(),
													]
													.spacing(5)
													.wrap(),
												]
												.spacing(5),
											]
											.padding(padding::all(5).left(0))
											.spacing(5),
											|| node.main_context_menu(
												Tab::Playlist,
												&state.shortcuts,
												Vec::new()
											)
										))
										.interaction(Interaction::Pointer)
									)
								])
								.height(self.playlist.borrow().scale.y)
								.style(container_with_radius(
									selectable_box(weakest_bordered_box, node.id == self.selected),
									border::left(5),
								)),
							)
							.on_select_maybe(
								(node.id != self.selected)
									.then_some(Message::ChannelSelect(node.id)),
							)
							.into()
						})
				)
				.on_drag(Message::TrackMove)
				.style(sweeten_column_with_radius(
					sweeten_column_style,
					border::left(5)
				)),
				container(
					button(plus().size(LINE_HEIGHT + 6.0))
						.padding(5)
						.style(button_with_radius(button::primary, f32::INFINITY))
						.on_press(Message::TrackAdd),
				)
				.padding(padding::right(5))
			]
			.spacing(5)
			.align_x(Center),
			ContextMenu::new(
				Playlist::new(
					&self.playlist,
					self.arrangement.transport(),
					&state.grid,
					self.arrangement
						.tracks()
						.iter()
						.enumerate()
						.map(|(t, track)| {
							let node = self.arrangement.node(track.id);

							let enabled = node.enabled
								&& self.arrangement.solo().is_none_or(|solo| solo == node.id);

							Track::new(
								track
									.clips
									.iter()
									.enumerate()
									.map(|(c, clip)| match clip {
										generic_daw_core::Clip::Audio(clip) => Clip::new(
											AudioClipRef {
												sample: &self.arrangement.samples()[&clip.sample],
												clip,
												index: (t, c),
											},
											&self.playlist,
											self.arrangement.transport(),
											&state.grid,
											enabled,
											Message::PlaylistAction,
										),
										generic_daw_core::Clip::Midi(clip) => Clip::new(
											MidiClipRef {
												pattern: &self.arrangement.midi_patterns()
													[&clip.pattern],
												clip,
												index: (t, c),
											},
											&self.playlist,
											self.arrangement.transport(),
											&state.grid,
											enabled,
											Message::PlaylistAction,
										),
									})
									.chain(track.audio_recording.as_ref().map(|recording| {
										Clip::new(
											recording,
											&self.playlist,
											self.arrangement.transport(),
											&state.grid,
											enabled,
											Message::PlaylistAction,
										)
									}))
									.chain(track.midi_recording.as_ref().map(|recording| {
										Clip::new(
											recording,
											&self.playlist,
											self.arrangement.transport(),
											&state.grid,
											enabled,
											Message::PlaylistAction,
										)
									})),
							)
						}),
					Message::PlaylistAction,
				),
				move || self.playlist_context_menu(state),
			),
			Message::SeekTo,
			Message::SetLoopMarker,
			|pos_diff, _, visible| {
				Message::PlaylistAction(playlist::Action::Pan(pos_diff, visible))
			},
			|scale_diff, cursor, _, visible| {
				Message::PlaylistAction(playlist::Action::Zoom(scale_diff, cursor, visible))
			},
		)
		.into()
	}

	fn view_piano_roll<'a>(&'a self, state: &'a State) -> Element<'a, Message> {
		let Some(clip) = self.midi_clip() else {
			return center("No MIDI clip selected.").into();
		};

		Seeker::new(
			self.arrangement.transport(),
			&state.grid,
			self.piano_roll.borrow().position
				+ Vector::new(
					((clip
						.position
						.start()
						.to_frames(self.arrangement.transport()) as f64
						- clip
							.position
							.offset()
							.to_frames(self.arrangement.transport()) as f64)
						/ f64::from(frames_per_px(
							self.piano_roll.borrow().scale,
							self.arrangement.transport(),
						))) as f32,
					0.0,
				),
			self.piano_roll.borrow().scale,
			Piano::new(self.piano_roll.borrow().scale),
			ContextMenu::new(
				PianoRoll::new(
					&self.piano_roll,
					self.arrangement.transport(),
					&state.grid,
					self.arrangement.midi_patterns()[&clip.pattern]
						.notes
						.iter()
						.enumerate()
						.map(|(index, note)| {
							Note::new(
								index,
								note,
								&self.piano_roll,
								self.arrangement.transport(),
								&state.grid,
								Message::PianoRollAction,
							)
						}),
					Message::PianoRollAction,
				),
				move || Self::piano_roll_context_menu(state),
			),
			Message::SeekTo,
			Message::SetLoopMarker,
			|pos_diff, height, visible| {
				Message::PianoRollAction(piano_roll::Action::Pan(pos_diff, height, visible))
			},
			|scale_diff, cursor, height, visible| {
				Message::PianoRollAction(piano_roll::Action::Zoom(
					scale_diff, cursor, height, visible,
				))
			},
		)
		.into()
	}

	pub fn subscription(&self) -> Subscription<Message> {
		let playing = self.arrangement.transport().playing;
		Subscription::batch([
			every(Duration::from_secs(1)).map(|_| Message::RequestUpdate),
			// Only use high-frequency frames subscription when playing to reduce CPU usage
			if playing {
				frames().map(|_| Message::DrainQueue)
			} else if self.arrangement.queue_empty() {
				Subscription::none()
			} else {
				every(Duration::from_millis(16)).map(|_| Message::DrainQueue)
			},
		])
	}

	/// The message of a shortcut that applies to the arrangement.
	pub fn keybinds(action: ShortcutAction) -> Option<fn(Tab) -> Message> {
		match action {
			ShortcutAction::Delete => Some(Message::Delete),
			ShortcutAction::Escape => Some(Message::Escape),
			ShortcutAction::CursorUp => Some(Message::ArrowUp),
			ShortcutAction::CursorDown => Some(Message::ArrowDown),
			ShortcutAction::CursorLeft => Some(Message::ArrowLeft),
			ShortcutAction::CursorRight => Some(Message::ArrowRight),
			ShortcutAction::SelectAll => Some(Message::SelectAll),
			ShortcutAction::SelectInverse => Some(Message::SelectInverse),
			ShortcutAction::Duplicate => Some(Message::Duplicate),
			ShortcutAction::MakeUnique => Some(Message::MakeUnique),
			ShortcutAction::TransposeOctaveUp => Some(Message::TransposeOctUp),
			ShortcutAction::TransposeOctaveDown => Some(Message::TransposeOctDown),
			ShortcutAction::Invert => Some(Message::Invert),
			ShortcutAction::Join => Some(Message::Join),
			ShortcutAction::ToggleEnabled => Some(Message::ToggleEnabled),
			ShortcutAction::Normalize => Some(Message::Normalize),
			ShortcutAction::Quantize => Some(Message::Quantize),
			ShortcutAction::Reverse => Some(Message::Reverse),
			ShortcutAction::ToggleSolo => Some(Message::ToggleSolo),
			_ => None,
		}
	}

	pub fn autoscroll(
		&mut self,
		before: SecondsTime,
		after: SecondsTime,
		config: &Config,
		state: &mut State,
	) {
		if state.autoscroll && after != before {
			let pos_diff = Vector::new(
				(after.to_frames(self.arrangement.transport()) as f32
					- before.to_frames(self.arrangement.transport()) as f32)
					/ frames_per_px(self.playlist.get_mut().scale, self.arrangement.transport()),
				0.0,
			);

			_ = self.handle_playlist_action(
				playlist::Action::Pan(pos_diff, f32::INFINITY),
				config,
				state,
			);
		}
	}

	pub fn save(
		&self,
		clap_host: &mut ClapHost,
		vst2_host: &Vst2Host,
		vst3_host: &Vst3Host,
	) -> Vec<u8> {
		let playlist = self.playlist.borrow();
		let piano_roll = self.piano_roll.borrow();

		let view = proto::ViewState {
			playlist: proto::TabState {
				position: proto::Vector {
					x: playlist.position.x,
					y: playlist.position.y,
				},
				scale: proto::Vector {
					x: playlist.scale.x,
					y: playlist.scale.y,
				},
			},
			piano_roll: proto::TabState {
				position: proto::Vector {
					x: piano_roll.position.x,
					y: piano_roll.position.y,
				},
				scale: proto::Vector {
					x: piano_roll.scale.x,
					y: piano_roll.scale.y,
				},
			},
		};

		self.arrangement.save(clap_host, vst2_host, vst3_host, view)
	}

	pub fn change_config(&mut self) -> Task<Message> {
		let (_, a_receiver) = oneshot::channel();
		Task::perform(
			self.arrangement
				.request_processor_and_pool(PullSlot::Empty(a_receiver))
				.into_future(),
			Result::ok,
		)
		.and_then(Task::done)
		.map(Box::new)
		.map(NoDebug)
		.map(NoClone)
		.map(Message::ChangeConfig)
	}

	pub fn hover_file(&mut self, file: Arc<Path>, kind: FileKind) {
		self.playlist.get_mut().status = playlist::Status::Hovering(file, kind, None);
	}

	pub fn preview_file(
		&mut self,
		path: Arc<Path>,
		config: &Config,
		state: &mut State,
	) -> Action<daw::Instruction, Message> {
		if let Some(sample) = self
			.arrangement
			.samples()
			.values()
			.find(|sample| sample.path == path)
		{
			self.update(Message::AudioPreview(sample.id), config, state)
		} else {
			self.loading += 1;
			Task::future(unblock(|| SamplePair::new(path)))
				.then(move |sample| {
					if let Some(sample) = sample {
						let id = sample.gui.id;
						Task::done(Message::SampleLoaded(Some(Box::new(sample))))
							.chain(Task::done(Message::AudioPreview(id)))
					} else {
						Task::done(Message::SampleLoaded(None))
					}
				})
				.into()
		}
	}

	pub fn finish(&mut self, tab: Tab) {
		match tab {
			Tab::Playlist => self.playlist.get_mut().finish(),
			Tab::Mixer => {}
			Tab::PianoRoll => self.piano_roll.get_mut().finish(),
		}
	}

	pub fn clear(&mut self, tab: Tab) {
		match tab {
			Tab::Playlist => self.playlist.get_mut().clear(),
			Tab::Mixer => {}
			Tab::PianoRoll => self.piano_roll.get_mut().clear(),
		}
	}

	pub fn loading(&self) -> bool {
		self.loading > 0
	}

	fn midi_clip(&self) -> Option<MidiClip> {
		let (track, clip) = self.midi_clip_id?;
		if let generic_daw_core::Clip::Midi(clip) =
			*self.arrangement.tracks().get(track)?.clips.get(clip)?
		{
			Some(clip)
		} else {
			None
		}
	}

	/// Undoes the last change to the clips or notes.
	pub fn undo(&mut self) -> bool {
		self.change_history(false)
	}

	/// Redoes the last change that was undone.
	pub fn redo(&mut self) -> bool {
		self.change_history(true)
	}

	fn change_history(&mut self, redo: bool) -> bool {
		let midi_clip = self.midi_clip().map(|clip| clip.id);

		self.finish(Tab::Playlist);
		self.finish(Tab::PianoRoll);

		let changed = if redo {
			self.history.redo(&mut self.arrangement)
		} else {
			self.history.undo(&mut self.arrangement)
		};

		if changed {
			self.playlist.get_mut().clear();
			self.piano_roll.get_mut().clear();
			self.midi_clip_id = midi_clip.and_then(|id| self.arrangement.clip_location(id));
		}

		changed
	}

	/// Plays notes through the currently selected track while the computer MIDI keyboard is active.
	pub fn midi_keyboard_note_on(&mut self, key: MidiKey, velocity: f32) -> Option<NodeId> {
		let id = self
			.arrangement
			.tracks()
			.iter()
			.find(|track| track.id == self.selected)?
			.id;
		self.arrangement.midi_keyboard_note_on(id, key, velocity);
		Some(id)
	}

	pub fn midi_keyboard_note_off(&mut self, id: NodeId, key: MidiKey) {
		if self.arrangement.track_of(id).is_some() {
			self.arrangement.midi_keyboard_note_off(id, key);
		}
	}

	/// Plays a note through the instrument chain of the track the piano roll belongs to.
	fn preview_midi_note(&mut self, key: MidiKey, velocity: f32) {
		let Some((track, _)) = self.midi_clip_id else {
			return;
		};

		let Some(id) = self.arrangement.tracks().get(track).map(|track| track.id) else {
			return;
		};

		self.arrangement.preview_midi_note(id, key, velocity);
	}

	fn select_prev(&mut self) {
		self.selected = match self.arrangement.node(self.selected).ty {
			NodeType::Master => self.selected,
			NodeType::Track => self
				.arrangement
				.tracks()
				.iter()
				.rev()
				.skip_while(|track| track.id != self.selected)
				.nth(1)
				.map_or_else(|| self.arrangement.master().id, |track| track.id),
			NodeType::Channel => self
				.arrangement
				.channels()
				.iter()
				.rev()
				.skip_while(|channel| channel.id != self.selected)
				.nth(1)
				.map(|channel| channel.id)
				.or_else(|| self.arrangement.tracks().last().map(|track| track.id))
				.unwrap_or_else(|| self.arrangement.master().id),
		};
	}

	fn select_next(&mut self) {
		self.selected = match self.arrangement.node(self.selected).ty {
			NodeType::Master => self
				.arrangement
				.tracks()
				.first()
				.map(|track| track.id)
				.or_else(|| {
					self.arrangement
						.channels()
						.first()
						.map(|channel| channel.id)
				})
				.unwrap_or(self.selected),
			NodeType::Track => self
				.arrangement
				.tracks()
				.iter()
				.skip_while(|track| track.id != self.selected)
				.nth(1)
				.map(|track| track.id)
				.or_else(|| {
					self.arrangement
						.channels()
						.first()
						.map(|channel| channel.id)
				})
				.unwrap_or(self.selected),
			NodeType::Channel => self
				.arrangement
				.channels()
				.iter()
				.skip_while(|&channel| channel.id != self.selected)
				.nth(1)
				.map_or(self.selected, |channel| channel.id),
		};
	}

	fn update_selection(&mut self, mut f: impl FnMut((usize, usize)) -> Option<(usize, usize)>) {
		self.midi_clip_id = self.midi_clip_id.and_then(&mut f);

		let playlist = self.playlist.get_mut();
		playlist.primary = playlist.primary.drain().filter_map(&mut f).collect();
		playlist.secondary = playlist.secondary.drain().filter_map(&mut f).collect();
	}
}

/// Does something to some clips, recording what changed so that it can be
/// undone.
fn edit_clips(
	arrangement: &mut Arrangement,
	history: &mut history::History,
	clips: impl IntoIterator<Item = (usize, usize)>,
	mut edit: impl FnMut(&mut Arrangement, usize, usize),
) {
	for (track, clip) in clips {
		let before = history::ClipSnapshot::of(arrangement, track, clip);

		edit(arrangement, track, clip);

		if let Some(before) = before {
			history::record_clip_change(arrangement, history, before);
		}
	}
}

/// Does something to some notes, recording what changed so that it can be
/// undone.
fn edit_notes(
	arrangement: &mut Arrangement,
	history: &mut history::History,
	pattern: MidiPatternId,
	notes: impl IntoIterator<Item = usize>,
	mut edit: impl FnMut(&mut Arrangement, usize),
) {
	for note in notes {
		let before = arrangement.midi_patterns()[&pattern].notes[note];

		edit(arrangement, note);

		history::record_note_change(arrangement, history, pattern, before);
	}
}

fn update_selection_insert_track((ct, cc): (usize, usize), track: usize) -> (usize, usize) {
	(if ct < track { ct } else { ct + 1 }, cc)
}

fn update_selection_delete_track((ct, cc): (usize, usize), track: usize) -> Option<(usize, usize)> {
	match ct.cmp(&track) {
		Ordering::Less => Some((ct, cc)),
		Ordering::Equal => None,
		Ordering::Greater => Some((ct - 1, cc)),
	}
}

fn update_selection_move_track(
	(ct, cc): (usize, usize),
	track: usize,
	new_track: usize,
) -> (usize, usize) {
	match (ct.cmp(&track), ct.cmp(&new_track)) {
		(Ordering::Less, Ordering::Less) | (Ordering::Greater, Ordering::Greater) => (ct, cc),
		(Ordering::Equal, _) => (new_track, cc),
		(Ordering::Less, Ordering::Equal | Ordering::Greater) => (ct + 1, cc),
		(Ordering::Greater, Ordering::Less | Ordering::Equal) => (ct - 1, cc),
	}
}

fn update_selection_delete_clip(
	(ct, cc): (usize, usize),
	track: usize,
	clip: usize,
) -> Option<(usize, usize)> {
	if ct == track {
		match cc.cmp(&clip) {
			Ordering::Less => Some((ct, cc)),
			Ordering::Equal => None,
			Ordering::Greater => Some((ct, cc - 1)),
		}
	} else {
		Some((ct, cc))
	}
}

fn update_selection_move_clip(
	(ct, cc): (usize, usize),
	track: usize,
	clip: usize,
	new_track: usize,
	new_clip: usize,
) -> (usize, usize) {
	match (
		ct.cmp(&track),
		cc.cmp(&clip),
		ct.cmp(&new_track),
		cc.cmp(&new_clip),
	) {
		(Ordering::Less | Ordering::Greater, _, Ordering::Less | Ordering::Greater, _) => (ct, cc),
		(Ordering::Equal, Ordering::Equal, _, _) => (new_track, new_clip),
		(_, Ordering::Greater, Ordering::Less | Ordering::Greater, _)
		| (Ordering::Equal, Ordering::Greater, Ordering::Equal, Ordering::Less) => (ct, cc - 1),
		(Ordering::Less | Ordering::Greater, _, _, Ordering::Equal | Ordering::Greater)
		| (Ordering::Equal, Ordering::Less, Ordering::Equal, Ordering::Equal | Ordering::Greater) => {
			(ct, cc + 1)
		}
		_ => (ct, cc),
	}
}

pub fn amp_to_db(amp: f32) -> f32 {
	20.0 * amp.log10()
}

pub fn db_to_amp(db: f32) -> f32 {
	10f32.powf(db / 20.0)
}

pub fn format_db(amp: f32) -> String {
	let db = amp_to_db(amp.abs());
	let dba = db.abs();

	format!(
		"{}{dba:.*} dB{}",
		if dba < 0.05 {
			""
		} else if db.is_sign_positive() {
			"+"
		} else {
			"-"
		},
		(dba < 99.95).into(),
		if amp.is_sign_positive() { "" } else { " (i)" }
	)
}

pub fn format_pan(pan: f32) -> String {
	let pan = (pan * 100.0).round() as i8;
	match pan.cmp(&0) {
		Ordering::Greater => format!("{}% right", pan.abs()),
		Ordering::Equal => "center".to_owned(),
		Ordering::Less => format!("{}% left", pan.abs()),
	}
}

pub fn format_mix(amt: f32) -> String {
	format!(
		"{:.0}%{}",
		amt.abs() * 100.0,
		if amt.is_sign_positive() { "" } else { " (i)" }
	)
}

fn crc(mut r: impl Read) -> io::Result<u32> {
	#[repr(align(8))]
	struct Aligned([u8; 4096]);
	let Aligned(buf) = &mut Aligned([0; 4096]);

	let mut crc = 0;
	let mut len;

	while {
		len = r.read(buf)?;
		len != 0
	} {
		crc = crc32c::crc32c_append(crc, &buf[..len]);
	}

	Ok(crc)
}

fn backoff(chunk_size: usize) -> impl FnMut(usize) -> Timer {
	let min = 32.0 / 192_000.0;
	let max = 2048.0 / 44_100.0;
	let mut backoff = min;
	move |count: usize| {
		let divisor = (count as f32 / chunk_size as f32).max(0.5);
		backoff = ((backoff + backoff / divisor) / 2.0).clamp(min, max);
		Timer::after(Duration::from_secs_f32(backoff))
	}
}

fn poll_consumer<T>(mut consumer: Consumer<T>) -> impl Stream<Item = T> {
	let mut backoff = backoff(1);
	stream::channel(consumer.buffer().capacity(), async move |mut sender| {
		loop {
			let mut count = 0;
			while let Ok(t) = consumer.pop() {
				count += 1;
				sender.send(t).await.unwrap();
			}
			if consumer.is_abandoned() {
				return;
			}
			backoff(count).await;
		}
	})
}
