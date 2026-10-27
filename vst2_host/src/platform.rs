use crate::editor::EditorSize;
use std::ffi::c_void;

/// Makes the window the plugin made for its editor as big as the editor wants to be.
#[cfg(windows)]
pub fn resize_child(parent: *mut c_void, size: EditorSize) {
	#[link(name = "user32")]
	unsafe extern "system" {
		fn GetWindow(hwnd: *mut c_void, command: u32) -> *mut c_void;
		fn SetWindowPos(
			hwnd: *mut c_void,
			insert_after: *mut c_void,
			x: i32,
			y: i32,
			width: i32,
			height: i32,
			flags: u32,
		) -> i32;
	}

	/// The first child window of a window.
	const GW_CHILD: u32 = 5;
	/// Don't change the order of the windows.
	const SWP_NOZORDER: u32 = 0x0004;
	/// Don't activate the window.
	const SWP_NOACTIVATE: u32 = 0x0010;

	// SAFETY: the parent is a window that the plugin made its editor in.
	let child = unsafe { GetWindow(parent, GW_CHILD) };

	if child.is_null() {
		return;
	}

	// SAFETY: the child is a window of the plugin.
	unsafe {
		SetWindowPos(
			child,
			std::ptr::null_mut(),
			0,
			0,
			size.width,
			size.height,
			SWP_NOZORDER | SWP_NOACTIVATE,
		);
	}
}

/// Makes the window the plugin made for its editor as big as the editor wants to be.
#[cfg(not(windows))]
pub fn resize_child(_parent: *mut c_void, _size: EditorSize) {
	// The plugin makes its own window on these platforms, so there's nothing to do.
}

/// Fits the plugin's child editor to the host window's current client area.
#[cfg(windows)]
pub fn resize_child_to_parent(parent: *mut c_void) {
	#[repr(C)]
	struct Rect {
		left: i32,
		top: i32,
		right: i32,
		bottom: i32,
	}

	#[link(name = "user32")]
	unsafe extern "system" {
		fn GetWindow(hwnd: *mut c_void, command: u32) -> *mut c_void;
		fn GetClientRect(hwnd: *mut c_void, rect: *mut Rect) -> i32;
		fn SetWindowPos(
			hwnd: *mut c_void,
			insert_after: *mut c_void,
			x: i32,
			y: i32,
			width: i32,
			height: i32,
			flags: u32,
		) -> i32;
	}

	const GW_CHILD: u32 = 5;
	const SWP_NOZORDER: u32 = 0x0004;
	const SWP_NOACTIVATE: u32 = 0x0010;

	let child = unsafe { GetWindow(parent, GW_CHILD) };
	if child.is_null() {
		return;
	}

	let mut rect = Rect {
		left: 0,
		top: 0,
		right: 0,
		bottom: 0,
	};
	if unsafe { GetClientRect(parent, &raw mut rect) } == 0 {
		return;
	}

	unsafe {
		SetWindowPos(
			child,
			std::ptr::null_mut(),
			0,
			0,
			(rect.right - rect.left).max(1),
			(rect.bottom - rect.top).max(1),
			SWP_NOZORDER | SWP_NOACTIVATE,
		);
	}
}

/// The plugin makes its own window on these platforms, so there's nothing to resize.
#[cfg(not(windows))]
pub fn resize_child_to_parent(_parent: *mut c_void) {}
