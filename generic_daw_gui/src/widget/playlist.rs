use crate::{
	file_tree::FileKind,
	state::Grid,
	widget::{
		ALPHA_1_3, Delta, LINE_HEIGHT, clip, frames_per_px, px_to_time, time_to_px, track::Track,
	},
};
use generic_daw_core::{Transport, time::BeatTime};
use iced::{
	Color, Element, Event, Fill, Length, Point, Rectangle, Renderer, Size, Theme, Vector,
	advanced::{
		Renderer as _, Shell,
		layout::{Layout, Limits, Node},
		mouse::{self, Cursor, Interaction},
		overlay,
		renderer::{Quad, Style},
		widget::{Operation, Tree, Widget},
	},
	border,
	gradient::Linear,
	keyboard,
};
use std::{
	cell::RefCell,
	collections::HashSet,
	f32::consts::{FRAC_PI_2, PI},
	path::Path,
	sync::Arc,
};

#[derive(Clone, Debug)]
pub enum Action {
	Pan(Vector, f32),
	Zoom(Vector, Point, f32),
	Add(Option<(Arc<Path>, FileKind)>, Option<usize>, BeatTime),
	Open(usize, usize),
	Clone,
	Drag(Delta<usize>, Delta<BeatTime>),
	TrimStart(Delta<BeatTime>),
	TrimEnd(Delta<BeatTime>),
	InvertPolarity,
	DragVolume(f32),
	FadeStartLen(Delta<BeatTime>),
	FadeStartP(Point),
	FadeStartToggleSymmetric,
	FadeEndLen(Delta<BeatTime>),
	FadeEndP(Point),
	FadeEndToggleSymmetric,
	StretchStart(Delta<BeatTime>),
	StretchEnd(Delta<BeatTime>),
	SplitAt(BeatTime),
	DragSplit(BeatTime),
	DragSlip(Delta<BeatTime>),
	Delete,
	/// The mouse button was released, so a drag is over.
	Finished,
}

/// What the user right-clicked on.
#[derive(Clone, Copy, Debug)]
pub enum ContextTarget {
	/// An audio clip, with the track it is in and the index of the clip.
	AudioClip(usize, usize),
	/// A MIDI clip, with the track it is in and the index of the clip.
	MidiClip(usize, usize),
	/// The empty space of a track.
	EmptyTrack { track: usize, time: BeatTime },
	/// The empty space below the tracks.
	EmptyArea { time: BeatTime },
}

/// What the user can do to a clip.
#[derive(Clone, Copy, Debug)]
pub enum ClipAction {
	/// Make a copy of the clip.
	Duplicate,
	/// Remove the clip.
	Delete,
	/// Play the clip backwards.
	Reverse,
	/// Make the clip as loud as it can be.
	Normalize,
	/// Turn the clip upside down.
	InvertPolarity,
	/// Cut the clip in two at the playhead.
	Split,
	/// Open the clip in the piano roll.
	OpenInPianoRoll,
	/// Move the notes of the clip onto the grid.
	Quantize,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub enum Status {
	Hovering(Arc<Path>, FileKind, Option<(Option<usize>, BeatTime)>),
	Selecting(usize, usize, BeatTime, BeatTime, bool),
	Dragging(usize, BeatTime),
	TrimmingStart(BeatTime),
	TrimmingEnd(BeatTime),
	DraggingVolume(f32),
	FadingStartLen(BeatTime),
	FadingStartP(usize, usize),
	FadingEndLen(BeatTime),
	FadingEndP(usize, usize),
	DraggingSplit(BeatTime),
	DraggingSlip(BeatTime),
	#[default]
	None,
}

#[derive(Debug, Default)]
pub struct State {
	pub status: Status,
	pub primary: HashSet<(usize, usize)>,
	pub secondary: HashSet<(usize, usize)>,
	pub position: Vector,
	pub scale: Vector,
	/// What the user right-clicked on, if the menu is open.
	pub context_menu: Option<ContextTarget>,
}

impl State {
	pub fn new(position: Vector, scale: Vector) -> Self {
		Self {
			position,
			scale,
			..Self::default()
		}
	}

	pub fn finish(&mut self) {
		match std::mem::take(&mut self.status) {
			Status::Selecting(..) => self.primary.clear(),
			Status::DraggingSplit(..) => self.primary.extend(self.secondary.drain()),
			_ => {}
		}
	}

