use std::{
	fmt::{Display, Formatter},
	path::Path,
	sync::Arc,
};

#[derive(Debug)]
pub enum Error {
	/// The platform's dynamic loader refused the library.
	Load(Arc<Path>, libloading::Error),
	/// The library doesn't have the entry point a VST2 plugin has.
	NoEntryPoint(Arc<Path>),
	/// The plugin didn't give the host its main struct.
	NoEffect(Arc<Path>),
	/// The plugin's main struct isn't a VST2 plugin.
	NotAPlugin(Arc<Path>),
	/// The plugin is already active, deactivate it first.
	AlreadyActive,
	/// The plugin has no editor window.
	NoEditor,
	/// The plugin's editor can't be shown in this kind of window.
	UnsupportedEditorPlatform,
	/// The editor is already in a window.
	EditorAttached,
	/// The plugin refused to open its editor.
	EditorFailed,
	/// The saved state isn't something this crate wrote, or it's damaged.
	InvalidState(&'static str),
	/// The plugin refused to save or load its state.
	StateFailed(&'static str),
}

impl Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Load(path, err) => write!(f, "{}: failed to load: {err}", path.display()),
			Self::NoEntryPoint(path) => {
				write!(f, "{}: no VSTPluginMain export", path.display())
			}
			Self::NoEffect(path) => write!(f, "{}: the plugin gave no main struct", path.display()),
			Self::NotAPlugin(path) => write!(f, "{}: not a VST2 plugin", path.display()),
			Self::AlreadyActive => write!(f, "the plugin is already active"),
			Self::NoEditor => write!(f, "the plugin has no editor"),
			Self::UnsupportedEditorPlatform => {
				write!(f, "the plugin's editor doesn't support this kind of window")
			}
			Self::EditorAttached => write!(f, "the editor is already in a window"),
			Self::EditorFailed => write!(f, "the plugin refused to open its editor"),
			Self::InvalidState(why) => write!(f, "invalid plugin state: {why}"),
			Self::StateFailed(what) => write!(f, "the plugin failed to {what} its state"),
		}
	}
}

impl std::error::Error for Error {}
