//! The keyboard shortcuts, and how the user can change them.

use iced::keyboard;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::{collections::HashMap, fmt, str::FromStr};

/// Where a shortcut applies.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Context {
	/// Everywhere in the app.
	Global,
	/// The arrangement.
	Arrangement,
	/// The MIDI editor.
	MidiEditor,
	/// The plugin picker.
	PluginPicker,
	/// The settings.
	Settings,
}

impl Context {
	/// All the contexts, in the order they are shown in.
	pub const ALL: [Self; 5] = [
		Self::Global,
		Self::Arrangement,
		Self::MidiEditor,
		Self::PluginPicker,
		Self::Settings,
	];

	/// What the context is called.
	#[must_use]
	pub const fn label(self) -> &'static str {
		match self {
			Self::Global => "Global",
			Self::Arrangement => "Arrangement",
			Self::MidiEditor => "MIDI Editor",
			Self::PluginPicker => "Plugin Picker",
			Self::Settings => "Settings",
		}
	}
}

/// Something the user can do with the keyboard.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Action {
	// Global
	/// Start and stop the playback.
	TogglePlayback,
	/// Stop the playback.
	Stop,
	/// Make the window fill the screen, or not.
	ToggleFullscreen,
	/// Turn the metronome on or off.
	ToggleMetronome,
	/// Show or hide the settings.
	ToggleSettings,
	/// Make a new project.
	NewFile,
	/// Open a project.
	OpenFile,
	/// Open the project that was open last.
	OpenLastFile,
	/// Save the project.
	SaveFile,
	/// Save the project under a different name.
	SaveAsFile,
	/// Render the project to a file.
	RenderFile,
	/// Close the pane that has the focus.
	ClosePane,
	/// Make the grid finer.
	NarrowedGrid,
	/// Make the grid coarser.
	WidenedGrid,
	/// Turn the triplets in the grid on or off.
	ToggleGridTriplets,
	/// Turn the grid on or off.
	ToggleGrid,
	/// Go to the next tab.
	CycleForwards,
	/// Go to the previous tab.
	CycleBackwards,
	/// Give the focus to the pane above.
	SelectPaneUp,
	/// Give the focus to the pane below.
	SelectPaneDown,
	/// Move the pane that has the focus up.
	MovePaneUp,
	/// Move the pane that has the focus down.
	MovePaneDown,
	/// Undo the last change.
	Undo,
	/// Redo the last change that was undone.
	Redo,

	// Arrangement
	/// Delete what is selected.
	Delete,
	/// Stop selecting things.
	Escape,
	/// Move the selection up.
	CursorUp,
	/// Move the selection down.
	CursorDown,
	/// Move the selection to the left.
	CursorLeft,
	/// Move the selection to the right.
	CursorRight,
	/// Select everything.
	SelectAll,
	/// Select everything that isn't selected.
	SelectInverse,
	/// Copy what is selected.
	Duplicate,
	/// Give a clip its own audio sample or MIDI pattern.
	MakeUnique,
	/// Transpose what is selected an octave up.
	TransposeOctaveUp,
	/// Transpose what is selected an octave down.
	TransposeOctaveDown,
	/// Invert what is selected.
	Invert,
	/// Join what is selected into one thing.
	Join,
	/// Turn what is selected on or off.
	ToggleEnabled,
	/// Make what is selected as loud as it can be.
	Normalize,
	/// Move what is selected to the grid.
	Quantize,
	/// Play what is selected backwards.
	Reverse,
	/// Turn the solo of what is selected on or off.
	ToggleSolo,

	// MIDI editor
	/// Toggle per-note expression editing in the MIDI editor.
	ToggleMpeMode,

	// Plugin picker
	/// Close the plugin picker.
	ClosePluginPicker,
	/// Add the plugin that is highlighted.
	SelectPlugin,
	/// Highlight the plugin above.
	HoverPrevious,
	/// Highlight the plugin below.
	HoverNext,

	// Settings
	/// Close the settings.
	CloseSettings,
}

