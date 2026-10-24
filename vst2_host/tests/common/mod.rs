#![allow(dead_code)]

use std::{
	path::{Path, PathBuf},
	process::Command,
	sync::OnceLock,
};

/// Builds the `test_plugin` crate, which is the plugin the tests use.
///
/// Returns the path of the plugin.
pub fn plugin_path() -> &'static Path {
	static PATH: OnceLock<PathBuf> = OnceLock::new();

	PATH.get_or_init(|| {
		// other crates in the workspace use this file too, from their own directory
		let root = Path::new(env!("CARGO_MANIFEST_DIR"));
		let manifest = [
			root.join("test_plugin/Cargo.toml"),
			root.join("../vst2_host/test_plugin/Cargo.toml"),
		]
		.into_iter()
		.find(|manifest| manifest.exists())
		.expect("can't find the test plugin");

		let status = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
			.args(["build", "--manifest-path"])
			.arg(&manifest)
			.status()
			.expect("failed to run cargo");
		assert!(status.success(), "building the test plugin failed");

		manifest
			.parent()
			.unwrap()
			.join("target/debug")
			.join(if cfg!(windows) {
				"vst2_gain.dll"
			} else {
				"libvst2_gain.so"
			})
	})
}
