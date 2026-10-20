//! Tests for the VST3 side of the plugin code, using the small plugins in
//! `vst3_host/test_plugin`.

#[path = "../../vst3_host/tests/common/mod.rs"]
mod common;

use crate::{
	AudioBackend, AudioProcessor, Event,
	plugin::process_vst3,
	time::{BeatRange, BeatTime, SecondsTime},
	Transport,
};
use clap_host::{
	ClapId,
	events::{EventFlags, Match},
};
use std::{collections::HashMap, num::NonZero, path::Path, sync::Arc};
use vst3_host::{AudioConfig, AudioThread, Plugin, descriptors};

fn assert_close(a: f64, b: f64) {
	assert!((a - b).abs() < 1e-4, "{a} != {b}");
}

fn transport() -> Transport {
	let mut transport = Transport::new(
		0,
		NonZero::new(2).unwrap(),
		NonZero::new(48000).unwrap(),
		NonZero::new(256).unwrap(),
	);

	transport.bpm = NonZero::new(120).unwrap();
	transport.numerator = NonZero::new(3).unwrap();
	transport.playing = true;
	transport.position = SecondsTime::from_frames(96000, &transport);

	transport
}

fn activate(name: &str) -> (Plugin, AudioThread) {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| d.name.starts_with(name))
		.unwrap();

	let (mut plugin, _receiver) = Plugin::new(&descriptor).unwrap();
	let audio = plugin
		.activate(AudioConfig {
			sample_rate: 48000.0,
			max_frames: 256,
			offline: false,
		})
		.unwrap();

	(plugin, audio)
}

fn note_on(time: u32, key: u8, note_id: Match<u32>) -> Event {
	Event::On {
		time,
		key,
		velocity: 0.75,
		note_id,
		flags: EventFlags::empty(),
	}
}

fn param(param_id: u32, value: f32) -> Event {
	Event::ParamValue {
		time: 0,
		param_id: ClapId::new(param_id),
		value,
		flags: EventFlags::empty(),
	}
}

// ---------------------------------------------------------------------------------------------
// conversions
// ---------------------------------------------------------------------------------------------

#[test]
fn notes_convert_both_ways() {
	let on = note_on(5, 60, Match::Specific(7));
	assert_eq!(
		on.to_vst3(),
		vst3_host::Event::NoteOn {
			time: 5,
			channel: 0,
			pitch: 60,
			velocity: 0.75,
			note_id: Some(7),
		}
	);
	assert!(matches!(
		Event::from_vst3(on.to_vst3()),
		Some(Event::On {
			time: 5,
			key: 60,
			note_id: Match::Specific(7),
			..
		})
	));

	let off = Event::Off {
		time: 9,
		key: 61,
		velocity: 0.0,
		note_id: Match::All,
		flags: EventFlags::empty(),
	};
	assert_eq!(
		off.to_vst3(),
		vst3_host::Event::NoteOff {
			time: 9,
			channel: 0,
			pitch: 61,
			velocity: 0.0,
			note_id: None,
		}
	);
	assert!(matches!(
		Event::from_vst3(off.to_vst3()),
		Some(Event::Off {
			time: 9,
			key: 61,
			note_id: Match::All,
			..
		})
	));
}

#[test]
fn parameters_convert_both_ways() {
	let event = param(5, 0.25);
	assert_eq!(
		event.to_vst3(),
		vst3_host::Event::ParamValue {
			time: 0,
			id: 5,
			value: 0.25,
		}
	);

	let Some(Event::ParamValue {
		param_id, value, ..
	}) = Event::from_vst3(event.to_vst3())
	else {
		panic!("should convert back");
	};
	assert_eq!(param_id, ClapId::new(5));
	assert_close(f64::from(value), 0.25);
}

#[test]
fn what_cant_be_represented_is_dropped() {
	// CLAP reserves this parameter id
	assert!(
		Event::from_vst3(vst3_host::Event::ParamValue {
			time: 0,
			id: u32::MAX,
			value: 0.5,
		})
		.is_none()
	);

	// notes outside of the MIDI range
	for pitch in [-1, 256, 1000] {
		assert!(
			Event::from_vst3(vst3_host::Event::NoteOn {
				time: 0,
				channel: 0,
				pitch,
				velocity: 1.0,
				note_id: None,
			})
			.is_none(),
			"{pitch}"
		);
	}

	// a note id that isn't an id turns into "all notes", and ids that don't fit are dropped
	assert!(matches!(
		Event::from_vst3(vst3_host::Event::NoteOff {
			time: 0,
			channel: 0,
			pitch: 60,
			velocity: 0.0,
			note_id: Some(-5),
		}),
		Some(Event::Off {
			note_id: Match::All,
			..
		})
	));
	assert_eq!(
		note_on(0, 60, Match::Specific(u32::MAX)).to_vst3(),
		vst3_host::Event::NoteOn {
			time: 0,
			channel: 0,
			pitch: 60,
			velocity: 0.75,
			note_id: None,
		}
	);
}

#[test]
fn the_transport_converts() {
	let mut transport = transport();
	let vst3 = transport.as_vst3();

	assert!(vst3.playing);
	assert_close(vst3.tempo, 120.0);
	assert_eq!(vst3.time_sig_numerator, 3);
	assert_eq!(vst3.time_sig_denominator, 4);
	// 96000 frames at 48 kHz are 2 seconds, which are 4 quarter notes at 120 bpm
	assert!((vst3.position_samples - 96000).abs() <= 1);
	assert_close(vst3.position_quarters, 4.0);
	assert_eq!(vst3.cycle, None);

	transport.loop_range = Some(BeatRange::new(
		BeatTime::from_float(2.0),
		BeatTime::from_float(6.0),
	));
	transport.playing = false;
	let vst3 = transport.as_vst3();

	assert!(!vst3.playing);
	let (start, end) = vst3.cycle.unwrap();
	assert_close(start, 2.0);
	assert_close(end, 6.0);
}

