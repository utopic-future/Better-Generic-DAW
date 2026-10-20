#![allow(dead_code)]

use std::{
	path::{Path, PathBuf},
	process::Command,
	sync::OnceLock,
};

/// The folder in a bundle that holds the binary, for this platform.
pub const ARCH_DIR: &str = if cfg!(windows) {
	if cfg!(target_arch = "aarch64") {
		"arm64-win"
	} else {
		"x86_64-win"
	}
} else if cfg!(target_arch = "aarch64") {
	"aarch64-linux"
} else {
	"x86_64-linux"
};

/// The name of a bundle's binary for this platform.
pub fn binary_name(stem: &str) -> String {
	if cfg!(windows) {
		format!("{stem}.vst3")
	} else {
		format!("{stem}.so")
	}
}

/// Builds the `test_plugin` crate and wraps it in a proper `.vst3` bundle layout.
///
/// Returns the directory that *contains* `Gain.vst3`.
pub fn plugin_dir() -> &'static Path {
	static DIR: OnceLock<PathBuf> = OnceLock::new();

	DIR.get_or_init(|| {
		// other crates in the workspace use this file too, from their own directory
		let root = Path::new(env!("CARGO_MANIFEST_DIR"));
		let manifest = [
			root.join("test_plugin/Cargo.toml"),
			root.join("../vst3_host/test_plugin/Cargo.toml"),
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

		let built = manifest
			.parent()
			.unwrap()
			.join("target/debug")
			.join(if cfg!(windows) {
				"gain.dll"
			} else {
				"libgain.so"
			});

		// only integration tests get a directory of their own to put things in
		let base = option_env!("CARGO_TARGET_TMPDIR").map_or_else(
			|| std::env::temp_dir().join(format!("vst3_host_tests_{}", std::process::id())),
			PathBuf::from,
		);
		let dir = base.join("vst3_plugins");
		let _ = std::fs::remove_dir_all(&dir);

		let bundle = dir.join("Gain.vst3/Contents").join(ARCH_DIR);
		std::fs::create_dir_all(&bundle).unwrap();
		std::fs::copy(&built, bundle.join(binary_name("Gain"))).unwrap();

		dir
	})
}

pub fn bundle() -> PathBuf {
	plugin_dir().join("Gain.vst3")
}
