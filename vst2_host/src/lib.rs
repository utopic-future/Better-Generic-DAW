//! Hosts VST2 plugins.
//!
//! VST2 is an old format, and the plugins that use it are all sorts of shapes. The
//! host does the smallest thing that works: it loads the plugin, asks it what it can
//! do, and lets it process audio and show its editor.
//!
//! Unlike VST3, a VST2 plugin is one object that does everything, so the main thread
//! and the audio thread share it. The main thread asks the plugin about itself, and
//! the audio thread has it process audio.

use crate::{api::AEffect, host::HostState};
use util::{c_array_to_string, non_empty};
use std::{
	ffi::c_void,
	path::{Path, PathBuf},
	sync::{Arc, LazyLock, mpsc},
};
use walkdir::WalkDir;

mod audio_buffers;
mod audio_thread;
mod editor;
mod error;
mod events;
mod module;
mod param;
mod platform;
mod plugin;
mod plugin_descriptor;
mod state;
mod transport;
mod util;

pub mod api;
pub mod host;
pub mod scan;

pub use audio_thread::{AudioConfig, AudioThread};
pub use editor::{EditorSize, ParentWindow};
pub use error::Error;
pub use events::Event;
pub use host::{HOST_NAME, MainThreadMessage};
pub use module::Module;
pub use param::{ParamFlags, ParamInfo};
pub use plugin::Plugin;
pub use plugin_descriptor::PluginDescriptor;
pub use transport::Transport;

/// The directories VST2 plugins are usually in.
pub static DEFAULT_VST2_PATHS: LazyLock<Box<[Arc<Path>]>> = LazyLock::new(|| {
	let mut paths: Vec<PathBuf> = Vec::new();

	if cfg!(windows) {
		if let Some(path) = std::env::var_os("COMMONPROGRAMFILES").map(PathBuf::from) {
			paths.push(path.join("VST2"));
			paths.push(path.join("Steinberg\\VstPlugins"));
		}

		if let Some(path) = std::env::var_os("PROGRAMFILES").map(PathBuf::from) {
			paths.push(path.join("VSTPlugins"));
		}

		if let Some(path) = std::env::var_os("PROGRAMFILES(X86)").map(PathBuf::from) {
			paths.push(path.join("VSTPlugins"));
		}
	} else if cfg!(target_os = "macos") {
		paths.push(PathBuf::from("/Library/Audio/Plug-Ins/VST"));

		if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
			paths.push(path.join("Library/Audio/Plug-Ins/VST"));
		}
	} else if cfg!(unix) {
		if let Some(path) = std::env::var_os("HOME").map(PathBuf::from) {
			paths.push(path.join(".vst2"));
		}

		paths.push(PathBuf::from("/usr/lib/vst2"));
		paths.push(PathBuf::from("/usr/local/lib/vst2"));
	}

	if let Some(vst2_path) = std::env::var_os("VST2_PATH") {
		paths.extend(std::env::split_paths(&vst2_path));
	}

	paths.into_iter().map(Arc::from).collect()
});

/// Finds all the VST2 plugins below the given directories.
///
/// The files are only candidates: whether they're plugins at all is found out by
/// loading them.
pub fn find_plugin_paths(
	paths: impl IntoIterator<Item: AsRef<Path>>,
) -> impl Iterator<Item = Arc<Path>> {
	paths.into_iter().flat_map(|root| {
		let mut walker = WalkDir::new(root).follow_links(true).into_iter();
		let mut found = Vec::new();

		while let Some(entry) = walker.next() {
			let Ok(entry) = entry else { continue };

			let path = entry.path();

			if path.is_dir() {
				// a `.vst` bundle on macOS holds a plugin
				if path.extension().is_some_and(|ext| ext == "vst") {
					found.push(Arc::from(path));
					walker.skip_current_dir();
				}

				continue;
			}

			if path
				.extension()
				.is_some_and(|ext| ext == "dll" || ext == "so" || ext == "vst2")
			{
				found.push(Arc::from(path));
			}
		}

		found
	})
}

/// Loads the module at `path` and lists the plugins in it.
#[must_use]
pub fn descriptors(path: &Arc<Path>) -> Option<Vec<PluginDescriptor>> {
	find_descriptors(path)
		.inspect_err(|err| log::warn!("{err}"))
		.ok()
}

