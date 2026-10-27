use crate::{
	arrangement_view::{self, Arrangement, ArrangementView, Feedback},
	clap_host::{self, ClapHost, PluginId},
	components::{MenuItem, menu_entry, menu_width, number_input},
	config::Config,
	config_view::{self, ConfigView},
	file_tree::{self, FileKind, FileTree},
	icons::{
		arrow_big_right, chart_no_axes_gantt, cpu, file, folder_open, gavel, history,
		keyboard_music, magnet, menu as menu_icon, metronome, midi_port, panel_bottom_dashed,
		pause, play, plus, save, settings, sliders_vertical, square,
	},
	plugin_descriptor::PluginDescriptor,
	plugin_memory::{PluginEntry, PluginMemory},
	plugin_picker::{self, PluginPicker},
	shortcuts::{Action, Chord, Context, Manager},
	state::{DEFAULT_SPLIT_HEIGHT, DEFAULT_SPLIT_WIDTH, MIN_SPLIT_HEIGHT, MIN_SPLIT_WIDTH, State},
	stylefns::{
		button_with_radius, container_with_radius, progress_bar_with_radius, selectable_box,
		split_style, weak_bordered_box, weaker_bordered_box, weakest_bordered_box,
	},
	vst2_host::{self, Vst2Host},
	vst3_host::{self, Vst3Host},
	widget::{ALPHA_2_3, LINE_HEIGHT},
};
use generic_daw_core::{
	AudioThread, BpmTapper, MidiKey, NodeId, PullSlot, build_streams,
	clap_host::{
		ClapId, DEFAULT_CLAP_PATHS, MainThreadMessage, Plugin, RenderMode, StateContextType,
	},
};
use generic_daw_project::proto;
use generic_daw_widget::{context_menu::ContextMenu, menu::Menu, select_area::SelectArea};
use iced::{
	Center, Color, Element, Fill, Font, Shrink, Subscription, Task, Theme, border,
	font::Weight,
	keyboard,
	mouse::Interaction,
	padding,
	time::every,
	widget::{
		bottom_center, button, center, checkbox, column, container, mouse_area, opaque,
		progress_bar, right, row, rule, scrollable, slider, space, stack, text,
	},
	window,
};
use iced_split::{Strategy, horizontal_split, vertical_split};
use log::{error, info, trace, warn};
use rfd::AsyncFileDialog;
use smol::{Timer, unblock};
	use std::{
	collections::HashMap,
	convert::Infallible,
	ffi::CStr,
	fs::File,
	io::{BufReader, Read as _},
	num::NonZero,
	path::Path,
	sync::{Arc, LazyLock, mpsc::Receiver},
	time::{Duration, Instant},
};
use utils::{NoClone, NoDebug, unique_id};

unique_id!(Save);
unique_id!(Scan);
unique_id!(Project);

pub static CONFIG_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let config_dir = dirs::config_dir().unwrap().join("Generic DAW").into();
	_ = std::fs::create_dir(&config_dir);
	config_dir
});

pub static DATA_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let data_dir = dirs::data_dir().unwrap().join("Generic DAW").into();
	_ = std::fs::create_dir(&data_dir);
	data_dir
});

pub static STATE_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let state_dir = dirs::state_dir()
		.or_else(dirs::data_dir)
		.unwrap()
		.join("Generic DAW")
		.into();
	_ = std::fs::create_dir(&state_dir);
	state_dir
});

pub static CRASHES_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let crashes_dir = DATA_DIR.join("crashes").into();
	_ = std::fs::create_dir(&crashes_dir);
	crashes_dir
});

pub static PROJECTS_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let projects_dir = DATA_DIR.join("projects").into();
	_ = std::fs::create_dir(&projects_dir);
	projects_dir
});

pub static AUTOSAVED_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let autosaved_dir = PROJECTS_DIR.join("autosaved").into();
	_ = std::fs::create_dir(&autosaved_dir);
	autosaved_dir
});

pub static FREEZES_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let freezes_dir = DATA_DIR.join("freezes").into();
	_ = std::fs::create_dir(&freezes_dir);
	freezes_dir
});

pub static RECORDINGS_DIR: LazyLock<Arc<Path>> = LazyLock::new(|| {
	let recordings_dir = DATA_DIR.join("recordings").into();
	_ = std::fs::create_dir(&recordings_dir);
	recordings_dir
});

pub fn format_now() -> jiff::fmt::strtime::Display<'static> {
	jiff::Zoned::now().strftime("%F %H-%M-%S")
}

pub enum Instruction {
	Message(Message),
	Freeze(NodeId),
	PluginLoad(PluginDescriptor),
	PluginAdd(PluginId, Plugin, Receiver<MainThreadMessage>),
	PluginCopyState(PluginId, PluginId),
	PluginActivate(PluginId, clap_host::AudioThread),
	PluginRestart(PluginId, Option<clap_host::AudioThread>),
	Vst2PluginAdd(
		PluginId,
		generic_daw_core::vst2_host::Plugin,
		Receiver<generic_daw_core::vst2_host::MainThreadMessage>,
	),
	/// The audio thread, and how many parameters the plugin has.
	Vst2PluginActivate(PluginId, generic_daw_core::vst2_host::AudioThread, usize),
	Vst2PluginRestart(
		PluginId,
		Option<(generic_daw_core::vst2_host::AudioThread, usize)>,
	),
	Vst3PluginAdd(
		PluginId,
		generic_daw_core::vst3_host::Plugin,
		Receiver<generic_daw_core::vst3_host::MainThreadMessage>,
	),
	/// The audio thread, and how many parameters the plugin has.
	Vst3PluginActivate(PluginId, generic_daw_core::vst3_host::AudioThread, usize),
	Vst3PluginRestart(
		PluginId,
		Option<(generic_daw_core::vst3_host::AudioThread, usize)>,
	),
	PluginParamChanged(PluginId, ClapId, f32),
}

#[derive(Clone, Debug)]
pub enum Message {
	Arrangement(Project, arrangement_view::Message),
	ClapHost(clap_host::Message),
	Vst2Host(vst2_host::Message),
	Vst3Host(vst3_host::Message),
	FileTree(file_tree::Message),
	PluginPicker(plugin_picker::Message),
	ConfigView(config_view::Message),

	Keyboard(keyboard::Event),
	MainWindowFocus(bool),
	CloseRequested(window::Id),
	ProjectLoaded(
		Box<(
			Project,
			Option<Arc<Path>>,
			NoClone<NoDebug<Arrangement>>,
			NoClone<NoDebug<AudioThread>>,
			Option<proto::ViewState>,
		)>,
	),

	SaveFinished(Save),
	SavePendingAction,
	DiscardPendingAction,
	CancelPendingAction,
	SaveAsCancelled,

	ScanProgress(Scan, f32),
	ScanStatus(Scan, Option<Arc<str>>),
	PluginScanned(Scan, Box<PluginDescriptor>),
	ScanFinished(Scan),

	NewFile,
	OpenLastFile,
	OpenCrashLog,
	OpenRecoveryFile,
	CloseRecoveryView,
	DismissRecoveryView,
	SaveFile,
	SaveAsFileDialog,
	SaveAsFile(Arc<Path>),
	AutosaveFile,
	ToggleFullscreen,

	Progress(f32),
	Status(Option<Arc<str>>),

	OpenFileDialog,
	OpenFile(Arc<Path>, bool),
	CantFindPlugin(Arc<CStr>, NoClone<oneshot::Sender<Feedback<Infallible>>>),
	CantFindSample(Arc<str>, NoClone<oneshot::Sender<Feedback<Arc<Path>>>>),
	FindPlugin(usize, Feedback<Infallible>),
	FindSampleFileDialog(usize),
	FindSampleFile(usize, Feedback<Arc<Path>>),
	OpenedFile,

	RenderFileDialog,
	RenderFile(Arc<Path>),
	RenderedFile,

	TogglePluginPicker,
	ToggleConfigView,
	LoadConfig(Box<Config>),

	/// Puts the state of a plugin in the clipboard.
	PluginCopyState(PluginId),
	/// Puts the state in the clipboard into a plugin.
	PluginPasteState(PluginId),

	RescanDevices,
	RescanPlugins,
	FullRescanPlugins,
	GenerateMeshes,

	FileHovered,
	FileDropped(Arc<Path>),
	FileHoveredLeft,

	TogglePlayback,
	Stop,
	Undo,
	Redo,
	ToggleShowSeconds,
	ToggleMetronome,
	ToggleAutoscroll,
	TappedBpm,
	ChangedBpm(Option<u16>),
	ChangedNumerator(Option<u8>),
	ChangedGridSize(f32),
	NarrowedGrid,
	WidenedGrid,
	ToggleGridTriplets,
	ToggleGrid,

	CycleForwards,
	CycleBackwards,
	TopPane(Tab),
	BottomPane(Tab),
	BottomSelected(bool),
	MovePaneUp,
	MovePaneDown,
	ClosePane,

	OnFileTreeDrag(f32),
	OnBottomPaneDrag(f32),
	OnBottomPaneDragEnd,
	OnFileTreeDragEnd,
	OnFileTreeDoubleClick,
	OnBottomPaneDoubleClick,

	/// A key was pressed, and it might be a shortcut.
	Shortcut(Chord, bool),
}

const _: () = assert!(size_of::<Message>() == 64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Tab {
	Playlist,
	Mixer,
	PianoRoll,
}

impl Tab {
	fn tab<'a>(
		self,
		top_pane: Self,
		bottom_pane: Option<Self>,
		bottom_selected: bool,
	) -> Element<'a, Message> {
		ContextMenu::new(
			button(match self {
				Self::Playlist => chart_no_axes_gantt(),
				Self::Mixer => sliders_vertical(),
				Self::PianoRoll => keyboard_music(),
			})
			.style(button_with_radius(
				if if bottom_pane.is_some() && bottom_selected {
					top_pane == self
				} else {
					bottom_pane == Some(self)
				} {
					button::secondary
				} else {
					button::primary
				},
				match self {
					Self::Playlist => border::left(5),
					Self::Mixer => border::radius(0),
					Self::PianoRoll => border::right(5),
				},
			))
			.padding(padding::horizontal(7).vertical(5))
			.on_press_maybe((top_pane != self && bottom_pane != Some(self)).then_some(
				if bottom_pane.is_some() && bottom_selected {
					Message::BottomPane(self)
				} else {
					Message::TopPane(self)
				},
			)),
			move || {
				container(
					menu_entry(panel_bottom_dashed(), "Detach", "")
						.on_press_maybe(bottom_pane.is_none().then_some(Message::BottomPane(self))),
				)
				.width(160)
				.style(container_with_radius(weaker_bordered_box, 5))
				.into()
			},
		)
		.into()
	}

	fn radius(self) -> border::Radius {
		match self {
			Self::Playlist | Self::PianoRoll => border::left(10),
			Self::Mixer => border::radius(10).bottom_left(0),
		}
	}
}