// ---------------------------------------------------------------------------------------------
// processing
// ---------------------------------------------------------------------------------------------

#[test]
fn parameters_reach_the_plugin() {
	let (mut plugin, mut audio) = activate("Gain");
	let mut updates = HashMap::new();
	let transport = transport();

	let mut events = vec![param(0, 0.5)];
	let mut buf = vec![[0.8, 0.8]; 64];
	let latency = process_vst3(&mut audio, &mut updates, &transport, &mut buf, &mut events, 1.0);

	assert!((buf[10][0] - 0.4).abs() < 1e-6);
	assert_eq!(latency, 0);
	// the events were consumed and the plugin made none of its own
	assert!(events.is_empty());
	assert!(updates.is_empty());

	// the mix level is passed on
	let mut buf = vec![[0.8, 0.8]; 64];
	process_vst3(&mut audio, &mut updates, &transport, &mut buf, &mut events, 0.0);
	assert!((buf[10][0] - 0.8).abs() < 1e-6);

	plugin.deactivate(audio);
}

#[test]
fn the_plugins_own_events_come_back() {
	let (mut plugin, mut audio) = activate("Probe");
	let mut updates = HashMap::new();
	let transport = transport();

	let mut events = vec![note_on(10, 60, Match::Specific(5)), param(7, 0.3)];
	let mut buf = vec![[0.0, 0.0]; 128];
	let latency = process_vst3(&mut audio, &mut updates, &transport, &mut buf, &mut events, 1.0);

	// the probe latency
	assert_eq!(latency, 64);

	// it plays every note an octave higher, which goes on to whatever comes next
	assert_eq!(events.len(), 1);
	assert!(matches!(
		events[0],
		Event::On {
			time: 10,
			key: 72,
			note_id: Match::Specific(5),
			..
		}
	));

	// and it changed parameter 100 on its own, which is for the UI
	assert_eq!(updates.len(), 1);
	assert_close(f64::from(updates[&ClapId::new(100)]), 0.1);

	plugin.deactivate(audio);
}

#[test]
fn the_plugin_sees_the_transport() {
	let (mut plugin, mut audio) = activate("Probe");
	let mut updates = HashMap::new();

	let mut events = Vec::new();
	let mut buf = vec![[0.0, 0.0]; 64];
	process_vst3(&mut audio, &mut updates, &transport(), &mut buf, &mut events, 1.0);

	// the probe reports what it got: tempo, time signature, position in quarters and samples
	assert_close(f64::from(buf[0][0]), 120.0);
	assert_close(f64::from(buf[2][0]), 3.0);
	assert_close(f64::from(buf[3][0]), 4.0);
	assert_close(f64::from(buf[4][0]), 4.0);
	assert!((f64::from(buf[0][1]) - 96000.0).abs() <= 1.0);

	plugin.deactivate(audio);
}

#[test]
fn latency_changes_are_handled_in_place() {
	let (mut plugin, mut audio) = activate("Restarter");
	let mut updates = HashMap::new();
	let transport = transport();

	let mut buf = vec![[0.5, 0.5]; 64];
	let before = process_vst3(&mut audio, &mut updates, &transport, &mut buf, &mut Vec::new(), 1.0);
	assert_eq!(before, 0);

	// the plugin asks for this from the audio thread, and gets it without a restart
	let mut events = vec![param(0, 0.9)];
	let after = process_vst3(&mut audio, &mut updates, &transport, &mut buf, &mut events, 1.0);
	assert_eq!(after, 16);
	assert!(!audio.restart_flags().needs_restart());
	assert_eq!(audio.restart_flags().0, 0);

	plugin.deactivate(audio);
}

// ---------------------------------------------------------------------------------------------
// the processor
// ---------------------------------------------------------------------------------------------

#[test]
fn restarts_are_noticed() {
	let (mut plugin, audio) = activate("Restarter");
	let (processor, _consumer) = AudioProcessor::create_vst3(audio, plugin.params().len());

	assert!(!processor.needs_restart());
	assert_eq!(processor.sample_rate(), 48000);
	assert_eq!(processor.max_frames(), 256);

	// the plugin turns mono, which needs the audio to be set up again
	plugin.set_param_normalized(0, 0.2).unwrap();
	assert!(processor.needs_restart());

	let AudioBackend::Vst3(audio) = processor.into_backend() else {
		panic!("should still be a VST3 plugin");
	};
	let audio = plugin.restart(audio).unwrap();
	let (processor, _consumer) = AudioProcessor::create_vst3(audio, plugin.params().len());
	assert!(!processor.needs_restart());

	let AudioBackend::Vst3(audio) = processor.into_backend() else {
		panic!("should still be a VST3 plugin");
	};
	plugin.deactivate(audio);
}

#[test]
#[should_panic(expected = "not a CLAP plugin")]
fn a_vst3_processor_is_not_a_clap_one() {
	let (_plugin, audio) = activate("Gain");
	let (processor, _consumer) = AudioProcessor::create_vst3(audio, 5);
	let _ = processor.into_clap();
}

#[test]
fn processors_work_without_parameters() {
	let (mut plugin, audio) = activate("Probe");
	assert!(plugin.params().is_empty());

	let (processor, mut consumer) = AudioProcessor::create_vst3(audio, 0);
	assert!(consumer.pop().is_err());

	let AudioBackend::Vst3(audio) = processor.into_backend() else {
		panic!("should be a VST3 plugin");
	};
	plugin.deactivate(audio);
}
