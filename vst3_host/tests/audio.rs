#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use raw_window_handle as _;
use std::{path::Path, sync::Arc};
use vst3 as _;
use vst3_host::{AudioConfig, AudioThread, Error, Plugin, ProcessStatus, Transport, descriptors};
use walkdir as _;

const CONFIG: AudioConfig = AudioConfig {
	sample_rate: 48000.0,
	max_frames: 256,
	offline: false,
};

fn plugin(name: &str) -> Plugin {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| d.name.starts_with(name))
		.unwrap();

	Plugin::new(&descriptor).unwrap().0
}

fn assert_close(a: f32, b: f32) {
	assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

fn frames(len: usize, left: f32, right: f32) -> Vec<[f32; 2]> {
	vec![[left, right]; len]
}

// The probe plugin writes what it saw into its output, see `test_plugin/src/extra.rs`.
fn l(out: &[[f32; 2]], index: usize) -> f32 {
	out[index][0]
}

fn r(out: &[[f32; 2]], index: usize) -> f32 {
	out[index][1]
}

const PLAYING: f32 = 2.0;
const CYCLE_ACTIVE: f32 = 4.0;
const TEMPO_VALID: f32 = 1024.0;
const TIME_SIG_VALID: f32 = 8192.0;
const PROJECT_TIME_MUSIC_VALID: f32 = 512.0;
const BAR_POSITION_VALID: f32 = 2048.0;
const CYCLE_VALID: f32 = 4096.0;
const CONT_TIME_VALID: f32 = 131_072.0;

fn transport() -> Transport {
	Transport {
		playing: true,
		recording: false,
		tempo: 120.0,
		time_sig_numerator: 3,
		time_sig_denominator: 4,
		position_samples: 96000,
		position_quarters: 4.5,
		cycle: None,
	}
}

fn activate(plugin: &mut Plugin) -> AudioThread {
	plugin.activate(CONFIG).expect("plugin should activate")
}

#[test]
fn gain_passes_audio_through_at_unity() {
	let mut plugin = plugin("Gain");
	let mut audio = activate(&mut plugin);

	assert_eq!(audio.layout().main_input, Some((0, 2)));
	assert_eq!(audio.layout().main_output, Some((0, 2)));
	assert_eq!(audio.latency(), 0);

	let mut buf = frames(100, 0.5, -0.25);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert!(buf.iter().all(|f| *f == [0.5, -0.25]));

	plugin.deactivate(audio);
}

#[test]
fn mono_plugins_get_a_mono_downmix() {
	let mut plugin = plugin("Mono");
	let mut audio = activate(&mut plugin);

	assert_eq!(audio.layout().main_input, Some((0, 1)));
	assert_eq!(audio.layout().main_output, Some((0, 1)));

	// the mono mix is (1.0 + 0.0) / 2 = 0.5, and the plugin halves it
	let mut buf = frames(64, 1.0, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert!(buf.iter().all(|f| *f == [0.25, 0.25]), "{:?}", buf[0]);

	plugin.deactivate(audio);
}

#[test]
fn dry_wet_mix() {
	let mut plugin = plugin("Mono");
	let mut audio = activate(&mut plugin);

	let mut run = |mix| {
		let mut buf = frames(8, 1.0, 0.0);
		assert_eq!(audio.process(&mut buf, None, mix), ProcessStatus::Processed);
		buf[0]
	};

	let dry = run(0.0);
	assert_close(dry[0], 1.0);
	assert_close(dry[1], 0.0);

	let wet = run(1.0);
	assert_close(wet[0], 0.25);
	assert_close(wet[1], 0.25);

	let half = run(0.5);
	assert_close(half[0], 0.625);
	assert_close(half[1], 0.125);

	// a negative mix level inverts the dry signal
	let inverted = run(-0.5);
	assert_close(inverted[0], -0.375);
	assert_close(inverted[1], 0.125);

	plugin.deactivate(audio);
}

#[test]
fn instruments_have_no_input_and_an_event_bus() {
	let mut plugin = plugin("Synth");
	assert!(plugin.descriptor().is_instrument());

	let mut audio = activate(&mut plugin);

	assert_eq!(audio.layout().main_input, None);
	assert_eq!(audio.layout().input_channels(), 0);
	assert_eq!(audio.layout().main_output, Some((0, 2)));
	assert_eq!(audio.layout().event_inputs, 1);

	// the synth writes 0.25 when it got no input bus, as it should
	let mut buf = frames(64, 0.5, 0.5);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert!(buf.iter().all(|f| *f == [0.25, 0.25]));

	// a mix keeps part of what was in the buffer
	let mut buf = frames(64, 0.5, 0.5);
	assert_eq!(audio.process(&mut buf, None, 0.5), ProcessStatus::Processed);
	assert!(buf.iter().all(|f| (f[0] - 0.375).abs() < 1e-6));

	plugin.deactivate(audio);
}

#[test]
fn sidechains_are_deactivated_but_present() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	assert_eq!(audio.layout().input_buses, 2);
	assert_eq!(audio.layout().output_buses, 1);
	assert_eq!(audio.layout().main_input, Some((0, 2)));

	let mut buf = frames(128, 0.75, -0.5);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);

	// the plugin was told bus 0 in and out are active, and the sidechain isn't
	assert_eq!(r(&buf, 11), 0b101 as f32);
	// but it still gets an entry for both input buses, and the sidechain has no channels
	assert_eq!(r(&buf, 6), 2.0);
	assert_eq!(r(&buf, 7), 1.0);
	assert_eq!(r(&buf, 10), 0.0);

	// the input arrives intact
	assert_eq!(r(&buf, 8), 0.75);
	assert_eq!(r(&buf, 9), -0.5);

	plugin.deactivate(audio);
}