/// Loads the module at `path` and lists the plugins in it.
fn find_descriptors(path: &Arc<Path>) -> Result<Vec<PluginDescriptor>, Error> {
	let module = Module::load(path)?;

	let mut descriptors = Vec::new();

	// The state is made before the plugin, so that the plugin can talk to the host
	// while it's being made.
	let (sender, _receiver) = mpsc::channel();
	let mut state = Box::new(HostState::new(44100.0, 512, false, sender));

	// the first instance of the plugin says what the module holds
	let effect = module
		.create_effect(0, &raw mut *state)
		.ok_or_else(|| Error::NoEffect(path.clone()))?;

	let category = dispatch(effect, api::opcodes::GET_PLUG_CATEGORY, 0, 0, null(), 0.0) as i32;

	if category == api::categories::SHELL {
		// the module holds more than one plugin, and says what they're called
		let mut shells = Vec::new();

		loop {
			let mut name = [0 as std::os::raw::c_char; 256];

			let unique_id = dispatch(
				effect,
				api::opcodes::SHELL_GET_NEXT_PLUGIN,
				0,
				0,
				name.as_mut_ptr().cast(),
				0.0,
			) as i32;

			if unique_id == 0 {
				break;
			}

			shells.push((unique_id, c_array_to_string(&name)));
		}

		dispatch(effect, api::opcodes::CLOSE, 0, 0, null(), 0.0);

		for (unique_id, name) in shells {
			let Some(effect) = module.create_effect(unique_id, &raw mut *state) else {
				continue;
			};

			let mut descriptor = describe(effect, path);

			if descriptor.name.is_empty() {
				descriptor.name = name.into();
			}

			dispatch(effect, api::opcodes::CLOSE, 0, 0, null(), 0.0);

			descriptors.push(descriptor);
		}
	} else {
		descriptors.push(describe(effect, path));
		dispatch(effect, api::opcodes::CLOSE, 0, 0, null(), 0.0);
	}

	Ok(descriptors)
}

/// Asks a plugin what it is.
fn describe(effect: *mut AEffect, path: &Arc<Path>) -> PluginDescriptor {
	let mut name = [0 as std::os::raw::c_char; api::MAX_NAME_LEN];
	dispatch(
		effect,
		api::opcodes::GET_PRODUCT_STRING,
		0,
		0,
		name.as_mut_ptr().cast(),
		0.0,
	);
	let mut name = c_array_to_string(&name);

	if name.is_empty() {
		let mut effect_name = [0 as std::os::raw::c_char; api::MAX_NAME_LEN];
		dispatch(
			effect,
			api::opcodes::GET_EFFECT_NAME,
			0,
			0,
			effect_name.as_mut_ptr().cast(),
			0.0,
		);
		name = c_array_to_string(&effect_name);
	}

	let mut vendor = [0 as std::os::raw::c_char; api::MAX_NAME_LEN];
	dispatch(
		effect,
		api::opcodes::GET_VENDOR_STRING,
		0,
		0,
		vendor.as_mut_ptr().cast(),
		0.0,
	);
	let vendor = c_array_to_string(&vendor);

	let version = dispatch(effect, api::opcodes::GET_VENDOR_VERSION, 0, 0, null(), 0.0);
	let version = (version > 0).then(|| Arc::<str>::from(format_version(version as u32)));

	let category = dispatch(effect, api::opcodes::GET_PLUG_CATEGORY, 0, 0, null(), 0.0) as i32;

	// SAFETY: the effect is a valid plugin.
	let unique_id = unsafe { (*effect).unique_id };

	PluginDescriptor {
		unique_id,
		name: name.into(),
		vendor: non_empty(vendor),
		version,
		category,
		path: path.clone(),
	}
}

/// The version of a plugin, as `1.2.3.4`.
fn format_version(version: u32) -> String {
	let major = version / 1000;
	let minor = (version / 100) % 10;
	let patch = (version / 10) % 10;

	format!("{major}.{minor}.{patch}")
}

/// Calls the plugin.
fn dispatch(
	effect: *mut AEffect,
	opcode: i32,
	index: i32,
	value: isize,
	ptr: *mut c_void,
	opt: f32,
) -> isize {
	// SAFETY: the effect is a valid plugin.
	let dispatcher = unsafe { (*effect).dispatcher };

	// SAFETY: the effect is a valid plugin.
	unsafe { dispatcher(effect, opcode, index, value, ptr, opt) }
}

/// A pointer to nothing, for the calls that don't need one.
fn null() -> *mut c_void {
	std::ptr::null_mut()
}

/// Loads a plugin, and creates an instance of it.
pub fn load(descriptor: &PluginDescriptor) -> Result<(Plugin, mpsc::Receiver<MainThreadMessage>), Error> {
	Plugin::new(descriptor)
}
