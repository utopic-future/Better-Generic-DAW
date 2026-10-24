// the test only uses the crate itself, the dependencies are used by the library
#![allow(unused_crate_dependencies)]

mod common;

use std::sync::Arc;
use vst2_host::{AudioConfig, Event, Plugin};

/// The plugin of the tests.
fn plugin() -> Plugin {
	let path = Arc::from(common::plugin_path());

	let descriptors = vst2_host::descriptors(&path).expect("the plugin wasn't found");
	assert_eq!(descriptors.len(), 1);

	let descriptor = &descriptors[0];
	assert_eq!(&*descriptor.name, "Test Gain");
	assert_eq!(descriptor.vendor.as_deref(), Some("Generic DAW"));
	assert_eq!(descriptor.unique_id, 0x1234);

	let (plugin, _receiver) = Plugin::new(descriptor).expect("the plugin didn't load");

	plugin
}

#[test]
fn the_plugin_has_one_parameter() {
	let plugin = plugin();

	assert_eq!(plugin.params().len(), 1);

	let param = &plugin.params()[0];
	assert_eq!(param.id, 0);
	assert_eq!(&*param.title, "Gain");
	assert!(param.can_automate());
}

#[test]
fn the_parameter_can_be_set() {
	let plugin = plugin();

	assert_eq!(plugin.param_normalized(0), Some(1.0));

	let value = plugin.set_param_normalized(0, 0.5).unwrap();
	assert_eq!(value, 0.5);
	assert_eq!(plugin.param_normalized(0), Some(0.5));

	let display = plugin.param_display(0, 0.5).unwrap();
	assert_eq!(display, "0.50");
}

#[test]
fn the_plugin_processes_audio() {
	let mut plugin = plugin();

	let config = AudioConfig {
		sample_rate: 44100.0,
		max_frames: 512,
		offline: false,
	};

	let mut audio = plugin.activate(config).expect("the plugin didn't start");

	let mut buffer = [[1.0f32; 2]; 512];
	audio.process(&mut buffer, None, 1.0);

	// the gain is 1, so the signal is the same
	assert_eq!(buffer[0], [1.0, 1.0]);

	plugin.set_param_normalized(0, 0.5).unwrap();

	let mut buffer = [[1.0f32; 2]; 512];
	audio.process(&mut buffer, None, 1.0);

	assert_eq!(buffer[0], [0.5, 0.5]);
}

#[test]
fn the_dry_signal_is_kept_when_the_plugin_is_quiet() {
	let mut plugin = plugin();

	let config = AudioConfig {
		sample_rate: 44100.0,
		max_frames: 512,
		offline: false,
	};

	let mut audio = plugin.activate(config).expect("the plugin didn't start");

	plugin.set_param_normalized(0, 0.0).unwrap();

	let mut buffer = [[1.0f32; 2]; 512];
	audio.process(&mut buffer, None, 0.5);

	// half of the dry signal, and half of the quiet signal
	assert_eq!(buffer[0], [0.5, 0.5]);
}

#[test]
fn the_state_is_saved_and_restored() {
	let first = plugin();

	first.set_param_normalized(0, 0.25).unwrap();

	let state = first.save_state().expect("the state wasn't saved");

	let second = plugin();
	second.load_state(&state).expect("the state wasn't loaded");

	assert_eq!(second.param_normalized(0), Some(0.25));
}

#[test]
fn notes_are_sent_to_the_plugin() {
	let mut plugin = plugin();

	let config = AudioConfig {
		sample_rate: 44100.0,
		max_frames: 512,
		offline: false,
	};

	let mut audio = plugin.activate(config).expect("the plugin didn't start");

	assert!(audio.push_event(Event::NoteOn {
		time: 0,
		key: 60,
		velocity: 1.0,
	}));

	let mut buffer = [[0.0f32; 2]; 512];
	audio.process(&mut buffer, None, 1.0);

	// the plugin doesn't make any noise, but it didn't fall over
	assert_eq!(buffer[0], [0.0, 0.0]);
}

#[test]
fn the_plugin_is_deactivated() {
	let mut plugin = plugin();

	let config = AudioConfig {
		sample_rate: 44100.0,
		max_frames: 512,
		offline: false,
	};

	let audio = plugin.activate(config).expect("the plugin didn't start");
	assert!(plugin.is_active());

	plugin.deactivate(audio);
	assert!(!plugin.is_active());
}
