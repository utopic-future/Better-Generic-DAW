use super::*;
use iced::widget::column;

impl ArrangementView {
	pub(super) fn view_mixer<'a>(&'a self, state: &'a State) -> Element<'a, Message> {
		let node = self.arrangement.node(self.selected);

		let enabled = node.enabled
			&& (node.ty != NodeType::Track
				|| self.arrangement.solo().is_none_or(|solo| solo == node.id));

		let routing_targets = self
			.arrangement
			.channels()
			.iter()
			.map(|channel| channel.id)
			.chain(self.arrangement.tracks().iter().map(|track| track.id))
			.filter(|target| *target != node.id)
			.collect::<Vec<_>>();

		let routing: Element<'a, Message> = if routing_targets.is_empty() {
			text("No routing targets available").size(12).into()
		} else {
			sweeten::column(routing_targets.iter().map(|target_id| {
				let target_id = *target_id;
				let target = self.arrangement.node(target_id);
				let mix = node.outgoing.get(&target_id).copied();

				let mix_slider: Element<'a, Message> = if let Some(mix) = mix {
					ContextMenu::new(
						slider(0.0..=1.0, mix.cbrt(), move |value| {
							Message::SetMix(node.id, target_id, value.powi(3))
						})
						.default(1.0)
						.step(f32::EPSILON)
						.handle((4, 4))
						.height(14)
						.style(if enabled {
							slider::primary
						} else {
							slider::secondary
						}),
						move || {
							container(menu_entry(rotate_ccw(), "Reset", "Ctrl-Click")
								.on_press(Message::SetMix(node.id, target_id, 1.0)))
							.width(160)
							.style(container_with_radius(weaker_bordered_box, 5))
							.into()
						},
					)
					.into()
				} else {
					space().height(0).into()
				};

				column![
					row![
						text(&*target.name).width(Fill),
						text(mix.map_or_else(
							|| "Not connected".to_owned(),
							|mix| format_mix(mix),
						))
						.size(12),
						icon_button(
							if mix.is_some() { check() } else { plus() },
							if mix.is_some() {
								button::primary
							} else {
								button::secondary
							}
						)
						.on_press(if mix.is_some() {
							Message::Disconnect(node.id, target_id)
						} else {
							Message::Connect(node.id, target_id)
						}),
					]
					.align_y(Center)
					.spacing(5),
					mix_slider,
				]
				.spacing(2)
				.into()
			}))
			.spacing(3)
			.into()
		};

		Split::new(
			row![
				self.view_channel(state, self.arrangement.master(), "M", None),
				(!self.arrangement.tracks().is_empty()).then(|| rule::vertical(1)),
				scrollable(
					row![
						(!self.arrangement.tracks().is_empty()).then(|| column![
							text("TRACKS").size(11),
							sweeten::row(
								self.arrangement
									.tracks()
									.iter()
									.enumerate()
									.map(|(i, track)| self.view_channel(
										state,
										self.arrangement.node(track.id),
										format!("T{}", i + 1),
										Some(track)
									))
							)
							.on_drag(Message::TrackMove)
							.style(sweeten_row_with_radius(sweeten_row_style, border::top(5)))
							.spacing(5)
						]
						.align_x(Center)
						.spacing(3)),
						(!self.arrangement.channels().is_empty()).then(|| rule::vertical(1)),
						(!self.arrangement.channels().is_empty()).then(|| column![
							text("BUSES").size(11),
							sweeten::row(
								self.arrangement
									.channels()
									.iter()
									.enumerate()
									.map(|(i, channel)| self.view_channel(
										state,
										self.arrangement.node(channel.id),
										format!("C{}", i + 1),
										None
									))
							)
							.on_drag(Message::ChannelMove)
							.style(sweeten_row_with_radius(sweeten_row_style, border::top(5)))
							.spacing(5)
						]
						.align_x(Center)
						.spacing(3)),
						button(plus().size(LINE_HEIGHT + 6.0))
							.padding(5)
							.style(button_with_radius(button::primary, f32::INFINITY))
							.on_press(Message::ChannelAdd)
					]
					.height(Fill)
					.align_y(Center)
					.spacing(5),
				)
				.id("mixer")
				.direction(scrollable::Direction::Horizontal(
					scrollable::Scrollbar::hidden(),
				))
			]
			.spacing(5)
			.width(Fill.min(MIN_SPLIT_WIDTH)),
			scrollable(
				column![
					container(
						row![
							column![
								text(&*node.name).size(16),
								text(format!("{} effects", node.plugins.len())).size(12),
							]
							.spacing(2),
							space().width(Fill),
							button(row![plus().size(TEXT_HEIGHT), text("Add effect")].align_y(Center).spacing(5))
								.padding(8)
								.style(button_with_radius(button::primary, 5))
								.on_press(Message::PluginPicker),
						]
						.align_y(Center)
						.spacing(8)
					)
					.padding(8)
					.style(container_with_radius(weak_bordered_box, 5)),
					sweeten::column(node.plugins.iter().enumerate().map(|(i, plugin)| {
						let button_style = |cond: bool| {
							if !plugin.active || !enabled {
								button::secondary
							} else if cond {
								button::warning
							} else {
								button::primary
							}
						};

						row![
							opaque(ContextMenu::new(
								row![
									ContextMenu::new(
										Knob::new(0.0..=1.0, plugin.mix.abs(), move |mix| {
											Message::PluginMixChanged(
												self.selected,
												i,
												mix.copysign(plugin.mix),
											)
										})
										.radius(19.44444)
										.enabled(plugin.active && enabled)
										.tooltip(format_mix(plugin.mix)),
										move || container(column![
											menu_entry(rotate_ccw(), "Reset", "Ctrl-Click")
												.on_press(Message::PluginMixChanged(
													self.selected,
													i,
													1.0
												)),
											container(rule::horizontal(1))
												.padding(padding::horizontal(5)),
											menu_entry(arrow_up_down(), "Invert polarity", "")
												.on_press(Message::PluginMixChanged(
													self.selected,
													i,
													-plugin.mix
												)),
										])
										.width(160)
										.style(container_with_radius(weaker_bordered_box, 5))
										.into(),
									),
									button(
										text(plugin.descriptor.name())
											.wrapping(text::Wrapping::None)
											.ellipsis(text::Ellipsis::End)
									)
									.padding((35.0 - LINE_HEIGHT) / 2.0)
									.style(button_with_radius(button_style(false), border::left(5)))
									.width(Fill)
									.on_press(Message::PluginShow(plugin.id)),
									column![
										icon_button(
											if plugin.active && !node.bypassed {
												power()
											} else {
												power_off()
											},
											button_style(node.bypassed)
										)
										.on_press(Message::PluginToggleActive(self.selected, i)),
										icon_button(
											x(),
											if plugin.active && enabled {
												button::danger
											} else {
												button::secondary
											}
										)
										.on_press(Message::PluginRemove(self.selected, i)),
									]
									.spacing(5),
								]
								.align_y(Center)
								.spacing(5),
								move || {
									menu_width(
										vec![
											MenuItem::Action {
												icon: Some(sliders_vertical()),
												label: "Show editor".into(),
												shortcut: String::new(),
												message: Message::PluginShow(plugin.id),
											},
											MenuItem::Action {
												icon: Some(copy()),
												label: "Duplicate".into(),
												shortcut: String::new(),
												message: Message::PluginDuplicate(self.selected, i),
											},
											MenuItem::Action {
												icon: Some(clipboard_paste()),
												label: "Copy state".into(),
												shortcut: String::new(),
												message: Message::PluginCopyState(self.selected, i),
											},
											MenuItem::Action {
												icon: Some(clipboard_paste()),
												label: "Paste state".into(),
												shortcut: String::new(),
												message: Message::PluginPasteState(
													self.selected,
													i,
												),
											},
											MenuItem::Action {
												icon: Some(rotate_ccw()),
												label: "Reset mix".into(),
												shortcut: "Ctrl-Click".into(),
												message: Message::PluginMixChanged(
													self.selected,
													i,
													1.0,
												),
											},
											MenuItem::Action {
												icon: Some(arrow_up_down()),
												label: "Invert polarity".into(),
												shortcut: String::new(),
												message: Message::PluginMixChanged(
													self.selected,
													i,
													-plugin.mix,
												),
											},
											MenuItem::Action {
												icon: Some(trash_2()),
												label: "Remove".into(),
												shortcut: String::new(),
												message: Message::PluginRemove(self.selected, i),
											},
											MenuItem::Separator,
										],
										160.0,
									)
								},
							)),
							mouse_area(
								container(grip_vertical())
									.center_y(LINE_HEIGHT + 14.0)
									.style(container_with_radius(
										weak_bordered_box,
										border::right(5)
									))
							)
							.interaction(Interaction::Grab),
						]
						.align_y(Center)
						.spacing(5)
						.into()
					}))
					.spacing(5)
					.on_drag(|node| Message::PluginMove(self.selected, node))
					.style(sweeten_column_with_radius(sweeten_column_style, 5)),
					container(
						column![
							row![
								text("Routing").size(14),
								space().width(Fill),
								text(format!("{} outputs", node.outgoing.len())).size(12),
							]
							.align_y(Center),
							routing
						]
						.spacing(5)
					)
					.padding(8)
					.style(container_with_radius(weaker_bordered_box, 5)),
				]
				.width(Fill)
				.align_x(Center)
				.spacing(5),
			)
			.direction(scrollable::Direction::Vertical(
				scrollable::Scrollbar::hidden(),
			))
			.width(Fill.min(MIN_SPLIT_WIDTH)),
			state.plugins_pane_split_at,
		)
		.on_drag(Message::OnDrag)
		.on_drag_end(Message::OnDragEnd)
		.on_double_click(Message::OnDoubleClick)
		.strategy(Strategy::End)
		.focus_delay(Duration::ZERO)
		.style(split_style)
		.into()
	}

	fn view_channel<'a>(
		&'a self,
		state: &'a State,
		node: &'a Node,
		placeholder: impl text::IntoFragment<'a>,
		track: Option<&'a track::Track>,
	) -> Element<'a, Message> {
		let enabled = node.enabled
			&& (node.ty != NodeType::Track
				|| self.arrangement.solo().is_none_or(|solo| solo == node.id));

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
			container(column![
				mouse_area(center_x(grip_horizontal())).interaction(
					if node.ty == NodeType::Master {
						Interaction::NoDrop
					} else {
						Interaction::Grab
					}
				),
				opaque(
					mouse_area(ContextMenu::new(
						column![
							text_input(placeholder, &*node.name)
								.id(node.name_id.clone())
								.on_input(|name| Message::ChannelNameChanged(node.id, name.into()))
								.padding(0)
								.size(13)
								.align_x(Center)
								.style(text_input_transparent),
							node.pan_knob(25.0, enabled),
							row![
								text_icon_button("M", button_style(soloed)).on_press(
									if node.ty == NodeType::Track {
										Message::TrackToggleEnabled(node.id)
									} else {
										Message::ChannelToggleEnabled(node.id)
									}
								),
								text_icon_button("S", button_style(soloed)).on_press_maybe(
									(node.ty == NodeType::Track)
										.then_some(Message::TrackToggleSolo(node.id)),
								),
								select_area::Blocker::new(
									icon_button(
										x(),
										if enabled {
											button::danger
										} else {
											button::secondary
										},
									)
									.on_press_maybe(match node.ty {
										NodeType::Master => None,
										NodeType::Channel => Some(Message::ChannelRemove(node.id)),
										NodeType::Track => Some(Message::TrackRemove(node.id)),
									})
								),
							]
							.spacing(5),
							row![
								icon_button(
									if node.bypassed { power_off() } else { power() },
									button_style(node.bypassed)
								)
								.on_press(Message::ChannelToggleBypassed(node.id)),
								icon_button(
									arrow_up_down(),
									button_style(node.utility.volume.is_sign_negative())
								)
								.on_press(Message::ChannelVolumeChanged(
									node.id,
									-node.utility.volume
								)),
								match node.utility.pan {
									PanMode::Stereo(..) => icon_button(
										chevrons_left_right_ellipsis(),
										button_style(false),
									)
									.on_press(Message::ChannelPanChanged(
										node.id,
										PanMode::SplitStereo(-1.0, 1.0),
									)),
									PanMode::SplitStereo(..) => {
										icon_button(circle_ellipsis(), button_style(false))
											.on_press(Message::ChannelPanChanged(
												node.id,
												PanMode::Stereo(0.0),
											))
									}
								}
							]
							.spacing(5),
							center_x(
								text(format_db(node.utility.volume.abs()))
									.size(13)
									.line_height(1.0)
							)
							.style(weak_bordered_box)
							.padding(2),
							row![
								column![
									space().height(3),
									row![
										PeakMeter::new(&node.peaks[0]).width(20.0).enabled(enabled),
										PeakMeter::new(&node.peaks[1]).width(20.0).enabled(enabled),
									]
									.spacing(3),
									container(space().width(35).height(3)).style(
										container_with_radius(
											if node.ty != NodeType::Track {
												container::transparent
											} else if node.polyphony > 0 {
												container::primary
											} else {
												container::secondary
											},
											border::bottom(f32::INFINITY),
										),
									)
								]
								.spacing(5),
								ContextMenu::new(
									vertical_slider(
										0.0..=MAX_VOL,
										node.utility.volume.abs().cbrt(),
										|v| Message::ChannelVolumeChanged(
											node.id,
											v.powi(3).copysign(node.utility.volume),
										)
									)
									.default(1f32)
									.width(24)
									.step(f32::EPSILON)
									.handle((20, 26))
									.style(slider_with_radius(
										if enabled {
											slider::primary
										} else {
											slider::secondary
										},
										5
									)),
									|| node.volume_context_menu(Tab::Mixer)
								),
							]
							.spacing(3),
							{
								let incoming =
									self.arrangement.node(node.id).outgoing.get(&self.selected);
								let outgoing =
									self.arrangement.node(self.selected).outgoing.get(&node.id);

								let down = |r: border::Radius| {
									select_area::Blocker::new(
										button(chevron_down())
											.padding(0)
											.style(button_with_radius(
												if enabled && incoming.is_some() {
													button::primary
												} else {
													button::secondary
												},
												r,
											))
											.on_press_maybe(if outgoing.is_some() {
												None
											} else if incoming.is_some() {
												Some(Message::Disconnect(node.id, self.selected))
											} else {
												Some(Message::Connect(node.id, self.selected))
											}),
									)
								};

								let up = |r: border::Radius| {
									select_area::Blocker::new(
										button(chevron_up())
											.padding(0)
											.style(button_with_radius(
												if enabled && outgoing.is_some() {
													button::primary
												} else {
													button::secondary
												},
												r,
											))
											.on_press_maybe(if incoming.is_some() {
												None
											} else if outgoing.is_some() {
												Some(Message::Disconnect(self.selected, node.id))
											} else {
												Some(Message::Connect(self.selected, node.id))
											}),
									)
								};

								column![
									track.map_or_else(
										|| space::vertical().height(36).into(),
										|track| track
											.inputs_toolbar(enabled, self.arrangement.transport())
									),
									incoming
										.map(|val| (val, node.id, self.selected))
										.or_else(|| outgoing.map(|val| (
											val,
											self.selected,
											node.id
										)))
										.map_or_else(
											|| Element::new(space::vertical().height(14)),
											|(val, from, to)| {
												select_area::Blocker::new(ContextMenu::new(
													slider(0.0..=1.0, val.cbrt(), move |val| {
														Message::SetMix(from, to, val.powi(3))
													})
													.default(1f32)
													.step(f32::EPSILON)
													.handle((4, 4))
													.height(14)
													.style(if enabled {
														slider::primary
													} else {
														slider::secondary
													}),
													move || {
														container(
															menu_entry(
																rotate_ccw(),
																"Reset",
																"Ctrl-Click",
															)
															.on_press(
																Message::SetMix(from, to, 1.0),
															),
														)
														.width(160)
														.style(container_with_radius(
															weaker_bordered_box,
															5,
														))
														.into()
													},
												))
												.into()
											}
										),
									if node.id == self.selected {
										row![
											ContextMenu::new(
												button(volume_2().size(TEXT_HEIGHT))
													.padding((LINE_HEIGHT - TEXT_HEIGHT) / 2.0)
													.style(button_with_radius(
														if !node.output.enable_audio {
															button::secondary
														} else if !node.output.fits_in(
															self.arrangement
																.transport()
																.output_channels
																.get(),
														) {
															button::warning
														} else if enabled {
															button::primary
														} else {
															button::secondary
														},
														border::left(5),
													))
													.on_press(Message::OutputChangeChannels(
														node.id,
														node.output.enable_audio(
															!node.output.enable_audio
														),
													)),
												|| node.audio_output_context_menu(
													self.arrangement.transport()
												)
											),
											ContextMenu::new(
												button(midi_port().size(TEXT_HEIGHT))
													.padding((LINE_HEIGHT - TEXT_HEIGHT) / 2.0)
													.style(button_with_radius(
														if !node.output.enable_midi {
															button::secondary
														} else if enabled {
															button::primary
														} else {
															button::secondary
														},
														border::right(5),
													))
													.on_press(Message::OutputChangeChannels(
														node.id,
														node.output
															.enable_midi(!node.output.enable_midi),
													)),
												|| node.midi_output_context_menu()
											)
										]
									} else {
										match (node.ty, self.arrangement.node(self.selected).ty) {
											(NodeType::Track, NodeType::Track) => {
												row![].height(LINE_HEIGHT)
											}
											(_, NodeType::Master)
											| (NodeType::Track, NodeType::Channel) => row![down(border::radius(5))],
											(NodeType::Master, _) | (_, NodeType::Track) => {
												row![up(border::radius(5))]
											}
											_ => row![down(border::left(5)), up(border::right(5))],
										}
									}
								]
								.align_x(Center)
							},
						]
						.align_x(Center)
						.padding(5)
						.spacing(5),
						|| node.main_context_menu(
							Tab::Mixer,
							&state.shortcuts,
							self.route_items(node)
						)
					))
					.interaction(Interaction::Pointer)
				)
			])
			.width(100)
			.id(node.widget_id.clone())
			.style(container_with_radius(
				selectable_box(weakest_bordered_box, node.id == self.selected),
				border::top(5),
			)),
		)
		.on_select_maybe((node.id != self.selected).then_some(Message::ChannelSelect(node.id)))
		.into()
	}
}
