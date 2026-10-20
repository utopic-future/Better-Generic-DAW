use crate::Error;

const MAGIC: [u8; 4] = *b"V3ST";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 4 + 4 + 8 + 8;

/// Packs the two halves of a plugin's state into one blob:
///
/// ```text
/// "V3ST"            4 bytes   magic
/// version           4 bytes   little endian, currently 1
/// component length  8 bytes   little endian
/// controller length 8 bytes   little endian
/// component state
/// controller state
/// ```
pub fn encode(component: &[u8], controller: &[u8]) -> Vec<u8> {
	let mut blob = Vec::with_capacity(HEADER_LEN + component.len() + controller.len());

	blob.extend_from_slice(&MAGIC);
	blob.extend_from_slice(&VERSION.to_le_bytes());
	blob.extend_from_slice(&(component.len() as u64).to_le_bytes());
	blob.extend_from_slice(&(controller.len() as u64).to_le_bytes());
	blob.extend_from_slice(component);
	blob.extend_from_slice(controller);

	blob
}

/// The inverse of [`encode`], as `(component, controller)`.
pub fn decode(blob: &[u8]) -> Result<(&[u8], &[u8]), Error> {
	if blob.len() < HEADER_LEN {
		return Err(Error::InvalidState("too short"));
	}

	if blob[..4] != MAGIC {
		return Err(Error::InvalidState("not a VST3 plugin state"));
	}

	let version = u32::from_le_bytes(blob[4..8].try_into().unwrap());
	if version != VERSION {
		return Err(Error::InvalidState("unknown version"));
	}

	let component_len = u64::from_le_bytes(blob[8..16].try_into().unwrap());
	let controller_len = u64::from_le_bytes(blob[16..24].try_into().unwrap());
	let body = &blob[HEADER_LEN..];

	let lengths_fit = component_len
		.checked_add(controller_len)
		.is_some_and(|total| total == body.len() as u64);
	if !lengths_fit {
		return Err(Error::InvalidState("lengths don't match the data"));
	}

	Ok(body.split_at(component_len as usize))
}