impl Action {
	/// All the actions, in the order they are shown in.
	pub const ALL: [Self; 49] = [
		Self::TogglePlayback,
		Self::Stop,
		Self::ToggleFullscreen,
		Self::ToggleMetronome,
		Self::ToggleSettings,
		Self::NewFile,
		Self::OpenFile,
		Self::OpenLastFile,
		Self::SaveFile,
		Self::SaveAsFile,
		Self::RenderFile,
		Self::ClosePane,
		Self::NarrowedGrid,
		Self::WidenedGrid,
		Self::ToggleGridTriplets,
		Self::ToggleGrid,
		Self::CycleForwards,
		Self::CycleBackwards,
		Self::SelectPaneUp,
		Self::SelectPaneDown,
		Self::MovePaneUp,
		Self::MovePaneDown,
		Self::Undo,
		Self::Redo,
		Self::Delete,
		Self::Escape,
		Self::CursorUp,
		Self::CursorDown,
		Self::CursorLeft,
		Self::CursorRight,
		Self::SelectAll,
		Self::SelectInverse,
		Self::Duplicate,
		Self::MakeUnique,
		Self::TransposeOctaveUp,
		Self::TransposeOctaveDown,
		Self::Invert,
		Self::Join,
		Self::ToggleEnabled,
		Self::Normalize,
		Self::Quantize,
		Self::Reverse,
		Self::ToggleSolo,
		Self::ToggleMpeMode,
		Self::ClosePluginPicker,
		Self::SelectPlugin,
		Self::HoverPrevious,
		Self::HoverNext,
		Self::CloseSettings,
	];

	/// Where the action applies.
	#[must_use]
	pub const fn context(self) -> Context {
		match self {
			Self::Delete
			| Self::Escape
			| Self::CursorUp
			| Self::CursorDown
			| Self::CursorLeft
			| Self::CursorRight
			| Self::SelectAll
			| Self::SelectInverse
			| Self::Duplicate
			| Self::MakeUnique
			| Self::TransposeOctaveUp
			| Self::TransposeOctaveDown
			| Self::Invert
			| Self::Join
			| Self::ToggleEnabled
			| Self::Normalize
			| Self::Quantize
			| Self::Reverse
			| Self::ToggleSolo => Context::Arrangement,
			Self::ToggleMpeMode => Context::MidiEditor,
			Self::ClosePluginPicker
			| Self::SelectPlugin
			| Self::HoverPrevious
			| Self::HoverNext => Context::PluginPicker,
			Self::CloseSettings => Context::Settings,
			_ => Context::Global,
		}
	}

	/// What the action is called.
	#[must_use]
	pub const fn label(self) -> &'static str {
		match self {
			Self::TogglePlayback => "Play / Pause",
			Self::Stop => "Stop",
			Self::ToggleFullscreen => "Fullscreen",
			Self::ToggleMetronome => "Metronome",
			Self::ToggleSettings => "Settings",
			Self::NewFile => "New project",
			Self::OpenFile => "Open project...",
			Self::OpenLastFile => "Open last project",
			Self::SaveFile => "Save project",
			Self::SaveAsFile => "Save project as...",
			Self::RenderFile => "Render to file...",
			Self::ClosePane => "Close pane",
			Self::NarrowedGrid => "Narrow grid",
			Self::WidenedGrid => "Widen grid",
			Self::ToggleGridTriplets => "Triplets in grid",
			Self::ToggleGrid => "Grid",
			Self::CycleForwards => "Next tab",
			Self::CycleBackwards => "Previous tab",
			Self::SelectPaneUp => "Focus pane above",
			Self::SelectPaneDown => "Focus pane below",
			Self::MovePaneUp => "Move pane up",
			Self::MovePaneDown => "Move pane down",
			Self::Undo => "Undo",
			Self::Redo => "Redo",
			Self::Delete => "Delete",
			Self::Escape => "Deselect",
			Self::CursorUp => "Move up",
			Self::CursorDown => "Move down",
			Self::CursorLeft => "Move left",
			Self::CursorRight => "Move right",
			Self::SelectAll => "Select all",
			Self::SelectInverse => "Invert selection",
			Self::Duplicate => "Duplicate",
			Self::MakeUnique => "Make unique",
			Self::TransposeOctaveUp => "Transpose up an octave",
			Self::TransposeOctaveDown => "Transpose down an octave",
			Self::Invert => "Invert",
			Self::Join => "Join",
			Self::ToggleEnabled => "Toggle enabled",
			Self::Normalize => "Normalize",
			Self::Quantize => "Quantize",
			Self::Reverse => "Reverse",
			Self::ToggleSolo => "Toggle solo",
			Self::ToggleMpeMode => "Toggle MPE mode",
			Self::ClosePluginPicker | Self::CloseSettings => "Close",
			Self::SelectPlugin => "Add plugin",
			Self::HoverPrevious => "Previous plugin",
			Self::HoverNext => "Next plugin",
		}
	}

	/// Whether the action can happen again while the key is held down.
	#[must_use]
	pub const fn repeatable(self) -> bool {
		!matches!(
			self,
			Self::Delete
				| Self::Escape
				| Self::MakeUnique
				| Self::ToggleMpeMode
				| Self::SelectAll
				| Self::SelectInverse
				| Self::ClosePluginPicker
				| Self::SelectPlugin
				| Self::CloseSettings
		)
	}
}

