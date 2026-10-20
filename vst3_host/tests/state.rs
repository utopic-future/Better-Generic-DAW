#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use raw_window_handle as _;
use std::{ffi::c_void, path::Path, ptr::null_mut, sync::Arc};
use vst3::{
	ComWrapper,
	Steinberg::{
		IBStream, IBStream_::IStreamSeekMode_, IBStreamTrait as _, kInvalidArgument, kResultOk,
	},
};
use vst3_host::{AudioConfig, Error, Event, MemoryStream, Plugin, ProcessStatus, descriptors};
use walkdir as _;

const CONFIG: AudioConfig = AudioConfig {
	sample_rate: 48000.0,
	max_frames: 256,
	offline: false,
};

const BYPASS: u32 = 1;
const MODE: u32 = 2;
const SECRET: u32 = 4_000_000_000;

fn plugin(name: &str) -> Plugin {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| d.name.starts_with(name))
		.unwrap();

	Plugin::new(&descriptor).unwrap().0
}

fn assert_close(a: f64, b: f64) {
	assert!((a - b).abs() < 1e-9, "{a} != {b}");
}

/// Runs a block of constant `0.8` through the gain plugin, and returns what comes out.
fn gain_of(plugin: &mut Plugin) -> f32 {
	let mut audio = plugin.activate(CONFIG).unwrap();
	let mut buf = vec![[0.8, 0.8]; 64];
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	plugin.deactivate(audio);
	buf[10][0] / 0.8
}

/// Sets the gain the way a host does: the processor through `process`, the controller directly.
fn set_gain(plugin: &mut Plugin, gain: f64) {
	let mut audio = plugin.activate(CONFIG).unwrap();
	audio.push_event(Event::ParamValue {
		time: 0,
		id: 0,
		value: gain,
	});
	let mut buf = vec![[0.0, 0.0]; 64];
	audio.process(&mut buf, None, 1.0);
	plugin.deactivate(audio);

	plugin.set_param_normalized(0, gain).unwrap();
}

#[test]
fn the_processors_state_comes_back() {
	let mut original = plugin("Gain");
	set_gain(&mut original, 0.5);
	assert!((gain_of(&mut original) - 0.5).abs() < 1e-6);

	let blob = original.save_state().unwrap();

	let mut restored = plugin("Gain");
	assert!((gain_of(&mut restored) - 1.0).abs() < 1e-6);

	restored.load_state(&blob).unwrap();
	assert!((gain_of(&mut restored) - 0.5).abs() < 1e-6);
}

#[test]
fn the_controller_learns_the_components_state() {
	let mut original = plugin("Gain");
	set_gain(&mut original, 0.25);
	let blob = original.save_state().unwrap();

	let restored = plugin("Gain");
	assert_close(restored.param_normalized(0).unwrap(), 1.0);

	// the controller only knows through `setComponentState`
	restored.load_state(&blob).unwrap();
	assert_close(restored.param_normalized(0).unwrap(), 0.25);
}

#[test]
fn the_controllers_own_state_comes_back() {
	let original = plugin("Gain");
	original.set_param_normalized(MODE, 2.0 / 3.0).unwrap();
	original.set_param_normalized(BYPASS, 1.0).unwrap();
	original.set_param_normalized(SECRET, 0.125).unwrap();
	let blob = original.save_state().unwrap();

	let restored = plugin("Gain");
	assert_close(restored.param_normalized(MODE).unwrap(), 0.0);

	restored.load_state(&blob).unwrap();
	assert_close(restored.param_normalized(MODE).unwrap(), 2.0 / 3.0);
	assert_close(restored.param_normalized(BYPASS).unwrap(), 1.0);
	assert_close(restored.param_normalized(SECRET).unwrap(), 0.125);
}

#[test]
fn saving_again_gives_the_same_bytes() {
	let original = plugin("Gain");
	original.set_param_normalized(MODE, 1.0 / 3.0).unwrap();
	let blob = original.save_state().unwrap();

	let restored = plugin("Gain");
	restored.load_state(&blob).unwrap();
	assert_eq!(restored.save_state().unwrap(), blob);
}

#[test]
fn the_blob_format_is_stable() {
	let plugin = plugin("Gain");
	let blob = plugin.save_state().unwrap();

	// 24 bytes of header, 8 bytes for the component (the gain) and 24 for the controller
	assert_eq!(blob.len(), 24 + 8 + 24);
	assert_eq!(&blob[..4], b"V3ST");
	assert_eq!(blob[4..8], 1u32.to_le_bytes());
	assert_eq!(blob[8..16], 8u64.to_le_bytes());
	assert_eq!(blob[16..24], 24u64.to_le_bytes());
	assert_eq!(blob[24..32], 1.0f64.to_le_bytes());
}

