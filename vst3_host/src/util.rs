use std::ffi::c_char;

/// Converts a fixed-size, null-terminated `char8` array into a `String`.
pub fn c_array_to_string(array: &[c_char]) -> String {
	let bytes = array
		.iter()
		.take_while(|&&c| c != 0)
		.map(|&c| c as u8)
		.collect::<Vec<_>>();

	String::from_utf8_lossy(&bytes).into_owned()
}

/// Converts a fixed-size, null-terminated UTF-16 array into a `String`.
pub fn utf16_array_to_string(array: &[u16]) -> String {
	let units = array
		.iter()
		.copied()
		.take_while(|&c| c != 0)
		.collect::<Vec<_>>();

	String::from_utf16_lossy(&units)
}

/// `Some(s)` unless `s` is empty.
pub fn non_empty(s: String) -> Option<std::sync::Arc<str>> {
	(!s.is_empty()).then(|| s.into())
}
