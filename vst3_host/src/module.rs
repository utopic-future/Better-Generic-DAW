use crate::{
	Error, PluginDescriptor,
	plugin_descriptor::AUDIO_MODULE_CLASS,
	util::{c_array_to_string, utf16_array_to_string},
};
use libloading::Library;
use log::warn;
use std::{
	fmt::{Debug, Formatter},
	mem::zeroed,
	path::{Path, PathBuf},
	sync::Arc,
};
use vst3::{
	ComPtr,
	Steinberg::{
		IPluginFactory, IPluginFactory2, IPluginFactory2Trait as _, IPluginFactory3,
		IPluginFactory3Trait as _, IPluginFactoryTrait as _, PClassInfo, PClassInfo2, PClassInfoW,
		PFactoryInfo, kResultOk,
	},
};

type GetPluginFactory = unsafe extern "system" fn() -> *mut IPluginFactory;
type ModuleExit = unsafe extern "system" fn() -> bool;

/// A loaded `.vst3` bundle: the dynamic library plus its plugin factory.
///
/// Dropping a `Module` releases the factory, runs the platform's exit function and only then
/// unloads the library. Everything created from the factory must be dropped before the `Module`.
pub struct Module {
	factory: Option<ComPtr<IPluginFactory>>,
	exit: Option<ModuleExit>,
	path: PathBuf,
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

impl Drop for Module {
	fn drop(&mut self) {
		// The factory must be released before the module exits, and the module must exit before
		// the library is unloaded (which happens when `self._library` is dropped after this).
		drop(self.factory.take());

		if let Some(exit) = self.exit.take() {
			// SAFETY:
			// `exit` was resolved from `self.library`, which is still loaded.
			unsafe { exit(); }
		}
	}
}

impl Module {
	pub fn load(bundle: &Path) -> Result<Self, Error> {
		let binary = platform::binary_path(bundle)?;
		let (library, exit) = platform::open(bundle, &binary)?;

		// SAFETY:
		// The VST3 spec defines `GetPluginFactory` as `IPluginFactory* (*)()`.
		let get_factory = unsafe { library.get::<GetPluginFactory>(b"GetPluginFactory\0") }
			.map_err(|err| Error::NoFactory(bundle.to_owned(), err))?;
		let get_factory = *get_factory;

		// SAFETY:
		// `get_factory` was resolved from `library`, which is still loaded.
		let ptr = unsafe { get_factory() };

		// SAFETY:
		// `GetPluginFactory` returns an owned, valid `IPluginFactory` pointer (or null).
		let factory = unsafe { ComPtr::from_raw(ptr) }
			.ok_or_else(|| Error::NullFactory(bundle.to_owned()))?;

		Ok(Self {
			factory: Some(factory),
			exit,
			path: bundle.to_owned(),
			_library: library,
		})
	}

	#[must_use]
	pub fn path(&self) -> &Path {
		&self.path
	}

	#[must_use]
	pub(crate) fn factory(&self) -> &ComPtr<IPluginFactory> {
		self.factory.as_ref().unwrap()
	}