#[test]
fn a_state_written_by_hand_loads() {
	// a saved state as of this format version, made without this crate's code
	let mut blob = Vec::new();
	blob.extend_from_slice(b"V3ST");
	blob.extend_from_slice(&1u32.to_le_bytes());
	blob.extend_from_slice(&8u64.to_le_bytes());
	blob.extend_from_slice(&24u64.to_le_bytes());
	blob.extend_from_slice(&0.5f64.to_le_bytes()); // gain
	blob.extend_from_slice(&1.0f64.to_le_bytes()); // bypass
	blob.extend_from_slice(&(1.0f64 / 3.0).to_le_bytes()); // mode
	blob.extend_from_slice(&0.75f64.to_le_bytes()); // secret

	let mut plugin = plugin("Gain");
	plugin.load_state(&blob).unwrap();

	assert!((gain_of(&mut plugin) - 0.5).abs() < 1e-6);
	assert_close(plugin.param_normalized(0).unwrap(), 0.5);
	assert_close(plugin.param_normalized(BYPASS).unwrap(), 1.0);
	assert_close(plugin.param_normalized(MODE).unwrap(), 1.0 / 3.0);
	assert_close(plugin.param_normalized(SECRET).unwrap(), 0.75);
}

#[test]
fn damaged_states_are_refused() {
	let plugin = plugin("Gain");
	let good = plugin.save_state().unwrap();

	let mut bad_magic = good.clone();
	bad_magic[0] = b'X';

	let mut bad_version = good.clone();
	bad_version[4] = 9;

	let mut too_long = good.clone();
	too_long.push(0);

	let mut overflowing = good.clone();
	overflowing[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
	overflowing[16..24].copy_from_slice(&u64::MAX.to_le_bytes());

	let mut too_big = good.clone();
	too_big[8..16].copy_from_slice(&u64::MAX.to_le_bytes());
	too_big[16..24].copy_from_slice(&0u64.to_le_bytes());

	for (name, blob) in [
		("empty", Vec::new()),
		("short", good[..10].to_vec()),
		("header only", good[..24].to_vec()),
		("truncated", good[..good.len() - 1].to_vec()),
		("bad magic", bad_magic),
		("bad version", bad_version),
		("trailing data", too_long),
		("lengths overflow", overflowing),
		("length bigger than data", too_big),
	] {
		assert!(
			matches!(plugin.load_state(&blob), Err(Error::InvalidState(_))),
			"{name}"
		);
	}

	// and none of that broke anything
	plugin.load_state(&good).unwrap();
}

#[test]
fn plugins_with_a_state_the_plugin_rejects_fail_cleanly() {
	let plugin = plugin("Gain");

	// well formed, but the component state is too short for the plugin
	let blob = vst3_host_blob(&[1, 2, 3], &[]);
	assert!(matches!(
		plugin.load_state(&blob),
		Err(Error::StateFailed(..))
	));

	// the same for the controller
	let blob = vst3_host_blob(&[], &[1, 2, 3]);
	assert!(matches!(
		plugin.load_state(&blob),
		Err(Error::StateFailed(..))
	));
}

fn vst3_host_blob(component: &[u8], controller: &[u8]) -> Vec<u8> {
	let mut blob = Vec::new();
	blob.extend_from_slice(b"V3ST");
	blob.extend_from_slice(&1u32.to_le_bytes());
	blob.extend_from_slice(&(component.len() as u64).to_le_bytes());
	blob.extend_from_slice(&(controller.len() as u64).to_le_bytes());
	blob.extend_from_slice(component);
	blob.extend_from_slice(controller);
	blob
}

#[test]
fn plugins_without_state_or_controller() {
	// the probe has no state and no controller
	let probe = plugin("Probe");
	let blob = probe.save_state().unwrap();
	assert_eq!(blob.len(), 24);
	probe.load_state(&blob).unwrap();

	let mono = plugin("Mono");
	mono.load_state(&mono.save_state().unwrap()).unwrap();
}

#[test]
fn a_state_can_be_loaded_while_active() {
	let mut original = plugin("Gain");
	set_gain(&mut original, 0.5);
	let blob = original.save_state().unwrap();

	let mut plugin = plugin("Gain");
	let mut audio = plugin.activate(CONFIG).unwrap();

	let mut buf = vec![[0.8, 0.8]; 64];
	audio.process(&mut buf, None, 1.0);
	assert!((buf[10][0] - 0.8).abs() < 1e-6);

	plugin.load_state(&blob).unwrap();

	let mut buf = vec![[0.8, 0.8]; 64];
	assert_eq!(audio.process(&mut buf, None, 1.0), ProcessStatus::Processed);
	assert!((buf[10][0] - 0.4).abs() < 1e-6);

	plugin.deactivate(audio);
}

// ---------------------------------------------------------------------------------------------
// the stream itself, called the way a plugin would
// ---------------------------------------------------------------------------------------------

fn stream() -> (ComWrapper<MemoryStream>, vst3::ComPtr<IBStream>) {
	let wrapper = ComWrapper::new(MemoryStream::new());
	let ptr = wrapper.to_com_ptr::<IBStream>().unwrap();
	(wrapper, ptr)
}

fn write(ptr: &vst3::ComPtr<IBStream>, bytes: &[u8]) -> i32 {
	let mut written = -1;
	let mut bytes = bytes.to_vec();

	// SAFETY:
	// `bytes` is valid for its length, and `written` is a valid out pointer.
	let result = unsafe {
		ptr.write(
			bytes.as_mut_ptr().cast(),
			bytes.len() as i32,
			&raw mut written,
		)
	};
	assert_eq!(result, kResultOk);
	written
}

fn read(ptr: &vst3::ComPtr<IBStream>, len: usize) -> Vec<u8> {
	let mut buf = vec![0xAA; len];
	let mut read = -1;

	// SAFETY:
	// `buf` is valid for `len` bytes, and `read` is a valid out pointer.
	let result = unsafe { ptr.read(buf.as_mut_ptr().cast(), len as i32, &raw mut read) };
	assert_eq!(result, kResultOk);

	buf.truncate(read as usize);
	buf
}

fn seek(ptr: &vst3::ComPtr<IBStream>, pos: i64, mode: i32) -> Option<i64> {
	let mut result = -1;

	// SAFETY:
	// `result` is a valid out pointer.
	let status = unsafe { ptr.seek(pos, mode, &raw mut result) };
	(status == kResultOk).then_some(result)
}

fn tell(ptr: &vst3::ComPtr<IBStream>) -> i64 {
	let mut pos = -1;

	// SAFETY:
	// `pos` is a valid out pointer.
	assert_eq!(unsafe { ptr.tell(&raw mut pos) }, kResultOk);
	pos
}

#[expect(
	clippy::multiple_unsafe_ops_per_block,
	reason = "these calls pass deliberately invalid pointers to the stream"
)]
fn invalid_arguments(ptr: &vst3::ComPtr<IBStream>) {
	// SAFETY:
	// The pointers are deliberately invalid, the stream has to check for them.
	unsafe {
		assert_eq!(
			ptr.read(null_mut::<c_void>(), 4, null_mut()),
			kInvalidArgument
		);
		assert_eq!(
			ptr.write(null_mut::<c_void>(), 4, null_mut()),
			kInvalidArgument
		);
		assert_eq!(
			ptr.read(null_mut::<c_void>(), -1, null_mut()),
			kInvalidArgument
		);
		assert_eq!(ptr.tell(null_mut()), kInvalidArgument);
	}
}

