use crate::{MainThreadMessage, host::LIVE_HOST_OBJECTS};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::{
	ffi::c_void,
	sync::{atomic::Ordering::Relaxed, mpsc::Sender},
};
use vst3::{
	Class,
	Steinberg::{
		FIDString, IPlugFrame, IPlugFrameTrait, IPlugView, ViewRect, kInvalidArgument, kResultOk,
		kPlatformTypeHIView, kPlatformTypeHWND, kPlatformTypeNSView, kPlatformTypeX11EmbedWindowID,
		tresult,
	},
};

/// How big a plugin's editor is, in pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorSize {
	pub width: i32,
	pub height: i32,
}

impl EditorSize {
	pub(crate) const fn from_rect(rect: &ViewRect) -> Self {
		Self {
			width: rect.right.saturating_sub(rect.left),
			height: rect.bottom.saturating_sub(rect.top),
		}
	}

	pub(crate) const fn to_rect(self) -> ViewRect {
		ViewRect {
			left: 0,
			top: 0,
			right: self.width,
			bottom: self.height,
		}
	}
}

/// The kind of native window an editor is put in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlatformType {
	/// A Windows `HWND`.
	Hwnd,
	/// A macOS `NSView`.
	NsView,
	/// An old style macOS `HIView`.
	HiView,
	/// An X11 window id, for embedding.
	X11EmbedWindowId,
}

impl PlatformType {
	/// The kind of window the platform this was built for uses.
	#[must_use]
	pub const fn native() -> Self {
		if cfg!(windows) {
			Self::Hwnd
		} else if cfg!(target_os = "macos") {
			Self::NsView
		} else {
			Self::X11EmbedWindowId
		}
	}

	pub(crate) const fn to_fid(self) -> FIDString {
		match self {
			Self::Hwnd => kPlatformTypeHWND,
			Self::NsView => kPlatformTypeNSView,
			Self::HiView => kPlatformTypeHIView,
			Self::X11EmbedWindowId => kPlatformTypeX11EmbedWindowID,
		}
	}
}

/// The window an editor goes in.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParentWindow {
	/// The `HWND`, `NSView` or window id, as a pointer.
	pub handle: *mut c_void,
	pub kind: PlatformType,
}

impl ParentWindow {
	/// The parent window for an editor, from a window of a GUI toolkit.
	///
	/// This is `None` for windows that editors can't go in, like Wayland ones.
	#[must_use]
	pub fn from_window(window: &impl HasWindowHandle) -> Option<Self> {
		let (handle, kind) = match window.window_handle().ok()?.as_raw() {
			RawWindowHandle::Win32(handle) => (
				std::ptr::without_provenance_mut(handle.hwnd.get() as usize),
				PlatformType::Hwnd,
			),
			RawWindowHandle::AppKit(handle) => (handle.ns_view.as_ptr(), PlatformType::NsView),
			RawWindowHandle::Xlib(handle) => (
				std::ptr::without_provenance_mut(handle.window as usize),
				PlatformType::X11EmbedWindowId,
			),
			RawWindowHandle::Xcb(handle) => (
				std::ptr::without_provenance_mut(handle.window.get() as usize),
				PlatformType::X11EmbedWindowId,
			),
			_ => return None,
		};

		Some(Self { handle, kind })
	}

	/// A window of the kind that is native to the platform.
	#[must_use]
	pub const fn native(handle: *mut c_void) -> Self {
		Self {
			handle,
			kind: PlatformType::native(),
		}
	}
}

/// The host's `IPlugFrame`: the editor tells it when it wants to change its size.
#[derive(Debug)]
pub struct PlugFrame {
	sender: Sender<MainThreadMessage>,
}

impl PlugFrame {
	#[must_use]
	pub fn new(sender: Sender<MainThreadMessage>) -> Self {
		LIVE_HOST_OBJECTS.fetch_add(1, Relaxed);
		Self { sender }
	}
}

impl Drop for PlugFrame {
	fn drop(&mut self) {
		LIVE_HOST_OBJECTS.fetch_sub(1, Relaxed);
	}
}

impl Class for PlugFrame {
	type Interfaces = (IPlugFrame,);
}

#[expect(clippy::renamed_function_params, reason = "the names of the trait's parameters aren't snake case")]
impl IPlugFrameTrait for PlugFrame {
	unsafe fn resizeView(&self, _view: *mut IPlugView, new_size: *mut ViewRect) -> tresult {
		if new_size.is_null() {
			return kInvalidArgument;
		}

		// SAFETY:
		// `new_size` is non-null and points to a `ViewRect`.
		let size = EditorSize::from_rect(unsafe { &*new_size });

		// The window can't be resized from in here, since this is called from inside the plugin
		// and the window belongs to the GUI. So the GUI is told, and does it later.
		let _ = self.sender.send(MainThreadMessage::ResizeEditor {
			width: size.width,
			height: size.height,
		});

		kResultOk
	}
}
