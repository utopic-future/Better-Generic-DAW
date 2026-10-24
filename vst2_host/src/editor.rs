use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::ffi::c_void;

/// How big a plugin's editor is, in pixels.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EditorSize {
	pub width: i32,
	pub height: i32,
}

/// The window an editor goes in.
///
/// A VST2 plugin makes its own window inside the one the host gives it, so all the host
/// has to do is hand over the handle of a window.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ParentWindow {
	/// The `HWND` on Windows, or the window id on Linux.
	pub handle: *mut c_void,
}

impl ParentWindow {
	/// The parent window for an editor, from a window of a GUI toolkit.
	///
	/// This is `None` for windows that editors can't go in, like Wayland ones, and for
	/// macOS, where VST2 editors are Carbon windows that can't be embedded at all.
	#[must_use]
	pub fn from_window(window: &impl HasWindowHandle) -> Option<Self> {
		let handle = match window.window_handle().ok()?.as_raw() {
			RawWindowHandle::Win32(handle) => {
				std::ptr::without_provenance_mut(handle.hwnd.get() as usize)
			}
			RawWindowHandle::Xlib(handle) => {
				std::ptr::without_provenance_mut(handle.window as usize)
			}
			RawWindowHandle::Xcb(handle) => {
				std::ptr::without_provenance_mut(handle.window.get() as usize)
			}
			_ => return None,
		};

		Some(Self { handle })
	}
}