const SET: i32 = IStreamSeekMode_::kIBSeekSet as i32;
const CUR: i32 = IStreamSeekMode_::kIBSeekCur as i32;
const END: i32 = IStreamSeekMode_::kIBSeekEnd as i32;

#[test]
fn stream_reads_writes_and_seeks() {
	let (wrapper, ptr) = stream();

	assert_eq!(write(&ptr, b"0123456789"), 10);
	assert_eq!(tell(&ptr), 10);

	assert_eq!(seek(&ptr, 0, SET), Some(0));
	assert_eq!(read(&ptr, 4), b"0123");
	assert_eq!(tell(&ptr), 4);

	assert_eq!(seek(&ptr, 2, CUR), Some(6));
	assert_eq!(read(&ptr, 100), b"6789");
	// at the end, reading gives nothing, and that's not an error
	assert_eq!(read(&ptr, 10), b"");

	assert_eq!(seek(&ptr, -2, END), Some(8));
	assert_eq!(read(&ptr, 10), b"89");

	// overwrite in the middle
	assert_eq!(seek(&ptr, 3, SET), Some(3));
	assert_eq!(write(&ptr, b"abc"), 3);
	assert_eq!(wrapper.take(), b"012abc6789");
}

#[test]
fn stream_writing_past_the_end_pads_with_zeroes() {
	let (wrapper, ptr) = stream();

	write(&ptr, b"ab");
	assert_eq!(seek(&ptr, 5, SET), Some(5));
	write(&ptr, b"c");

	assert_eq!(wrapper.take(), b"ab\0\0\0c");
}

#[test]
fn stream_refuses_nonsense() {
	let (_wrapper, ptr) = stream();

	assert_eq!(seek(&ptr, -1, SET), None);
	assert_eq!(seek(&ptr, 0, 99), None);
	assert_eq!(seek(&ptr, i64::MAX, SET), None);
	assert_eq!(seek(&ptr, i64::MAX, END), None);

	invalid_arguments(&ptr);

	// nothing to do is fine, with or without a buffer
	assert_eq!(write(&ptr, b""), 0);
	assert_eq!(read(&ptr, 0), b"");
}

#[test]
fn stream_from_bytes_starts_at_the_beginning() {
	let wrapper = ComWrapper::new(MemoryStream::from_bytes(b"hello"));
	let ptr = wrapper.to_com_ptr::<IBStream>().unwrap();

	assert_eq!(read(&ptr, 3), b"hel");
	wrapper.rewind();
	assert_eq!(read(&ptr, 10), b"hello");
}
