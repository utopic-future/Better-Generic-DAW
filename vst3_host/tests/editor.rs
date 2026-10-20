#![cfg(not(target_os = "macos"))]

mod common;

use dsp as _;
use libloading as _;
use log as _;
use std::{
	path::Path,
	ptr::{null_mut, without_provenance_mut},
	sync::{Arc, mpsc::Receiver},
};
use vst3 as _;
use vst3_host::{
	EditorSize, Error, MainThreadMessage, ParentWindow, PlatformType, Plugin, descriptors,
};
use walkdir as _;

// what the editor of the `Restarter` plugin reports, see its source
const TRIGGER: u32 = 0;
const ATTACHED: u32 = 3;
const WIDTH: u32 = 4;
const HEIGHT: u32 = 5;
const SCALE: u32 = 6;
const REMOVED: u32 = 7;
const FRAME: u32 = 8;
const PARENT: u32 = 9;

fn plugin(name: &str) -> (Plugin, Receiver<MainThreadMessage>) {
	let path: Arc<Path> = common::bundle().into();
	let descriptor = descriptors(&path)
		.unwrap()
		.into_iter()
		.find(|d| d.name.starts_with(name))
		.unwrap();

	Plugin::new(&descriptor).unwrap()
}

fn window() -> ParentWindow {
	ParentWindow::native(without_provenance_mut(0x1000))
}

fn assert_close(a: f64, b: f64) {
	assert!((a - b).abs() < 1e-6, "{a} != {b}");
}

fn reported(plugin: &Plugin, id: u32) -> f64 {
	plugin.param_normalized(id).unwrap()
}

fn size(width: i32, height: i32) -> EditorSize {
	EditorSize { width, height }
}

#[test]
fn plugins_without_an_editor() {
	// the gain plugin's controller doesn't make views, the others have no controller at all
	for name in ["Gain", "Probe", "Mono", "Synth"] {
		let (mut plugin, _receiver) = plugin(name);

		assert!(matches!(plugin.create_editor(), Err(Error::NoEditor)), "{name}");
		assert!(matches!(plugin.attach_editor(window()), Err(Error::NoEditor)), "{name}");
		assert!(!plugin.has_editor_open());
		assert_eq!(plugin.editor_size(), None);
		assert!(!plugin.editor_can_resize());
		assert!(!plugin.set_editor_scale(2.0));
		assert_eq!(plugin.resize_editor(size(300, 300)), None);

		// closing what isn't there is fine
		plugin.close_editor();
	}
}

#[test]
fn the_editor_has_a_size_before_it_is_shown() {
	let (mut plugin, _receiver) = plugin("Restarter");

	assert_eq!(plugin.create_editor().unwrap(), size(400, 300));
	assert!(plugin.has_editor_open());
	assert_eq!(plugin.editor_size(), Some(size(400, 300)));

	// it's not in a window yet, and doesn't have a frame
	assert_close(reported(&plugin, ATTACHED), 0.0);
	assert_close(reported(&plugin, FRAME), 0.0);
	assert_close(reported(&plugin, PARENT), 0.0);

	// asking again doesn't make another one
	assert!(plugin.set_editor_scale(2.0));
	assert_eq!(plugin.create_editor().unwrap(), size(400, 300));
	assert_close(reported(&plugin, SCALE), 0.5);
}

#[test]
fn attaching_shows_the_editor() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();
	plugin.attach_editor(window()).unwrap();

	assert_close(reported(&plugin, ATTACHED), 1.0);
	assert_close(reported(&plugin, FRAME), 1.0);
	assert_close(reported(&plugin, PARENT), 1.0);
	assert_eq!(plugin.editor_size(), Some(size(400, 300)));

	assert!(matches!(plugin.attach_editor(window()), Err(Error::EditorAttached)));
}

#[test]
fn attaching_can_fail_and_be_tried_again() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();

	// no window at all
	let result = plugin.attach_editor(ParentWindow::native(null_mut()));
	assert!(matches!(result, Err(Error::SetupFailed(..))));
	assert_close(reported(&plugin, ATTACHED), 0.0);
	// the frame doesn't stay behind
	assert_close(reported(&plugin, FRAME), 0.0);

	// a kind of window the plugin doesn't know
	let wrong = ParentWindow {
		handle: without_provenance_mut(0x1000),
		kind: if PlatformType::native() == PlatformType::Hwnd {
			PlatformType::NsView
		} else {
			PlatformType::Hwnd
		},
	};
	assert!(matches!(plugin.attach_editor(wrong), Err(Error::SetupFailed(..))));

	plugin.attach_editor(window()).unwrap();
	assert_close(reported(&plugin, ATTACHED), 1.0);
}

#[test]
fn closing_takes_the_editor_out_of_its_window() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();
	plugin.attach_editor(window()).unwrap();

	plugin.close_editor();

	assert!(!plugin.has_editor_open());
	assert_eq!(plugin.editor_size(), None);
	assert_close(reported(&plugin, ATTACHED), 0.0);
	assert_close(reported(&plugin, REMOVED), 0.01);
	assert_close(reported(&plugin, FRAME), 0.0);

	// once only
	plugin.close_editor();
	assert_close(reported(&plugin, REMOVED), 0.01);
}

#[test]
fn closing_an_editor_that_never_was_shown() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();
	plugin.close_editor();

	// there was nothing to remove from a window
	assert_close(reported(&plugin, REMOVED), 0.0);
	assert!(!plugin.has_editor_open());
}

#[test]
fn the_editor_can_be_opened_again() {
	let (mut plugin, _receiver) = plugin("Restarter");

	for _ in 0..5 {
		assert_eq!(plugin.create_editor().unwrap(), size(400, 300));
		plugin.attach_editor(window()).unwrap();
		assert_close(reported(&plugin, ATTACHED), 1.0);
		plugin.close_editor();
	}
}