/// The named keys that a shortcut can use.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum NamedKey {
	/// The backspace key.
	Backspace,
	/// The delete key.
	Delete,
	/// The enter key.
	Enter,
	/// The escape key.
	Escape,
	/// The space bar.
	Space,
	/// The tab key.
	Tab,
	/// The up arrow.
	ArrowUp,
	/// The down arrow.
	ArrowDown,
	/// The left arrow.
	ArrowLeft,
	/// The right arrow.
	ArrowRight,
	/// The home key.
	Home,
	/// The end key.
	End,
	/// The page up key.
	PageUp,
	/// The page down key.
	PageDown,
	/// The first function key.
	F1,
	/// The second function key.
	F2,
	/// The third function key.
	F3,
	/// The fourth function key.
	F4,
	/// The fifth function key.
	F5,
	/// The sixth function key.
	F6,
	/// The seventh function key.
	F7,
	/// The eighth function key.
	F8,
	/// The ninth function key.
	F9,
	/// The tenth function key.
	F10,
	/// The eleventh function key.
	F11,
	/// The twelfth function key.
	F12,
}

impl NamedKey {
	/// What the key is called.
	#[must_use]
	pub const fn label(self) -> &'static str {
		match self {
			Self::Backspace => "Backspace",
			Self::Delete => "Delete",
			Self::Enter => "Enter",
			Self::Escape => "Esc",
			Self::Space => "Space",
			Self::Tab => "Tab",
			Self::ArrowUp => "↑",
			Self::ArrowDown => "↓",
			Self::ArrowLeft => "←",
			Self::ArrowRight => "→",
			Self::Home => "Home",
			Self::End => "End",
			Self::PageUp => "PgUp",
			Self::PageDown => "PgDn",
			Self::F1 => "F1",
			Self::F2 => "F2",
			Self::F3 => "F3",
			Self::F4 => "F4",
			Self::F5 => "F5",
			Self::F6 => "F6",
			Self::F7 => "F7",
			Self::F8 => "F8",
			Self::F9 => "F9",
			Self::F10 => "F10",
			Self::F11 => "F11",
			Self::F12 => "F12",
		}
	}

	/// The key of an iced key event, if it is one we know.
	fn from_named(named: keyboard::key::Named) -> Option<Self> {
		Some(match named {
			keyboard::key::Named::Backspace => Self::Backspace,
			keyboard::key::Named::Delete => Self::Delete,
			keyboard::key::Named::Enter => Self::Enter,
			keyboard::key::Named::Escape => Self::Escape,
			keyboard::key::Named::Space => Self::Space,
			keyboard::key::Named::Tab => Self::Tab,
			keyboard::key::Named::ArrowUp => Self::ArrowUp,
			keyboard::key::Named::ArrowDown => Self::ArrowDown,
			keyboard::key::Named::ArrowLeft => Self::ArrowLeft,
			keyboard::key::Named::ArrowRight => Self::ArrowRight,
			keyboard::key::Named::Home => Self::Home,
			keyboard::key::Named::End => Self::End,
			keyboard::key::Named::PageUp => Self::PageUp,
			keyboard::key::Named::PageDown => Self::PageDown,
			keyboard::key::Named::F1 => Self::F1,
			keyboard::key::Named::F2 => Self::F2,
			keyboard::key::Named::F3 => Self::F3,
			keyboard::key::Named::F4 => Self::F4,
			keyboard::key::Named::F5 => Self::F5,
			keyboard::key::Named::F6 => Self::F6,
			keyboard::key::Named::F7 => Self::F7,
			keyboard::key::Named::F8 => Self::F8,
			keyboard::key::Named::F9 => Self::F9,
			keyboard::key::Named::F10 => Self::F10,
			keyboard::key::Named::F11 => Self::F11,
			keyboard::key::Named::F12 => Self::F12,
			_ => return None,
		})
	}
}

