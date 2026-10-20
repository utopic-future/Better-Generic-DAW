#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use raw_window_handle as _;
use log as _;
use std::{path::Path, sync::Arc};
use vst3 as _;
use vst3_host::{ParamFlags, ParamInfo, Plugin, descriptors};
use walkdir as _;

const BYPASS: u32 = 1;
const MODE: u32 = 2;
const METER: u32 = 3;
const SECRET: u32 = 4_000_000_000;

fn plugin() -> Plugin {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path).unwrap().remove(0);
	Plugin::new(&descriptor).unwrap().0
}

fn assert_close(a: f64, b: f64) {
	assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

#[test]
fn lists_all_parameters_in_order() {
	let plugin = plugin();

	let ids = plugin.params().iter().map(|p| p.id).collect::<Vec<_>>();
	assert_eq!(ids, [0, BYPASS, MODE, METER, SECRET]);

	let titles = plugin.params().iter().map(|p| &*p.title).collect::<Vec<_>>();
	assert_eq!(titles, ["Gain", "Bypass", "Mode", "Meter", "Secret"]);
}

#[test]
fn reads_flags_steps_and_units() {
	let plugin = plugin();

	let gain = plugin.param(0).unwrap();
	assert!(gain.can_automate() && !gain.is_read_only() && gain.is_visible());
	assert!(!gain.is_discrete());
	assert_close(gain.default_normalized, 1.0);

	let bypass = plugin.param(BYPASS).unwrap();
	assert!(bypass.is_bypass() && bypass.is_discrete());
	assert_eq!(bypass.step_count, 1);

	let mode = plugin.param(MODE).unwrap();
	assert!(mode.is_list() && mode.is_discrete());
	assert_eq!(mode.step_count, 3);

	let meter = plugin.param(METER).unwrap();
	assert!(meter.is_read_only() && !meter.can_automate());
	assert_eq!(&*meter.units, "dB");

	let secret = plugin.param(SECRET).unwrap();
	assert!(secret.is_hidden() && !secret.is_visible());

	assert!(plugin.param(12345).is_none());
}

#[test]
fn initial_values_match_defaults() {
	let plugin = plugin();

	for param in plugin.params() {
		assert_close(
			plugin.param_normalized(param.id).unwrap(),
			param.default_normalized,
		);
	}
}

#[test]
fn set_and_get_with_clamping() {
	let plugin = plugin();

	assert_close(plugin.set_param_normalized(0, 0.25).unwrap(), 0.25);
	assert_close(plugin.param_normalized(0).unwrap(), 0.25);

	assert_close(plugin.set_param_normalized(0, 5.0).unwrap(), 1.0);
	assert_close(plugin.param_normalized(0).unwrap(), 1.0);

	assert_close(plugin.set_param_normalized(0, -3.0).unwrap(), 0.0);
	assert_close(plugin.param_normalized(0).unwrap(), 0.0);

	// NaN must never reach the plugin
	assert_close(plugin.set_param_normalized(0, f64::NAN).unwrap(), 1.0);
}

#[test]
fn discrete_values_snap_to_steps() {
	let plugin = plugin();

	assert_close(plugin.set_param_normalized(MODE, 0.4).unwrap(), 1.0 / 3.0);
	assert_close(plugin.param_normalized(MODE).unwrap(), 1.0 / 3.0);

	assert_close(plugin.set_param_normalized(BYPASS, 0.6).unwrap(), 1.0);
	assert_close(plugin.set_param_normalized(BYPASS, 0.4).unwrap(), 0.0);
}

#[test]
fn read_only_and_unknown_parameters_are_refused() {
	let plugin = plugin();
	let before = plugin.param_normalized(METER).unwrap();

	assert!(plugin.set_param_normalized(METER, 0.9).is_none());
	assert_close(plugin.param_normalized(METER).unwrap(), before);

	assert!(plugin.set_param_normalized(12345, 0.5).is_none());
	assert!(plugin.param_normalized(12345).is_none());
	assert!(plugin.param_display(12345, 0.5).is_none());
	assert!(plugin.param_from_display(12345, "1").is_none());
	assert!(plugin.normalized_to_plain(12345, 0.5).is_none());
	assert!(plugin.plain_to_normalized(12345, 0.5).is_none());
}

#[test]
fn display_strings() {
	let plugin = plugin();

	assert_eq!(plugin.param_display(MODE, 1.0 / 3.0).unwrap(), "Warm");
	assert_eq!(plugin.param_display(MODE, 1.0).unwrap(), "Fuzz");
	assert_eq!(plugin.param_display(BYPASS, 0.0).unwrap(), "Off");
	assert_eq!(plugin.param_display(BYPASS, 1.0).unwrap(), "On");
	assert_eq!(plugin.param_display(METER, 0.5).unwrap(), "-30.0");
	assert_eq!(plugin.param_display(0, 0.5).unwrap(), "0.5");
}

#[test]
fn parsing_display_strings() {
	let plugin = plugin();

	assert_close(plugin.param_from_display(MODE, "Hot").unwrap(), 2.0 / 3.0);
	assert_close(plugin.param_from_display(MODE, "Fuzz").unwrap(), 1.0);
	assert!(plugin.param_from_display(MODE, "Nope").is_none());
	assert_close(plugin.param_from_display(METER, "-30").unwrap(), 0.5);
	assert_close(plugin.param_from_display(0, "0.75").unwrap(), 0.75);

	// values the plugin returns out of range get clamped
	assert_close(plugin.param_from_display(0, "7").unwrap(), 1.0);
	assert!(plugin.param_from_display(0, "banana").is_none());
}

#[test]
fn plain_conversions() {
	let plugin = plugin();

	assert_close(plugin.normalized_to_plain(MODE, 2.0 / 3.0).unwrap(), 2.0);
	assert_close(plugin.plain_to_normalized(MODE, 3.0).unwrap(), 1.0);
	assert_close(plugin.normalized_to_plain(METER, 0.5).unwrap(), -30.0);
	assert_close(plugin.plain_to_normalized(METER, -60.0).unwrap(), 0.0);
}

fn info(step_count: i32, default_normalized: f64) -> ParamInfo {
	ParamInfo {
		id: 0,
		title: "".into(),
		short_title: "".into(),
		units: "".into(),
		step_count,
		default_normalized,
		unit_id: 0,
		flags: ParamFlags(0),
	}
}

#[test]
fn snap_without_a_plugin() {
	let continuous = info(0, 0.5);
	assert_close(continuous.snap(0.3), 0.3);
	assert_close(continuous.snap(2.0), 1.0);
	assert_close(continuous.snap(f64::NAN), 0.5);

	let switch = info(1, 0.0);
	assert_close(switch.snap(0.49), 0.0);
	assert_close(switch.snap(0.51), 1.0);

	let four = info(4, 0.0);
	assert_close(four.snap(0.3), 0.25);
	assert_close(four.snap(0.9), 1.0);
	assert_close(four.snap(0.62), 0.5);
	assert_close(four.snap(0.7), 0.75);
}
