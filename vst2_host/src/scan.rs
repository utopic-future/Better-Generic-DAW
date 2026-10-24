//! Finds VST2 plugins, in a program of its own.
//!
//! Some plugins crash when they're made, and a crash takes the whole program with it.
//! The scan therefore runs in a program of its own: the program that asked for the scan
//! keeps what the scan found, and skips the plugins that made it crash.

use crate::{PluginDescriptor, descriptors};
use std::{
	io::{BufRead as _, BufReader, Write as _},
	path::{Path, PathBuf},
	process::{Command, Stdio},
	sync::Arc,
};

/// The argument that tells the program to scan for plugins instead of starting.
pub const SCAN_ARGUMENT: &str = "--scan-vst2";

/// The line that says which plugin is being made.
const PATH: &str = "path";

/// The line that says which plugin was found.
const PLUGIN: &str = "plugin";

/// What the scan found.
#[derive(Clone, Debug)]
pub enum Event {
	/// The plugin is being made.
	Scanning(Arc<Path>),
	/// The plugin was found.
	Found(PluginDescriptor),
}

/// Scans the plugins in the paths the program was given, and prints them.
pub fn print_plugins(paths: &[PathBuf]) {
	let stdout = std::io::stdout();
	let mut stdout = stdout.lock();

	for path in paths {
		// The path is printed first, so that the program that asked for the scan knows
		// which plugin it was making when it crashed.
		let _ = writeln!(stdout, "{PATH}\t{}", path.display());
		let _ = stdout.flush();

		if let Some(found) = descriptors(&Arc::from(path.as_path())) {
			for descriptor in found {
				let _ = writeln!(stdout, "{PLUGIN}\t{}", format_descriptor(&descriptor));
			}
		}

		let _ = stdout.flush();
	}
}

/// The paths the program was given to scan, if it was asked to scan.
#[must_use]
pub fn paths_from_args(args: impl Iterator<Item = String>) -> Option<Vec<PathBuf>> {
	let args = args.skip_while(|arg| arg != SCAN_ARGUMENT).skip(1);

	let paths = args.map(PathBuf::from).collect::<Vec<_>>();

	if paths.is_empty() {
		return None;
	}

	Some(paths)
}

/// A plugin, in one line: the fields separated by tabs, the path last.
fn format_descriptor(descriptor: &PluginDescriptor) -> String {
	format!(
		"{}\t{}\t{}\t{}\t{}\t{}",
		descriptor.unique_id,
		descriptor.name,
		descriptor.vendor.as_deref().unwrap_or_default(),
		descriptor.version.as_deref().unwrap_or_default(),
		descriptor.category,
		descriptor.path.display(),
	)
}

/// The plugin of a line, if there is one.
fn parse_descriptor(line: &str) -> Option<PluginDescriptor> {
	let mut fields = line.splitn(6, '\t');

	let unique_id = fields.next()?.parse().ok()?;
	let name = fields.next()?;
	let vendor = fields.next()?;
	let version = fields.next()?;
	let category = fields.next()?.parse().ok()?;
	let path = fields.next()?;

	Some(PluginDescriptor {
		unique_id,
		name: name.into(),
		vendor: (!vendor.is_empty()).then(|| vendor.into()),
		version: (!version.is_empty()).then(|| version.into()),
		category,
		path: Arc::from(Path::new(path)),
	})
}

#[cfg(test)]
mod tests {
	use super::*;

	/// A plugin, with everything there is to know about it.
	fn descriptor() -> PluginDescriptor {
		PluginDescriptor {
			unique_id: 1234,
			name: "Test Plugin".into(),
			vendor: Some("Test Vendor".into()),
			version: Some("1.2.3".into()),
			category: 1,
			path: Arc::from(Path::new("/somewhere/Test Plugin.dll")),
		}
	}

	#[test]
	fn a_plugin_survives_a_round_trip() {
		let descriptor = descriptor();

		let line = format_descriptor(&descriptor);
		let restored = parse_descriptor(&line).expect("the plugin wasn't read back");

		assert_eq!(restored, descriptor);
	}

	#[test]
	fn a_plugin_without_a_vendor_survives_a_round_trip() {
		let mut descriptor = descriptor();
		descriptor.vendor = None;
		descriptor.version = None;

		let line = format_descriptor(&descriptor);
		let restored = parse_descriptor(&line).expect("the plugin wasn't read back");

		assert_eq!(restored, descriptor);
	}

	#[test]
	fn the_paths_of_the_scan_are_read() {
		let args = ["something".to_owned(), SCAN_ARGUMENT.to_owned()]
			.into_iter()
			.chain(["/a.dll".to_owned(), "/b.dll".to_owned()]);

		let paths = paths_from_args(args).expect("the paths weren't read");

		assert_eq!(paths, [PathBuf::from("/a.dll"), PathBuf::from("/b.dll")]);
	}

	#[test]
	fn a_program_that_wasnt_asked_to_scan_has_no_paths() {
		let args = ["something".to_owned()].into_iter();

		assert_eq!(paths_from_args(args), None);
	}
}

/// Finds the plugins in `paths`, and tells the caller about them.
///
/// The plugins are made in a program of its own, so that a plugin that crashes doesn't
/// take this program with it. The plugins that made it crash are skipped.
pub fn scan(paths: &[Arc<Path>], mut on_event: impl FnMut(Event)) {
	let Some(exe) = std::env::current_exe().ok() else {
		return;
	};

	let mut remaining: Vec<Arc<Path>> = paths.to_vec();

	while !remaining.is_empty() {
		let (finished, crashed) = run(&exe, &remaining, &mut on_event);

		if finished {
			break;
		}

		let Some(crashed) = crashed else {
			break;
		};

		log::warn!(
			"{}: the plugin crashed the program, so it's skipped",
			crashed.display()
		);

		// the plugins before the one that crashed are found already, so the program
		// starts again with the ones after it
		if let Some(index) = remaining.iter().position(|path| *path == crashed) {
			remaining.drain(..=index);
		} else {
			break;
		}
	}
}

/// Makes the plugins in `paths` in a program of its own.
///
/// Returns whether the program finished, and the plugin it was making when it didn't.
fn run(
	exe: &Path,
	paths: &[Arc<Path>],
	on_event: &mut impl FnMut(Event),
) -> (bool, Option<Arc<Path>>) {
	let mut child = match Command::new(exe)
		.arg(SCAN_ARGUMENT)
		.args(paths.iter().map(AsRef::as_ref))
		.stdout(Stdio::piped())
		.spawn()
	{
		Ok(child) => child,
		Err(err) => {
			log::warn!("couldn't start the program that scans for plugins: {err}");
			return (true, None);
		}
	};

	let Some(stdout) = child.stdout.take() else {
		return (true, None);
	};

	let mut current = None;

	for line in BufReader::new(stdout).lines() {
		let Ok(line) = line else {
			break;
		};

		let Some((kind, rest)) = line.split_once('\t') else {
			continue;
		};

		match kind {
			PATH => {
				let path = Arc::<Path>::from(Path::new(rest));

				current = Some(path.clone());

				on_event(Event::Scanning(path));
			}
			PLUGIN => {
				if let Some(descriptor) = parse_descriptor(rest) {
					on_event(Event::Found(descriptor));
				}
			}
			_ => {}
		}
	}

	let status = child.wait();

	// the program ends with an error when it crashed
	let finished = matches!(status, Ok(status) if status.success());

	(finished, current)
}