/// The key of a shortcut.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Deserialize, Serialize)]
pub enum Key {
	/// A key that types a character.
	Character(char),
	/// A key with a name, like the escape key.
	Named(NamedKey),
}

impl fmt::Display for Key {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Character('+') => f.write_str("Plus"),
			Self::Character(c) => write!(f, "{}", c.to_ascii_uppercase()),
			Self::Named(named) => f.write_str(named.label()),
		}
	}
}

/// The keys and modifiers that trigger an action.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Chord {
	/// The key that was pressed.
	pub key: Key,
	/// Whether the shift key was held down.
	pub shift: bool,
	/// Whether the command key was held down.
	pub command: bool,
	/// Whether the alt key was held down.
	pub alt: bool,
}

impl Chord {
	/// The chord of the escape key.
	pub const ESCAPE: Self = Self::of(Key::Named(NamedKey::Escape));

	/// The chord of a key, without any modifiers.
	#[must_use]
	pub const fn of(key: Key) -> Self {
		Self {
			key,
			shift: false,
			command: false,
			alt: false,
		}
	}

	/// The chord of a key, with the shift key.
	#[must_use]
	pub const fn shifted(key: Key) -> Self {
		Self {
			key,
			shift: true,
			command: false,
			alt: false,
		}
	}

	/// The chord of a key, with the command key.
	#[must_use]
	pub const fn commanded(key: Key) -> Self {
		Self {
			key,
			shift: false,
			command: true,
			alt: false,
		}
	}

	/// The chord of a key, with the command and alt keys.
	#[must_use]
	pub const fn commanded_alt(key: Key) -> Self {
		Self {
			key,
			shift: false,
			command: true,
			alt: true,
		}
	}

	/// The chord of a key, with the command and shift keys.
	#[must_use]
	pub const fn commanded_shifted(key: Key) -> Self {
		Self {
			key,
			shift: true,
			command: true,
			alt: false,
		}
	}

	/// The chord of a key, with the alt key.
	#[must_use]
	pub const fn alt(key: Key) -> Self {
		Self {
			key,
			shift: false,
			command: false,
			alt: true,
		}
	}

	/// The chord of a key, with the alt and shift keys.
	#[must_use]
	pub const fn alt_shifted(key: Key) -> Self {
		Self {
			key,
			shift: true,
			command: false,
			alt: true,
		}
	}

	/// The chord of a keyboard event, if the key is one we can use.
	#[must_use]
	pub fn from_event(
		key: &keyboard::Key,
		physical_key: keyboard::key::Physical,
		modifiers: keyboard::Modifiers,
	) -> Option<Self> {
		let key = match key {
			keyboard::Key::Named(named) => Key::Named(NamedKey::from_named(*named)?),
			keyboard::Key::Character(_) => {
				// the character depends on the layout of the keyboard, so we ask
				// for the latin character of the key that was pressed
				Key::Character(key.to_latin(physical_key)?.to_ascii_lowercase())
			}
			keyboard::Key::Unidentified => return None,
		};

		Some(Self {
			key,
			shift: modifiers.shift(),
			command: modifiers.command(),
			alt: modifiers.alt(),
		})
	}
}

impl fmt::Display for Chord {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		if self.command {
			f.write_str(if cfg!(target_os = "macos") {
				"Cmd+"
			} else {
				"Ctrl+"
			})?;
		}

		if self.alt {
			f.write_str("Alt+")?;
		}

		if self.shift {
			f.write_str("Shift+")?;
		}

