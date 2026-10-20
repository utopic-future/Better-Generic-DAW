use std::{
	fmt::{Display, Formatter},
	path::PathBuf,
};

#[derive(Debug)]
pub enum Error {
	/// The platform's dynamic loader refused the library.
	Load(PathBuf, libloading::Error),
	/// The bundle didn't contain a loadable binary for this platform.
	NoBinary(PathBuf),
	/// The library doesn't export `GetPluginFactory`.
	NoFactory(PathBuf, libloading::Error),
	/// The platform's module entry point returned `false`.
	EntryFailed(PathBuf),
	/// `GetPluginFactory` returned a null pointer.
	NullFactory(PathBuf),
	/// Loading VST3 modules isn't implemented for this platform yet.
	Unsupported(&'static str),
	/// The factory refused to create an object (`createInstance` failed).
	CreateFailed(&'static str),
	/// The plugin doesn't implement an interface every plugin must implement.
	MissingInterface(&'static str),
	/// `initialize` on the component or controller failed.
	InitializeFailed(&'static str, i32),
	/// A call that sets up audio processing failed.
	SetupFailed(&'static str, i32),
	/// The plugin's main audio buses can't be run as mono or stereo.
	UnsupportedBusLayout,
	/// The plugin is already active, deactivate it first.
	AlreadyActive,
	/// The plugin has no editor window.
	NoEditor,
	/// The plugin's editor can't be shown in this kind of window.
	UnsupportedEditorPlatform,
	/// The editor is already shown in a window.
	EditorAttached,
	/// The saved state isn't something this crate wrote, or it's damaged.
	InvalidState(&'static str),
	/// The plugin refused to save or load its state.
	StateFailed(&'static str, i32),
}

impl Display for Error {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		match self {
			Self::Load(path, err) => write!(f, "{}: failed to load: {err}", path.display()),
			Self::NoBinary(path) => write!(f, "{}: no binary found in bundle", path.display()),
			Self::NoFactory(path, err) => {
				write!(f, "{}: no GetPluginFactory export: {err}", path.display())
			}
			Self::EntryFailed(path) => write!(f, "{}: module entry returned false", path.display()),
			Self::NullFactory(path) => {
				write!(f, "{}: GetPluginFactory returned null", path.display())
			}
			Self::Unsupported(what) => write!(f, "unsupported: {what}"),
			Self::CreateFailed(what) => write!(f, "failed to create {what}"),
			Self::MissingInterface(what) => write!(f, "plugin doesn't implement {what}"),
			Self::InitializeFailed(what, code) => {
				write!(f, "failed to initialize {what} (tresult {code})")
			}
			Self::SetupFailed(what, code) | Self::StateFailed(what, code) => {
				write!(f, "{what} failed (tresult {code})")
			}
			Self::UnsupportedBusLayout => {
				write!(
					f,
					"the plugin's main audio buses are neither mono nor stereo"
				)
			}
			Self::AlreadyActive => write!(f, "the plugin is already active"),
			Self::NoEditor => write!(f, "the plugin has no editor"),
			Self::UnsupportedEditorPlatform => {
				write!(f, "the plugin's editor doesn't support this kind of window")
			}
			Self::EditorAttached => write!(f, "the editor is already in a window"),
			Self::InvalidState(why) => write!(f, "invalid plugin state: {why}"),
		}
	}
}

impl std::error::Error for Error {}
