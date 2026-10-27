//! Hosts VST2 plugins in the GUI, next to [`ClapHost`](crate::clap_host::ClapHost) and
//! [`Vst3Host`](crate::vst3_host::Vst3Host).
//!
//! This does the same things for VST2 plugins as `ClapHost` does for CLAP plugins: it owns
//! the plugins on the main thread, shows their editors (or else a list of their parameters)
//! in windows, and passes messages between the plugins and the audio thread.

use crate::{
	action::Action,
	clap_host::PluginId,
	components::menu_entry,
	daw,
	icons::rotate_ccw,
	stylefns::{container_with_radius, scrollable_style, weaker_bordered_box},
	widget::LINE_HEIGHT,
};
use fragile::Fragile;
use generic_daw_core::{
	AudioBackend, AudioThreadMessage, Transport,
	clap_host::{ClapId, RenderMode},
	vst2_host::{AudioConfig, EditorSize, MainThreadMessage, ParentWindow, Plugin},
};
use generic_daw_widget::{context_menu::ContextMenu, knob::Knob};
use iced::{
	Center, Element, Fill, Font, Subscription, Task,
	widget::{column, container, row, rule, scrollable, space, text},
	window,
};
use log::{info, warn};
use smol::{Timer, unblock};
use std::{
	collections::{HashMap, HashSet},
	sync::mpsc::Receiver,
	time::Duration,
};
use utils::{NoClone, NoDebug};

#[derive(Clone, Debug)]
pub enum Message {
	MainThread(PluginId, MainThreadMessage),
	AudioThread(PluginId, NoClone<Box<AudioThreadMessage>>),
	/// The plugin changed a parameter on its own.
	PluginParamValueChange(PluginId, ClapId, f32),
	/// The user changed a parameter in the list of parameters.
	HostParamValueChange(PluginId, ClapId, f32),
	DestroyInactive(PluginId),
	Activate(PluginId),
	SetState(PluginId, NoDebug<Box<[u8]>>),
	GuiOpen(PluginId),
	GuiOpened(PluginId, NoClone<NoDebug<Box<Fragile<Hosted>>>>),
	WindowResized(window::Id, iced::Size),
	WindowRescaled(window::Id, f32),
	WindowCloseRequested(window::Id),
	WindowClosed(window::Id),
}

/// A plugin, and what is known about it besides what the plugin knows itself.
#[derive(Debug)]
pub struct Hosted {
	plugin: Plugin,
	/// Whether there is an audio thread for the plugin out there.
	active: bool,
}

/// Hosts VST2 plugins.
#[derive(Debug)]
pub struct Vst2Host {
	plugins: HashMap<PluginId, Hosted>,
	window_of_plugin: HashMap<PluginId, window::Id>,
	plugin_of_window: HashMap<window::Id, PluginId>,
	/// The windows that an editor goes in, as opposed to the ones with a list of
	/// parameters.
	editor_windows: HashSet<window::Id>,
	editor_size_of_window: HashMap<window::Id, EditorSize>,
	scale_factor_of_window: HashMap<window::Id, f32>,
	offline: bool,
}

impl Default for Vst2Host {
	fn default() -> Self {
		Self::new()
	}
}

impl Vst2Host {
	pub fn new() -> Self {
		Self {
			plugins: HashMap::new(),
			window_of_plugin: HashMap::new(),
			plugin_of_window: HashMap::new(),
			editor_windows: HashSet::new(),
			editor_size_of_window: HashMap::new(),
			scale_factor_of_window: HashMap::new(),
			offline: false,
		}
	}

	/// Converts VST2 editor pixels to Iced's logical window dimensions.
	fn logical_editor_size(&self, window: window::Id, size: EditorSize) -> iced::Size {
		let scale_factor = self
			.scale_factor_of_window
			.get(&window)
			.copied()
			.filter(|scale_factor| scale_factor.is_finite() && *scale_factor > 0.0)
			.unwrap_or(1.0);

		(
			size.width.max(1) as f32 / scale_factor,
			size.height.max(1) as f32 / scale_factor,
		)
			.into()
	}