		write!(f, "{}", self.key)
	}
}

/// The error of a chord that can't be read.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParseError;

impl fmt::Display for ParseError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.write_str("the shortcut is not a key with modifiers")
	}
}

impl std::error::Error for ParseError {}

impl FromStr for Chord {
	type Err = ParseError;

	fn from_str(s: &str) -> Result<Self, Self::Err> {
		let mut chord = Self {
			key: Key::Character('\0'),
			shift: false,
			command: false,
			alt: false,
		};

		let mut key = None;

		for part in s.split('+') {
			match part.to_ascii_lowercase().as_str() {
				"ctrl" | "cmd" | "command" | "control" => chord.command = true,
				"shift" => chord.shift = true,
				"alt" | "option" => chord.alt = true,
				"plus" => key = Some(Key::Character('+')),
				_ => {
					let mut chars = part.chars();

					if let (Some(c), None) = (chars.next(), chars.next()) {
						key = Some(Key::Character(c.to_ascii_lowercase()));
					} else if let Some(named) = named_key(part) {
						key = Some(Key::Named(named));
					} else {
						return Err(ParseError);
					}
				}
			}
		}

		chord.key = key.ok_or(ParseError)?;

		Ok(chord)
	}
}

/// The named key with this name, if there is one.
fn named_key(name: &str) -> Option<NamedKey> {
	Some(match name.to_ascii_lowercase().as_str() {
		"backspace" => NamedKey::Backspace,
		"delete" | "del" => NamedKey::Delete,
		"enter" | "return" => NamedKey::Enter,
		"esc" | "escape" => NamedKey::Escape,
		"space" => NamedKey::Space,
		"tab" => NamedKey::Tab,
		"arrowup" | "up" => NamedKey::ArrowUp,
		"arrowdown" | "down" => NamedKey::ArrowDown,
		"arrowleft" | "left" => NamedKey::ArrowLeft,
		"arrowright" | "right" => NamedKey::ArrowRight,
		"home" => NamedKey::Home,
		"end" => NamedKey::End,
		"pageup" | "pgup" => NamedKey::PageUp,
		"pagedown" | "pgdn" => NamedKey::PageDown,
		"f1" => NamedKey::F1,
		"f2" => NamedKey::F2,
		"f3" => NamedKey::F3,
		"f4" => NamedKey::F4,
		"f5" => NamedKey::F5,
		"f6" => NamedKey::F6,
		"f7" => NamedKey::F7,
		"f8" => NamedKey::F8,
		"f9" => NamedKey::F9,
		"f10" => NamedKey::F10,
		"f11" => NamedKey::F11,
		"f12" => NamedKey::F12,
		_ => return None,
	})
}