	pub fn clear(&mut self) {
		self.status = Status::None;
		self.primary.clear();
		self.secondary.clear();
	}
}

#[derive(Debug)]
pub struct Playlist<'a, Message> {
	state: &'a RefCell<State>,
	transport: &'a Transport,
	grid: &'a Grid,
	tracks: Box<[Track<'a, Message>]>,
	action: fn(Action) -> Message,
}

impl<'a, Message: 'a> Widget<Message, Theme, Renderer> for Playlist<'a, Message> {
	fn diff(&mut self, tree: &mut Tree) {
		// Diff all tracks - diff is fast, and skipping it breaks scrolling
		tree.diff_children(&mut self.tracks);
	}

	fn size(&self) -> Size<Length> {
		Size::new(Fill, Fill)
	}

	fn layout(&mut self, tree: &mut Tree, renderer: &Renderer, limits: &Limits) -> Node {
		let state = self.state.borrow();
		let track_height = state.scale.y;
		drop(state);

		let mut height = 0.0;

		let children = self
			.tracks
			.iter_mut()
			.zip(&mut tree.children)
			.enumerate()
			.map(|(_track_idx, (child, tree))| {
				// Always layout tracks - it's fast and needed for clip tree positioning
				let node = child.layout(tree, renderer, &limits.height(track_height));
				let node = node.translate(Vector::new(0.0, height));
				height += node.bounds().height;
				node
			})
			.collect();

		Node::with_children(Size::new(limits.max().width, height), children)
	}

