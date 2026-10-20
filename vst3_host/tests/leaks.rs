#![cfg(not(target_os = "macos"))]
// This file contains a single test, because it watches a global counter.

mod common;

use dsp as _;
use libloading as _;
use raw_window_handle as _;
use log as _;
use std::{path::Path, sync::Arc};
use vst3 as _;
use vst3_host::{ParentWindow, Plugin, descriptors, live_host_objects};
use walkdir as _;

#[test]
fn host_objects_are_released() {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path).unwrap().remove(0);

	assert_eq!(live_host_objects(), 0);

	for _ in 0..500 {
		let (plugin, _receiver) = Plugin::new(&descriptor).unwrap();
		assert_eq!(live_host_objects(), 2);
		drop(plugin);
		assert_eq!(live_host_objects(), 0);
	}

	// the error path has to clean up as well
	let mut bad = descriptor;
	bad.id = [0xAB; 16];
	for _ in 0..50 {
		assert!(Plugin::new(&bad).is_err());
		assert_eq!(live_host_objects(), 0);
	}

	// editors have a frame of their own that has to go as well
	let restarter = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| &*d.name == "Restarter")
		.unwrap();

	for _ in 0..100 {
		let (mut plugin, _receiver) = Plugin::new(&restarter).unwrap();
		plugin.create_editor().unwrap();
		assert_eq!(live_host_objects(), 3);

		plugin
			.attach_editor(ParentWindow::native(std::ptr::without_provenance_mut(0x1000)))
			.unwrap();
		plugin.close_editor();
		assert_eq!(live_host_objects(), 2);

		plugin.create_editor().unwrap();
		drop(plugin);
		assert_eq!(live_host_objects(), 0);
	}
}
