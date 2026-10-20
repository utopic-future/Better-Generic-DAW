use std::{
	path::{Path, PathBuf},
	sync::{Arc, LazyLock},
};
use walkdir::WalkDir;

mod audio_buffers;
mod audio_thread;
mod bstream;
mod buses;
mod component_handler;
mod editor;
mod error;
mod event_list;
mod events;
mod host;
mod main_thread;
mod message;
mod module;
mod param;
mod param_changes;
mod plugin;
mod plugin_descriptor;
mod state;
mod transport;
mod util;

pub use audio_thread::{AudioConfig, AudioThread, INFINITE_TAIL, ProcessStatus};
pub use bstream::{MAX_STREAM_BYTES, MemoryStream};
pub use buses::AudioLayout;
pub use component_handler::ComponentHandler;
pub use editor::{EditorSize, ParentWindow, PlatformType, PlugFrame};
pub use error::Error;
pub use events::Event;
pub use host::{HOST_NAME, HostApplication, live_host_objects};
pub use main_thread::{MainThreadMessage, RestartActions, RestartFlags};
pub use message::{HostAttributeList, HostMessage};
pub use module::Module;
pub use param::{ParamFlags, ParamInfo};
pub use plugin::Plugin;
pub use plugin_descriptor::PluginDescriptor;
pub use transport::Transport;

pub static DEFAULT_VST3_PATHS: LazyLock<Box<[Arc<Path>]>> = LazyLock::new(|| {
	let mut paths = Vec::new();

	if cfg!(windows) {
		if let Some(path) = std::env::var_os("COMMONPROGRAMFILES").map(PathBuf::from) {
			paths.push(path.join("VST3").into());
		}

		if let Some(path) = std::env::var_os("LOCALAPPDATA").map(PathBuf::from) {
			paths.push(path.join("Programs\\Common\\VST3").into());
		}
	} else if cfg!(target_os = "macos") {
		paths.push(Path::new("/Library/Audio/Plug-Ins/VST3").into());

		if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
			paths.push(path.join("Library/Audio/Plug-Ins/VST3").into());
		}
	} else if cfg!(unix) {
		if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
			paths.push(path.join(".vst3").into());
		}

		paths.push(Path::new("/usr/lib/vst3").into());
		paths.push(Path::new("/usr/local/lib/vst3").into());
	}

	if let Some(vst3_path) = std::env::var_os("VST3_PATH") {
		paths.extend(std::env::split_paths(&vst3_path).map(Arc::from));
	}

	paths.into_boxed_slice()
});

/// Finds all `.vst3` plugins (bundles, or single files on Windows) below the given directories.
///
/// A found plugin is never descended into, so nested binaries don't show up as extra plugins.
pub fn find_plugin_paths(
	paths: impl IntoIterator<Item: AsRef<Path>>,
) -> impl Iterator<Item = Arc<Path>> {
	paths.into_iter().flat_map(|root| {
		let mut walker = WalkDir::new(root).follow_links(true).into_iter();
		let mut found = Vec::new();

		while let Some(entry) = walker.next() {
			let Ok(entry) = entry else { continue };

			if entry.path().extension().is_some_and(|ext| ext == "vst3") {
				if entry.file_type().is_dir() {
					walker.skip_current_dir();
				}
				found.push(Arc::from(entry.into_path()));
			}
		}

		found
	})
}

/// Loads the module at `path` and lists the audio plugins in it.
#[must_use]
pub fn descriptors(path: &Arc<Path>) -> Option<Vec<PluginDescriptor>> {
	Module::load(path)
		.inspect_err(|err| log::warn!("{err}"))
		.ok()
		.map(|module| module.descriptors(path))
}
