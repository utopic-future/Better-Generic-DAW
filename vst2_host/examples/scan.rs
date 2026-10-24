//! Scans the default VST2 paths and lists the plugins it finds.
//!
//! This is useful to find out whether a plugin is found at all, and whether it can be
//! loaded. The plugins are made in a program of its own, so that a plugin that crashes
//! doesn't take this program with it.

use std::{sync::Arc, time::Instant};
use vst2_host::{DEFAULT_VST2_PATHS, find_plugin_paths, scan};

fn main() {
	// the program can be told to scan instead of starting, which is how the scan works
	if let Some(paths) = scan::paths_from_args(std::env::args().skip(1)) {
		scan::print_plugins(&paths);
		return;
	}

	// the files to scan, or all the ones in the default paths
	let args = std::env::args().skip(1).collect::<Vec<_>>();

	let found = if args.is_empty() {
		find_plugin_paths(DEFAULT_VST2_PATHS.iter()).collect::<Vec<_>>()
	} else {
		args.iter().map(std::path::PathBuf::from).map(Arc::from).collect()
	};

	println!("{} files found", found.len());

	let start = Instant::now();
	let mut plugins = 0;

	scan::scan(&found, |event| match event {
		scan::Event::Scanning(path) => {
			print!("{}: ", path.display());
		}
		scan::Event::Found(descriptor) => {
			plugins += 1;

			println!(
				"  {} ({}) by {}",
				descriptor.name,
				descriptor.unique_id,
				descriptor.vendor.as_deref().unwrap_or("?"),
			);
		}
	});

	println!("\n{plugins} plugins found in {:?}", start.elapsed());
}