/// The chords that trigger an action, when the user didn't change them.
const DEFAULTS: &[(Action, &[Chord])] = &[
	(
		Action::TogglePlayback,
		&[Chord::of(Key::Named(NamedKey::Space))],
	),
	(Action::Stop, &[Chord::shifted(Key::Named(NamedKey::Space))]),
	(
		Action::ToggleFullscreen,
		&[Chord::of(Key::Named(NamedKey::F11))],
	),
	(
		Action::ToggleMetronome,
		&[Chord::commanded(Key::Character('m'))],
	),
	(
		Action::ToggleSettings,
		&[Chord::commanded(Key::Character(','))],
	),
	(Action::NewFile, &[Chord::commanded(Key::Character('n'))]),
	(Action::OpenFile, &[Chord::commanded(Key::Character('o'))]),
	(
		Action::OpenLastFile,
		&[Chord::commanded_shifted(Key::Character('o'))],
	),
	(Action::SaveFile, &[Chord::commanded(Key::Character('s'))]),
	(
		Action::SaveAsFile,
		&[Chord::commanded_shifted(Key::Character('s'))],
	),
	(Action::RenderFile, &[Chord::commanded(Key::Character('r'))]),
	(Action::ClosePane, &[Chord::commanded(Key::Character('w'))]),
	(
		Action::NarrowedGrid,
		&[Chord::commanded(Key::Character('1'))],
	),
	(
		Action::WidenedGrid,
		&[Chord::commanded(Key::Character('2'))],
	),
	(
		Action::ToggleGridTriplets,
		&[Chord::commanded(Key::Character('3'))],
	),
	(Action::ToggleGrid, &[Chord::commanded(Key::Character('4'))]),
	(
		Action::ToggleMpeMode,
		&[Chord::commanded_shifted(Key::Character('m'))],
	),
	(
		Action::CycleForwards,
		&[Chord::of(Key::Named(NamedKey::Tab))],
	),
	(
		Action::CycleBackwards,
		&[Chord::shifted(Key::Named(NamedKey::Tab))],
	),
	(
		Action::SelectPaneUp,
		&[Chord::alt(Key::Named(NamedKey::ArrowUp))],
	),
	(
		Action::SelectPaneDown,
		&[Chord::alt(Key::Named(NamedKey::ArrowDown))],
	),
	(
		Action::MovePaneUp,
		&[Chord::alt_shifted(Key::Named(NamedKey::ArrowUp))],
	),
	(
		Action::MovePaneDown,
		&[Chord::alt_shifted(Key::Named(NamedKey::ArrowDown))],
	),
	(Action::Undo, &[Chord::commanded(Key::Character('z'))]),
	(Action::Redo, &[Chord::commanded_alt(Key::Character('z'))]),
	(
		Action::Delete,
		&[
			Chord::of(Key::Named(NamedKey::Delete)),
			Chord::of(Key::Named(NamedKey::Backspace)),
		],
	),
	(Action::Escape, &[Chord::of(Key::Named(NamedKey::Escape))]),
	(
		Action::CursorUp,
		&[Chord::of(Key::Named(NamedKey::ArrowUp))],
	),
	(
		Action::CursorDown,
		&[Chord::of(Key::Named(NamedKey::ArrowDown))],
	),
	(
		Action::CursorLeft,
		&[Chord::of(Key::Named(NamedKey::ArrowLeft))],
	),
	(
		Action::CursorRight,
		&[Chord::of(Key::Named(NamedKey::ArrowRight))],
	),
	(Action::SelectAll, &[Chord::commanded(Key::Character('a'))]),
	(
		Action::SelectInverse,
		&[Chord::commanded_shifted(Key::Character('a'))],
	),
	(Action::Duplicate, &[Chord::commanded(Key::Character('d'))]),
	(
		Action::MakeUnique,
		&[Chord::commanded_shifted(Key::Character('d'))],
	),
	(
		Action::TransposeOctaveUp,
		&[Chord::commanded(Key::Named(NamedKey::ArrowUp))],
	),
	(
		Action::TransposeOctaveDown,
		&[Chord::commanded(Key::Named(NamedKey::ArrowDown))],
	),
	(Action::Invert, &[Chord::alt(Key::Character('i'))]),
	(Action::Join, &[Chord::alt(Key::Character('j'))]),
	(Action::ToggleEnabled, &[Chord::alt(Key::Character('m'))]),
	(Action::Normalize, &[Chord::alt(Key::Character('n'))]),
	(Action::Quantize, &[Chord::alt(Key::Character('q'))]),
	(Action::Reverse, &[Chord::alt(Key::Character('r'))]),
	(Action::ToggleSolo, &[Chord::alt(Key::Character('s'))]),
	(
		Action::ClosePluginPicker,
		&[Chord::of(Key::Named(NamedKey::Escape))],
	),
	(
		Action::SelectPlugin,
		&[Chord::of(Key::Named(NamedKey::Enter))],
	),
	(
		Action::HoverPrevious,
		&[
			Chord::of(Key::Named(NamedKey::ArrowUp)),
			Chord::shifted(Key::Named(NamedKey::Tab)),
		],
	),
	(
		Action::HoverNext,
		&[
			Chord::of(Key::Named(NamedKey::ArrowDown)),
			Chord::of(Key::Named(NamedKey::Tab)),
		],
	),
	(
		Action::CloseSettings,
		&[Chord::of(Key::Named(NamedKey::Escape))],
	),
];

/// The shortcuts the user changed.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(default)]
pub struct Overrides {
	/// The actions the user changed, and the chords they set for them.
	#[serde(skip_serializing_if = "Vec::is_empty")]
	pub bindings: Vec<ActionChords>,
}

