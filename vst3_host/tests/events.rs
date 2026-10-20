#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use raw_window_handle as _;
use std::{path::Path, sync::Arc};
use vst3 as _;
use vst3_host::{AudioConfig, AudioThread, Event, Plugin, ProcessStatus, descriptors};
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

fn frames(len: usize, value: f32) -> Vec<[f32; 2]> {
	vec![[value, value]; len]
}

fn note_on(time: u32, pitch: i16) -> Event {
	Event::NoteOn {
		time,
		channel: 3,
		pitch,
		velocity: 0.75,
		note_id: Some(5),
	}
}

fn note_off(time: u32, pitch: i16) -> Event {
	Event::NoteOff {
		time,
		channel: 3,
		pitch,
		velocity: 0.0,
		note_id: Some(5),
	}
}

fn param(time: u32, id: u32, value: f64) -> Event {
	Event::ParamValue { time, id, value }
}

fn run(audio: &mut AudioThread, len: usize) -> Vec<[f32; 2]> {
	let mut buf = frames(len, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	buf
}

// What the probe reports in its left channel, see `test_plugin/src/extra.rs`.
const EVENT_COUNT: usize = 8;
const FIRST_PITCH: usize = 9;
const FIRST_VELOCITY: usize = 10;
const FIRST_OFFSET: usize = 11;
const FIRST_CHANNEL: usize = 12;
const FIRST_NOTE_ID: usize = 13;
const NOTE_OFFS: usize = 14;
const QUEUE_COUNT: usize = 15;
/// Queue `q` reports `[id, points, first offset, first value, last offset, last value]`.
const fn queue(q: usize) -> usize {
	16 + 6 * q
}

fn l(buf: &[[f32; 2]], index: usize) -> f32 {
	buf[index][0]
}

fn assert_close(a: f32, b: f32) {
	assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

#[test]
fn notes_arrive_with_their_details() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	assert!(audio.push_event(note_on(10, 60)));
	assert!(audio.push_event(note_off(40, 60)));
	let buf = run(&mut audio, 128);

	assert_eq!(l(&buf, EVENT_COUNT), 2.0);
	assert_eq!(l(&buf, FIRST_PITCH), 60.0);
	assert_close(l(&buf, FIRST_VELOCITY), 0.75);
	assert_eq!(l(&buf, FIRST_OFFSET), 10.0);
	assert_eq!(l(&buf, FIRST_CHANNEL), 3.0);
	assert_eq!(l(&buf, FIRST_NOTE_ID), 5.0);
	assert_eq!(l(&buf, NOTE_OFFS), 1.0);

	// events are consumed
	let buf = run(&mut audio, 128);
	assert_eq!(l(&buf, EVENT_COUNT), 0.0);
	assert_eq!(l(&buf, QUEUE_COUNT), 0.0);

	plugin.deactivate(audio);
}

#[test]
fn notes_without_an_id_say_so() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(Event::NoteOn {
		time: 0,
		channel: 0,
		pitch: 64,
		velocity: 1.0,
		note_id: None,
	});
	let buf = run(&mut audio, 64);
	assert_eq!(l(&buf, FIRST_NOTE_ID), -1.0);

	plugin.deactivate(audio);
}

#[test]
fn events_are_sorted_by_time() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	// pushed late first, but the earlier one has to come first
	audio.push_event(note_on(50, 72));
	audio.push_event(note_on(20, 60));
	let buf = run(&mut audio, 128);

	assert_eq!(l(&buf, EVENT_COUNT), 2.0);
	assert_eq!(l(&buf, FIRST_PITCH), 60.0);
	assert_eq!(l(&buf, FIRST_OFFSET), 20.0);

	plugin.deactivate(audio);
}

#[test]
fn parameter_changes_become_ordered_queues() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(param(32, 7, 0.25));
	audio.push_event(param(8, 7, 0.1));
	audio.push_event(param(20, 9, 0.9));
	let buf = run(&mut audio, 128);

	assert_eq!(l(&buf, QUEUE_COUNT), 2.0);

	// queues are in the order of their first change, and points are ordered by time
	assert_eq!(l(&buf, queue(0)), 7.0);
	assert_eq!(l(&buf, queue(0) + 1), 2.0);
	assert_eq!(l(&buf, queue(0) + 2), 8.0);
	assert_close(l(&buf, queue(0) + 3), 0.1);
	assert_eq!(l(&buf, queue(0) + 4), 32.0);
	assert_close(l(&buf, queue(0) + 5), 0.25);

	assert_eq!(l(&buf, queue(1)), 9.0);
	assert_eq!(l(&buf, queue(1) + 1), 1.0);
	assert_eq!(l(&buf, queue(1) + 2), 20.0);
	assert_close(l(&buf, queue(1) + 3), 0.9);

	// parameter changes aren't events
	assert_eq!(l(&buf, EVENT_COUNT), 0.0);

	plugin.deactivate(audio);
}