#[derive(Clone, Debug)]
enum PendingAction {
	Exit(window::Id),
	Open(Arc<Path>, bool),
	New,
}

#[derive(Debug)]
pub struct Daw {
	config: Config,
	state: State,
	current_project: Option<Arc<Path>>,
	saved_project: Option<Vec<u8>>,
	pending_action: Option<PendingAction>,
	saving_pending_action: bool,
	allow_dirty_transition: bool,

	arrangement_view: ArrangementView,
	clap_host: ClapHost,
	vst2_host: Vst2Host,
	vst3_host: Vst3Host,
	file_tree: FileTree,
	plugin_picker: Option<PluginPicker>,
	config_view: Option<ConfigView>,
	recovering: bool,

	/// The keyboard shortcuts that are in effect.
	shortcuts: Manager,
	/// Whether Caps Lock has enabled the computer MIDI keyboard.
	keyboard_midi_active: bool,
	/// Notes held by physical keys, with the track that received each note.
	keyboard_midi_notes: HashMap<keyboard::key::Physical, (NodeId, MidiKey)>,
	main_window_focused: bool,

	/// The state of the plugin that was copied, if there is one.
	plugin_state: Option<Box<[u8]>>,

	plugins: plugin_picker::State,
	bpm_tapper: BpmTapper,

	progress: Option<f32>,
	status: Option<Arc<str>>,
	missing_plugins: Vec<(Arc<CStr>, oneshot::Sender<Feedback<Infallible>>)>,
	missing_samples: Vec<(Arc<str>, oneshot::Sender<Feedback<Arc<Path>>>)>,

	save: Option<Save>,
	save_status: Option<Arc<str>>,

	scan: Option<Scan>,
	scan_progress: Option<f32>,
	scan_status: Option<Arc<str>>,

	top_pane: Tab,
	bottom_pane: Option<Tab>,
	bottom_selected: bool,

	main_window_id: window::Id,
	project: Project,
	files_hovered: bool,
	last_ui_frame: Option<Instant>,
	last_ui_frame_report: Instant,
	slow_ui_frames: u64,
	max_ui_frame_gap: Duration,
}

impl Daw {
	pub fn create() -> (Self, Task<Message>) {
		let (main_window_id, window) = window::open(window::Settings {
			exit_on_close_request: false,
			maximized: true,
			..window::Settings::default()
		});
		let window = window.discard();

		let config = Config::read();
		let mut state = State::read();

		let (sender, receiver) = oneshot::channel();
		let result = build_streams(
			&config.audio.devices.as_core(),
			config.midi.input.as_deref(),
			config.midi.output.as_deref(),
			config.audio.sample_rate,
			config.audio.buffer_size,
			PullSlot::Empty(receiver),
		);
		let (streams, input_channels, output_channels, sample_rate, frames) = match result {
			Ok(v) => v,
			Err(e) => {
				error!("Failed to build audio streams: {e}");
				std::process::exit(1);
			}
		};

		let project = Project::unique();
		let (mut arrangement, processor, batches) =
			Arrangement::create(input_channels, output_channels, sample_rate, frames);
		let pool = processor.create_pool();
		sender.send((PullSlot::Full(processor), pool)).unwrap();
		arrangement.replace_streams(Some(streams));

		let arrangement_view = ArrangementView::new(arrangement, &state, None);
		let clap_host = ClapHost::new(main_window_id);
		let vst2_host = Vst2Host::new();
		let vst3_host = Vst3Host::new();
		let file_tree = FileTree::new(&config.sample_paths);
		let recovering = state.recovery_project.is_some() || state.crash_log.is_some();
		let bottom_pane = state.bottom_pane_split_at != 0.0;

		let shortcuts = Manager::new(&config.shortcuts);
		state.shortcuts = shortcuts.clone();

		let mut this = Self {
			config,
			state,
			current_project: None,
			saved_project: None,
			pending_action: None,
			saving_pending_action: false,
			allow_dirty_transition: false,

			arrangement_view,
			clap_host,
			vst2_host,
			vst3_host,
			file_tree,
			config_view: None,
			plugin_picker: None,
			recovering: true,

			shortcuts,
			keyboard_midi_active: false,
			keyboard_midi_notes: HashMap::new(),
			main_window_focused: true,
			plugin_state: None,

			plugins: plugin_picker::State::read(),
			bpm_tapper: BpmTapper::default(),

			progress: None,
			status: None,
			missing_plugins: Vec::new(),
			missing_samples: Vec::new(),

			save: None,
			save_status: None,

			scan: None,
			scan_progress: None,
			scan_status: None,

			top_pane: Tab::Playlist,
			bottom_pane: bottom_pane.then_some(Tab::Mixer),
			bottom_selected: false,

			main_window_id,
			project,
			files_hovered: false,
			last_ui_frame: None,
			last_ui_frame_report: Instant::now(),
			slow_ui_frames: 0,
			max_ui_frame_gap: Duration::ZERO,
		};

		this.saved_project = Some(this.serialize_project());

		let init = if recovering {
			Task::none()
		} else {
			this.update(Message::DismissRecoveryView)
		};

		(
			this,
			Task::batch([
				window,
				batches
					.map(Box::new)
					.map(NoClone)
					.map(arrangement_view::Message::Batch)
					.map(move |message| Message::Arrangement(project, message)),
				init,
			]),
		)
	}