	fn update(
		&mut self,
		tree: &mut Tree,
		event: &Event,
		layout: Layout<'_>,
		cursor: Cursor,
		renderer: &Renderer,
		shell: &mut Shell<'_, Message>,
		viewport: &Rectangle,
	) {
		let state = self.state.borrow();
		let track_height = state.scale.y;
		let viewport_y = state.position.y;
		let viewport_height = viewport.height;

		// Estimate visible track range
		let first_visible_track = ((viewport_y / track_height) as usize).saturating_sub(1);
		let visible_track_count = ((viewport_height / track_height).ceil() as usize) + 2;
		let last_visible_track = (first_visible_track + visible_track_count).min(self.tracks.len());
		drop(state);

		self.tracks
			.iter_mut()
			.zip(&mut tree.children)
			.zip(layout.children())
			.enumerate()
			.for_each(|(track_idx, ((child, tree), layout))| {
				if track_idx >= first_visible_track && track_idx < last_visible_track {
					child.update(tree, event, layout, cursor, renderer, shell, viewport);
				}
			});

		let state = &mut *self.state.borrow_mut();

		if let Event::Mouse(mouse::Event::ButtonReleased { .. }) = event
			&& state.status != Status::None
		{
			match std::mem::take(&mut state.status) {
				Status::Hovering(path, kind, Some((track, time))) => {
					state.primary.clear();
					shell.publish((self.action)(Action::Add(Some((path, kind)), track, time)));
				}
				Status::Selecting(.., true) => shell.publish((self.action)(Action::Delete)),
				Status::DraggingSplit(..) => state.primary.extend(state.secondary.drain()),
				_ => {}
			}
			shell.publish((self.action)(Action::Finished));
			shell.capture_event();
			shell.request_redraw();
			return;
		}

		if shell.is_event_captured() {
			return;
		}

		let cursor = match cursor.position_from(layout.position()) {
			Some(cursor) => cursor,
			None if let Status::None | Status::Hovering(.., None) = state.status => return,
			None if let Status::DraggingVolume(..) | Status::DraggingSlip(..) = state.status => {
				match cursor.land().position_from(layout.position()) {
					Some(cursor) => cursor,
					None => return,
				}
			}
			None => {
				shell.capture_event();
				match cursor.land().position_from(layout.position()) {
					Some(cursor) => Point::new(
						cursor.x.clamp(
							viewport.x - layout.bounds().x,
							viewport.x - layout.bounds().x + viewport.width,
						),
						cursor.y.clamp(
							viewport.y - layout.bounds().y,
							viewport.y - layout.bounds().y + viewport.height,
						),
					),
					None => return,
				}
			}
		};

		let new_time = px_to_time(cursor.x, state.position, state.scale, self.transport);
		let snap_step = self.grid.beats_snap_step(state.scale, self.transport);

		match event {
			Event::Mouse(mouse::Event::ButtonPressed {
				button: mouse::Button::Left,
				modifiers,
			}) if state.status == Status::None => {
				let track = track_index(&layout, cursor);
				state.primary.clear();

				if modifiers.command() {
					let Some(track) = track.or_else(|| layout.children().len().checked_sub(1))
					else {
						return;
					};

					let time = self
						.grid
						.maybe_snap(new_time, *modifiers, |time| time.round(snap_step));

					state.status = Status::Selecting(track, track, time, time, false);
				} else if let Some(track) = track {
					let time = self
						.grid
						.maybe_snap(new_time, *modifiers, |time| time.floor(snap_step));

					shell.publish((self.action)(Action::Add(None, Some(track), time)));

					state.status = Status::Dragging(track, new_time);
				}

				shell.capture_event();
				shell.request_redraw();
			}
			Event::Mouse(mouse::Event::ButtonPressed {
				button: mouse::Button::Right,
				modifiers,
			}) => {
				// the menu of the last thing the user right-clicked on is gone
				state.context_menu = None;

				if state.status != Status::None {
					return;
				}

				let track =
					track_index(&layout, cursor).or_else(|| layout.children().len().checked_sub(1));

				// the clip the user right-clicked on, if there is one
				let clip_target = track.and_then(|track| {
					self.tracks[track]
						.clips
						.iter()
						.enumerate()
						.find(|(clip, _)| {
							layout.child(track).child(*clip).bounds().contains(cursor)
						})
						.map(|(clip, inner)| (track, clip, inner))
				});

				if modifiers.command() {
					// select the clip, so that it can be deleted
					state.primary.clear();

					if let Some(track) = track {
						let time = self
							.grid
							.maybe_snap(new_time, *modifiers, |time| time.round(snap_step));

						if let Some((_, _, clip)) = clip_target {
							let clip_index = match clip.inner {
								clip::Inner::AudioClip(inner) => Some(inner.index),
								clip::Inner::MidiClip(inner) => Some(inner.index),
								_ => None,
							};
							if let Some(index) = clip_index {
								state.primary.insert(index);
							}
						}

						state.status = Status::Selecting(track, track, time, time, true);
					}

					shell.capture_event();
				} else {
					// the menu of the clip, or of the background
					state.context_menu = Some(match (clip_target, track) {
						(Some((track, clip, _)), _) => {
							let inner = &self.tracks[track].clips[clip].inner;

							match inner {
								clip::Inner::AudioClip(_) => ContextTarget::AudioClip(track, clip),
								clip::Inner::MidiClip(_) => ContextTarget::MidiClip(track, clip),
								clip::Inner::AudioRecording(..)
								| clip::Inner::MidiRecording(..) => ContextTarget::EmptyTrack {
									track,
									time: new_time,
								},
							}
						}
						(None, Some(track)) => ContextTarget::EmptyTrack {
							track,
							time: new_time,
						},
						(None, None) => ContextTarget::EmptyArea { time: new_time },
					});
				}

				shell.request_redraw();
			}
			Event::Mouse(mouse::Event::CursorMoved { modifiers, .. })
			| Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) => match state.status.clone() {
				Status::Hovering(path, kind, time) => {
					let track = track_index(&layout, cursor);

					let new_time = self
						.grid
						.maybe_snap(new_time, *modifiers, |time| time.floor(snap_step));

					let new_time = Some((track, new_time));

					if time != new_time {
						state.status = Status::Hovering(path, kind, new_time);
						shell.capture_event();
						shell.request_redraw();
					}
				}
				Status::Selecting(start_track, last_end_track, start_pos, last_end_pos, delete) => {
					let Some(end_track) = track_index(&layout, cursor)
						.or_else(|| layout.children().len().checked_sub(1))
					else {
						return;
					};

					let end_pos = self
						.grid
						.maybe_snap(new_time, *modifiers, |time| time.round(snap_step));

					if end_track == last_end_track && end_pos == last_end_pos {
						return;
					}

					state.status =
						Status::Selecting(start_track, end_track, start_pos, end_pos, delete);

					let (start_track, end_track) =
						(start_track.min(end_track), start_track.max(end_track));
					let (start_pos, end_pos) = (start_pos.min(end_pos), start_pos.max(end_pos));

					state
						.primary
						.retain(|&(track, _)| (start_track..=end_track).contains(&track));

					self.tracks[start_track..=end_track]
						.iter()
						.flat_map(|track| &track.clips)
						.for_each(|clip| {
							let (start, end, index) = match clip.inner {
								clip::Inner::AudioClip(inner) => (
									inner.clip.position.start(),
									inner.clip.position.end(self.transport),
									inner.index,
								),
								clip::Inner::MidiClip(inner) => (
									inner.clip.position.start(),
									inner.clip.position.end(),
									inner.index,
								),
								clip::Inner::AudioRecording(..)
								| clip::Inner::MidiRecording(..) => {
									return;
								}
							};

							if start_pos.max(start) < end_pos.min(end) {
								state.primary.insert(index);
							} else {
								state.primary.remove(&index);
							}
						});

					shell.request_redraw();
				}
				Status::Dragging(track, time) => {
					let Some(new_track) = track_index(&layout, cursor)
						.or_else(|| layout.children().len().checked_sub(1))
					else {
						return;
					};

					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if new_track != track || abs_diff != BeatTime::ZERO {
						let track_delta = if new_track > track {
							Delta::Positive
						} else {
							Delta::Negative
						}(new_track.abs_diff(track));

						let time_delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::Dragging(new_track, time + time_delta);
						shell.publish((self.action)(Action::Drag(track_delta, time_delta)));
						shell.capture_event();
					}
				}
				Status::TrimmingStart(time) => {
					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if abs_diff != BeatTime::ZERO {
						let delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::TrimmingStart(time + delta);
						shell.publish((self.action)(if modifiers.shift() {
							Action::StretchStart(delta)
						} else {
							Action::TrimStart(delta)
						}));
						shell.capture_event();
					}
				}
				Status::TrimmingEnd(time) => {
					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if abs_diff != BeatTime::ZERO {
						let delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::TrimmingEnd(time + delta);
						shell.publish((self.action)(if modifiers.shift() {
							Action::StretchEnd(delta)
						} else {
							Action::TrimEnd(delta)
						}));
						shell.capture_event();
					}
				}
				Status::DraggingVolume(last_y) => {
					let y = cursor.y + layout.position().y;
					state.status = Status::DraggingVolume(y);
					shell.publish((self.action)(Action::DragVolume(
						(last_y - y) / LINE_HEIGHT,
					)));
					shell.capture_event();
				}
				Status::FadingStartLen(time) => {
					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if abs_diff != BeatTime::ZERO {
						let delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::FadingStartLen(time + delta);
						shell.publish((self.action)(Action::FadeStartLen(delta)));
						shell.capture_event();
					}
				}
				Status::FadingStartP(track, clip) => {
					let clip::Inner::AudioClip(inner) = self.tracks[track].clips[clip].inner else {
						panic!();
					};

					let frames_per_px = frames_per_px(state.scale, self.transport);
					let fade_start_px =
						inner.clip.fade_start.len.to_frames(self.transport) as f32 / frames_per_px;

					let clip_bounds = layout.child(track).child(clip).bounds()
						- Vector::new(layout.position().x, layout.position().y);

					let x = ((cursor.x - clip_bounds.x) / fade_start_px).clamp(0.0, 1.0);
					let y = ((clip_bounds.height + clip_bounds.y - cursor.y)
						/ (clip_bounds.height - LINE_HEIGHT))
						.clamp(0.0, 1.0);

					shell.publish((self.action)(Action::FadeStartP(Point::new(x, y))));
				}
				Status::FadingEndLen(time) => {
					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if abs_diff != BeatTime::ZERO {
						let delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::FadingEndLen(time + delta);
						shell.publish((self.action)(Action::FadeEndLen(-delta)));
						shell.capture_event();
					}
				}
				Status::FadingEndP(track, clip) => {
					let clip::Inner::AudioClip(inner) = self.tracks[track].clips[clip].inner else {
						panic!();
					};

					let frames_per_px = frames_per_px(state.scale, self.transport);
					let fade_end_px =
						inner.clip.fade_end.len.to_frames(self.transport) as f32 / -frames_per_px;

					let clip_bounds = layout.child(track).child(clip).bounds()
						- Vector::new(layout.position().x, layout.position().y);

					let x = ((cursor.x - clip_bounds.x - clip_bounds.width) / fade_end_px)
						.clamp(0.0, 1.0);
					let y = ((clip_bounds.y + clip_bounds.height - cursor.y)
						/ (clip_bounds.height - LINE_HEIGHT))
						.clamp(0.0, 1.0);

					shell.publish((self.action)(Action::FadeEndP(Point::new(x, y))));
				}
				Status::DraggingSplit(time) => {
					let new_time = self
						.grid
						.maybe_snap(new_time, *modifiers, |time| time.round(snap_step));

					if new_time != time {
						state.status = Status::DraggingSplit(new_time);
						shell.publish((self.action)(Action::DragSplit(new_time)));
						shell.capture_event();
					}
				}
				Status::DraggingSlip(time) => {
					let abs_diff =
						self.grid
							.maybe_snap(new_time.abs_diff(time), *modifiers, |abs_diff| {
								abs_diff.round(snap_step)
							});

					if abs_diff != BeatTime::ZERO {
						let delta = if new_time > time {
							Delta::Positive
						} else {
							Delta::Negative
						}(abs_diff);

						state.status = Status::DraggingSlip(time + delta);
						shell.publish((self.action)(Action::DragSlip(-delta)));
						shell.capture_event();
					}
				}
				Status::None => {}
			},
			_ => {}
		}
	}

	fn draw(
		&self,
		tree: &Tree,
		renderer: &mut Renderer,
		theme: &Theme,
		style: &Style,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
	) {
		let state = self.state.borrow();

		for layout in layout.children() {
			let Some(bounds) = Rectangle::new(
				layout.position() + Vector::new(0.0, layout.bounds().height - 1.0),
				Size::new(f32::INFINITY, 1.0),
			)
			.intersection(viewport) else {
				continue;
			};

			renderer.fill_quad(
				Quad {
					bounds,
					..Quad::default()
				},
				theme.palette().background.strong.color,
			);
		}

		let mut allocs = Vec::new();
		let active = &mut Vec::<(usize, _)>::new();

		layout
			.children()
			.enumerate()
			.filter(|(_, track_layout)| track_layout.bounds().intersects(viewport))
			.for_each(|(track, track_layout)| {
				active.clear();

				for (clip, layout) in track_layout.children().enumerate() {
					let Some(bounds) = layout.bounds().intersection(viewport) else {
						continue;
					};

					active.retain(|&(_, e)| e > bounds.x);
					let layer = active
						.iter()
						.map(|&(l, _)| l)
						.max()
						.map_or_default(|l| l + 1);
					active.push((layer, bounds.x + bounds.width));

					if layer == allocs.len() {
						allocs.push(Vec::new());
					}

					allocs[layer].push((track, clip));
				}
			});

		for (i, alloc) in allocs.into_iter().enumerate() {
			if i != 0 {
				renderer.start_layer(Rectangle::INFINITE);
			}

			for (track, clip) in alloc {
				self.tracks[track].clips[clip].draw(
					&tree.children[track].children[clip],
					renderer,
					theme,
					style,
					layout.child(track).child(clip),
					cursor,
					viewport,
				);
			}

			if i != 0 {
				renderer.end_layer();
			}
		}

		renderer.with_layer(*viewport, |renderer| match state.status {
			Status::Hovering(_, _, Some((Some(track), time))) => {
				let bounds = layout.child(track).bounds();
				renderer.fill_quad(
					Quad {
						bounds: Rectangle::new(
							bounds.position()
								+ Vector::new(
									time_to_px(time, state.position, state.scale, self.transport),
									0.0,
								),
							Size::new(50.0, bounds.height),
						),
						..Quad::default()
					},
					Linear::new(FRAC_PI_2)
						.add_stop(
							0.0,
							theme
								.palette()
								.background
								.strong
								.text
								.scale_alpha(ALPHA_1_3),
						)
						.add_stop(1.0, Color::TRANSPARENT),
				);
			}
			Status::Hovering(_, _, Some(_)) => {
				renderer.fill_quad(
					Quad {
						bounds: Rectangle::new(
							layout.children().next_back().map_or_else(
								|| layout.position(),
								|layout| {
									layout.position() + Vector::new(0.0, layout.bounds().height)
								},
							),
							Size::new(viewport.width, 50.0),
						),
						..Quad::default()
					},
					Linear::new(PI)
						.add_stop(
							0.0,
							theme
								.palette()
								.background
								.strong
								.text
								.scale_alpha(ALPHA_1_3),
						)
						.add_stop(1.0, Color::TRANSPARENT),
				);
			}
			Status::Selecting(start_track, end_track, start_pos, end_pos, _)
				if start_pos != end_pos =>
			{
				let (start_track, end_track) =
					(start_track.min(end_track), start_track.max(end_track));
				let (start_pos, end_pos) = (start_pos.min(end_pos), start_pos.max(end_pos));

				let y = layout.child(start_track).position().y;
				let height = layout.child(end_track).position().y
					+ layout.child(end_track).bounds().height
					- y;

				let x = time_to_px(start_pos, state.position, state.scale, self.transport);
				let width = time_to_px(end_pos, state.position, state.scale, self.transport) - x;
				let x = x + viewport.x;

				renderer.fill_quad(
					Quad {
						bounds: Rectangle {
							x,
							y,
							width,
							height,
						},
						border: border::width(1).color(theme.palette().danger.weak.color),
						..Quad::default()
					},
					theme.palette().danger.weak.color.scale_alpha(ALPHA_1_3),
				);
			}
			_ => {}
		});
	}

	fn mouse_interaction(
		&self,
		tree: &Tree,
		layout: Layout<'_>,
		cursor: Cursor,
		viewport: &Rectangle,
		renderer: &Renderer,
	) -> Interaction {
		match self.state.borrow().status {
			Status::Hovering(..) => Interaction::Copy,
			Status::Selecting(.., false) => Interaction::Idle,
			Status::Selecting(.., true) => Interaction::NoDrop,
			Status::Dragging(..) => Interaction::Grabbing,
			Status::TrimmingStart(..)
			| Status::TrimmingEnd(..)
			| Status::DraggingSplit(..)
			| Status::DraggingSlip(..) => Interaction::ResizingHorizontally,
			Status::DraggingVolume(..) => Interaction::ResizingVertically,
			Status::FadingStartLen(..) | Status::FadingEndLen(..) => Interaction::Pointer,
			Status::FadingStartP(..) | Status::FadingEndP(..) => Interaction::Crosshair,
			Status::None => {
				let Some(position) = cursor.position_from(layout.position()) else {
					return Interaction::default();
				};
				let Some(track) = track_index(&layout, position) else {
					return Interaction::default();
				};

				self.tracks[track].mouse_interaction(
					&tree.children[track],
					layout.child(track),
					cursor,
					viewport,
					renderer,
				)
			}
		}
	}

	fn overlay<'b>(
		&'b mut self,
		tree: &'b mut Tree,
		layout: Layout<'b>,
		renderer: &Renderer,
		viewport: &Rectangle,
		translation: Vector,
	) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
		let children = self
			.tracks
			.iter_mut()
			.zip(&mut tree.children)
			.zip(layout.children())
			.filter(|((_, _), layout)| layout.bounds().intersects(viewport))
			.filter_map(|((child, tree), layout)| {
				child.overlay(tree, layout, renderer, viewport, translation)
			})
			.collect::<Vec<_>>();

		(!children.is_empty()).then(|| overlay::Group::with_children(children).overlay())
	}

	fn operate(
		&mut self,
		tree: &mut Tree,
		layout: Layout<'_>,
		renderer: &Renderer,
		operation: &mut dyn Operation,
	) {
		operation.container(None, layout.bounds());
		operation.traverse(&mut |operation| {
			self.tracks
				.iter_mut()
				.zip(&mut tree.children)
				.zip(layout.children())
				.for_each(|((child, tree), layout)| {
					child.operate(tree, layout, renderer, operation);
				});
		});
	}
}

impl<'a, Message: 'a> Playlist<'a, Message> {
	pub fn new(
		state: &'a RefCell<State>,
		transport: &'a Transport,
		grid: &'a Grid,
		tracks: impl IntoIterator<Item = Track<'a, Message>>,
		action: fn(Action) -> Message,
	) -> Self {
		Self {
			state,
			transport,
			grid,
			tracks: tracks.into_iter().collect(),
			action,
		}
	}
}

impl<'a, Message: 'a> From<Playlist<'a, Message>> for Element<'a, Message> {
	fn from(value: Playlist<'a, Message>) -> Self {
		Self::new(value)
	}
}

fn track_index(layout: &Layout<'_>, cursor: Point) -> Option<usize> {
	layout.children().position(|child| {
		(child.bounds() - Vector::new(layout.position().x, layout.position().y)).contains(cursor)
	})
}