	/// All audio plugins ("Audio Module Class" classes) in this module.
	#[must_use]
	pub fn descriptors(&self, path: &Arc<Path>) -> Vec<PluginDescriptor> {
		let factory = self.factory();

		let mut factory_info = PFactoryInfo {
			vendor: [0; 64],
			url: [0; 256],
			email: [0; 128],
			flags: 0,
		};
		// SAFETY:
		// `factory` is a valid factory and `factory_info` is a valid out pointer.
		let factory_vendor = if unsafe { factory.getFactoryInfo(&raw mut factory_info) }
			== kResultOk
		{
			c_array_to_string(&factory_info.vendor)
		} else {
			String::new()
		};

		let factory2 = factory.cast::<IPluginFactory2>();
		let factory3 = factory.cast::<IPluginFactory3>();

		// SAFETY:
		// `factory` is a valid factory.
		let count = unsafe { factory.countClasses() };

		(0..count)
			.filter_map(|index| {
				if let Some(factory3) = &factory3 {
					// SAFETY:
					// `PClassInfoW` is plain old data, all zeroes is a valid value.
					let mut info = unsafe { zeroed::<PClassInfoW>() };

					// SAFETY:
					// `factory3` is valid and `info` is a valid out pointer.
					if unsafe { factory3.getClassInfoUnicode(index, &raw mut info) } == kResultOk {
						return (c_array_to_string(&info.category) == AUDIO_MODULE_CLASS).then(
							|| {
								PluginDescriptor::new(
									tuid_to_bytes(&info.cid),
									utf16_array_to_string(&info.name),
									utf16_array_to_string(&info.vendor),
									utf16_array_to_string(&info.version),
									&c_array_to_string(&info.subCategories),
									path,
								)
							},
						);
					}
				}

				if let Some(factory2) = &factory2 {
					// SAFETY:
					// `PClassInfo2` is plain old data, all zeroes is a valid value.
					let mut info = unsafe { zeroed::<PClassInfo2>() };

					// SAFETY:
					// `factory2` is valid and `info` is a valid out pointer.
					if unsafe { factory2.getClassInfo2(index, &raw mut info) } == kResultOk {
						return (c_array_to_string(&info.category) == AUDIO_MODULE_CLASS).then(
							|| {
								PluginDescriptor::new(
									tuid_to_bytes(&info.cid),
									c_array_to_string(&info.name),
									c_array_to_string(&info.vendor),
									c_array_to_string(&info.version),
									&c_array_to_string(&info.subCategories),
									path,
								)
							},
						);
					}
				}

				// SAFETY:
				// `PClassInfo` is plain old data, all zeroes is a valid value.
				let mut info = unsafe { zeroed::<PClassInfo>() };

				// SAFETY:
				// `factory` is valid and `info` is a valid out pointer.
				if unsafe { factory.getClassInfo(index, &raw mut info) } != kResultOk {
					warn!("{}: couldn't read class {index}", self.path.display());
					return None;
				}

				(c_array_to_string(&info.category) == AUDIO_MODULE_CLASS).then(|| {
					PluginDescriptor::new(
						tuid_to_bytes(&info.cid),
						c_array_to_string(&info.name),
						factory_vendor.clone(),
						String::new(),
						"",
						path,
					)
				})
			})
			.collect()
	}
}

pub fn tuid_to_bytes(tuid: &vst3::Steinberg::TUID) -> [u8; 16] {
	tuid.map(|b| b as u8)
}

#[cfg(all(unix, not(target_os = "macos")))]
mod platform {
	use super::{Error, Library, ModuleExit};
	use std::{
		ffi::c_void,
		path::{Path, PathBuf},
	};

	const ARCH: &str = if cfg!(target_arch = "x86_64") {
		"x86_64-linux"
	} else if cfg!(target_arch = "aarch64") {
		"aarch64-linux"
	} else if cfg!(target_arch = "x86") {
		"i386-linux"
	} else if cfg!(target_arch = "arm") {
		"armv7l-linux"
	} else {
		"unknown-linux"
	};

	pub fn binary_path(bundle: &Path) -> Result<PathBuf, Error> {
		let dir = bundle.join("Contents").join(ARCH);
		let stem = bundle.file_stem().unwrap_or_default();

		let preferred = dir.join(stem).with_extension("so");
		if preferred.is_file() {
			return Ok(preferred);
		}

		std::fs::read_dir(&dir)
			.ok()
			.into_iter()
			.flatten()
			.flatten()
			.map(|entry| entry.path())
			.find(|path| path.extension().is_some_and(|ext| ext == "so"))
			.ok_or_else(|| Error::NoBinary(bundle.to_owned()))
	}

