//! Hosts VST3 plugins in the GUI, next to [`ClapHost`](crate::clap_host::ClapHost).
//!
//! This does the same things for VST3 plugins as `ClapHost` does for CLAP plugins: it owns the
//! plugins on the main thread, shows their editors (or else a list of their parameters) in
//! windows, and passes messages between the plugins and the audio thread.

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
	vst3_host::{AudioConfig, EditorSize, MainThreadMessage, ParentWindow, Plugin},
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

#[derive(Debug)]
pub struct Vst3Host {
	plugins: HashMap<PluginId, Hosted>,
	window_of_plugin: HashMap<PluginId, window::Id>,
	plugin_of_window: HashMap<window::Id, PluginId>,
	/// The windows that an editor goes in, as opposed to the ones with a list of parameters.
	editor_windows: HashSet<window::Id>,
	scale_factor_of_window: HashMap<window::Id, f32>,
	offline: bool,
}

impl Default for Vst3Host {
	fn default() -> Self {
		Self::new()
	}
}

impl Vst3Host {
	pub fn new() -> Self {
		Self {
			plugins: HashMap::new(),
			window_of_plugin: HashMap::new(),
			plugin_of_window: HashMap::new(),
			editor_windows: HashSet::new(),
			scale_factor_of_window: HashMap::new(),
			offline: false,
		}
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
				// The processor changed the value, the controller has to know. It refuses
				// parameters that are only displayed, which is no problem.
				let _ = plugin!(id)
					.plugin
					.set_param_normalized(param_id.get(), f64::from(value));
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

						return Action::instruction(daw::Instruction::Vst3PluginActivate(
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

				return match hosted.plugin.create_editor() {
					Ok(size) => {
						let (window, spawn) = window::open(window::Settings {
							size: (size.width as f32, size.height as f32).into(),
							resizable: hosted.plugin.editor_can_resize(),
							exit_on_close_request: false,
							level: window::Level::AlwaysOnTop,
							blank: true,
							..window::Settings::default()
						});
						self.window_of_plugin.insert(id, window);
						self.plugin_of_window.insert(window, id);
						self.editor_windows.insert(window);

						let mut hosted = Fragile::new(self.plugins.remove(&id).unwrap());
						let embed = window::run(window, move |window| {
							let hosted_ref = hosted.get_mut();

							if let Some(parent) = ParentWindow::from_window(&window) {
								if let Err(err) = hosted_ref.plugin.attach_editor(parent) {
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

						spawn.discard().chain(embed)
					}
					// the plugin has no editor, so the parameters are shown instead
					Err(_) => self.open_parameters(id),
				}
				.into();
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

				if let Some(&scale_factor) = self.scale_factor_of_window.get(&window) {
					return self.update(Message::WindowRescaled(window, scale_factor), transport);
				}
			}
			Message::WindowResized(window, size) => {
				// Until the scale of the screen is known the size isn't the size of the editor, since
				// the window is sized for a scale that is then replaced by a scale of 1.
				if self.editor_windows.contains(&window)
					&& self.scale_factor_of_window.contains_key(&window)
					&& let Some(&id) = self.plugin_of_window.get(&window)
					&& let Some(hosted) = self.plugins.get_mut(&id)
					&& hosted.plugin.has_editor_open()
				{
					// the window has a scale of 1, so a point is a pixel (see `scale_factor`)
					let wanted = EditorSize {
						width: size.width.round() as i32,
						height: size.height.round() as i32,
					};

					if let Some(new_size) = hosted.plugin.resize_editor(wanted)
						&& new_size != wanted
					{
						return window::resize(
							window,
							(new_size.width as f32, new_size.height as f32).into(),
						)
						.into();
					}
				}
			}
			Message::WindowRescaled(window, scale_factor) => {
				self.scale_factor_of_window.insert(window, scale_factor);

				if self.editor_windows.contains(&window)
					&& let Some(&id) = self.plugin_of_window.get(&window)
				{
					let hosted = plugin!(id);

					if hosted.plugin.has_editor_open() {
						// the plugin draws for the screen, and tells how many pixels it needs
						let _ = hosted.plugin.set_editor_scale(scale_factor);

						if let Some(size) = hosted.plugin.editor_size() {
							return window::resize(
								window,
								(size.width as f32, size.height as f32).into(),
							)
							.into();
						}
					}
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
		macro_rules! plugin {
			() => {{
				let Some(hosted) = self.plugins.get_mut(&id) else {
					let message = Message::MainThread(id, message);
					info!("retrying {message:?}");
					return Task::perform(Timer::after(Duration::from_millis(100)), |_| message)
						.into();
				};
				hosted
			}};
		}

		match message {
			// the controller has the value already, the processor has to be told
			MainThreadMessage::PerformEdit(param, value) => {
				let hosted = plugin!();

				if hosted.active
					&& let Some(param_id) = ClapId::from_raw(param)
				{
					return Action::instruction(daw::Instruction::PluginParamChanged(
						id,
						param_id,
						value as f32,
					));
				}
			}
			MainThreadMessage::BeginEdit(_) | MainThreadMessage::EndEdit(_) => {}
			MainThreadMessage::Restart(flags) => {
				// What's left to do, like restarting the audio, is done when the audio thread
				// hands the processor back.
				let _ = plugin!().plugin.handle_restart(flags);
			}
			MainThreadMessage::ResizeEditor { width, height } => {
				let hosted = plugin!();

				if let Some(&window) = self.window_of_plugin.get(&id) {
					hosted
						.plugin
						.apply_editor_size(EditorSize { width, height });

					return window::resize(window, (width as f32, height as f32).into()).into();
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

				let AudioBackend::Vst3(audio) = processor.into_backend() else {
					warn!("a CLAP processor was given to a VST3 plugin");
					return Action::none();
				};

				return match hosted.plugin.restart(audio) {
					Ok(audio) => {
						let params = hosted.plugin.params().len();
						Action::instruction(daw::Instruction::Vst3PluginRestart(
							id,
							Some((audio, params)),
						))
					}
					Err(err) => {
						warn!("{}: {err}", hosted.plugin.descriptor().name);
						hosted.active = false;
						Action::instruction(daw::Instruction::Vst3PluginRestart(id, None))
					}
				};
			}
			AudioThreadMessage::Deactivate(processor) => {
				let hosted = plugin!(AudioThreadMessage::Deactivate(processor));

				if let AudioBackend::Vst3(audio) = processor.into_backend() {
					hosted.plugin.deactivate(audio);
				}
				hosted.active = false;
			}
			AudioThreadMessage::Destroy(processor) => {
				let hosted = plugin!(AudioThreadMessage::Destroy(processor));

				if let AudioBackend::Vst3(audio) = processor.into_backend() {
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
										.enabled(hosted.active && !param.is_read_only())
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

	/// Editors get a window in which a point is a pixel, and tell the plugin about the scale of
	/// the screen themselves.
	pub fn scale_factor(&self, window: window::Id) -> Option<f32> {
		if self.editor_windows.contains(&window) {
			self.scale_factor_of_window
				.get(&window)
				.map(|scale_factor| 1.0 / scale_factor)
		} else {
			None
		}
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

	pub fn get_state(&self, id: PluginId) -> Option<Vec<u8>> {
		self.plugins
			.get(&id)?
			.plugin
			.save_state()
			.inspect_err(|err| warn!("{err}"))
			.ok()
	}
}

#[cfg(test)]
mod tests;