#[test]
fn the_plugins_output_events_come_back() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(note_on(10, 60));
	audio.push_event(note_on(30, 62));
	audio.push_event(param(5, 1, 0.5));
	run(&mut audio, 128);

	let out = audio.drain_output_events().collect::<Vec<_>>();

	// the probe plays every note an octave higher, and reports parameter 100 at frame 3
	assert!(out.contains(&Event::NoteOn {
		time: 10,
		channel: 3,
		pitch: 72,
		velocity: 0.75,
		note_id: Some(5),
	}));
	assert!(out.contains(&Event::NoteOn {
		time: 30,
		channel: 3,
		pitch: 74,
		velocity: 0.75,
		note_id: Some(5),
	}));
	// one input parameter queue, so the probe says 0.1
	assert!(out.contains(&param(3, 100, 0.1)));
	assert_eq!(out.len(), 3);

	// they're gone once taken
	assert_eq!(audio.drain_output_events().count(), 0);

	// and untaken events don't pile up
	run(&mut audio, 64);
	run(&mut audio, 64);
	assert_eq!(audio.drain_output_events().count(), 1);

	plugin.deactivate(audio);
}

#[test]
fn events_in_long_blocks_go_to_the_right_piece() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	// 600 frames are processed as 256 + 256 + 88
	audio.push_event(note_on(300, 60));
	audio.push_event(param(520, 4, 0.5));
	let buf = run(&mut audio, 600);

	assert_eq!(l(&buf, EVENT_COUNT), 0.0);
	assert_eq!(l(&buf, 256 + EVENT_COUNT), 1.0);
	assert_eq!(l(&buf, 256 + FIRST_OFFSET), 44.0);
	assert_eq!(l(&buf, 512 + EVENT_COUNT), 0.0);

	assert_eq!(l(&buf, QUEUE_COUNT), 0.0);
	assert_eq!(l(&buf, 256 + QUEUE_COUNT), 0.0);
	assert_eq!(l(&buf, 512 + QUEUE_COUNT), 1.0);
	assert_eq!(l(&buf, 512 + queue(0) + 2), 8.0);

	// the output events use the time in the whole block
	let out = audio.drain_output_events().collect::<Vec<_>>();
	assert!(out.iter().any(|e| matches!(
		e,
		Event::NoteOn {
			time: 300,
			pitch: 72,
			..
		}
	)));

	plugin.deactivate(audio);
}

#[test]
fn events_past_the_end_happen_in_the_last_frame() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(note_on(1000, 60));
	let buf = run(&mut audio, 128);

	assert_eq!(l(&buf, EVENT_COUNT), 1.0);
	assert_eq!(l(&buf, FIRST_OFFSET), 127.0);

	plugin.deactivate(audio);
}

#[test]
fn empty_blocks_keep_the_events_for_later() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(note_on(0, 60));
	assert_eq!(audio.process(&mut [], None, 1.0), ProcessStatus::Processed);

	let buf = run(&mut audio, 64);
	assert_eq!(l(&buf, EVENT_COUNT), 1.0);

	plugin.deactivate(audio);
}

#[test]
fn parameter_changes_reach_the_processor() {
	let mut plugin = plugin("Gain");
	let mut audio = plugin.activate(CONFIG).unwrap();

	let mut buf = frames(64, 0.8);
	audio.process(&mut buf, None, 1.0);
	assert_close(buf[0][0], 0.8);

	// parameter 0 is the gain
	audio.push_event(param(0, 0, 0.5));
	let mut buf = frames(64, 0.8);
	audio.process(&mut buf, None, 1.0);
	assert_close(buf[10][0], 0.4);

	// the gain stays where it was put
	let mut buf = frames(64, 0.8);
	audio.process(&mut buf, None, 1.0);
	assert_close(buf[10][0], 0.4);

	plugin.deactivate(audio);
}

#[test]
fn the_last_change_at_the_same_time_wins() {
	let mut plugin = plugin("Gain");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(param(5, 0, 0.2));
	audio.push_event(param(5, 0, 0.7));
	let mut buf = frames(64, 1.0);
	audio.process(&mut buf, None, 1.0);
	assert_close(buf[10][0], 0.7);

	plugin.deactivate(audio);
}

#[test]
fn notes_for_plugins_without_event_inputs_are_ignored() {
	let mut plugin = plugin("Gain");
	let mut audio = plugin.activate(CONFIG).unwrap();
	assert_eq!(audio.layout().event_inputs, 0);

	audio.push_event(note_on(0, 60));
	let mut buf = frames(64, 0.5);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert_eq!(buf[0], [0.5, 0.5]);

	plugin.deactivate(audio);
}

#[test]
fn too_many_events_are_dropped_not_crashed_on() {
	let mut plugin = plugin("Probe");
	let mut audio = plugin.activate(CONFIG).unwrap();

	let accepted = (0..10_000)
		.filter(|&i| audio.push_event(note_on(i % 64, 60)))
		.count();

	assert!(accepted < 10_000);
	assert_eq!(audio.dropped_events(), 10_000 - accepted as u64);

	let buf = run(&mut audio, 64);
	assert!(l(&buf, EVENT_COUNT) > 0.0);

	// many parameters at once overflow the queues, which is counted too
	let before = audio.dropped_events();
	for id in 0..200 {
		audio.push_event(param(0, id, 0.5));
	}
	run(&mut audio, 64);
	assert!(audio.dropped_events() > before);

	plugin.deactivate(audio);
}

#[test]
fn the_synth_gets_notes_on_its_event_bus() {
	let mut plugin = plugin("Synth");
	let mut audio = plugin.activate(CONFIG).unwrap();

	audio.push_event(note_on(0, 60));
	let mut buf = frames(64, 0.0);
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert_eq!(buf[0], [0.25, 0.25]);

	plugin.deactivate(audio);
}