	pub fn open(bundle: &Path, binary: &Path) -> Result<(Library, Option<ModuleExit>), Error> {
		// SAFETY:
		// Loading an external library object file is inherently unsafe.
		let library = unsafe { libloading::os::unix::Library::new(binary) }
			.map_err(|err| Error::Load(bundle.to_owned(), err))?;

		// The VST3 spec asks us to pass the library handle to `ModuleEntry`.
		let handle = library.into_raw();

		// SAFETY:
		// `handle` was just produced by `into_raw` and nothing else owns it.
		let library = Library::from(unsafe { libloading::os::unix::Library::from_raw(handle) });

		// SAFETY:
		// The VST3 spec defines `ModuleEntry` as `bool (*)(void*)`.
		let entry = unsafe { library.get::<unsafe extern "system" fn(*mut c_void) -> bool>(b"ModuleEntry\0") }
			.ok()
			.map(|entry| *entry);

		if let Some(entry) = entry {
			// SAFETY:
			// `entry` was resolved from `library`, which is loaded.
			if !unsafe { entry(handle) } {
				return Err(Error::EntryFailed(bundle.to_owned()));
			}
		}

		// SAFETY:
		// The VST3 spec defines `ModuleExit` as `bool (*)()`.
		let exit = unsafe { library.get::<ModuleExit>(b"ModuleExit\0") }
			.ok()
			.map(|exit| *exit);

		Ok((library, exit))
	}
}

#[cfg(windows)]
mod platform {
	use super::{Error, Library, ModuleExit};
	use std::path::{Path, PathBuf};

	const ARCH: &str = if cfg!(target_arch = "x86_64") {
		"x86_64-win"
	} else if cfg!(target_arch = "aarch64") {
		"arm64-win"
	} else {
		"x86-win"
	};

	pub fn binary_path(bundle: &Path) -> Result<PathBuf, Error> {
		// Legacy plugins are a single `.vst3` DLL, modern ones are bundles.
		if bundle.is_file() {
			return Ok(bundle.to_owned());
		}

		let dir = bundle.join("Contents").join(ARCH);
		let stem = bundle.file_stem().unwrap_or_default();

		let preferred = dir.join(stem).with_extension("vst3");
		if preferred.is_file() {
			return Ok(preferred);
		}

		std::fs::read_dir(&dir)
			.ok()
			.into_iter()
			.flatten()
			.flatten()
			.map(|entry| entry.path())
			.find(|path| path.extension().is_some_and(|ext| ext == "vst3"))
			.ok_or_else(|| Error::NoBinary(bundle.to_owned()))
	}

	pub fn open(bundle: &Path, binary: &Path) -> Result<(Library, Option<ModuleExit>), Error> {
		// Plugins often ship DLLs next to their binary, so those have to be found as well. This
		// flag makes Windows search the directory of the binary, and needs an absolute path.
		let binary = std::path::absolute(binary).unwrap_or_else(|_| binary.to_owned());

		// SAFETY:
		// Loading an external library object file is inherently unsafe.
		let library = unsafe {
			libloading::os::windows::Library::load_with_flags(
				&binary,
				libloading::os::windows::LOAD_WITH_ALTERED_SEARCH_PATH,
			)
		}
		.map(Library::from)
		.map_err(|err| Error::Load(bundle.to_owned(), err))?;

		// SAFETY:
		// The VST3 spec defines `InitDll` as `bool (*)()`.
		let entry = unsafe { library.get::<unsafe extern "system" fn() -> bool>(b"InitDll\0") }
			.ok()
			.map(|entry| *entry);

		if let Some(entry) = entry {
			// SAFETY:
			// `entry` was resolved from `library`, which is loaded.
			if !unsafe { entry() } {
				return Err(Error::EntryFailed(bundle.to_owned()));
			}
		}

		// SAFETY:
		// The VST3 spec defines `ExitDll` as `bool (*)()`.
		let exit = unsafe { library.get::<ModuleExit>(b"ExitDll\0") }
			.ok()
			.map(|exit| *exit);

		Ok((library, exit))
	}
}

#[cfg(target_os = "macos")]
mod platform {
	use super::{Error, Library, ModuleExit};
	use std::path::{Path, PathBuf};

	// TODO: macOS needs `bundleEntry(CFBundleRef)`, which requires creating a `CFBundle` from the
	// bundle path (via the `core-foundation` crate), and reading `CFBundleExecutable` from
	// `Info.plist` to find the binary in `Contents/MacOS`. This can't be tested from Linux.

	pub fn binary_path(_bundle: &Path) -> Result<PathBuf, Error> {
		Err(Error::Unsupported("loading VST3 bundles on macOS"))
	}

	pub fn open(_bundle: &Path, _binary: &Path) -> Result<(Library, Option<ModuleExit>), Error> {
		Err(Error::Unsupported("loading VST3 bundles on macOS"))
	}
}