impl Overrides {
	/// Whether the user changed the shortcuts of an action.
	#[must_use]
	pub fn is_customized(&self, action: Action) -> bool {
		self.bindings.iter().any(|binding| binding.action == action)
	}

	/// Sets the chords of an action.
	pub fn set(&mut self, action: Action, chords: Vec<Chord>) {
		self.bindings.retain(|binding| binding.action != action);
		self.bindings.push(ActionChords { action, chords });
	}

	/// Puts the shortcuts of an action back to what they were by default.
	pub fn reset(&mut self, action: Action) {
		self.bindings.retain(|binding| binding.action != action);
	}
}

/// The chords of an action, as the user set them.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ActionChords {
	/// The action the chords trigger.
	pub action: Action,
	/// The chords that trigger the action.
	pub chords: Vec<Chord>,
}

/// The shortcuts that are in effect.
#[derive(Clone, Debug)]
pub struct Manager {
	/// The chords that trigger each action.
	chords: HashMap<Action, Vec<Chord>>,
	/// The action that each chord triggers, in each context.
	triggers: HashMap<(Context, Chord), Action>,
}

impl Default for Manager {
	fn default() -> Self {
		Self::new(&Overrides::default())
	}
}

impl Manager {
	/// The shortcuts that are in effect, with the changes the user made.
	#[must_use]
	pub fn new(overrides: &Overrides) -> Self {
		let mut chords = HashMap::with_capacity(Action::ALL.len());

		for action in Action::ALL {
			let defaults = DEFAULTS
				.iter()
				.find(|(default, _)| *default == action)
				.map_or(&[][..], |(_, chords)| *chords);

			chords.insert(action, defaults.to_vec());
		}

		// the shortcuts the user set take the key away from the other actions
		// in the same context, and the one that was set last wins
		for binding in &overrides.bindings {
			for chord in &binding.chords {
				for (action, chords) in &mut chords {
					if *action != binding.action && action.context() == binding.action.context() {
						chords.retain(|other| other != chord);
					}
				}
			}

			chords.insert(binding.action, binding.chords.clone());
		}

		let mut triggers = HashMap::with_capacity(chords.len());

		for action in Action::ALL {
			for chord in chords.get(&action).into_iter().flatten() {
				triggers.insert((action.context(), *chord), action);
			}
		}

		Self { chords, triggers }
	}

	/// The action that this chord triggers in this context, if there is one.
	#[must_use]
	pub fn lookup(&self, context: Context, chord: &Chord) -> Option<Action> {
		self.triggers.get(&(context, *chord)).copied()
	}

	/// The chords that trigger an action.
	#[must_use]
	pub fn chords(&self, action: Action) -> &[Chord] {
		self.chords.get(&action).map_or(&[][..], Vec::as_slice)
	}

	/// The shortcut of an action, to show in the menus.
	#[must_use]
	pub fn label(&self, action: Action) -> String {
		self.chords(action)
			.first()
			.map_or_else(String::new, ToString::to_string)
	}
}

/// A chord, as a string, so that it can be read from the config.
impl Serialize for Chord {
	fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
		serializer.serialize_str(&self.to_string())
	}
}

impl<'de> Deserialize<'de> for Chord {
	fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
		let string = String::deserialize(deserializer)?;

		string.parse().map_err(serde::de::Error::custom)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn the_defaults_are_not_in_conflict() {
		let manager = Manager::new(&Overrides::default());

		for action in Action::ALL {
			let chords = manager.chords(action);

			for chord in chords {
				let context = action.context();

				assert_eq!(
					manager.lookup(context, chord),
					Some(action),
					"{action:?} and something else use {chord} in {context:?}",
				);
			}
		}
	}

	#[test]
	fn a_chord_finds_its_action() {
		let manager = Manager::new(&Overrides::default());

		assert_eq!(
			manager.lookup(Context::Global, &Chord::of(Key::Named(NamedKey::Space))),
			Some(Action::TogglePlayback),
		);

		assert_eq!(
			manager.lookup(Context::Global, &Chord::commanded(Key::Character('s'))),
			Some(Action::SaveFile),
		);

		assert_eq!(
			manager.lookup(Context::Arrangement, &Chord::commanded(Key::Character('a')),),
			Some(Action::SelectAll),
		);

		assert_eq!(
			manager.lookup(Context::Global, &Chord::of(Key::Character('s'))),
			None,
		);
	}

