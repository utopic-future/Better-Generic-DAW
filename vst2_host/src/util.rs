use std::{os::raw::c_char, sync::Arc};

/// Reads a string that a plugin wrote into a buffer.
///
/// The buffer doesn't have to be null-terminated, everything after the first null is
/// ignored.
#[must_use]
pub fn c_array_to_string(array: &[c_char]) -> String {
	let bytes = array
		.iter()
		.take_while(|c| **c != 0)
		.map(|c| *c as u8)
		.collect::<Vec<_>>();

	String::from_utf8_lossy(&bytes).into_owned()
}

/// A string that isn't empty, or nothing.
#[must_use]
pub fn non_empty(string: String) -> Option<Arc<str>> {
	if string.trim().is_empty() {
		None
	} else {
		Some(string.into())
	}
}