	pub fn update(
		&mut self,
		message: Message,
		transport: &Transport,
	) -> Action<daw::Instruction, Message> {
		// Messages for a plugin that is on its way to or from a window are tried again later.
		macro_rules! plugin {
			($id:expr) => {
				plugin!($id, message)
			};
			($id:expr,$expr:expr) => {{
				let Some(hosted) = self.plugins.get_mut(&$id) else {
					let message = $expr;
					info!("retrying {message:?}");
					return Task::perform(Timer::after(Duration::from_millis(100)), |_| message)
						.into();
				};
				hosted
			}};
		}

		match message {
			Message::MainThread(id, msg) => {
				return self.handle_main_thread_message(id, msg);
			}
			Message::AudioThread(id, NoClone(msg)) => {
				return self.handle_audio_thread_message(id, *msg, transport);
			}
			Message::PluginParamValueChange(id, param_id, value) => {
				// The plugin changed the value itself. A VST2 plugin has no controller
				// of its own, so the value it has is the value the list of parameters
				// shows, and there's nothing to do.
				let _ = (id, param_id, value);
			}
			Message::HostParamValueChange(id, param_id, value) => {
				let hosted = plugin!(id);

				let Some(value) = hosted
					.plugin
					.set_param_normalized(param_id.get(), f64::from(value))
				else {
					return Action::none();
				};

				if hosted.active {
					return Action::instruction(daw::Instruction::PluginParamChanged(
						id,
						param_id,
						value as f32,
					));
				}
			}
			Message::DestroyInactive(id) => {
				let hosted = self.plugins.remove(&id);
				debug_assert!(hosted.is_none_or(|hosted| !hosted.active));

				if let Some(&window) = self.window_of_plugin.get(&id) {
					return window::close(window).into();
				}
			}
			Message::Activate(id) => {
				let offline = self.offline;
				let hosted = plugin!(id);

				if hosted.active {
					return Action::none();
				}

				let config = AudioConfig {
					sample_rate: f64::from(transport.sample_rate.get()),
					max_frames: transport.frames.get(),
					offline,
				};

				match hosted.plugin.activate(config) {
					Ok(audio) => {
						hosted.active = true;

						return Action::instruction(daw::Instruction::Vst2PluginActivate(
							id,
							audio,
							hosted.plugin.params().len(),
						));
					}
					Err(err) => warn!("{}: {err}", hosted.plugin.descriptor().name),
				}
			}
			Message::SetState(id, state) => {
				let hosted = plugin!(id, Message::SetState(id, state));

				if let Err(err) = hosted.plugin.load_state(&state) {
					warn!("{}: {err}", hosted.plugin.descriptor().name);
				}
			}
			Message::GuiOpen(id) => {
				if self.window_of_plugin.contains_key(&id) {
					return Action::none();
				}

				let hosted = plugin!(id);

				let Some(size) = hosted.plugin.editor_size() else {
					// the plugin has no editor, so the parameters are shown instead
					return self.open_parameters(id).into();
				};

				let (window, spawn) = window::open(window::Settings {
					size: (size.width as f32, size.height as f32).into(),
					exit_on_close_request: false,
					level: window::Level::AlwaysOnTop,
					blank: true,
					..window::Settings::default()
				});
				self.window_of_plugin.insert(id, window);
				self.plugin_of_window.insert(window, id);
				self.editor_windows.insert(window);
				self.editor_size_of_window.insert(window, size);

				let mut hosted = Fragile::new(self.plugins.remove(&id).unwrap());
				let embed = window::run(window, move |window| {
					let hosted_ref = hosted.get_mut();

					if let Some(parent) = ParentWindow::from_window(&window) {
						if let Err(err) = hosted_ref.plugin.open_editor(parent) {
							warn!("{}: {err}", hosted_ref.plugin.descriptor().name);
							hosted_ref.plugin.close_editor();
						}
					} else {
						warn!(
							"{}: the window can't have an editor in it",
							hosted_ref.plugin.descriptor().name
						);
						hosted_ref.plugin.close_editor();
					}

					Message::GuiOpened(id, NoClone(NoDebug(Box::new(hosted))))
				});

				return spawn.discard().chain(embed).into();
			}
			Message::GuiOpened(id, NoClone(NoDebug(hosted))) => {
				let hosted = hosted.into_inner();
				let attached = hosted.plugin.has_editor_open();
				self.plugins.insert(id, hosted);

				let Some(&window) = self.window_of_plugin.get(&id) else {
					return Action::none();
				};

				if !attached {
					// the editor didn't work out, so there's the parameters instead
					self.editor_windows.remove(&window);
					return Task::batch([window::close(window), self.open_parameters(id)]).into();
				}

				let Some(editor_size) = self
					.plugins
					.get(&id)
					.and_then(|hosted| hosted.plugin.editor_size_if_open())
				else {
					return Action::none();
				};
				self.editor_size_of_window.insert(window, editor_size);
				return window::resize(window, self.logical_editor_size(window, editor_size)).into();
			}
			Message::WindowResized(window, size) => {
				// VST2 sizes are pixels; Iced window sizes are logical pixels.
				if self.editor_windows.contains(&window)
					&& let Some(&editor_size) = self.editor_size_of_window.get(&window)
				{
					let wanted = self.logical_editor_size(window, editor_size);

					if (size.width - wanted.width).abs() > 0.5
						|| (size.height - wanted.height).abs() > 0.5
					{
						return window::resize(window, wanted).into();
					}

					if let Some(&id) = self.plugin_of_window.get(&window)
						&& let Some(hosted) = self.plugins.get(&id)
					{
						hosted.plugin.resize_editor_child_to_parent();
					}
				}
			}
			Message::WindowRescaled(window, scale_factor) => {
				self.scale_factor_of_window.insert(window, scale_factor);

				if let Some(&editor_size) = self.editor_size_of_window.get(&window) {
					return window::resize(window, self.logical_editor_size(window, editor_size)).into();
				}
			}
			Message::WindowCloseRequested(window) => {
				if let Some(&id) = self.plugin_of_window.get(&window) {
					// the editor has to be gone before its window is
					if let Some(hosted) = self.plugins.get_mut(&id) {
						hosted.plugin.close_editor();
					} else {
						// the plugin is on its way back from the window, try once more
						let message = Message::WindowCloseRequested(window);
						return Task::perform(Timer::after(Duration::from_millis(100)), |_| {
							message
						})
						.into();
					}

					return window::close(window).into();
				}
			}
			Message::WindowClosed(window) => {
				self.scale_factor_of_window.remove(&window);
				self.editor_size_of_window.remove(&window);
				self.editor_windows.remove(&window);

				if let Some(id) = self.plugin_of_window.remove(&window)
					// a plugin can already have a new window by now
					&& self.window_of_plugin.get(&id) == Some(&window)
				{
					self.window_of_plugin.remove(&id);
				}
			}
		}

		Action::none()
	}