	#[test]
	fn the_user_can_change_a_shortcut() {
		let mut overrides = Overrides::default();

		overrides.set(
			Action::SaveFile,
			vec![Chord::commanded(Key::Character('k'))],
		);

		let manager = Manager::new(&overrides);

		assert_eq!(
			manager.lookup(Context::Global, &Chord::commanded(Key::Character('k'))),
			Some(Action::SaveFile),
		);

		// the shortcut the user took away doesn't do anything anymore
		assert_eq!(
			manager.lookup(Context::Global, &Chord::commanded(Key::Character('s'))),
			None,
		);
	}

	#[test]
	fn the_user_can_take_a_shortcut_away() {
		let mut overrides = Overrides::default();

		overrides.set(Action::SaveFile, vec![]);

		let manager = Manager::new(&overrides);

		assert!(manager.chords(Action::SaveFile).is_empty());

		assert_eq!(
			manager.lookup(Context::Global, &Chord::commanded(Key::Character('s'))),
			None,
		);
	}

	#[test]
	fn the_user_can_put_a_shortcut_back() {
		let mut overrides = Overrides::default();

		overrides.set(
			Action::SaveFile,
			vec![Chord::commanded(Key::Character('k'))],
		);
		overrides.reset(Action::SaveFile);

		let manager = Manager::new(&overrides);

		assert_eq!(
			manager.lookup(Context::Global, &Chord::commanded(Key::Character('s'))),
			Some(Action::SaveFile),
		);
	}

	#[test]
	fn the_last_shortcut_the_user_set_wins() {
		let mut overrides = Overrides::default();

		let chord = Chord::commanded(Key::Character('k'));

		overrides.set(Action::SaveFile, vec![chord]);
		overrides.set(Action::NewFile, vec![chord]);

		let manager = Manager::new(&overrides);

		assert_eq!(
			manager.lookup(Context::Global, &chord),
			Some(Action::NewFile),
		);

		assert!(manager.chords(Action::SaveFile).is_empty());
	}

	#[test]
	fn the_same_shortcut_can_be_used_in_another_context() {
		let mut overrides = Overrides::default();

		let chord = Chord::commanded(Key::Character('k'));

		overrides.set(Action::SaveFile, vec![chord]);
		overrides.set(Action::SelectAll, vec![chord]);

		let manager = Manager::new(&overrides);

		assert_eq!(
			manager.lookup(Context::Global, &chord),
			Some(Action::SaveFile),
		);

		assert_eq!(
			manager.lookup(Context::Arrangement, &chord),
			Some(Action::SelectAll),
		);
	}

	#[test]
	fn a_shortcut_can_be_read_from_a_string() {
		let chord = Chord::commanded_shifted(Key::Character('s'));

		assert_eq!(chord.to_string(), "Ctrl+Shift+S");
		assert_eq!("Ctrl+Shift+S".parse::<Chord>(), Ok(chord));

		let chord = Chord::of(Key::Named(NamedKey::Escape));

		assert_eq!(chord.to_string(), "Esc");
		assert_eq!("Esc".parse::<Chord>(), Ok(chord));

		let chord = Chord::of(Key::Character('+'));

		assert_eq!(chord.to_string(), "Plus");
		assert_eq!("Plus".parse::<Chord>(), Ok(chord));

		let chord = Chord::alt(Key::Character(','));

		assert_eq!(chord.to_string(), "Alt+,");
		assert_eq!("Alt+,".parse::<Chord>(), Ok(chord));
	}

	#[test]
	fn the_overrides_can_be_written_and_read() {
		let mut overrides = Overrides::default();

		overrides.set(
			Action::SaveFile,
			vec![Chord::commanded_shifted(Key::Character('s'))],
		);
		overrides.set(Action::Delete, vec![]);

		let text = toml::to_string(&overrides).unwrap();
		let read = toml::from_str::<Overrides>(&text).unwrap();

		assert_eq!(read, overrides);
	}
}