	pub fn update(&mut self, message: Message) -> Task<Message> {
		trace!("{message:?}");

		match message {
			Message::Arrangement(project, message) => {
				if project == self.project {
					// the plugin is being used, whether it was picked from the plugin
					// picker or came with a project
					let used = match &message {
						arrangement_view::Message::PluginAddTo(_, descriptor, _) => {
							self.plugins.mark_used(descriptor);
							true
						}
						_ => false,
					};

					let task = self
						.arrangement_view
						.update(message, &self.config, &mut self.state)
						.handle(
							move |message| Message::Arrangement(project, message),
							|instruction| self.handle_instruction(instruction),
						);

					return if used {
						task.chain(Self::save_plugin_memory(self.plugins.memory()))
					} else {
						task
					};
				}
			}
			Message::ClapHost(message) => {
				return self
					.clap_host
					.update(message, self.arrangement_view.arrangement.transport())
					.handle(Message::ClapHost, |instruction| {
						self.handle_instruction(instruction)
					});
			}
			Message::Vst2Host(message) => {
				return self
					.vst2_host
					.update(message, self.arrangement_view.arrangement.transport())
					.handle(Message::Vst2Host, |instruction| {
						self.handle_instruction(instruction)
					});
			}
			Message::Vst3Host(message) => {
				return self
					.vst3_host
					.update(message, self.arrangement_view.arrangement.transport())
					.handle(Message::Vst3Host, |instruction| {
						self.handle_instruction(instruction)
					});
			}
			Message::FileTree(message) => return self.handle_file_tree_message(message),
			Message::PluginPicker(message) => {
				if let Some(plugin_picker) = &mut self.plugin_picker {
					let changed = matches!(&message, plugin_picker::Message::ToggleFavorite(_));

					let action = plugin_picker.update(message, &mut self.plugins);

					let save = if changed {
						Self::save_plugin_memory(self.plugins.memory())
					} else {
						Task::none()
					};

					return action
						.handle(Message::PluginPicker, |instruction| {
							self.handle_instruction(instruction)
						})
						.chain(save);
				}
			}
			Message::ConfigView(message) => {
				if let Some(config_view) = &mut self.config_view {
					return config_view
						.update(message, &self.config)
						.handle(Message::ConfigView, |instruction| {
							self.handle_instruction(instruction)
						});
				}
			}
			Message::Keyboard(event) => return self.handle_keyboard_event(event),
			Message::MainWindowFocus(focused) => {
				self.main_window_focused = focused;
				if !focused {
					self.release_keyboard_midi_notes();
				}
			}
			Message::Shortcut(chord, repeat) => {
				let Some(message) = self.shortcut(chord, repeat) else {
					return Task::none();
				};

				return self.update(message);
			}
			Message::CloseRequested(window) => {
				if window == self.main_window_id {
					if self.allow_dirty_transition {
						self.allow_dirty_transition = false;
					} else if self.has_unsaved_changes() {
						self.pending_action = Some(PendingAction::Exit(window));
						return Task::none();
					} else {
						return self.perform_pending_action(PendingAction::Exit(window));
					}
				}
			}
			Message::ProjectLoaded(data) => {
				self.release_keyboard_midi_notes();
				let (
					project,
					path,
					NoClone(NoDebug(mut arrangement)),
					NoClone(NoDebug(processor)),
					view,
				) = *data;
				let is_new_project = path.is_none();

				self.plugin_picker = None;

				self.top_pane = Tab::Playlist;
				self.bottom_pane = self.bottom_pane.map(|_| Tab::Mixer);
				self.project = project;

				self.save = None;
				self.save_status = None;

				self.current_project.clone_from(&path);
				if let Some(path) = path {
					self.state.last_project = Some(path);
				}
				self.state.recovery_project = None;
				self.state.write();

				arrangement
					.replace_streams(self.arrangement_view.arrangement.replace_streams(None));
				let mut arrangement = std::mem::replace(
					&mut self.arrangement_view,
					ArrangementView::new(arrangement, &self.state, view),
				)
				.arrangement;
				if is_new_project {
					self.saved_project = Some(self.serialize_project());
				}

				let p_receiver = arrangement.request_processor(PullSlot::Full(processor));

				return Task::future(unblock(|| {
					while !arrangement.drain_queue() {
						std::thread::yield_now();
					}
					drop(arrangement);
					drop(p_receiver.recv().unwrap());
				}))
				.discard();
			}
			Message::SaveFinished(save) => {
				if self.save == Some(save) {
					self.save = None;
					self.save_status = None;
				}
			}
			Message::SavePendingAction => {
				if self.pending_action.is_some() {
					self.saving_pending_action = true;
					return self.update(Message::SaveFile);
				}
			}
			Message::DiscardPendingAction => {
				if let Some(action) = self.pending_action.take() {
					self.saving_pending_action = false;
					return self.perform_pending_action(action);
				}
			}
			Message::CancelPendingAction => {
				self.pending_action = None;
				self.saving_pending_action = false;
			}
			Message::SaveAsCancelled => {
				self.saving_pending_action = false;
			}
			Message::ScanProgress(scan, progress) => {
				if self.scan == Some(scan) {
					self.scan_progress = Some(progress);
				}
			}
			Message::ScanStatus(scan, status) => {
				if self.scan == Some(scan) {
					self.scan_status = status;
				}
			}
			Message::PluginScanned(scan, descriptor) => {
				if self.scan == Some(scan) {
					self.plugins.record_scan(&descriptor);
					self.plugins.add(*descriptor, self.plugin_picker.as_mut());
				}
			}
			Message::ScanFinished(scan) => {
				if self.scan == Some(scan) {
					self.scan = None;
					self.scan_progress = None;
					self.scan_status = None;
					self.plugins.finish_scan(self.plugin_picker.as_mut());
					return Self::save_plugin_memory(self.plugins.memory());
				}
			}
			Message::NewFile => {
				if self.allow_dirty_transition {
					self.allow_dirty_transition = false;
				} else if self.has_unsaved_changes() {
					self.pending_action = Some(PendingAction::New);
					return Task::none();
				}
				return Arrangement::empty(
					self.arrangement_view.arrangement.transport().input_channels,
					self.arrangement_view
						.arrangement
						.transport()
						.output_channels,
					self.arrangement_view.arrangement.transport().sample_rate,
					self.arrangement_view.arrangement.transport().frames,
				);
			}
			Message::OpenLastFile => {
				if let Some(last_project) = self.state.last_project.clone() {
					return self.update(Message::OpenFile(last_project, true));
				}
			}
			Message::OpenCrashLog => {
				if let Some(crash_log) = &self.state.crash_log
					&& let Err(err) = open::that_detached(&**crash_log)
				{
					warn!("{err}");
				}
			}
			Message::OpenRecoveryFile => {
				if let Some(recovery_project) = self.state.recovery_project.clone() {
					return self
						.update(Message::CloseRecoveryView)
						.chain(Task::done(Message::OpenFile(recovery_project, false)));
				}
			}
			Message::CloseRecoveryView => {
				self.recovering = false;
				self.state.recovery_project = None;
				self.state.crash_log = None;
				self.state.write();
				return self.update(Message::RescanPlugins);
			}
			Message::DismissRecoveryView => {
				return self.update(Message::CloseRecoveryView).chain(
					if self.config.open_last_project {
						Task::done(Message::OpenLastFile)
					} else {
						Task::none()
					},
				);
			}
			Message::SaveFile => {
				return self.update(
					self.current_project
						.clone()
						.map_or(Message::SaveAsFileDialog, Message::SaveAsFile),
				);
			}
			Message::SaveAsFileDialog => {
				let current_project = self.current_project.clone();
				return window::run(self.main_window_id, |window| {
					let mut dialog = AsyncFileDialog::new();
					if let Some(current_project) = current_project
						&& let Some(current_project) = current_project.file_name()
						&& let Some(current_project) = current_project.to_str()
					{
						dialog = dialog.set_file_name(current_project);
					}
					dialog
						.set_parent(window)
						.add_filter("Generic DAW project file", &["gdp"])
						.set_directory(&*PROJECTS_DIR)
						.save_file()
				})
				.then(Task::future)
				.map(|p| match p {
					Some(p) => Message::SaveAsFile(p.path().with_extension("gdp").into()),
					None => Message::SaveAsCancelled,
				});
			}
			Message::SaveAsFile(path) => {
				let contents = self.serialize_project();
				if let Err(err) = std::fs::write(&path, &contents) {
					warn!("{err}");
					return Task::none();
				}

				self.saved_project = Some(contents);
				let save = Save::unique();
				self.save = Some(save);
				self.save_status = path
					.file_name()
					.map(|name| format!("saved {}", name.display()).into());

				self.current_project = Some(path.clone());
				self.state.last_project = Some(path);
				self.state.recovery_project = None;
				self.state.write();

				let timer = Task::perform(Timer::after(Duration::from_secs(4)), move |_| {
					Message::SaveFinished(save)
				});
				if self.saving_pending_action {
					self.saving_pending_action = false;
					if let Some(action) = self.pending_action.take() {
						return Task::batch([timer, self.perform_pending_action(action)]);
					}
				}
				return timer;
			}
			Message::AutosaveFile => {
				let name = self
					.current_project
					.as_deref()
					.and_then(|path| path.file_prefix())
					.and_then(|name| name.to_str())
					.unwrap_or("autosaved");

				let path =
					Arc::<Path>::from(AUTOSAVED_DIR.join(format!("{} {}.gdp", name, format_now())));
				let gdp = self.arrangement_view.save(
					&mut self.clap_host,
					&self.vst2_host,
					&self.vst3_host,
				);

				if let Some(last_path) = self
					.state
					.recovery_project
					.as_deref()
					.or(self.current_project.as_deref())
					&& let Ok(last_file) = File::open(last_path)
					&& BufReader::new(last_file)
						.bytes()
						.zip(&gdp)
						.all(|(l, &r)| l.is_ok_and(|l| l == r))
				{
					info!("skipped autosave of {}", path.display());
					return Task::none();
				}

				if let Err(err) = std::fs::write(&path, gdp) {
					warn!("{err}");
					return Task::none();
				}

				let save = Save::unique();
				self.save = Some(save);
				self.save_status = path
					.file_name()
					.map(|name| format!("autosaved {}", name.display()).into());

				self.state.recovery_project = Some(path);
				self.state.write();

				return Task::perform(Timer::after(Duration::from_secs(4)), move |_| {
					Message::SaveFinished(save)
				});
			}
			Message::ToggleFullscreen => {
				let id = self.main_window_id;
				return window::mode(id).then(move |mode| match mode {
					window::Mode::Windowed => window::set_mode(id, window::Mode::Fullscreen),
					window::Mode::Fullscreen => window::set_mode(id, window::Mode::Windowed),
					window::Mode::Hidden => Task::none(),
				});
			}
			Message::ChangedGridSize(size) => {
				self.state.grid.size = size.clamp(0.5, 4.5);
				self.state.write();
			}
			Message::NarrowedGrid => {
				return self.update(Message::ChangedGridSize(self.state.grid.size - 1.0));
			}
			Message::WidenedGrid => {
				return self.update(Message::ChangedGridSize(self.state.grid.size + 1.0));
			}
			Message::ToggleGridTriplets => {
				self.state.grid.triplets ^= true;
				self.state.write();
			}
			Message::ToggleGrid => {
				self.state.grid.enabled ^= true;
				self.state.write();
			}
			Message::Progress(progress) => self.progress = Some(progress),
			Message::Status(status) => self.status = status,
			Message::OpenFileDialog => {
				return window::run(self.main_window_id, |window| {
					AsyncFileDialog::new()
						.set_parent(window)
						.add_filter("Generic DAW project file", &["gdp"])
						.set_directory(&*PROJECTS_DIR)
						.pick_file()
				})
				.then(Task::future)
				.and_then(Task::done)
				.map(|p| p.path().into())
				.map(|path| Message::OpenFile(path, true));
			}
			Message::OpenFile(path, current) => {
				if self.allow_dirty_transition {
					self.allow_dirty_transition = false;
				} else if self.has_unsaved_changes() {
					self.pending_action = Some(PendingAction::Open(path, current));
					return Task::none();
				}
				if self.progress.is_none() {
					self.progress = Some(0.0);
					return Arrangement::start_load(
						path,
						current,
						self.arrangement_view.arrangement.transport().input_channels,
						self.arrangement_view
							.arrangement
							.transport()
							.output_channels,
						self.arrangement_view.arrangement.transport().sample_rate,
						self.arrangement_view.arrangement.transport().frames,
						self.config.clone(),
						self.plugins.descriptors().to_owned(),
					);
				}
			}
			Message::CantFindPlugin(name, NoClone(sender)) => {
				if self.progress.is_some() {
					self.missing_plugins.push((name, sender));
				}
			}
			Message::CantFindSample(name, NoClone(sender)) => {
				if self.progress.is_some() {
					self.missing_samples.push((name, sender));
				}
			}
			Message::FindPlugin(index, response) => {
				self.missing_plugins.remove(index).1.send(response).unwrap();
			}
			Message::FindSampleFileDialog(index) => {
				return window::run(self.main_window_id, |window| {
					AsyncFileDialog::new().set_parent(window).pick_file()
				})
				.then(Task::future)
				.and_then(Task::done)
				.map(|p| p.path().into())
				.map(move |response| Message::FindSampleFile(index, Feedback::Use(response)));
			}
			Message::FindSampleFile(index, response) => {
				self.missing_samples.remove(index).1.send(response).unwrap();
			}
			Message::OpenedFile => {
				self.saved_project = Some(self.serialize_project());
				self.progress = None;
				self.status = None;
				self.missing_plugins.clear();
				self.missing_samples.clear();
			}
			Message::RenderFileDialog => {
				let current_project = self.current_project.clone();
				return window::run(self.main_window_id, |window| {
					let mut dialog = AsyncFileDialog::new();
					if let Some(current_project) = current_project
						&& let Some(current_project) = current_project.file_prefix()
						&& let Some(current_project) = current_project.to_str()
					{
						dialog = dialog.set_file_name(format!("{current_project}.wav"));
					}
					dialog
						.set_parent(window)
						.add_filter("Wave file", &["wav"])
						.set_directory(&*PROJECTS_DIR)
						.save_file()
				})
				.then(Task::future)
				.and_then(Task::done)
				.map(|p| p.path().with_extension("wav").into())
				.map(Message::RenderFile);
			}
			Message::RenderFile(path) => {
				if self.progress.is_none() {
					self.progress = Some(0.0);
					self.clap_host.set_render_mode(RenderMode::Offline);
					self.vst2_host.set_render_mode(RenderMode::Offline);
					self.vst3_host.set_render_mode(RenderMode::Offline);
					return self.arrangement_view.arrangement.render(&path);
				}
			}
			Message::RenderedFile => {
				self.clap_host.set_render_mode(RenderMode::Realtime);
				self.vst2_host.set_render_mode(RenderMode::Realtime);
				self.vst3_host.set_render_mode(RenderMode::Realtime);
				self.progress = None;
			}
			Message::TogglePluginPicker => {
				let mut fut = Task::none();
				self.plugin_picker = if self.plugin_picker.is_some() {
					None
				} else {
					let plugin_picker;
					(plugin_picker, fut) = PluginPicker::create(&self.plugins);
					Some(plugin_picker)
				};
				return fut.map(Message::PluginPicker);
			}
			Message::ToggleConfigView => {
				self.config_view = if self.config_view.is_some() {
					None
				} else {
					Some(ConfigView::new(self.main_window_id, &self.config))
				};
			}
			Message::LoadConfig(config) => {
				let mut fut = if self.config.clap_paths == config.clap_paths {
					Task::none()
				} else {
					self.update(Message::RescanPlugins)
				};

				if self.config.sample_paths != config.sample_paths {
					self.file_tree.diff(&config.sample_paths);
				}

				if self.config.audio != config.audio || self.config.midi != config.midi {
					fut = Task::batch([fut, self.update(Message::RescanDevices)]);
				}

				self.config = *config;
				self.shortcuts = Manager::new(&self.config.shortcuts);
				self.state.shortcuts = self.shortcuts.clone();
				self.config.write();

				return fut;
			}
			Message::PluginCopyState(id) => {
				if let Some(state) = self.vst2_host.get_state(id) {
					self.plugin_state = Some(state.into_boxed_slice());
				} else if let Some(state) = self.vst3_host.get_state(id) {
					self.plugin_state = Some(state.into_boxed_slice());
				} else if let Some(state) = self
					.clap_host
					.get_state(id, StateContextType::ForDuplicate)
					.map(Box::from)
				{
					self.plugin_state = Some(state);
				}
			}
			Message::PluginPasteState(id) => {
				let Some(state) = self.plugin_state.clone() else {
					return Task::none();
				};

				if self.vst2_host.has(id) {
					return self.update(Message::Vst2Host(vst2_host::Message::SetState(
						id,
						NoDebug(state),
					)));
				}

				if self.vst3_host.has(id) {
					return self.update(Message::Vst3Host(vst3_host::Message::SetState(
						id,
						NoDebug(state),
					)));
				}

				if self.clap_host.has(id) {
					return self.update(Message::ClapHost(clap_host::Message::SetState(
						id,
						NoDebug(state),
					)));
				}
			}
			Message::RescanDevices => {
				let project = self.project;
				return self
					.arrangement_view
					.change_config()
					.map(move |message| Message::Arrangement(project, message));
			}
			Message::RescanPlugins => {
				// Quick scan: only check if cached plugins still exist, and scan new directories
				return self.quick_scan_plugins();
			}
			Message::FullRescanPlugins => {
				// Full scan: scan all plugin directories from scratch
				return self.full_scan_plugins();
			}
			Message::GenerateMeshes => {
				let now = Instant::now();
				if let Some(previous) = self.last_ui_frame.replace(now) {
					let gap = now.saturating_duration_since(previous);
					if gap >= Duration::from_millis(50) {
						self.slow_ui_frames += 1;
						self.max_ui_frame_gap = self.max_ui_frame_gap.max(gap);
					}
				}
				if now.saturating_duration_since(self.last_ui_frame_report)
					>= Duration::from_secs(5)
				{
					if self.slow_ui_frames > 0 {
						warn!(
							"UI frame pacing: {} gaps over 50 ms, longest {:?}",
							self.slow_ui_frames,
						self.max_ui_frame_gap
						);
					}
					self.slow_ui_frames = 0;
					self.max_ui_frame_gap = Duration::ZERO;
					self.last_ui_frame_report = now;
				}
				crate::mesh_budget::MeshBudget::reset();
				return Task::none();
			}
			Message::FileHovered => self.files_hovered = true,
			Message::FileDropped(path) => {
				self.files_hovered = false;
				if self.state.file_tree_split_at != 0.0
					&& path.metadata().is_ok_and(|metadata| metadata.is_dir())
				{
					self.config.sample_paths.push(path);
					self.config.write();
					self.file_tree.diff(&self.config.sample_paths);
				}
			}
			Message::FileHoveredLeft => self.files_hovered = false,
			Message::TogglePlayback => {
				self.arrangement_view.arrangement.toggle_playback();
			}
			Message::Stop => {
				let before = self.arrangement_view.arrangement.transport().position;
				self.arrangement_view.arrangement.stop();
				let after = self.arrangement_view.arrangement.transport().position;
				self.arrangement_view
					.autoscroll(before, after, &self.config, &mut self.state);
			}
			Message::Undo => _ = self.arrangement_view.undo(),
			Message::Redo => _ = self.arrangement_view.redo(),
			Message::ToggleShowSeconds => {
				self.state.show_seconds ^= true;
				self.state.write();
			}
			Message::ToggleMetronome => {
				self.arrangement_view.arrangement.toggle_metronome();
				self.state.metronome ^= true;
				self.state.write();
			}
			Message::ToggleAutoscroll => {
				self.state.autoscroll ^= true;
				self.state.write();
			}
			Message::TappedBpm => {
				self.bpm_tapper.tap();
				return self.update(Message::ChangedBpm(
					self.bpm_tapper.get_bpm().map(NonZero::get),
				));
			}
			Message::ChangedBpm(bpm) => {
				if let Some(bpm) = bpm {
					self.arrangement_view
						.arrangement
						.set_bpm(NonZero::new(bpm.clamp(10, 999)).unwrap());
				}
			}
			Message::ChangedNumerator(numerator) => {
				if let Some(numerator) = numerator {
					self.arrangement_view
						.arrangement
						.set_numerator(NonZero::new(numerator.clamp(1, 99)).unwrap());
				}
			}
			Message::CycleForwards => {
				let (message, this, other): (fn(_) -> _, _, _) = if let Some(bottom_pane) =
					self.bottom_pane
					&& self.bottom_selected
				{
					(Message::BottomPane, bottom_pane, Some(self.top_pane))
				} else {
					(Message::TopPane, self.top_pane, self.bottom_pane)
				};

				return self.update(message(match this {
					Tab::Playlist if other != Some(Tab::Mixer) => Tab::Mixer,
					Tab::Playlist | Tab::Mixer if other != Some(Tab::PianoRoll) => Tab::PianoRoll,
					Tab::Mixer | Tab::PianoRoll if other != Some(Tab::Playlist) => Tab::Playlist,
					Tab::PianoRoll if other != Some(Tab::Mixer) => Tab::Mixer,
					_ => this,
				}));
			}
			Message::CycleBackwards => {
				let (message, this, other): (fn(_) -> _, _, _) = if let Some(bottom_pane) =
					self.bottom_pane
					&& self.bottom_selected
				{
					(Message::BottomPane, bottom_pane, Some(self.top_pane))
				} else {
					(Message::TopPane, self.top_pane, self.bottom_pane)
				};

				return self.update(message(match this {
					Tab::PianoRoll if other != Some(Tab::Mixer) => Tab::Mixer,
					Tab::Mixer | Tab::PianoRoll if other != Some(Tab::Playlist) => Tab::Playlist,
					Tab::Playlist | Tab::Mixer if other != Some(Tab::PianoRoll) => Tab::PianoRoll,
					Tab::Playlist if other != Some(Tab::Mixer) => Tab::Mixer,
					_ => this,
				}));
			}
			Message::TopPane(top_pane) => {
				if self.bottom_pane == Some(top_pane) {
					return self.update(Message::BottomSelected(true));
				} else if self.top_pane != top_pane {
					self.arrangement_view.finish(self.top_pane);
					self.top_pane = top_pane;
				}
			}
			Message::BottomPane(bottom_pane) => {
				if self.state.bottom_pane_split_at == 0.0 {
					self.state.bottom_pane_split_at = DEFAULT_SPLIT_HEIGHT;
					self.state.write();
				}

				let fut = if self.top_pane == bottom_pane {
					if let Some(bottom_pane) = self.bottom_pane {
						self.top_pane = bottom_pane;
						self.bottom_selected ^= true;
						Task::none()
					} else {
						self.bottom_selected = true;
						self.update(Message::CycleForwards)
					}
				} else {
					self.update(Message::BottomSelected(true))
				};

				self.bottom_pane = Some(bottom_pane);

				return fut;
			}
			Message::BottomSelected(bottom_selected) => {
				if self.bottom_selected != bottom_selected
					&& let Some(bottom_pane) = self.bottom_pane
				{
					self.bottom_selected = bottom_selected;
					self.arrangement_view.clear(if self.bottom_selected {
						self.top_pane
					} else {
						bottom_pane
					});
				}
			}
			Message::MovePaneUp => {
				return if self.bottom_pane.is_none() {
					Task::batch([
						self.update(Message::MovePaneDown),
						self.update(Message::MovePaneUp),
					])
				} else if self.bottom_selected {
					self.update(Message::BottomPane(self.top_pane))
				} else {
					self.update(Message::ClosePane)
				};
			}
			Message::MovePaneDown => {
				return if self.bottom_pane.is_none() || !self.bottom_selected {
					self.update(Message::BottomPane(self.top_pane))
				} else {
					self.update(Message::ClosePane)
				};
			}
			Message::ClosePane => {
				if self.bottom_pane.is_some() {
					return if self.bottom_selected {
						Task::batch([
							self.update(Message::OnBottomPaneDrag(0.0)),
							self.update(Message::OnBottomPaneDragEnd),
						])
					} else {
						Task::batch([
							self.update(Message::MovePaneDown),
							self.update(Message::ClosePane),
						])
					};
				}
			}
			Message::OnFileTreeDrag(split_at) => {
				self.state.file_tree_split_at = if split_at >= 0.1 * MIN_SPLIT_WIDTH {
					split_at
				} else {
					0.0
				};
			}
			Message::OnBottomPaneDrag(split_at) => {
				self.state.bottom_pane_split_at = if split_at >= 0.1 * MIN_SPLIT_HEIGHT {
					split_at
				} else {
					0.0
				};

				if self.state.bottom_pane_split_at != 0.0 && self.bottom_pane.is_none() {
					return self.update(Message::MovePaneUp);
				}
			}
			Message::OnBottomPaneDragEnd => {
				if self.state.bottom_pane_split_at == 0.0 {
					self.bottom_pane = None;
				}
				self.state.write();
			}
			Message::OnFileTreeDragEnd => {
				if self.state.file_tree_split_at == 0.0 {
					self.arrangement_view.arrangement.set_audio_preview(None);
				}
				self.state.write();
			}
			Message::OnFileTreeDoubleClick => {
				return Task::batch([
					self.update(Message::OnFileTreeDrag(DEFAULT_SPLIT_WIDTH)),
					self.update(Message::OnFileTreeDragEnd),
				]);
			}
			Message::OnBottomPaneDoubleClick => {
				return Task::batch([
					self.update(Message::OnBottomPaneDrag(DEFAULT_SPLIT_HEIGHT)),
					self.update(Message::OnFileTreeDragEnd),
				]);
			}
		}

		Task::none()
	}

