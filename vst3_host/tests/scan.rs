#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use raw_window_handle as _;
use vst3 as _;
use walkdir as _;

use std::{path::Path, sync::Arc};
use vst3_host::{Module, descriptors, find_plugin_paths};

#[test]
fn finds_bundle_but_not_binary_inside() {
	let found = find_plugin_paths([common::plugin_dir()]).collect::<Vec<_>>();

	assert_eq!(found.len(), 1, "{found:?}");
	assert!(found[0].ends_with("Gain.vst3"));
}

#[test]
fn lists_descriptors() {
	let path: Arc<Path> = common::bundle().into();
	let descriptors = descriptors(&path).expect("module should load");

	// controller classes are filtered out
	let names = descriptors.iter().map(|d| &*d.name).collect::<Vec<_>>();
	assert_eq!(
		names,
		[
			"Gain (vst3-rs example plugin)",
			"Probe",
			"Mono",
			"Synth",
			"Restarter"
		]
	);

	for descriptor in &descriptors {
		assert_ne!(descriptor.id, [0; 16]);
		assert_eq!(descriptor.id_hex().len(), 32);
		assert_eq!(descriptor.path, path);
		// these come from `IPluginFactory2`
		assert_eq!(descriptor.vendor.as_deref(), Some("Vendor"));
		assert_eq!(descriptor.version.as_deref(), Some("1.2.3"));
	}

	let ids = descriptors
		.iter()
		.map(|d| d.id)
		.collect::<std::collections::HashSet<_>>();
	assert_eq!(ids.len(), 5);

	assert!(!descriptors[0].is_instrument());
	assert_eq!(&*descriptors[0].subcategories, [Arc::from("Fx")]);

	assert!(descriptors[3].is_instrument());
	assert_eq!(
		&*descriptors[3].subcategories,
		[Arc::from("Instrument"), Arc::from("Synth")]
	);
}

#[test]
fn load_and_unload_repeatedly() {
	let path: Arc<Path> = common::bundle().into();

	for _ in 0..200 {
		let module = Module::load(&path).expect("module should load");
		assert_eq!(module.descriptors(&path).len(), 5);
	}
}

#[test]
fn bad_bundles_fail_gracefully() {
	let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("vst3_bad");
	let _ = std::fs::remove_dir_all(&dir);

	// an empty bundle
	let empty = dir.join("Empty.vst3");
	std::fs::create_dir_all(&empty).unwrap();
	assert!(Module::load(&empty).is_err());

	// a bundle whose binary isn't a shared library
	let garbage = dir.join("Garbage.vst3/Contents").join(common::ARCH_DIR);
	std::fs::create_dir_all(&garbage).unwrap();
	std::fs::write(
		garbage.join(common::binary_name("Garbage")),
		b"definitely not a library",
	)
	.unwrap();
	assert!(Module::load(&dir.join("Garbage.vst3")).is_err());

	// a path that doesn't exist at all
	assert!(Module::load(&dir.join("Missing.vst3")).is_err());

	let paths = [Arc::<Path>::from(dir.join("Garbage.vst3"))];
	assert!(descriptors(&paths[0]).is_none());
}