	/// Opens a window with the plugin's parameters in it, for plugins that have no editor.
	fn open_parameters(&mut self, id: PluginId) -> Task<Message> {
		let (window, spawn) = window::open(window::Settings {
			size: (400.0, 600.0).into(),
			exit_on_close_request: false,
			level: window::Level::AlwaysOnTop,
			..window::Settings::default()
		});
		self.window_of_plugin.insert(id, window);
		self.plugin_of_window.insert(window, id);

		spawn.discard()
	}

	fn handle_main_thread_message(
		&mut self,
		id: PluginId,
		message: MainThreadMessage,
	) -> Action<daw::Instruction, Message> {
		match message {
			MainThreadMessage::ResizeEditor { width, height } => {
				if let Some(&window) = self.window_of_plugin.get(&id) {
					let editor_size = EditorSize {
						width: width.max(1),
						height: height.max(1),
					};
					self.editor_size_of_window.insert(window, editor_size);
					return window::resize(
						window,
						self.logical_editor_size(window, editor_size),
					)
					.into();
				}
			}
		}

		Action::none()
	}

	fn handle_audio_thread_message(
		&mut self,
		id: PluginId,
		message: AudioThreadMessage,
		transport: &Transport,
	) -> Action<daw::Instruction, Message> {
		macro_rules! plugin {
			($expr:expr) => {{
				let Some(hosted) = self.plugins.get_mut(&id) else {
					let message = Message::AudioThread(id, NoClone(Box::new($expr)));
					info!("retrying {message:?}");
					return Task::perform(Timer::after(Duration::from_millis(100)), |_| message)
						.into();
				};
				hosted
			}};
		}

		match message {
			AudioThreadMessage::Restart(processor) => {
				let hosted = plugin!(AudioThreadMessage::Restart(processor));

				let AudioBackend::Vst2(audio) = processor.into_backend() else {
					warn!("a processor of another format was given to a VST2 plugin");
					return Action::none();
				};

				// The engine may have a new sample rate or block size.
				let config = AudioConfig {
					sample_rate: f64::from(transport.sample_rate.get()),
					max_frames: transport.frames.get(),
					offline: audio.config().offline,
				};

				hosted.plugin.deactivate(audio);
				return match hosted.plugin.activate(config) {
					Ok(audio) => {
						let params = hosted.plugin.params().len();
						Action::instruction(daw::Instruction::Vst2PluginRestart(
							id,
							Some((audio, params)),
						))
					}
					Err(err) => {
						warn!("{}: {err}", hosted.plugin.descriptor().name);
						hosted.active = false;
						Action::instruction(daw::Instruction::Vst2PluginRestart(id, None))
					}
				};
			}
			AudioThreadMessage::Deactivate(processor) => {
				let hosted = plugin!(AudioThreadMessage::Deactivate(processor));

				if let AudioBackend::Vst2(audio) = processor.into_backend() {
					hosted.plugin.deactivate(audio);
				}
				hosted.active = false;
			}
			AudioThreadMessage::Destroy(processor) => {
				let hosted = plugin!(AudioThreadMessage::Destroy(processor));

				if let AudioBackend::Vst2(audio) = processor.into_backend() {
					hosted.plugin.deactivate(audio);
				}
				hosted.active = false;

				return self.update(Message::DestroyInactive(id), transport);
			}
		}

		Action::none()
	}

