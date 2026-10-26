use crate::{
	api::{self, AEffect, HostCallback},
	error::Error,
	host::{HOST_CALLBACK, HostState, clear_creating, set_creating, set_current_id},
};
use libloading::Library;
use std::{
	fmt::{Debug, Formatter},
	path::{Path, PathBuf},
	sync::{Arc, Mutex, MutexGuard},
};

/// The function a VST2 plugin exports to give the host its main struct.
type MainCall = unsafe extern "C" fn(HostCallback) -> *mut AEffect;

/// A loaded VST2 plugin: the dynamic library, and the function that starts a plugin.
///
/// Dropping a `Module` unloads the library, so everything that was made from it must
/// be gone by then.
pub struct Module {
	main: MainCall,
	path: Arc<Path>,
	/// Kept alive so the plugin's code stays loaded; dropped after `Drop::drop` has run.
	_library: Library,
}

impl Debug for Module {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("Module")
			.field("path", &self.path)
			.finish_non_exhaustive()
	}
}

impl Module {
	/// Loads the plugin at `path`.
	pub fn load(path: &Arc<Path>) -> Result<Self, Error> {
		// Some plugins look for files of their own when the library is loaded, and they
		// look for them in the current directory, so it has to be the directory of the
		// plugin.
		let _guard = WorkingDirectory::set(path.parent());

		let (library, main) = platform::open(path)?;

		Ok(Self {
			main,
			path: path.clone(),
			_library: library,
		})
	}

	/// The path of the plugin.
	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}

	/// Starts a plugin, and returns its main struct.
	///
	/// `unique_id` tells shell plugins which of their plugins to make. It's zero for
	/// plugins that hold only one.
	///
	/// `state` is the host's state, which the plugin can already talk to while it's
	/// being made.
	#[must_use]
	pub fn create_effect(&self, unique_id: i32, state: *mut HostState) -> Option<*mut AEffect> {
		set_current_id(unique_id);
		set_creating(state);

		// Some plugins load files of their own when they're made, and they look for them
		// in the current directory, so it has to be the directory of the plugin.
		let _guard = WorkingDirectory::set(self.path.parent());

		// SAFETY:
		// `main` was resolved from the library, which is still loaded, and it takes the
		// function the plugin uses to talk to the host.
		let effect = unsafe { (self.main)(HOST_CALLBACK) };

		set_current_id(0);
		clear_creating();

		if effect.is_null() {
			return None;
		}

		// SAFETY: the plugin gave us its main struct, which is valid until it's closed.
		if unsafe { (*effect).magic } != api::MAGIC {
			return None;
		}

		Some(effect)
	}
}

/// The current directory, while a plugin is made.
///
/// The directory is the same for the whole process, so only one plugin can be made at a
/// time.
pub struct WorkingDirectory {
	previous: Option<PathBuf>,
	/// Keeps the lock, so that only one plugin is made at a time.
	_lock: MutexGuard<'static, ()>,
}

impl WorkingDirectory {
	/// Makes `directory` the current directory, until the guard is dropped.
	pub fn set(directory: Option<&Path>) -> Self {
		static LOCK: Mutex<()> = Mutex::new(());

		let lock = LOCK
			.lock()
			.unwrap_or_else(std::sync::PoisonError::into_inner);
		let previous = std::env::current_dir().ok();

		if let Some(directory) = directory {
			let _ = std::env::set_current_dir(directory);
		}

		Self {
			previous,
			_lock: lock,
		}
	}
}

impl Drop for WorkingDirectory {
	fn drop(&mut self) {
		if let Some(previous) = &self.previous {
			let _ = std::env::set_current_dir(previous);
		}
	}
}

#[cfg(windows)]
mod platform {
	use super::{Error, Library, MainCall};
	use std::{
		path::{Path, PathBuf},
		sync::Arc,
	};

	pub fn open(path: &Arc<Path>) -> Result<(Library, MainCall), Error> {
		// Plugins often have DLLs of their own next to them, so the directory of the
		// plugin has to be searched as well. That needs an absolute path.
		let path = std::path::absolute(path).unwrap_or_else(|_| PathBuf::from(path.as_ref()));

		// SAFETY: loading an external library object file is inherently unsafe.
		let library = unsafe {
			libloading::os::windows::Library::load_with_flags(
				&path,
				libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
			)
		}
		.map(Library::from)
		.map_err(|err| Error::Load(Arc::from(path.as_path()), err))?;

		// SAFETY: the library is loaded, and this is the entry point of a VST2 plugin.
		let main = unsafe { library.get::<MainCall>(b"VSTPluginMain\0") }
			.map(|main| *main)
			.or_else(|_| {
				// SAFETY: the library is loaded, and this is the entry point of a VST2
				// plugin.
				unsafe { library.get::<MainCall>(b"main\0") }.map(|main| *main)
			})
			.map_err(|_| Error::NoEntryPoint(Arc::from(path.as_path())))?;

		Ok((library, main))
	}
}

#[cfg(target_os = "macos")]
mod platform {
	use super::{Error, Library, MainCall};
	use std::{
		path::{Path, PathBuf},
		sync::Arc,
	};

	/// The binary of a `.vst` bundle.
	fn binary(bundle: &Path) -> Option<PathBuf> {
		let dir = bundle.join("Contents").join("MacOS");
		let stem = bundle.file_stem()?;

		let preferred = dir.join(stem);
		if preferred.is_file() {
			return Some(preferred);
		}

		std::fs::read_dir(dir)
			.ok()
			.into_iter()
			.flatten()
			.flatten()
			.map(|entry| entry.path())
			.find(|path| path.extension().is_some_and(|ext| ext == "so"))
	}

	pub fn open(path: &Arc<Path>) -> Result<(Library, MainCall), Error> {
		let binary = if path.is_dir() {
			binary(path).ok_or_else(|| Error::NoEntryPoint(path.clone()))?
		} else {
			path.to_owned()
		};

		// SAFETY: loading an external library object file is inherently unsafe.
		let library =
			unsafe { Library::new(&binary) }.map_err(|err| Error::Load(path.clone(), err))?;

		// SAFETY: the library is loaded, and this is the entry point of a VST2 plugin.
		let main = unsafe { library.get::<MainCall>(b"main_macho\0") }
			.map(|main| *main)
			.or_else(|_| {
				// SAFETY: the library is loaded, and this is the entry point of a VST2
				// plugin.
				unsafe { library.get::<MainCall>(b"VSTPluginMain\0") }.map(|main| *main)
			})
			.map_err(|_| Error::NoEntryPoint(path.clone()))?;

		Ok((library, main))
	}
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
	use super::{Error, Library, MainCall};
	use std::{path::Path, sync::Arc};

	pub fn open(path: &Arc<Path>) -> Result<(Library, MainCall), Error> {
		// SAFETY: loading an external library object file is inherently unsafe.
		let library =
			unsafe { Library::new(path) }.map_err(|err| Error::Load(path.clone(), err))?;

		// SAFETY: the library is loaded, and this is the entry point of a VST2 plugin.
		let main = unsafe { library.get::<MainCall>(b"VSTPluginMain\0") }
			.map(|main| *main)
			.map_err(|_| Error::NoEntryPoint(path.clone()))?;

		Ok((library, main))
	}
}
