use crate::{
	arrangement_view::{Message, Tab, format_pan, plugin::Plugin},
	components::{MenuItem, menu, menu_entry, menu_width},
	icons::{
		arrow_up_down, between_horizontal_start, between_vertical_start,
		chevrons_left_right_ellipsis, circle_ellipsis, copy, pencil, power, power_off, replace,
		rotate_ccw, route, snowflake, star, trash_2, volume_2, volume_off,
	},
	shortcuts::{Action as ShortcutAction, Manager},
	stylefns::{container_with_radius, scrollable_with_container, weaker_bordered_box},
};
use generic_daw_core::{Channels, NodeId, PanMode, Transport, Utility};
use generic_daw_widget::{context_menu::ContextMenu, knob::Knob, peak_meter};
use iced::{
	Center, Element, Fill, Fit, padding,
	widget::{self, checkbox, column, container, radio, row, rule, scrollable, space, text},
};
use std::{collections::BTreeMap, sync::Arc, time::Instant};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum NodeType {
	Master,
	Channel,
	Track,
}

#[derive(Debug)]
pub struct Node {
	pub ty: NodeType,
	pub id: NodeId,
	pub name: Arc<str>,
	pub widget_id: widget::Id,
	/// The identifier of the field the name of the node is written in.
	pub name_id: widget::Id,
	pub plugins: Vec<Plugin>,
	pub utility: Utility,
	pub enabled: bool,
	pub bypassed: bool,
	pub outgoing: BTreeMap<NodeId, f32>,
	pub output: Channels,
	pub peaks: [peak_meter::State; 2],
	pub polyphony: usize,
}

impl Node {
	pub fn new(ty: NodeType, id: NodeId, output: Channels) -> Self {
		Self {
			ty,
			id,
			name: Arc::default(),
			widget_id: widget::Id::unique(),
			name_id: widget::Id::unique(),
			plugins: Vec::new(),
			utility: Utility::default(),
			enabled: true,
			bypassed: false,
			outgoing: BTreeMap::new(),
			output,
			peaks: [peak_meter::State::default(), peak_meter::State::default()],
			polyphony: 0,
		}
	}

	pub fn update(&mut self, peaks: [f32; 2], now: Instant) {
		self.peaks[0].update(peaks[0], now);
		self.peaks[1].update(peaks[1], now);
	}

