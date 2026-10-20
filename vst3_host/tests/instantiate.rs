#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use raw_window_handle as _;
use log as _;
use std::{path::Path, sync::Arc};
use vst3 as _;
use vst3_host::{Error, Plugin, PluginDescriptor, descriptors};
use walkdir as _;

fn gain() -> PluginDescriptor {
	let path: Arc<Path> = common::bundle().into();
	descriptors(&path).unwrap().remove(0)
}

#[test]
fn creates_component_and_separate_controller() {
	let (plugin, receiver) = Plugin::new(&gain()).expect("plugin should instantiate");

	assert!(plugin.has_controller());
	assert!(!plugin.is_single_component());
	// the test plugin doesn't implement `IConnectionPoint`, so nothing to connect
	assert!(!plugin.is_connected());
	assert!(receiver.try_recv().is_err());
	assert!(plugin.descriptor().name.contains("Gain"));
}

#[test]
fn unknown_class_id_is_an_error() {
	let mut descriptor = gain();
	descriptor.id = [0xAB; 16];

	assert!(matches!(
		Plugin::new(&descriptor),
		Err(Error::CreateFailed("component"))
	));
}

#[test]
fn many_instances_of_the_same_module() {
	let descriptor = gain();
	let mut plugins = Vec::new();
	for _ in 0..16 {
		plugins.push(Plugin::new(&descriptor).unwrap());
	}

	assert_eq!(plugins.len(), 16);
}
