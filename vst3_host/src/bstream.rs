use std::{
	cell::{Cell, RefCell},
	ffi::c_void,
};
use vst3::{
	Class,
	Steinberg::{
		IBStream, IBStream_::IStreamSeekMode_, IBStreamTrait, int32, int64, kInvalidArgument,
		kResultFalse, kResultOk, tresult,
	},
};

/// The most a stream grows to, so a misbehaving plugin can't eat all memory.
pub const MAX_STREAM_BYTES: usize = 256 * 1024 * 1024;

/// An `IBStream` in memory, which plugins read and write their state through.
#[derive(Debug)]
pub struct MemoryStream {
	data: RefCell<Vec<u8>>,
	position: Cell<usize>,
}

impl MemoryStream {
	/// An empty stream.
	#[must_use]
	pub fn new() -> Self {
		Self::from_bytes(&[])
	}

	/// A stream that reads `bytes`, starting at the beginning.
	#[must_use]
	pub fn from_bytes(bytes: &[u8]) -> Self {
		Self {
			data: RefCell::new(bytes.to_vec()),
			position: Cell::new(0),
		}
	}

	/// Takes everything that was written, leaving the stream empty.
	pub fn take(&self) -> Vec<u8> {
		self.position.set(0);
		std::mem::take(&mut *self.data.borrow_mut())
	}

	/// How many bytes the stream holds.
	#[must_use]
	pub fn byte_len(&self) -> usize {
		self.data.borrow().len()
	}

	/// Goes back to the beginning, to read what was just written.
	pub fn rewind(&self) {
		self.position.set(0);
	}
}

impl Default for MemoryStream {
	fn default() -> Self {
		Self::new()
	}
}

impl Class for MemoryStream {
	type Interfaces = (IBStream,);
}

#[expect(
	clippy::renamed_function_params,
	reason = "the names of the trait's parameters aren't snake case"
)]
impl IBStreamTrait for MemoryStream {
	unsafe fn read(
		&self,
		buffer: *mut c_void,
		num_bytes: int32,
		num_bytes_read: *mut int32,
	) -> tresult {
		let Ok(wanted) = usize::try_from(num_bytes) else {
			return kInvalidArgument;
		};

		if buffer.is_null() && wanted > 0 {
			return kInvalidArgument;
		}

		let data = self.data.borrow();
		let position = self.position.get();
		let available = data.len().saturating_sub(position);
		let count = wanted.min(available);

		if count > 0 {
			// SAFETY:
			// `buffer` is non-null and has room for `wanted >= count` bytes, and `data` holds at
			// least `count` bytes after `position`. They can't overlap, `data` is ours.
			unsafe {
				std::ptr::copy_nonoverlapping(
					data[position..].as_ptr(),
					buffer.cast::<u8>(),
					count,
				);
			}
		}

		self.position.set(position + count);

		if !num_bytes_read.is_null() {
			// SAFETY:
			// `num_bytes_read` is non-null.
			unsafe {
				*num_bytes_read = count as int32;
			}
		}

		kResultOk
	}

	unsafe fn write(
		&self,
		buffer: *mut c_void,
		num_bytes: int32,
		num_bytes_written: *mut int32,
	) -> tresult {
		let Ok(count) = usize::try_from(num_bytes) else {
			return kInvalidArgument;
		};

		if buffer.is_null() && count > 0 {
			return kInvalidArgument;
		}

		let position = self.position.get();
		let Some(end) = position
			.checked_add(count)
			.filter(|&end| end <= MAX_STREAM_BYTES)
		else {
			return kResultFalse;
		};

		let mut data = self.data.borrow_mut();

		// writing after the end leaves a gap of zeroes
		if data.len() < end {
			data.resize(end, 0);
		}

		if count > 0 {
			// SAFETY:
			// `buffer` is non-null and holds `count` bytes, and `data` has room for `count`
			// bytes at `position` after the resize above.
			unsafe {
				std::ptr::copy_nonoverlapping(
					buffer.cast::<u8>(),
					data[position..].as_mut_ptr(),
					count,
				);
			}
		}

		self.position.set(end);

		if !num_bytes_written.is_null() {
			// SAFETY:
			// `num_bytes_written` is non-null.
			unsafe {
				*num_bytes_written = count as int32;
			}
		}

		kResultOk
	}

	unsafe fn seek(&self, pos: int64, mode: int32, result: *mut int64) -> tresult {
		let base = match mode {
			m if m == IStreamSeekMode_::kIBSeekSet as int32 => 0,
			m if m == IStreamSeekMode_::kIBSeekCur as int32 => self.position.get() as i64,
			m if m == IStreamSeekMode_::kIBSeekEnd as int32 => self.data.borrow().len() as i64,
			_ => return kInvalidArgument,
		};

		let Some(target) = base
			.checked_add(pos)
			.and_then(|target| usize::try_from(target).ok())
			.filter(|&target| target <= MAX_STREAM_BYTES)
		else {
			return kInvalidArgument;
		};

		self.position.set(target);

		if !result.is_null() {
			// SAFETY:
			// `result` is non-null.
			unsafe {
				*result = target as int64;
			}
		}

		kResultOk
	}

	unsafe fn tell(&self, pos: *mut int64) -> tresult {
		if pos.is_null() {
			return kInvalidArgument;
		}

		// SAFETY:
		// `pos` is non-null.
		unsafe {
			*pos = self.position.get() as int64;
		}
		kResultOk
	}
}
