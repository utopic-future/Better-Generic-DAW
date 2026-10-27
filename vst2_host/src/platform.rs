use crate::editor::EditorSize;
use std::ffi::c_void;

/// Lists the direct child windows of the host window.
#[cfg(windows)]
pub fn child_windows(parent: *mut c_void) -> Vec<*mut c_void> {
	#[link(name = "user32")]
	unsafe extern "system" {
		fn GetWindow(hwnd: *mut c_void, command: u32) -> *mut c_void;
	}

	const GW_CHILD: u32 = 5;
	const GW_HWNDNEXT: u32 = 2;

	let mut children = Vec::new();
	let mut child = unsafe { GetWindow(parent, GW_CHILD) };

	while !child.is_null() {
		children.push(child);
		child = unsafe { GetWindow(child, GW_HWNDNEXT) };
	}

	children
}

/// The plugin makes its own window on these platforms, so there are no child windows to list.
#[cfg(not(windows))]
pub fn child_windows(_parent: *mut c_void) -> Vec<*mut c_void> {
	Vec::new()
}

/// Finds the direct child window the plugin created while opening its editor.
pub fn newly_created_child(
	parent: *mut c_void,
	previous_children: &[*mut c_void],
) -> Option<*mut c_void> {
	child_windows(parent)
		.into_iter()
		.find(|child| !previous_children.contains(child))
}

/// Makes the plugin's editor window the size the plugin requested.
#[cfg(windows)]
pub fn resize_child(child: *mut c_void, size: EditorSize) {
	#[link(name = "user32")]
	unsafe extern "system" {
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

	const SWP_NOZORDER: u32 = 0x0004;
	const SWP_NOACTIVATE: u32 = 0x0010;

	if !child.is_null() {
		unsafe {
			SetWindowPos(
				child,
				std::ptr::null_mut(),
				0,
				0,
				size.width.max(1),
				size.height.max(1),
				SWP_NOZORDER | SWP_NOACTIVATE,
			);
		}
	}
}

/// The plugin makes its own window on these platforms, so there's nothing to resize.
#[cfg(not(windows))]
pub fn resize_child(_child: *mut c_void, _size: EditorSize) {}

/// Fits the plugin's child editor to the host window's current client area.
#[cfg(windows)]
pub fn resize_child_to_parent(parent: *mut c_void, child: *mut c_void) {
	#[repr(C)]
	struct Rect {
		left: i32,
		top: i32,
		right: i32,
		bottom: i32,
	}

	#[link(name = "user32")]
	unsafe extern "system" {
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

	const SWP_NOZORDER: u32 = 0x0004;
	const SWP_NOACTIVATE: u32 = 0x0010;

	if parent.is_null() || child.is_null() {
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
pub fn resize_child_to_parent(_parent: *mut c_void, _child: *mut c_void) {}