	/// Writes what we remember about the plugins to disk, without blocking the main
	/// thread while it does.
	fn save_plugin_memory(memory: &PluginMemory) -> Task<Message> {
		let memory = memory.clone();

		Task::future(unblock(move || {
			memory.write();
		}))
		.discard()
	}

	fn handle_instruction(&mut self, instruction: Instruction) -> Task<Message> {
		match instruction {
			Instruction::Message(message) => return self.update(message),
			Instruction::Freeze(node) => {
				self.progress = Some(0.0);
				self.clap_host.set_render_mode(RenderMode::Offline);
				self.vst2_host.set_render_mode(RenderMode::Offline);
				self.vst3_host.set_render_mode(RenderMode::Offline);
				return self.arrangement_view.arrangement.freeze(node, self.project);
			}
			Instruction::PluginLoad(descriptor) => {
				return self.update(Message::Arrangement(
					self.project,
					arrangement_view::Message::PluginAdd(Box::new(descriptor)),
				));
			}
			Instruction::PluginAdd(id, plugin, receiver) => {
				return self
					.clap_host
					.plugin_add(id, plugin, receiver)
					.map(Message::ClapHost);
			}
			Instruction::Vst2PluginAdd(id, plugin, receiver) => {
				return self
					.vst2_host
					.plugin_add(id, plugin, receiver)
					.map(Message::Vst2Host);
			}
			Instruction::Vst2PluginActivate(id, audio, params) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_activate_vst2(node, index, audio, params);
				}
			}
			Instruction::Vst2PluginRestart(id, processor) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_restart_vst2(node, index, processor);
				}
			}
			Instruction::Vst3PluginAdd(id, plugin, receiver) => {
				return self
					.vst3_host
					.plugin_add(id, plugin, receiver)
					.map(Message::Vst3Host);
			}
			Instruction::Vst3PluginActivate(id, audio, params) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_activate_vst3(node, index, audio, params);
				}
			}
			Instruction::Vst3PluginRestart(id, processor) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_restart_vst3(node, index, processor);
				}
			}
			Instruction::PluginCopyState(from, to) => {
				if let Some(state) = self.vst2_host.get_state(from) {
					return self.update(Message::Vst2Host(vst2_host::Message::SetState(
						to,
						NoDebug(state.into_boxed_slice()),
					)));
				}

				if let Some(state) = self.vst3_host.get_state(from) {
					return self.update(Message::Vst3Host(vst3_host::Message::SetState(
						to,
						NoDebug(state.into_boxed_slice()),
					)));
				}

				if let Some(state) = self
					.clap_host
					.get_state(from, StateContextType::ForDuplicate)
					.map(Box::from)
				{
					return self.update(Message::ClapHost(clap_host::Message::SetState(
						to,
						NoDebug(state),
					)));
				}
			}
			Instruction::PluginActivate(id, processor) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_activate(node, index, processor)
						.map(Message::ClapHost);
				}
			}
			Instruction::PluginRestart(id, processor) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					return self
						.arrangement_view
						.arrangement
						.plugin_restart(node, index, processor)
						.map(Message::ClapHost);
				}
			}
			Instruction::PluginParamChanged(id, param_id, value) => {
				if let Some((node, index)) = self.arrangement_view.arrangement.plugin_of(id) {
					self.arrangement_view
						.arrangement
						.plugin_param_changed(node, index, param_id, value);
				}
			}
		}

		Task::none()
	}

	fn handle_file_tree_message(&mut self, message: file_tree::Message) -> Task<Message> {
		match message {
			file_tree::Message::Action(id, action) => {
				return self
					.file_tree
					.update(id, action)
					.unwrap_or_default()
					.map(Message::FileTree);
			}
			file_tree::Message::DragFile(file, kind) => {
				if kind != FileKind::Project {
					self.arrangement_view.hover_file(file, kind);
				}
			}
			file_tree::Message::OpenFile(file, kind) => {
				return if kind == FileKind::Project {
					self.update(Message::OpenFile(file, true))
				} else {
					let project = self.project;
					self.arrangement_view
						.preview_file(file, &self.config, &mut self.state)
						.handle(
							move |message| Message::Arrangement(project, message),
							|instruction| self.handle_instruction(instruction),
						)
				};
			}
			file_tree::Message::OpenDir(dir) => {
				if let Err(err) = open::that_detached(&*dir) {
					warn!("{err}");
				}
			}
		}

		Task::none()
	}

	pub fn view(&self, window: window::Id) -> Element<'_, Message> {
		if let Some(gui) = self.clap_host.view(window) {
			return gui.map(Message::ClapHost);
		}

		if let Some(gui) = self.vst2_host.view(window) {
			return gui.map(Message::Vst2Host);
		}

		if let Some(gui) = self.vst3_host.view(window) {
			return gui.map(Message::Vst3Host);
		}

		debug_assert_eq!(window, self.main_window_id);

		let transport = self.arrangement_view.arrangement.transport();
		let now_beats = transport.position.to_beat_time(transport);

		stack![
			column![
				row![
					Menu::new(menu_icon(), || {
						menu_width(
							vec![
								MenuItem::Action {
									icon: Some(file()),
									label: "New".into(),
									shortcut: self.shortcuts.label(Action::NewFile),
									message: Message::NewFile,
								},
								MenuItem::Action {
									icon: Some(folder_open()),
									label: "Open".into(),
									shortcut: self.shortcuts.label(Action::OpenFile),
									message: Message::OpenFileDialog,
								},
								MenuItem::Action {
									icon: Some(history()),
									label: "Open last".into(),
									shortcut: self.shortcuts.label(Action::OpenLastFile),
									message: Message::OpenLastFile,
								},
								MenuItem::Action {
									icon: Some(save()),
									label: "Save".into(),
									shortcut: self.shortcuts.label(Action::SaveFile),
									message: Message::SaveFile,
								},
								MenuItem::Action {
									icon: Some(save()),
									label: "Save as".into(),
									shortcut: self.shortcuts.label(Action::SaveAsFile),
									message: Message::SaveAsFileDialog,
								},
								MenuItem::Action {
									icon: Some(gavel()),
									label: "Render".into(),
									shortcut: self.shortcuts.label(Action::RenderFile),
									message: Message::RenderFileDialog,
								},
								MenuItem::Separator,
								MenuItem::Action {
									icon: Some(history()),
									label: "Undo".into(),
									shortcut: self.shortcuts.label(Action::Undo),
									message: Message::Undo,
								},
								MenuItem::Action {
									icon: Some(arrow_big_right()),
									label: "Redo".into(),
									shortcut: self.shortcuts.label(Action::Redo),
									message: Message::Redo,
								},
								MenuItem::Separator,
								MenuItem::Action {
									icon: Some(midi_port()),
									label: "Reconnect devices".into(),
									shortcut: String::new(),
									message: Message::RescanDevices,
								},
								MenuItem::Action {
									icon: Some(sliders_vertical()),
									label: "Rescan plugins".into(),
									shortcut: String::new(),
									message: Message::RescanPlugins,
								},
								MenuItem::Action {
									icon: Some(sliders_vertical()),
									label: "Full rescan plugins".into(),
									shortcut: String::new(),
									message: Message::FullRescanPlugins,
								},
								MenuItem::Separator,
								MenuItem::Action {
									icon: Some(settings()),
									label: "Settings".into(),
									shortcut: self.shortcuts.label(Action::ToggleSettings),
									message: Message::ToggleConfigView,
								},
							],
							200.0,
						)
					})
					.style(button_with_radius(button::background, 5))
					.padding(padding::horizontal(7).vertical(5)),
					rule::vertical(1),
					row![
						button(if transport.playing { pause() } else { play() })
							.style(button_with_radius(button::primary, border::left(5)))
							.padding(padding::horizontal(7).vertical(5))
							.on_press(Message::TogglePlayback),
						button(square())
							.style(button_with_radius(button::primary, border::right(5)))
							.padding(padding::horizontal(7).vertical(5))
							.on_press(Message::Stop),
					],
					number_input(1..=99, transport.numerator.get().into(), 4, 5).map(|numerator| {
						Message::ChangedNumerator(numerator.map(|numerator| numerator as u8))
					}),
					row![
						number_input(10..=999, transport.bpm.get().into(), 140, border::left(5))
							.map(|bpm| Message::ChangedBpm(bpm.map(|bpm| bpm as u16))),
						button(metronome())
							.style(button_with_radius(
								if self.state.metronome {
									button::primary
								} else {
									button::secondary
								},
								0
							))
							.padding(5)
							.on_press(Message::ToggleMetronome),
						button(
							mouse_area(container(gavel()).padding(5)).on_press(Message::TappedBpm)
						)
						.style(button_with_radius(button::primary, border::right(5)))
						.padding(0)
						.on_press_with(|| unreachable!()),
					],
					row![
						mouse_area(
							container(
								if self.state.show_seconds {
									text!(
										"{:02}:{:02}:{:03.0}",
										transport.position.second() / 60,
										transport.position.second() % 60,
										(transport.position.to_float().fract() * 1000.0) as u16
									)
								} else {
									text!(
										"{:03}:{:0digits$}",
										now_beats.bar(transport) + 1,
										now_beats.beat_in_bar(transport) + 1,
										digits = transport.numerator.ilog10() as usize + 1,
									)
								}
								.font(Font::MONOSPACE)
							)
							.padding(padding::horizontal(7).vertical(5))
							.style(container_with_radius(weakest_bordered_box, border::left(5)))
						)
						.on_press(Message::ToggleShowSeconds)
						.interaction(Interaction::Pointer),
						button(arrow_big_right())
							.style(button_with_radius(
								if self.state.autoscroll {
									button::primary
								} else {
									button::secondary
								},
								border::right(5)
							))
							.padding(5)
							.on_press(Message::ToggleAutoscroll),
					],
					Menu::new(magnet(), || container(
						column![
							checkbox(self.state.grid.enabled)
								.label("Grid enabled")
								.on_toggle(|_| Message::ToggleGrid)
								.style(if self.state.grid.enabled {
									checkbox::primary
								} else {
									checkbox::secondary
								}),
							checkbox(self.state.grid.triplets)
								.label("Triplet grid")
								.on_toggle(|_| Message::ToggleGridTriplets)
								.style(if self.state.grid.enabled {
									checkbox::primary
								} else {
									checkbox::secondary
								}),
							slider(0.5..=4.5, self.state.grid.size, Message::ChangedGridSize)
								.style(if self.state.grid.enabled {
									slider::primary
								} else {
									slider::secondary
								})
						]
						.width(Shrink)
						.spacing(5)
					)
					.padding(5)
					.style(container_with_radius(weaker_bordered_box, 5))
					.into())
					.padding(5)
					.style(button_with_radius(
						if self.state.grid.enabled {
							button::primary
						} else {
							button::secondary
						},
						5
					)),
					right(
						self.save_status
							.as_deref()
							.map(|status| Element::new(
								text(status)
									.size(13)
									.wrapping(text::Wrapping::None)
									.ellipsis(text::Ellipsis::End)
							))
							.or_else(|| {
								self.scan_progress.map(|progress| {
									column![
										self.scan_status.as_deref().map(|status| text(status)
											.size(13)
											.wrapping(text::Wrapping::None)
											.ellipsis(text::Ellipsis::End)),
										progress_bar(0.0..=1.0, progress).girth(4).style(
											progress_bar_with_radius(
												progress_bar::secondary,
												f32::INFINITY
											)
										)
									]
									.spacing(5)
									.width(Fill.max(200))
									.into()
								})
							})
					),
					row![
						cpu(),
						text!("{:.1}%", self.arrangement_view.arrangement.load() * 100.0)
							.font(Font::MONOSPACE)
					]
					.spacing(5),
					row![
						Tab::Playlist.tab(self.top_pane, self.bottom_pane, self.bottom_selected),
						Tab::Mixer.tab(self.top_pane, self.bottom_pane, self.bottom_selected),
						Tab::PianoRoll.tab(self.top_pane, self.bottom_pane, self.bottom_selected),
					],
				]
				.height(Shrink)
				.align_y(Center)
				.spacing(10),
				vertical_split(
					(self.state.file_tree_split_at != 0.0).then(|| stack![
						self.file_tree
							.view(
								self.arrangement_view
									.arrangement
									.audio_preview()
									.map(|sample| &*self.arrangement_view.arrangement.samples()
										[&sample]
										.path)
							)
							.map(Message::FileTree),
						self.files_hovered.then(|| center(plus().size(40.0))
							.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))))
					]
					.width(Fill.min(MIN_SPLIT_WIDTH))),
					container(
						horizontal_split(
							container(
								SelectArea::new(
									container(
										self.arrangement_view.view(self.top_pane, &self.state).map(
											|message| Message::Arrangement(self.project, message)
										),
									)
									.padding(self.bottom_pane.map_or_default(|_| 5))
									.style(container_with_radius(
										selectable_box(
											container::transparent,
											self.bottom_pane.is_some() && !self.bottom_selected,
										),
										self.top_pane.radius(),
									)),
								)
								.on_select_maybe(
									(self.bottom_pane.is_some() && self.bottom_selected)
										.then_some(Message::BottomSelected(false)),
								),
							)
							.height(Fill.min(MIN_SPLIT_HEIGHT)),
							(self.state.bottom_pane_split_at != 0.0).then(|| container(
								SelectArea::new(
									container(
										self.arrangement_view
											.view(self.bottom_pane.unwrap(), &self.state)
											.map(|message| Message::Arrangement(
												self.project,
												message
											)),
									)
									.padding(5)
									.style(container_with_radius(
										selectable_box(
											container::transparent,
											self.bottom_selected,
										),
										self.bottom_pane.unwrap().radius(),
									)),
								)
								.on_select_maybe(
									(!self.bottom_selected)
										.then_some(Message::BottomSelected(true)),
								)
							)
							.height(Fill.min(MIN_SPLIT_HEIGHT))),
							self.state.bottom_pane_split_at,
							Message::OnBottomPaneDrag,
						)
						.on_drag_end(Message::OnBottomPaneDragEnd)
						.on_double_click(Message::OnBottomPaneDoubleClick)
						.strategy(Strategy::End)
						.focus_delay(Duration::ZERO)
						.style(split_style)
					)
					.width(Fill.min(2.0 * MIN_SPLIT_WIDTH + 21.0)),
					self.state.file_tree_split_at,
					Message::OnFileTreeDrag
				)
				.on_drag_end(Message::OnFileTreeDragEnd)
				.on_double_click(Message::OnFileTreeDoubleClick)
				.strategy(Strategy::Start)
				.focus_delay(Duration::ZERO)
				.style(split_style)
			]
			.padding(10)
			.spacing(10),
			self.arrangement_view
				.loading()
				.then(|| mouse_area(space().width(Fill).height(Fill))
					.interaction(Interaction::Progress)),
			self.plugin_picker.as_ref().map(|plugin_picker| opaque(
				mouse_area(
					center(opaque(
						plugin_picker.view(&self.plugins).map(Message::PluginPicker)
					))
					.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))),
				)
				.on_press(Message::TogglePluginPicker),
			)),
			self.config_view.as_ref().map(|config_view| opaque(
				mouse_area(
					center(opaque(
						config_view.view(&self.config).map(Message::ConfigView)
					))
					.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))),
				)
				.on_press(Message::ToggleConfigView),
			)),
			self.pending_action.as_ref().map(|action| {
				let verb = match action {
					PendingAction::Exit(_) => "quitting",
					PendingAction::Open(_, _) => "opening another project",
					PendingAction::New => "starting a new project",
				};
				opaque(
					mouse_area(
						center(
							container(
								column![
									text(format!("Save changes before {verb}?"))
										.size(LINE_HEIGHT)
										.font(Font::DEFAULT.weight(Weight::Bold)),
									rule::horizontal(1),
									row![
										button("Save")
											.style(button_with_radius(
												button::success,
												border::left(5)
											))
										.on_press(Message::SavePendingAction),
										button("Don't Save")
											.style(button_with_radius(button::warning, 0))
											.on_press(Message::DiscardPendingAction),
										button("Cancel")
											.style(button_with_radius(
												button::subtle,
												border::right(5)
											))
										.on_press(Message::CancelPendingAction),
								]
									.spacing(10),
								]
								.spacing(10)
							)
							.width(Shrink)
							.padding(15)
							.style(container_with_radius(weakest_bordered_box, 10))
						)
						.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))),
					)
					.on_press(Message::CancelPendingAction),
				)
			}),

			self.progress.map(|progress| mouse_area(
				container(
					column![
						bottom_center(self.status.as_deref().map(|status| {
							container(
								row![
									"scanning",
									container(
										text(status)
											.font(Font::MONOSPACE)
											.wrapping(text::Wrapping::None)
											.ellipsis(text::Ellipsis::Middle)
									)
									.padding(padding::horizontal(10).vertical(5))
									.style(container_with_radius(weakest_bordered_box, 5))
								]
								.align_y(Center)
								.spacing(10),
							)
							.padding(10)
							.style(container_with_radius(weak_bordered_box, 5))
						})),
						column![
							progress_bar(0.0..=1.0, progress).style(progress_bar_with_radius(
								if self.missing_plugins.is_empty()
									&& self.missing_samples.is_empty()
								{
									progress_bar::primary
								} else {
									progress_bar::danger
								},
								5
							)),
							scrollable(
								column(
									self.missing_plugins
										.iter()
										.map(|(name, _)| &**name)
										.enumerate()
										.map(|(i, name)| {
											row![
												"can't find plugin",
												container(
													text(name.to_string_lossy())
														.font(Font::MONOSPACE)
														.wrapping(text::Wrapping::None)
														.ellipsis(text::Ellipsis::Middle)
												)
												.padding(padding::horizontal(10).vertical(5))
												.style(container_with_radius(
													weakest_bordered_box,
													5
												)),
												row![
													button("Ignore")
														.on_press(Message::FindPlugin(
															i,
															Feedback::Ignore
														))
														.style(button_with_radius(
															button::warning,
															border::left(5)
														)),
													button("Cancel")
														.on_press(Message::FindPlugin(
															i,
															Feedback::Cancel
														))
														.style(button_with_radius(
															button::danger,
															border::right(5)
														))
												]
											]
											.align_y(Center)
											.spacing(10)
										})
										.chain(
											self.missing_samples
												.iter()
												.map(|(name, _)| &**name)
												.enumerate()
												.map(|(i, name)| {
													row![
														"can't find sample",
														container(
															text(name)
																.font(Font::MONOSPACE)
																.wrapping(text::Wrapping::None)
																.ellipsis(text::Ellipsis::Middle)
														)
														.padding(
															padding::horizontal(10).vertical(5)
														)
														.style(
															container_with_radius(
																weakest_bordered_box,
																5
															)
														),
														row![
															button("Pick")
																.on_press(
																	Message::FindSampleFileDialog(
																		i
																	)
																)
																.style(button_with_radius(
																	button::success,
																	border::left(5)
																)),
															button("Ignore")
																.on_press(Message::FindSampleFile(
																	i,
																	Feedback::Ignore
																))
																.style(button_with_radius(
																	button::warning,
																	0
																)),
															button("Cancel")
																.on_press(Message::FindSampleFile(
																	i,
																	Feedback::Cancel
																))
																.style(button_with_radius(
																	button::danger,
																	border::right(5)
																))
														]
													]
													.align_y(Center)
													.spacing(10)
												})
										)
										.map(|widget| container(widget)
											.padding(10)
											.style(container_with_radius(weak_bordered_box, 10))
											.into()),
								)
								.align_x(Center)
								.spacing(10)
							)
							.direction(scrollable::Direction::Vertical(
								scrollable::Scrollbar::hidden(),
							))
						]
						.align_x(Center)
						.spacing(20),
						space::vertical(),
					]
					.align_x(Center)
					.spacing(20)
				)
				.padding(50)
				.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))),
			)
			.interaction(Interaction::Progress)),
			self.recovering.then(|| opaque(
				mouse_area(
					center(opaque(
						container(
							column![
								text("Generic DAW closed unexpectedly!")
									.size(LINE_HEIGHT)
									.line_height(1.0)
									.font(Font::DEFAULT.weight(Weight::Bold)),
								rule::horizontal(1),
								row![
									row![
										self.state.crash_log.is_some().then(|| button(
											"View crash log"
										)
										.style(button_with_radius(
											button::subtle,
											if self.state.recovery_project.is_some() {
												border::left(5)
											} else {
												border::radius(5)
											}
										))
										.on_press(Message::OpenCrashLog)),
										self.state.recovery_project.is_some().then(|| button(
											"Continue from last autosave"
										)
										.style(button_with_radius(
											button::subtle,
											if self.state.crash_log.is_some() {
												border::right(5)
											} else {
												border::radius(5)
											}
										))
										.on_press(Message::OpenRecoveryFile)),
									],
									right(
										button("Dismiss")
											.style(button_with_radius(button::subtle, 5))
											.on_press(Message::DismissRecoveryView)
									)
								]
								.width(Shrink)
								.spacing(10)
							]
							.width(Shrink)
							.spacing(10)
							.padding(10)
						)
						.style(container_with_radius(weakest_bordered_box, 10))
					))
					.style(|_| container::background(Color::BLACK.scale_alpha(ALPHA_2_3))),
				)
				.on_press(Message::DismissRecoveryView),
			))
		]
		.into()
	}

	fn serialize_project(&mut self) -> Vec<u8> {
		self.arrangement_view.save(
			&mut self.clap_host,
			&self.vst2_host,
			&self.vst3_host,
		)
	}

	fn has_unsaved_changes(&mut self) -> bool {
		let current = self.serialize_project();
		self.saved_project
			.as_ref()
			.map_or(true, |saved| *saved != current)
	}

	fn perform_pending_action(&mut self, action: PendingAction) -> Task<Message> {
		match action {
			PendingAction::Exit(window) => {
				if window == self.main_window_id {
					if !self.recovering {
						self.state.recovery_project = None;
						self.state.write();
					}
					self.plugins.memory().write();
					iced::exit()
				} else {
					Task::none()
				}
			}
			PendingAction::Open(path, current) => {
				self.allow_dirty_transition = true;
				self.update(Message::OpenFile(path, current))
			}
			PendingAction::New => {
				self.allow_dirty_transition = true;
				self.update(Message::NewFile)
			}
		}
	}

	pub fn title(&self, window: window::Id) -> String {
		self.clap_host
			.title(window)
			.or_else(|| self.vst2_host.title(window))
			.or_else(|| self.vst3_host.title(window))
			.or_else(|| {
				self.current_project
					.as_deref()
					.and_then(|current_project| current_project.file_prefix())
					.and_then(|current_project| current_project.to_str())
					.map(|current_project| format!("{current_project} - Generic DAW"))
			})
			.unwrap_or_else(|| "Generic DAW".to_owned())
	}

	pub fn theme(&self, _window: window::Id) -> Theme {
		self.config.theme.into()
	}

	pub fn scale_factor(&self, window: window::Id) -> f32 {
		self.clap_host
			.scale_factor(window)
			.or_else(|| self.vst2_host.scale_factor(window))
			.or_else(|| self.vst3_host.scale_factor(window))
			.unwrap_or(self.config.scale_factor)
	}

	pub fn subscription(&self) -> Subscription<Message> {
		let main_window_id = self.main_window_id;

		Subscription::batch([
			self.arrangement_view
				.subscription()
				.with(self.project)
				.map(|(project, message)| Message::Arrangement(project, message)),
			self.clap_host.subscription().map(Message::ClapHost),
			Vst2Host::subscription().map(Message::Vst2Host),
			Vst3Host::subscription().map(Message::Vst3Host),
			if self.config.autosave.enabled && !self.recovering {
				every(Duration::from_secs(
					self.config.autosave.interval.get().into(),
				))
				.map(|_| Message::AutosaveFile)
			} else {
				Subscription::none()
			},
			keyboard::listen().filter_map(|event| match event {
				event @ (keyboard::Event::KeyPressed { .. }
				| keyboard::Event::KeyReleased { .. }) => Some(Message::Keyboard(event)),
				_ => None,
			}),
			// Reset mesh budget at the start of each frame
			window::frames().map(|_| Message::GenerateMeshes),
			window::events().filter_map(move |(window, event)| match event {
				window::Event::Focused if window == main_window_id => {
					Some(Message::MainWindowFocus(true))
				}
				window::Event::Unfocused if window == main_window_id => {
					Some(Message::MainWindowFocus(false))
				}
				window::Event::CloseRequested => Some(Message::CloseRequested(window)),
				window::Event::FileHovered(..) => Some(Message::FileHovered),
				window::Event::FileDropped(file) => Some(Message::FileDropped(file.into())),
				window::Event::FilesHoveredLeft => Some(Message::FileHoveredLeft),
				_ => None,
			}),
		])
	}

	/// The message of a shortcut that applies everywhere in the app.
	fn keybinds(action: Action) -> Option<Message> {
		match action {
			Action::TogglePlayback => Some(Message::TogglePlayback),
			Action::Stop => Some(Message::Stop),
			Action::Undo => Some(Message::Undo),
			Action::Redo => Some(Message::Redo),
			Action::ToggleFullscreen => Some(Message::ToggleFullscreen),
			Action::ToggleMetronome => Some(Message::ToggleMetronome),
			Action::ToggleSettings => Some(Message::ToggleConfigView),
			Action::NewFile => Some(Message::NewFile),
			Action::OpenFile => Some(Message::OpenFileDialog),
			Action::OpenLastFile => Some(Message::OpenLastFile),
			Action::SaveFile => Some(Message::SaveFile),
			Action::SaveAsFile => Some(Message::SaveAsFileDialog),
			Action::RenderFile => Some(Message::RenderFileDialog),
			Action::ClosePane => Some(Message::ClosePane),
			Action::NarrowedGrid => Some(Message::NarrowedGrid),
			Action::WidenedGrid => Some(Message::WidenedGrid),
			Action::ToggleGridTriplets => Some(Message::ToggleGridTriplets),
			Action::ToggleGrid => Some(Message::ToggleGrid),
			Action::CycleForwards => Some(Message::CycleForwards),
			Action::CycleBackwards => Some(Message::CycleBackwards),
			Action::SelectPaneUp => Some(Message::BottomSelected(false)),
			Action::SelectPaneDown => Some(Message::BottomSelected(true)),
			Action::MovePaneUp => Some(Message::MovePaneUp),
			Action::MovePaneDown => Some(Message::MovePaneDown),
			_ => None,
		}
	}

	/// The message of the shortcut that was pressed, if there is one.
	fn shortcut(&mut self, chord: Chord, repeat: bool) -> Option<Message> {
		if self.recovering {
			// the recovery view is a special case: the only thing that can be
			// done is to go back to what the user was doing
			return (chord == Chord::ESCAPE).then_some(Message::DismissRecoveryView);
		}

		// the settings are in the way, so they get the key first
		if let Some(config_view) = &mut self.config_view {
			if config_view.is_recording() {
				return Some(Message::ConfigView(config_view::Message::ShortcutPressed(
					chord,
				)));
			}

			let action = self.shortcuts.lookup(Context::Settings, &chord)?;

			if repeat && !action.repeatable() {
				return None;
			}

			return ConfigView::keybinds(action);
		}

		// the plugin picker is in the way, so it gets the key first
		if self.plugin_picker.is_some() {
			let action = self.shortcuts.lookup(Context::PluginPicker, &chord)?;

			if repeat && !action.repeatable() {
				return None;
			}

			return PluginPicker::keybinds(action);
		}

		// the arrangement gets the key before the app does
		let action = self
			.shortcuts
			.lookup(Context::Arrangement, &chord)
			.or_else(|| self.shortcuts.lookup(Context::Global, &chord))?;

		if repeat && !action.repeatable() {
			return None;
		}

		if let Some(message) = ArrangementView::keybinds(action) {
			let project = self.project;
			let tab = self
				.bottom_pane
				.filter(|_| self.bottom_selected)
				.unwrap_or(self.top_pane);

			return Some(Message::Arrangement(project, message(tab)));
		}

		Self::keybinds(action)
	}

	fn handle_keyboard_event(&mut self, event: keyboard::Event) -> Task<Message> {
		match event {
			keyboard::Event::KeyPressed {
				key,
				physical_key,
				modifiers,
				repeat,
				..
			} => {
				if physical_key == keyboard::key::Code::CapsLock {
					if !repeat && self.main_window_focused {
						self.keyboard_midi_active = !self.keyboard_midi_active;
						if !self.keyboard_midi_active {
							self.release_keyboard_midi_notes();
						}
					}
					return Task::none();
				}

				let midi_keyboard_ready = self.keyboard_midi_active
					&& self.main_window_focused
					&& self.progress.is_none()
					&& self.config_view.is_none()
					&& self.plugin_picker.is_none()
					&& !self.recovering;

				if midi_keyboard_ready
					&& !modifiers.command()
					&& !modifiers.alt()
					&& !modifiers.shift()
					&& let Some(midi_key) = Self::midi_keyboard_key(physical_key)
				{
					if !repeat && !self.keyboard_midi_notes.contains_key(&physical_key)
						&& let Some(track) =
							self.arrangement_view.midi_keyboard_note_on(midi_key, 0.8)
					{
						self.keyboard_midi_notes
							.insert(physical_key, (track, midi_key));
					}
					return Task::none();
				}

				if self.progress.is_none()
					&& let Some(chord) = Chord::from_event(&key, physical_key, modifiers)
				{
					return self.update(Message::Shortcut(chord, repeat));
				}
			}
			keyboard::Event::KeyReleased { physical_key, .. } => {
				if let Some((track, key)) = self.keyboard_midi_notes.remove(&physical_key) {
					self.arrangement_view.midi_keyboard_note_off(track, key);
				}
			}
			_ => {}
		}

		Task::none()
	}

	fn midi_keyboard_key(physical_key: keyboard::key::Physical) -> Option<MidiKey> {
		let keyboard::key::Physical::Code(code) = physical_key else {
			return None;
		};

		let note = match code {
			keyboard::key::Code::KeyZ => 48,
			keyboard::key::Code::KeyS => 49,
			keyboard::key::Code::KeyX => 50,
			keyboard::key::Code::KeyD => 51,
			keyboard::key::Code::KeyC => 52,
			keyboard::key::Code::KeyV => 53,
			keyboard::key::Code::KeyG => 54,
			keyboard::key::Code::KeyB => 55,
			keyboard::key::Code::KeyH => 56,
			keyboard::key::Code::KeyN => 57,
			keyboard::key::Code::KeyJ => 58,
			keyboard::key::Code::KeyM => 59,
			keyboard::key::Code::KeyQ => 60,
			keyboard::key::Code::Digit2 => 61,
			keyboard::key::Code::KeyW => 62,
			keyboard::key::Code::Digit3 => 63,
			keyboard::key::Code::KeyE => 64,
			keyboard::key::Code::KeyR => 65,
			keyboard::key::Code::Digit5 => 66,
			keyboard::key::Code::KeyT => 67,
			keyboard::key::Code::Digit6 => 68,
			keyboard::key::Code::KeyY => 69,
			keyboard::key::Code::Digit7 => 70,
			keyboard::key::Code::KeyU => 71,
			_ => return None,
		};

		Some(MidiKey(note))
	}

	fn release_keyboard_midi_notes(&mut self) {
		let notes = std::mem::take(&mut self.keyboard_midi_notes);
		for (_, (track, key)) in notes {
			self.arrangement_view.midi_keyboard_note_off(track, key);
		}
	}

	/// Quick scan: check if cached plugins still exist, and only scan directories that might have new plugins.
	fn quick_scan_plugins(&mut self) -> Task<Message> {
		let scan = Scan::unique();
		self.plugins.start_scan();
		self.scan = Some(scan);
		self.scan_progress = Some(0.0);
		self.scan_status = None;

		// First, load all cached plugins that still exist on disk
		let cached_descriptors = self
			.plugins
			.memory()
			.plugins
			.iter()
			.filter_map(PluginEntry::descriptor)
			.filter(|desc| desc.path().exists())
			.collect::<Vec<_>>();

		// Add cached plugins immediately
		for descriptor in &cached_descriptors {
			self.plugins.record_scan(descriptor);
			self.plugins
				.add(descriptor.clone(), self.plugin_picker.as_mut());
		}

		// Find plugin paths from config and defaults
		let clap_paths = self.config.clap_paths.clone();
		let (sender, receiver) = smol::channel::unbounded();

		return Task::batch([
			Task::future(unblock(move || {
				// Find all plugin paths
				let clap_plugin_paths =
					clap_host::find_plugin_paths(DEFAULT_CLAP_PATHS.iter().chain(&clap_paths))
						.collect::<Box<_>>();
				let vst2_plugin_paths = generic_daw_core::vst2_host::find_plugin_paths(
					generic_daw_core::vst2_host::DEFAULT_VST2_PATHS.iter(),
				)
				.collect::<Box<_>>();
				let vst3_plugin_paths = generic_daw_core::vst3_host::find_plugin_paths(
					generic_daw_core::vst3_host::DEFAULT_VST3_PATHS.iter(),
				)
				.collect::<Box<_>>();

				// Build a set of known plugin paths from cache
				let known_paths: std::collections::HashSet<Arc<Path>> = cached_descriptors
					.iter()
					.map(|d| d.path().clone())
					.collect();

				// Filter to only NEW paths that we haven't checked
				let new_clap_paths: Vec<_> = clap_plugin_paths
					.into_iter()
					.filter(|p| !known_paths.contains(p))
					.collect();
				let new_vst2_paths: Vec<_> = vst2_plugin_paths
					.into_iter()
					.filter(|p| !known_paths.contains(p))
					.collect();
				let new_vst3_paths: Vec<_> = vst3_plugin_paths
					.into_iter()
					.filter(|p| !known_paths.contains(p))
					.collect();

				let len = new_clap_paths.len() + new_vst2_paths.len() + new_vst3_paths.len();
				let mut done = 0;

				let status = |path: &Path| {
					sender
						.try_send(Message::ScanStatus(
							scan,
							path.file_name()
								.map(|name| format!("scanning {}", name.display()).into()),
						))
						.unwrap();
				};

				for path in new_clap_paths {
					status(&path);

					if let Some(descriptors) = Plugin::descriptors(&path) {
						for descriptor in descriptors {
							sender
								.try_send(Message::PluginScanned(
									scan,
									Box::new(PluginDescriptor::Clap(descriptor)),
								))
								.unwrap();
						}
					}

					done += 1;
					if len > 0 {
						sender
							.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
							.unwrap();
					}
				}

				// The plugins are made in a program of its own, so that a plugin
				// that crashes doesn't take this program with it.
				generic_daw_core::vst2_host::scan::scan(&new_vst2_paths, |event| match event {
					generic_daw_core::vst2_host::scan::Event::Scanning(path) => {
						status(&path);
					}
					generic_daw_core::vst2_host::scan::Event::Found(descriptor) => {
						sender
							.try_send(Message::PluginScanned(
								scan,
								Box::new(PluginDescriptor::Vst2(descriptor)),
							))
							.unwrap();
					}
				});

				done += new_vst2_paths.len();
				if len > 0 {
					sender
						.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
						.unwrap();
				}

				for path in new_vst3_paths {
					status(&path);

					if let Some(descriptors) = generic_daw_core::vst3_host::descriptors(&path) {
						for descriptor in descriptors {
							sender
								.try_send(Message::PluginScanned(
									scan,
									Box::new(PluginDescriptor::Vst3(descriptor)),
								))
								.unwrap();
						}
					}

					done += 1;
					if len > 0 {
						sender
							.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
							.unwrap();
					}
				}

				sender.try_send(Message::ScanFinished(scan)).unwrap();
			}))
			.discard(),
			Task::stream(receiver),
		]);
	}

	/// Full scan: scan all plugin directories from scratch.
	fn full_scan_plugins(&mut self) -> Task<Message> {
		let scan = Scan::unique();
		self.plugins.start_scan();
		self.scan = Some(scan);
		self.scan_progress = Some(0.0);
		self.scan_status = None;

		let (sender, receiver) = smol::channel::unbounded();
		let clap_paths = self.config.clap_paths.clone();

		return Task::batch([
			Task::future(unblock(move || {
				let clap_plugin_paths =
					clap_host::find_plugin_paths(DEFAULT_CLAP_PATHS.iter().chain(&clap_paths))
						.collect::<Box<_>>();
				let vst2_plugin_paths = generic_daw_core::vst2_host::find_plugin_paths(
					generic_daw_core::vst2_host::DEFAULT_VST2_PATHS.iter(),
				)
				.collect::<Box<_>>();
				let vst3_plugin_paths = generic_daw_core::vst3_host::find_plugin_paths(
					generic_daw_core::vst3_host::DEFAULT_VST3_PATHS.iter(),
				)
				.collect::<Box<_>>();

				let len =
					clap_plugin_paths.len() + vst2_plugin_paths.len() + vst3_plugin_paths.len();
				let mut done = 0;

				let status = |path: &Path| {
					sender
						.try_send(Message::ScanStatus(
							scan,
							path.file_name()
								.map(|name| format!("scanning {}", name.display()).into()),
						))
						.unwrap();
				};

				for path in clap_plugin_paths {
					status(&path);

					if let Some(descriptors) = Plugin::descriptors(&path) {
						for descriptor in descriptors {
							sender
								.try_send(Message::PluginScanned(
									scan,
									Box::new(PluginDescriptor::Clap(descriptor)),
								))
								.unwrap();
						}
					}

					done += 1;
					sender
						.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
						.unwrap();
				}

				// The plugins are made in a program of its own, so that a plugin
				// that crashes doesn't take this program with it.
				generic_daw_core::vst2_host::scan::scan(&vst2_plugin_paths, |event| match event {
					generic_daw_core::vst2_host::scan::Event::Scanning(path) => {
						status(&path);
					}
					generic_daw_core::vst2_host::scan::Event::Found(descriptor) => {
						sender
							.try_send(Message::PluginScanned(
								scan,
								Box::new(PluginDescriptor::Vst2(descriptor)),
							))
							.unwrap();
					}
				});

				done += vst2_plugin_paths.len();
				sender
					.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
					.unwrap();

				for path in vst3_plugin_paths {
					status(&path);

					if let Some(descriptors) = generic_daw_core::vst3_host::descriptors(&path) {
						for descriptor in descriptors {
							sender
								.try_send(Message::PluginScanned(
									scan,
									Box::new(PluginDescriptor::Vst3(descriptor)),
								))
								.unwrap();
						}
					}

					done += 1;
					sender
						.try_send(Message::ScanProgress(scan, done as f32 / len as f32))
						.unwrap();
				}

				sender.try_send(Message::ScanFinished(scan)).unwrap();
			}))
			.discard(),
			Task::stream(receiver),
		]);
	}
}
