#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use raw_window_handle as _;
use std::{
	path::Path,
	sync::{Arc, mpsc::Receiver},
};
use vst3 as _;
use vst3_host::{
	AudioConfig, Event, MainThreadMessage, Plugin, ProcessStatus, RestartActions, RestartFlags,
	descriptors,
};
use walkdir as _;

const CONFIG: AudioConfig = AudioConfig {
	sample_rate: 44100.0,
	max_frames: 128,
	offline: false,
};

const TRIGGER: u32 = 0;
const NAME: u32 = 1;
const VALUE: u32 = 2;

fn plugin() -> (Plugin, Receiver<MainThreadMessage>) {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| &*d.name == "Restarter")
		.unwrap();

	Plugin::new(&descriptor).unwrap()
}

fn assert_close(a: f64, b: f64) {
	assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

/// The restart flags the plugin asked for, as a plain number.
fn restart_flags(receiver: &Receiver<MainThreadMessage>) -> RestartFlags {
	let Ok(MainThreadMessage::Restart(flags)) = receiver.try_recv() else {
		panic!("the plugin should have asked for a restart");
	};
	assert!(receiver.try_recv().is_err(), "only one request expected");
	flags
}

fn ramp(len: usize) -> Vec<[f32; 2]> {
	(0..len)
		.map(|i| [i as f32 + 1.0, -(i as f32) - 1.0])
		.collect()
}

#[test]
fn single_component_plugins_work() {
	let (plugin, _receiver) = plugin();

	assert!(plugin.is_single_component());
	assert!(plugin.has_controller());
	assert!(!plugin.is_connected());

	// the others report what the editor was told, see the plugin
	let titles = plugin
		.params()
		.iter()
		.take(3)
		.map(|p| &*p.title)
		.collect::<Vec<_>>();
	assert_eq!(titles, ["Trigger", "Name A", "Value"]);
	assert_eq!(plugin.params().len(), 10);

	assert_close(plugin.set_param_normalized(VALUE, 0.7).unwrap(), 0.7);
	assert_close(plugin.param_normalized(VALUE).unwrap(), 0.7);
}

#[test]
fn single_component_state_round_trips() {
	let (plugin, _receiver) = plugin();
	plugin.set_param_normalized(VALUE, 0.7).unwrap();

	let blob = plugin.save_state().unwrap();
	// the 3 values are the component's state, and the controller adds nothing of its own
	assert_eq!(blob.len(), 24 + 24);
	assert_eq!(blob[16..24], 0u64.to_le_bytes());

	let (restored, _receiver) = self::plugin();
	assert_close(restored.param_normalized(VALUE).unwrap(), 0.0);
	restored.load_state(&blob).unwrap();
	assert_close(restored.param_normalized(VALUE).unwrap(), 0.7);
}

#[test]
fn dropping_single_component_plugins_is_clean() {
	for _ in 0..50 {
		let (plugin, _receiver) = plugin();
		drop(plugin);
	}
}

#[test]
fn latency_changes_dont_need_a_restart() {
	let (mut plugin, receiver) = plugin();
	let mut audio = plugin.activate(CONFIG).unwrap();
	assert_eq!(audio.latency(), 0);

	plugin.set_param_normalized(TRIGGER, 0.1).unwrap();
	let flags = restart_flags(&receiver);
	assert!(flags.contains(RestartFlags::LATENCY_CHANGED));

	assert_eq!(
		plugin.handle_restart(flags),
		RestartActions {
			update_latency: true,
			..RestartActions::default()
		}
	);

	// nothing changes until the audio thread is told
	assert_eq!(audio.latency(), 0);
	assert_eq!(audio.update_latency(), 16);

	// with a mix level of zero only the dry signal, delayed by the new latency, is left
	let mut buf = ramp(64);
	assert_eq!(audio.process(&mut buf, None, 0.0), ProcessStatus::Processed);
	assert!(buf[..16].iter().all(|f| f[0] == 0.0));
	assert_eq!(buf[16][0], 1.0);
	assert_eq!(buf[63][0], 48.0);

	plugin.deactivate(audio);
}

#[test]
fn io_changes_need_a_restart_and_a_new_layout() {
	let (mut plugin, receiver) = plugin();
	let audio = plugin.activate(CONFIG).unwrap();
	assert_eq!(audio.layout().main_input, Some((0, 2)));

	plugin.set_param_normalized(TRIGGER, 0.2).unwrap();
	let flags = restart_flags(&receiver);
	assert!(flags.needs_restart());

	let actions = plugin.handle_restart(flags);
	assert!(actions.reactivate);
	assert!(actions.params_changed);
	// the restart takes care of the latency
	assert!(!actions.update_latency);

	let mut audio = plugin.restart(audio).unwrap();

	// the plugin only does mono now
	assert_eq!(audio.layout().main_input, Some((0, 1)));
	assert_eq!(audio.layout().main_output, Some((0, 1)));
	assert_eq!(audio.config(), CONFIG);

	let mut buf = vec![[1.0, 0.0]; 32];
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert_eq!(buf[0], [0.5, 0.5]);

	plugin.deactivate(audio);
}

#[test]
fn reloading_keeps_the_configuration() {
	let (mut plugin, receiver) = plugin();
	let config = AudioConfig {
		sample_rate: 96000.0,
		max_frames: 64,
		offline: true,
	};
	let audio = plugin.activate(config).unwrap();

	plugin.set_param_normalized(TRIGGER, 0.3).unwrap();
	let flags = restart_flags(&receiver);
	assert!(flags.contains(RestartFlags::RELOAD_COMPONENT));

	let actions = plugin.handle_restart(flags);
	assert!(actions.reactivate);
	assert!(actions.reread_param_values);

	let mut audio = plugin.restart(audio).unwrap();
	assert_eq!(audio.config(), config);
	assert_eq!(audio.layout().main_input, Some((0, 2)));

	let mut buf = vec![[0.25, 0.5]; 200];
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert_eq!(buf[199], [0.25, 0.5]);

	plugin.deactivate(audio);
}

#[test]
fn restarting_can_be_repeated() {
	let (mut plugin, _receiver) = plugin();
	let mut audio = plugin.activate(CONFIG).unwrap();

	for _ in 0..10 {
		audio = plugin.restart(audio).unwrap();

		let mut buf = vec![[0.5, 0.5]; 32];
		assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	}

	plugin.deactivate(audio);
}

#[test]
fn renamed_parameters_are_picked_up() {
	let (mut plugin, receiver) = plugin();
	assert_eq!(&*plugin.param(NAME).unwrap().title, "Name A");

	plugin.set_param_normalized(TRIGGER, 0.4).unwrap();
	let flags = restart_flags(&receiver);
	assert!(flags.contains(RestartFlags::PARAM_TITLES_CHANGED));

	// the list is stale until the host handles the request
	assert_eq!(&*plugin.param(NAME).unwrap().title, "Name A");

	let actions = plugin.handle_restart(flags);
	assert_eq!(
		actions,
		RestartActions {
			params_changed: true,
			..RestartActions::default()
		}
	);
	assert_eq!(&*plugin.param(NAME).unwrap().title, "Name B");
}

#[test]
fn values_changed_by_the_plugin_can_be_read_again() {
	let (mut plugin, receiver) = plugin();
	assert_close(plugin.param_normalized(VALUE).unwrap(), 0.0);

	plugin.set_param_normalized(TRIGGER, 0.5).unwrap();
	let flags = restart_flags(&receiver);

	let actions = plugin.handle_restart(flags);
	assert_eq!(
		actions,
		RestartActions {
			reread_param_values: true,
			..RestartActions::default()
		}
	);
	assert_close(plugin.param_normalized(VALUE).unwrap(), 0.9);
}

#[test]
fn requests_from_the_audio_thread_arrive_too() {
	let (mut plugin, receiver) = plugin();
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(Event::ParamValue {
		time: 0,
		id: TRIGGER,
		value: 0.9,
	});

	let audio = std::thread::spawn(move || {
		let mut buf = vec![[0.5, 0.5]; 32];
		assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
		audio
	})
	.join()
	.unwrap();

	let flags = restart_flags(&receiver);
	assert!(flags.contains(RestartFlags::LATENCY_CHANGED));

	let actions = plugin.handle_restart(flags);
	assert!(actions.update_latency);

	let mut audio = audio;
	assert_eq!(audio.update_latency(), 16);

	plugin.deactivate(audio);
}

#[test]
fn restart_flags_are_combined_and_unknown_ones_ignored() {
	let (mut plugin, _receiver) = plugin();

	assert_eq!(
		plugin.handle_restart(RestartFlags(0)),
		RestartActions::default()
	);

	// routing info changes, which this host doesn't use
	assert_eq!(
		plugin.handle_restart(RestartFlags(512)),
		RestartActions::default()
	);

	let both = RestartFlags(RestartFlags::LATENCY_CHANGED | RestartFlags::PARAM_VALUES_CHANGED);
	assert_eq!(
		plugin.handle_restart(both),
		RestartActions {
			update_latency: true,
			reread_param_values: true,
			..RestartActions::default()
		}
	);
}

#[test]
fn the_audio_thread_sees_requests_without_the_main_thread() {
	let (mut plugin, _receiver) = plugin();
	let mut audio = plugin.activate(CONFIG).unwrap();

	assert_eq!(audio.restart_flags(), RestartFlags(0));

	// a request from the main thread...
	plugin.set_param_normalized(TRIGGER, 0.2).unwrap();
	assert!(audio.restart_flags().needs_restart());
	// ...can be looked at as often as needed
	assert!(audio.restart_flags().needs_restart());

	// and is gone once taken
	let taken = audio.take_restart_flags();
	assert!(taken.contains(RestartFlags::IO_CHANGED));
	assert_eq!(audio.restart_flags(), RestartFlags(0));

	// requests from the audio thread itself show up in the same place
	audio.push_event(Event::ParamValue {
		time: 0,
		id: TRIGGER,
		value: 0.9,
	});
	let mut buf = vec![[0.5, 0.5]; 32];
	audio.process(&mut buf, None, 1.0);
	assert!(
		audio
			.restart_flags()
			.contains(RestartFlags::LATENCY_CHANGED)
	);

	// requests are combined
	plugin.set_param_normalized(TRIGGER, 0.4).unwrap();
	let flags = audio.take_restart_flags();
	assert!(flags.contains(RestartFlags::LATENCY_CHANGED));
	assert!(flags.contains(RestartFlags::PARAM_TITLES_CHANGED));

	plugin.deactivate(audio);
}

#[test]
fn activating_again_forgets_old_requests() {
	let (mut plugin, _receiver) = plugin();
	let audio = plugin.activate(CONFIG).unwrap();

	plugin.set_param_normalized(TRIGGER, 0.2).unwrap();
	assert!(audio.restart_flags().needs_restart());

	let audio = plugin.restart(audio).unwrap();
	assert_eq!(audio.restart_flags(), RestartFlags(0));

	plugin.deactivate(audio);
}