#[test]
fn the_host_can_resize_within_what_the_plugin_allows() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();
	plugin.attach_editor(window()).unwrap();
	assert!(plugin.editor_can_resize());

	// the plugin only takes multiples of 10, and nothing smaller than 200 x 100
	assert_eq!(plugin.resize_editor(size(433, 90)), Some(size(430, 100)));
	assert_eq!(plugin.editor_size(), Some(size(430, 100)));
	assert_close(reported(&plugin, WIDTH), 0.43);
	assert_close(reported(&plugin, HEIGHT), 0.1);

	// and nothing bigger than 800 x 600
	assert_eq!(plugin.resize_editor(size(5000, 5000)), Some(size(800, 600)));
	assert_eq!(plugin.resize_editor(size(1, 1)), Some(size(200, 100)));
}

#[test]
fn editors_that_cant_be_resized_stay_as_they_are() {
	let (mut plugin, _receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();
	plugin.attach_editor(window()).unwrap();

	// the plugin turns resizing off
	plugin.set_param_normalized(TRIGGER, 0.7).unwrap();

	assert!(!plugin.editor_can_resize());
	assert_eq!(plugin.resize_editor(size(500, 500)), None);
	assert_eq!(plugin.editor_size(), Some(size(400, 300)));
}

#[test]
fn the_editor_can_ask_for_a_size() {
	let (mut plugin, receiver) = plugin("Restarter");
	plugin.create_editor().unwrap();

	// without a frame there's nobody to ask
	plugin.set_param_normalized(TRIGGER, 0.6).unwrap();
	assert!(receiver.try_recv().is_err());

	plugin.attach_editor(window()).unwrap();
	plugin.set_param_normalized(TRIGGER, 0.6).unwrap();

	let Ok(MainThreadMessage::ResizeEditor { width, height }) = receiver.try_recv() else {
		panic!("the editor should have asked for a size");
	};
	assert_eq!((width, height), (640, 480));
	assert!(receiver.try_recv().is_err());

	// the editor only changes size once the host has resized the window, and says so
	assert_eq!(plugin.editor_size(), Some(size(400, 300)));
	plugin.apply_editor_size(size(width, height));
	assert_eq!(plugin.editor_size(), Some(size(640, 480)));
}

#[test]
fn the_scale_factor_reaches_the_editor() {
	let (mut plugin, _receiver) = plugin("Restarter");
	assert!(!plugin.set_editor_scale(2.0));

	plugin.create_editor().unwrap();
	assert!(plugin.set_editor_scale(2.0));
	assert_close(reported(&plugin, SCALE), 0.5);

	assert!(plugin.set_editor_scale(1.25));
	assert_close(reported(&plugin, SCALE), 0.3125);
}

#[test]
fn dropping_a_plugin_with_an_open_editor_removes_it_first() {
	for _ in 0..20 {
		let (mut plugin, _receiver) = plugin("Restarter");
		plugin.create_editor().unwrap();
		plugin.attach_editor(window()).unwrap();
		drop(plugin);
	}
}

// ---------------------------------------------------------------------------------------------
// windows from a GUI toolkit
// ---------------------------------------------------------------------------------------------

mod windows {
	use raw_window_handle::{
		AppKitWindowHandle, HasWindowHandle, HandleError, RawWindowHandle, WaylandWindowHandle,
		Win32WindowHandle, WindowHandle, XcbWindowHandle, XlibWindowHandle,
	};
	use std::{num::NonZero, ptr::NonNull};
	use vst3_host::{ParentWindow, PlatformType};

	struct Window(RawWindowHandle);

	impl HasWindowHandle for Window {
		fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
			// SAFETY:
			// The handles in these tests are never used as windows.
			Ok(unsafe { WindowHandle::borrow_raw(self.0) })
		}
	}

	#[test]
	fn windows_become_parents() {
		let win32 = Window(RawWindowHandle::Win32(Win32WindowHandle::new(
			NonZero::new(0x1234).unwrap(),
		)));
		let parent = ParentWindow::from_window(&win32).unwrap();
		assert_eq!(parent.handle.addr(), 0x1234);
		assert_eq!(parent.kind, PlatformType::Hwnd);

		let appkit = Window(RawWindowHandle::AppKit(AppKitWindowHandle::new(
			NonNull::new(0x5678 as *mut _).unwrap(),
		)));
		let parent = ParentWindow::from_window(&appkit).unwrap();
		assert_eq!(parent.handle.addr(), 0x5678);
		assert_eq!(parent.kind, PlatformType::NsView);

		let xlib = Window(RawWindowHandle::Xlib(XlibWindowHandle::new(0x9abc)));
		let parent = ParentWindow::from_window(&xlib).unwrap();
		assert_eq!(parent.handle.addr(), 0x9abc);
		assert_eq!(parent.kind, PlatformType::X11EmbedWindowId);

		let xcb = Window(RawWindowHandle::Xcb(XcbWindowHandle::new(
			NonZero::new(0xdef0).unwrap(),
		)));
		let parent = ParentWindow::from_window(&xcb).unwrap();
		assert_eq!(parent.handle.addr(), 0xdef0);
		assert_eq!(parent.kind, PlatformType::X11EmbedWindowId);
	}

	#[test]
	fn windows_editors_cant_go_in_are_refused() {
		let wayland = Window(RawWindowHandle::Wayland(WaylandWindowHandle::new(
			NonNull::new(0x1000 as *mut _).unwrap(),
		)));
		assert_eq!(ParentWindow::from_window(&wayland), None);
	}
}