	/// The menu of a track header or of a channel in the mixer.
	pub fn main_context_menu<'a>(
		&'a self,
		tab: Tab,
		shortcuts: &'a Manager,
		routes: Vec<MenuItem<Message>>,
	) -> Element<'a, Message> {
		let mut items: Vec<MenuItem<Message>> = Vec::new();

		if self.ty != NodeType::Master {
			items.push(MenuItem::Action {
				icon: Some(if tab == Tab::Mixer {
					between_vertical_start()
				} else {
					between_horizontal_start()
				}),
				label: "Insert after".into(),
				shortcut: String::new(),
				message: match self.ty {
					NodeType::Channel => Message::ChannelInsert(self.id),
					NodeType::Track => Message::TrackInsert(self.id),
					NodeType::Master => unreachable!(),
				},
			});

			items.push(MenuItem::Action {
				icon: Some(copy()),
				label: "Duplicate".into(),
				shortcut: shortcuts.label(ShortcutAction::Duplicate),
				message: match self.ty {
					NodeType::Channel => Message::ChannelDuplicate(self.id),
					NodeType::Track => Message::TrackDuplicate(self.id),
					NodeType::Master => unreachable!(),
				},
			});
		}

		if tab == Tab::Mixer && self.ty != NodeType::Master {
			items.push(MenuItem::Action {
				icon: Some(replace()),
				label: if self.ty == NodeType::Channel {
					"Convert to track".into()
				} else {
					"Convert to channel".into()
				},
				shortcut: String::new(),
				message: Message::ToggleKind(self.id),
			});
		}

		items.push(MenuItem::Separator);

		items.push(MenuItem::Action {
			icon: Some(pencil()),
			label: "Rename".into(),
			shortcut: String::new(),
			message: Message::ChannelRename(self.id),
		});

		if self.ty != NodeType::Master {
			items.push(MenuItem::Action {
				icon: Some(trash_2()),
				label: "Remove".into(),
				shortcut: String::new(),
				message: match self.ty {
					NodeType::Channel => Message::ChannelRemove(self.id),
					NodeType::Track => Message::TrackRemove(self.id),
					NodeType::Master => unreachable!(),
				},
			});
		}

		if self.ty == NodeType::Track {
			items.push(MenuItem::Action {
				icon: Some(snowflake()),
				label: "Freeze".into(),
				shortcut: String::new(),
				message: Message::Freeze(self.id),
			});
		}

		items.push(MenuItem::Separator);

		items.push(MenuItem::Action {
			icon: Some(if self.bypassed { power_off() } else { power() }),
			label: if self.bypassed {
				"Engage FX".into()
			} else {
				"Bypass FX".into()
			},
			shortcut: String::new(),
			message: Message::ChannelToggleBypassed(self.id),
		});

		items.push(MenuItem::Action {
			icon: Some(if self.enabled { volume_2() } else { volume_off() }),
			label: if self.enabled { "Mute".into() } else { "Unmute".into() },
			shortcut: shortcuts.label(ShortcutAction::ToggleEnabled),
			message: match self.ty {
				NodeType::Track => Message::TrackToggleEnabled(self.id),
				_ => Message::ChannelToggleEnabled(self.id),
			},
		});

		if self.ty == NodeType::Track {
			items.push(MenuItem::Action {
				icon: Some(star()),
				label: "Solo".into(),
				shortcut: shortcuts.label(ShortcutAction::ToggleSolo),
				message: Message::TrackToggleSolo(self.id),
			});
		}

		items.push(MenuItem::Separator);

		items.push(MenuItem::Action {
			icon: Some(arrow_up_down()),
			label: "Invert polarity".into(),
			shortcut: shortcuts.label(ShortcutAction::Invert),
			message: Message::ChannelVolumeChanged(self.id, -self.utility.volume),
		});

		if tab == Tab::Mixer && !routes.is_empty() {
			items.push(MenuItem::Separator);

			items.push(MenuItem::Submenu {
				icon: Some(route()),
				label: "Route to".into(),
				items: routes,
			});
		}

		match self.utility.pan {
			PanMode::Stereo(..) => items.push(MenuItem::Action {
				icon: Some(chevrons_left_right_ellipsis()),
				label: "Split stereo pan".into(),
				shortcut: String::new(),
				message: Message::ChannelPanChanged(self.id, PanMode::SplitStereo(-1.0, 1.0)),
			}),
			PanMode::SplitStereo(..) => items.push(MenuItem::Action {
				icon: Some(circle_ellipsis()),
				label: "Stereo pan".into(),
				shortcut: String::new(),
				message: Message::ChannelPanChanged(self.id, PanMode::Stereo(0.0)),
			}),
		}

		menu(items)
	}

	pub fn volume_context_menu(&self, tab: Tab) -> Element<'_, Message> {
		menu_width(
			vec![
				MenuItem::Action {
					icon: Some(rotate_ccw()),
					label: "Reset".into(),
					shortcut: "Ctrl-Click".into(),
					message: Message::ChannelVolumeChanged(self.id, 1.0),
				},
				MenuItem::Separator,
				MenuItem::Action {
					icon: Some(arrow_up_down()),
					label: "Invert polarity".into(),
					shortcut: if tab == Tab::Mixer {
						"Alt-I".into()
					} else {
						String::new()
					},
					message: Message::ChannelVolumeChanged(self.id, -self.utility.volume),
				},
			],
			if tab == Tab::Mixer { 180.0 } else { 160.0 },
		)
	}

	pub fn pan_knob(&self, radius: f32, enabled: bool) -> Element<'_, Message> {
		const RADIUS: f32 = 0.571_595_13; // 1.95 - sqrt(1.9)
		const SPACING: f32 = -0.286_380_5; // 2 * (2 * sqrt(1.9) - 2.9)

		match self.utility.pan {
			PanMode::Stereo(pan) => ContextMenu::new(
				Knob::new(-1.0..=1.0, pan, |pan| {
					Message::ChannelPanChanged(self.id, PanMode::Stereo(pan))
				})
				.origin(0.0)
				.default(0.0)
				.radius(radius)
				.enabled(enabled)
				.tooltip(format_pan(pan)),
				move || {
					container(column![
						menu_entry(rotate_ccw(), "Reset", "Ctrl-Click")
							.on_press(Message::ChannelPanChanged(self.id, PanMode::Stereo(0.0))),
						container(rule::horizontal(1)).padding(padding::horizontal(5)),
						menu_entry(chevrons_left_right_ellipsis(), "Split stereo pan", "")
							.on_press(Message::ChannelPanChanged(
								self.id,
								PanMode::SplitStereo(-1.0, 1.0)
							)),
					])
					.width(160)
					.style(container_with_radius(weaker_bordered_box, 5))
					.into()
				},
			)
			.into(),
			PanMode::SplitStereo(l, r) => ContextMenu::new(
				row![
					container(ContextMenu::new(
						Knob::new(-1.0..=1.0, l, move |l| {
							Message::ChannelPanChanged(self.id, PanMode::SplitStereo(l, r))
						})
						.origin(0.0)
						.default(-1.0)
						.radius(radius * RADIUS)
						.enabled(enabled)
						.tooltip(format_pan(l)),
						move || container(column![
							menu_entry(rotate_ccw(), "Reset", "Ctrl-Click").on_press(
								Message::ChannelPanChanged(self.id, PanMode::SplitStereo(-1.0, r))
							),
							container(rule::horizontal(1)).padding(padding::horizontal(5)),
							menu_entry(circle_ellipsis(), "Stereo pan", "").on_press(
								Message::ChannelPanChanged(self.id, PanMode::Stereo(0.0))
							),
						])
						.width(160)
						.style(container_with_radius(weaker_bordered_box, 5))
						.into()
					))
					.align_top(Fill),
					container(ContextMenu::new(
						Knob::new(-1.0..=1.0, r, move |r| {
							Message::ChannelPanChanged(self.id, PanMode::SplitStereo(l, r))
						})
						.origin(0.0)
						.default(1.0)
						.radius(radius * RADIUS)
						.enabled(enabled)
						.tooltip(format_pan(r)),
						move || container(column![
							menu_entry(rotate_ccw(), "Reset", "Ctrl-Click").on_press(
								Message::ChannelPanChanged(self.id, PanMode::SplitStereo(l, 1.0))
							),
							container(rule::horizontal(1)).padding(padding::horizontal(5)),
							menu_entry(circle_ellipsis(), "Stereo pan", "").on_press(
								Message::ChannelPanChanged(self.id, PanMode::Stereo(0.0))
							),
						])
						.width(160)
						.style(container_with_radius(weaker_bordered_box, 5))
						.into()
					))
					.align_bottom(Fill)
				]
				.spacing(radius * SPACING)
				.width(2.0 * radius)
				.height(1.8 * radius),
				move || {
					container(
						menu_entry(circle_ellipsis(), "Stereo pan", "")
							.on_press(Message::ChannelPanChanged(self.id, PanMode::Stereo(0.0))),
					)
					.width(160)
					.style(container_with_radius(weaker_bordered_box, 5))
					.into()
				},
			)
			.into(),
		}
	}

	pub fn audio_output_context_menu(&self, transport: &Transport) -> Element<'_, Message> {
		container(
			row![
				column![
					space().width(15).height(15),
					container(text("L").size(13).line_height(1.0)).padding(1),
					container(text("R").size(13).line_height(1.0)).padding(1),
				]
				.align_x(Center)
				.spacing(5),
				scrollable(
					row((0..transport.output_channels.get()).map(|channel| {
						column![
							container(text(channel + 1).size(13).line_height(1.0)).padding(1),
							radio(channel, Some(self.output.left), |_| {
								Message::OutputChangeChannels(self.id, self.output.left(channel))
							})
							.size(15),
							radio(channel, Some(self.output.right), |_| {
								Message::OutputChangeChannels(self.id, self.output.right(channel))
							})
							.size(15)
						]
						.align_x(Center)
						.spacing(5)
						.into()
					}))
					.spacing(5)
				)
				.direction(scrollable::Direction::Horizontal(
					scrollable::Scrollbar::hidden(),
				))
				.width(Fit.max(328))
			]
			.padding(5)
			.spacing(5),
		)
		.style(container_with_radius(weaker_bordered_box, 5))
		.into()
	}

	pub fn midi_output_context_menu(&self) -> Element<'_, Message> {
		scrollable(
			row((0..16).map(|channel| {
				column![
					container(text(channel + 1).size(13).line_height(1.0)).padding(1),
					checkbox(self.output.midi & (1 << channel) != 0)
						.on_toggle(move |_| {
							Message::OutputChangeChannels(
								self.id,
								self.output.midi(self.output.midi ^ (1 << channel)),
							)
						})
						.size(15)
				]
				.align_x(Center)
				.spacing(5)
				.into()
			}))
			.padding(5)
			.spacing(5),
		)
		.direction(scrollable::Direction::Horizontal(
			scrollable::Scrollbar::hidden(),
		))
		.style(scrollable_with_container(
			scrollable::default,
			container_with_radius(weaker_bordered_box, 5),
		))
		.into()
	}
}
