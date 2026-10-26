use crate::Error;

const MAGIC: [u8; 4] = *b"V2ST";
const VERSION: u32 = 1;
const HEADER_LEN: usize = 4 + 4 + 4 + 4 + 8;

/// What the plugin needs to come back as it is now.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct State {
	/// The program (preset) the plugin was on.
	pub program: i32,
	/// The values of the parameters, normalized.
	pub params: Vec<f32>,
	/// The plugin's own state, if it has any.
	pub chunk: Vec<u8>,
}

/// Packs the state of a plugin into one blob:
///
/// ```text
/// "V2ST"            4 bytes   magic
/// version           4 bytes   little endian, currently 1
/// program           4 bytes   little endian
/// parameter count   4 bytes   little endian
/// chunk length      8 bytes   little endian
/// parameters        4 bytes each, little endian
/// chunk
/// ```
#[must_use]
pub fn encode(state: &State) -> Vec<u8> {
	let mut blob = Vec::with_capacity(HEADER_LEN + state.params.len() * 4 + state.chunk.len());

	blob.extend_from_slice(&MAGIC);
	blob.extend_from_slice(&VERSION.to_le_bytes());
	blob.extend_from_slice(&state.program.to_le_bytes());
	blob.extend_from_slice(&(state.params.len() as u32).to_le_bytes());
	blob.extend_from_slice(&(state.chunk.len() as u64).to_le_bytes());

	for value in &state.params {
		blob.extend_from_slice(&value.to_le_bytes());
	}

	blob.extend_from_slice(&state.chunk);

	blob
}

/// The inverse of [`encode`].
pub fn decode(blob: &[u8]) -> Result<State, Error> {
	if blob.len() < HEADER_LEN {
		return Err(Error::InvalidState("too short"));
	}

	if blob[..4] != MAGIC {
		return Err(Error::InvalidState("not a VST2 plugin state"));
	}

	let version = u32::from_le_bytes(blob[4..8].try_into().unwrap());
	if version != VERSION {
		return Err(Error::InvalidState("unknown version"));
	}

	let program = i32::from_le_bytes(blob[8..12].try_into().unwrap());
	let param_count = u32::from_le_bytes(blob[12..16].try_into().unwrap()) as usize;
	let chunk_len = u64::from_le_bytes(blob[16..24].try_into().unwrap()) as usize;

	let params_len = param_count
		.checked_mul(4)
		.ok_or(Error::InvalidState("too many parameters"))?;
	let body = &blob[HEADER_LEN..];

	if body.len() != params_len + chunk_len {
		return Err(Error::InvalidState("lengths don't match the data"));
	}

	let (params, chunk) = body.split_at(params_len);

	let params = params
		.as_chunks::<4>()
		.0
		.iter()
		.map(|bytes| f32::from_le_bytes(*bytes))
		.collect();

	Ok(State {
		program,
		params,
		chunk: chunk.to_vec(),
	})
}