#[test]
fn setup_is_passed_to_the_plugin() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	let mut buf = frames(100, 0.0, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);

	assert_eq!(r(&buf, 2), 100.0); // numSamples
	assert_eq!(r(&buf, 3), 0.0); // realtime
	assert_eq!(r(&buf, 4), 0.0); // 32 bit samples
	assert_eq!(r(&buf, 5), 256.0); // max block size
	assert_eq!(r(&buf, 12), 48000.0); // from `setupProcessing`
	assert_eq!(r(&buf, 1), 48000.0); // from the process context

	plugin.deactivate(audio);

	let offline = AudioConfig {
		offline: true,
		sample_rate: 44100.0,
		..CONFIG
	};
	let mut audio = plugin.activate(offline).unwrap();

	let mut buf = frames(100, 0.0, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert_eq!(r(&buf, 3), 2.0); // offline
	assert_eq!(r(&buf, 12), 44100.0);

	plugin.deactivate(audio);
}

#[test]
fn transport_reaches_the_plugin() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	let mut buf = frames(64, 0.0, 0.0);
	let status = audio.process(&mut buf, Some(&transport()), 1.0);
	assert_eq!(status, ProcessStatus::Processed);

	assert_eq!(l(&buf, 0), 120.0);
	assert_eq!(l(&buf, 2), 3.0);
	assert_eq!(l(&buf, 3), 4.0);
	assert_eq!(l(&buf, 4), 4.5);
	// bars are 3 quarter notes long, and 4.5 is in the second one
	assert_eq!(l(&buf, 5), 3.0);
	assert_eq!(r(&buf, 0), 96000.0);
	assert_eq!(
		l(&buf, 1),
		PLAYING
			+ TEMPO_VALID
			+ TIME_SIG_VALID
			+ PROJECT_TIME_MUSIC_VALID
			+ BAR_POSITION_VALID
			+ CONT_TIME_VALID
	);
	// no loop
	assert_eq!(l(&buf, 6), 0.0);
	assert_eq!(l(&buf, 7), 0.0);

	plugin.deactivate(audio);
}

#[test]
fn stopped_and_looping_transports() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	let looping = Transport {
		playing: false,
		cycle: Some((2.0, 6.0)),
		..transport()
	};

	let mut buf = frames(64, 0.0, 0.0);
	audio.process(&mut buf, Some(&looping), 1.0);

	assert_eq!(
		l(&buf, 1),
		TEMPO_VALID
			+ TIME_SIG_VALID
			+ PROJECT_TIME_MUSIC_VALID
			+ BAR_POSITION_VALID
			+ CONT_TIME_VALID
			+ CYCLE_VALID
			+ CYCLE_ACTIVE
	);
	assert_eq!(l(&buf, 6), 2.0);
	assert_eq!(l(&buf, 7), 6.0);

	// without any transport only the continuous time is valid
	let mut buf = frames(64, 0.0, 0.0);
	audio.process(&mut buf, None, 1.0);
	assert_eq!(l(&buf, 1), CONT_TIME_VALID);
	assert_eq!(l(&buf, 0), 0.0);

	plugin.deactivate(audio);
}

#[test]
fn continuous_time_counts_up_until_reset() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	let run = |audio: &mut AudioThread| {
		let mut buf = frames(100, 0.0, 0.0);
		audio.process(&mut buf, None, 1.0);
		r(&buf, 13)
	};

	assert_eq!(run(&mut audio), 0.0);
	assert_eq!(run(&mut audio), 100.0);
	assert_eq!(run(&mut audio), 200.0);

	audio.reset();
	assert_eq!(run(&mut audio), 0.0);

	plugin.deactivate(audio);
}