	pub fn view(&self, window: window::Id) -> Option<Element<'_, Message>> {
		let id = *self.plugin_of_window.get(&window)?;

		let Some(hosted) = self.plugins.get(&id) else {
			return Some(space().into());
		};

		// editors draw themselves
		if self.editor_windows.contains(&window) {
			return Some(space().into());
		}

		let plugin = &hosted.plugin;

		Some(
			column![
				text(&*plugin.descriptor().name)
					.size(LINE_HEIGHT)
					.line_height(1.0)
					.font(Font::MONOSPACE),
				rule::horizontal(1),
				scrollable(
					row(plugin
						.params()
						.iter()
						.filter(|param| param.is_visible())
						.filter_map(|param| {
							let param_id = ClapId::from_raw(param.id)?;
							let value = plugin.param_normalized(param.id)?;
							let default = param.default_normalized as f32;

							Some(
								column![
									ContextMenu::new(
										Knob::new(0.0..=1.0, value as f32, move |value| {
											Message::HostParamValueChange(id, param_id, value)
										})
										.default(default)
										.radius(25.0)
										.enabled(hosted.active)
										.stepped(param.is_discrete())
										.maybe_tooltip(plugin.param_display(param.id, value)),
										move || container(
											menu_entry(rotate_ccw(), "Reset", "Ctrl-Click")
												.on_press(Message::HostParamValueChange(
													id, param_id, default
												)),
										)
										.width(160)
										.style(container_with_radius(weaker_bordered_box, 5))
										.into()
									),
									text(&*param.title)
										.align_x(Center)
										.wrapping(text::Wrapping::WordOrGlyph)
								]
								.width(70)
								.align_x(Center)
								.spacing(5)
								.into(),
							)
						}))
					.spacing(10)
					.wrap()
					.vertical_spacing(10)
				)
				.width(Fill)
				.spacing(5)
				.style(scrollable_style)
			]
			.padding(10)
			.spacing(5)
			.into(),
		)
	}

	pub fn title(&self, window: window::Id) -> Option<String> {
		self.plugin_of_window
			.get(&window)
			.and_then(|id| self.plugins.get(id))
			.map(|hosted| hosted.plugin.descriptor().name.to_string())
	}

	/// Keeps VST2 editor windows in the plugin's pixel coordinate system.
	///
	/// The editor reports pixel dimensions but has no API for negotiating a UI scale.
	pub fn scale_factor(&self, window: window::Id) -> Option<f32> {
		self.editor_windows.contains(&window).then_some(1.0)
	}

	pub fn subscription() -> Subscription<Message> {
		window::events().filter_map(|(id, event)| match event {
			window::Event::Resized(size) => Some(Message::WindowResized(id, size)),
			window::Event::Opened { scale_factor, .. } | window::Event::Rescaled(scale_factor) => {
				Some(Message::WindowRescaled(id, scale_factor))
			}
			window::Event::CloseRequested => Some(Message::WindowCloseRequested(id)),
			window::Event::Closed => Some(Message::WindowClosed(id)),
			_ => None,
		})
	}

	pub fn plugin_add(
		&mut self,
		id: PluginId,
		plugin: Plugin,
		receiver: Receiver<MainThreadMessage>,
	) -> Task<Message> {
		self.plugins.insert(
			id,
			Hosted {
				plugin,
				active: false,
			},
		);

		let (sender, stream) = smol::channel::unbounded();

		Task::batch([
			Task::future(unblock(move || {
				for msg in receiver {
					if sender.try_send(msg).is_err() {
						break;
					}
				}
			}))
			.discard(),
			Task::run(stream, move |msg| Message::MainThread(id, msg)),
		])
	}

	pub fn set_render_mode(&mut self, render_mode: RenderMode) {
		self.offline = matches!(render_mode, RenderMode::Offline);
	}

	/// Whether the host has the plugin with this identifier.
	pub fn has(&self, id: PluginId) -> bool {
		self.plugins.contains_key(&id)
	}

	pub fn get_state(&self, id: PluginId) -> Option<Vec<u8>> {
		self.plugins
			.get(&id)?
			.plugin
			.save_state()
			.inspect_err(|err| warn!("{err}"))
			.ok()
	}
}