#[test]
fn long_blocks_are_split_and_the_transport_advances() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	// 600 frames in a plugin that takes 256 at most: 256 + 256 + 88
	let mut buf = frames(600, 0.0, 0.0);
	let status = audio.process(&mut buf, Some(&transport()), 1.0);
	assert_eq!(status, ProcessStatus::Processed);

	assert_eq!(r(&buf, 2), 256.0);
	assert_eq!(r(&buf, 256 + 2), 256.0);
	assert_eq!(r(&buf, 512 + 2), 88.0);

	assert_eq!(r(&buf, 0), 96000.0);
	assert_eq!(r(&buf, 256), 96256.0);
	assert_eq!(r(&buf, 512), 96512.0);

	// at 120 bpm and 48 kHz a quarter note is 24000 samples
	assert_close(l(&buf, 4), 4.5);
	assert_close(l(&buf, 256 + 4), 4.5 + 256.0 / 24000.0);

	// nothing to process is fine
	assert_eq!(audio.process(&mut [], None, 1.0), ProcessStatus::Processed);

	plugin.deactivate(audio);
}

#[test]
fn latency_is_compensated_in_the_dry_signal() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);

	assert_eq!(audio.latency(), 64);
	assert_eq!(audio.update_latency(), 64);
	assert_eq!(audio.tail_samples(), 128);

	// with a mix level of zero only the (delayed) dry signal remains
	let ramp = |start: usize| {
		(start..start + 128)
			.map(|i| [i as f32 + 1.0, -(i as f32) - 1.0])
			.collect::<Vec<_>>()
	};

	let mut first = ramp(0);
	audio.process(&mut first, None, 0.0);
	for (i, frame) in first.iter().enumerate() {
		let expected = if i < 64 { 0.0 } else { (i - 64) as f32 + 1.0 };
		assert_eq!(frame[0], expected, "frame {i}");
	}

	// the delay carries on into the next block
	let mut second = ramp(128);
	audio.process(&mut second, None, 0.0);
	assert_eq!(second[0][0], 128.0 - 64.0 + 1.0);
	assert_eq!(second[127][0], 255.0 - 64.0 + 1.0);

	plugin.deactivate(audio);
}

#[test]
fn activation_can_be_repeated_and_not_doubled() {
	let mut plugin = plugin("Probe");
	assert!(!plugin.is_active());

	let mut audio = activate(&mut plugin);
	assert!(plugin.is_active());
	assert!(matches!(plugin.activate(CONFIG), Err(Error::AlreadyActive)));

	let mut buf = frames(32, 0.0, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	plugin.deactivate(audio);
	assert!(!plugin.is_active());

	// the plugin is picky about the order of calls, and fails `process` if they were wrong
	for _ in 0..5 {
		let mut audio = activate(&mut plugin);
		let mut buf = frames(32, 0.0, 0.0);
		assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
		plugin.deactivate(audio);
	}
}

#[test]
fn invalid_configurations_are_refused() {
	let mut plugin = plugin("Probe");

	for config in [
		AudioConfig {
			max_frames: 0,
			..CONFIG
		},
		AudioConfig {
			sample_rate: 0.0,
			..CONFIG
		},
		AudioConfig {
			sample_rate: f64::NAN,
			..CONFIG
		},
	] {
		assert!(plugin.activate(config).is_err(), "{config:?}");
		assert!(!plugin.is_active());
	}

	// and it still works afterwards
	let audio = activate(&mut plugin);
	plugin.deactivate(audio);
}

#[test]
fn dropping_the_plugin_makes_the_audio_thread_inert() {
	let mut plugin = plugin("Probe");
	let mut audio = activate(&mut plugin);
	drop(plugin);

	// only the dry signal is left, and it's scaled like a failed block would be
	let mut buf = frames(32, 1.0, 1.0);
	assert_eq!(audio.process(&mut buf, None, 0.25), ProcessStatus::Inactive);
	assert_close(buf[0][0], 0.0); // delayed by the latency of 64
	let mut buf = frames(64, 1.0, 1.0);
	assert_eq!(audio.process(&mut buf, None, 0.25), ProcessStatus::Inactive);
	assert_close(buf[63][0], 0.75);
}

#[test]
fn audio_threads_can_move_between_threads() {
	let mut plugin = plugin("Probe");
	let audio = activate(&mut plugin);

	let audio = std::thread::spawn(move || {
		let mut audio = audio;
		let mut buf = frames(64, 0.0, 0.0);
		assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
		audio
	})
	.join()
	.unwrap();

	plugin.deactivate(audio);
}

#[test]
fn plugins_without_a_controller_work() {
	let plugin = plugin("Probe");

	assert!(!plugin.has_controller());
	assert!(plugin.params().is_empty());
	assert!(plugin.param_normalized(0).is_none());
	assert!(plugin.set_param_normalized(0, 0.5).is_none());
}
