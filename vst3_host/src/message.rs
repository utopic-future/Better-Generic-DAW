use std::{
	collections::HashMap,
	ffi::{CStr, CString, c_void},
	sync::Mutex,
};
use vst3::{
	Class, ComWrapper,
	Steinberg::{
		Vst::{
			IAttributeList, IAttributeListTrait, IAttributeList_::AttrID, IMessage,
			IMessageTrait, TChar,
		},
		FIDString, int64, kInvalidArgument, kResultOk, tresult, uint32,
	},
};

#[derive(Clone, Debug, PartialEq)]
enum Attribute {
	Int(i64),
	Float(f64),
	/// UTF-16, without a terminator.
	String(Vec<u16>),
	Binary(Vec<u8>),
}

/// The host's `IAttributeList`: a typed key-value store that plugins use to send data to each
/// other through `IMessage`s.
#[derive(Debug, Default)]
pub struct HostAttributeList {
	attributes: Mutex<HashMap<CString, Attribute>>,
}

impl HostAttributeList {
	fn key(id: AttrID) -> Option<CString> {
		if id.is_null() {
			return None;
		}

		// SAFETY:
		// Attribute ids are null-terminated strings, and we checked for null.
		Some(unsafe { CStr::from_ptr(id) }.to_owned())
	}

	fn set(&self, id: AttrID, attribute: Attribute) -> tresult {
		let Some(key) = Self::key(id) else {
			return kInvalidArgument;
		};

		self.attributes.lock().unwrap().insert(key, attribute);
		kResultOk
	}

	fn with<T>(&self, id: AttrID, f: impl FnOnce(&Attribute) -> Option<T>) -> Option<T> {
		let key = Self::key(id)?;
		f(self.attributes.lock().unwrap().get(&key)?)
	}
}

impl Class for HostAttributeList {
	type Interfaces = (IAttributeList,);
}

#[expect(clippy::renamed_function_params, reason = "the names of the trait's parameters aren't snake case")]
impl IAttributeListTrait for HostAttributeList {
	unsafe fn setInt(&self, id: AttrID, value: int64) -> tresult {
		self.set(id, Attribute::Int(value))
	}

	unsafe fn getInt(&self, id: AttrID, value: *mut int64) -> tresult {
		match self.with(id, |a| if let Attribute::Int(v) = a { Some(*v) } else { None }) {
			Some(v) if !value.is_null() => {
				// SAFETY:
				// `value` is non-null and the caller guarantees it points to an `int64`.
				unsafe { *value = v; }
				kResultOk
			}
			_ => kInvalidArgument,
		}
	}

	unsafe fn setFloat(&self, id: AttrID, value: f64) -> tresult {
		self.set(id, Attribute::Float(value))
	}

	unsafe fn getFloat(&self, id: AttrID, value: *mut f64) -> tresult {
		match self.with(id, |a| if let Attribute::Float(v) = a { Some(*v) } else { None }) {
			Some(v) if !value.is_null() => {
				// SAFETY:
				// `value` is non-null and the caller guarantees it points to an `f64`.
				unsafe { *value = v; }
				kResultOk
			}
			_ => kInvalidArgument,
		}
	}

	unsafe fn setString(&self, id: AttrID, string: *const TChar) -> tresult {
		if string.is_null() {
			return kInvalidArgument;
		}

		let mut len = 0;
		loop {
			// SAFETY:
			// The caller guarantees `string` is null-terminated, so we never go past the end.
			let unit = unsafe { string.add(len) };

			// SAFETY:
			// `unit` is within the null-terminated string.
			if unsafe { *unit } == 0 {
				break;
			}

			len += 1;
		}

		// SAFETY:
		// We just measured `len` valid elements.
		let units = unsafe { std::slice::from_raw_parts(string, len) };
		self.set(id, Attribute::String(units.to_vec()))
	}

	unsafe fn getString(&self, id: AttrID, string: *mut TChar, size_in_bytes: uint32) -> tresult {
		let capacity = size_in_bytes as usize / size_of::<TChar>();

		let Some(units) = self.with(id, |a| {
			if let Attribute::String(s) = a { Some(s.clone()) } else { None }
		}) else {
			return kInvalidArgument;
		};

		// the terminator has to fit as well
		if string.is_null() || capacity == 0 {
			return kInvalidArgument;
		}

		let len = units.len().min(capacity - 1);

		// SAFETY:
		// The caller guarantees `string` has room for `size_in_bytes` bytes, and `len + 1` units
		// fit in that.
		let out = unsafe { std::slice::from_raw_parts_mut(string, len + 1) };
		out[..len].copy_from_slice(&units[..len]);
		out[len] = 0;
		kResultOk
	}

	unsafe fn setBinary(&self, id: AttrID, data: *const c_void, size_in_bytes: uint32) -> tresult {
		let bytes = if data.is_null() || size_in_bytes == 0 {
			Vec::new()
		} else {
			// SAFETY:
			// The caller guarantees `data` points to `size_in_bytes` readable bytes.
			unsafe { std::slice::from_raw_parts(data.cast::<u8>(), size_in_bytes as usize) }
				.to_vec()
		};

		self.set(id, Attribute::Binary(bytes))
	}

	unsafe fn getBinary(
		&self,
		id: AttrID,
		data: *mut *const c_void,
		size_in_bytes: *mut uint32,
	) -> tresult {
		if data.is_null() || size_in_bytes.is_null() {
			return kInvalidArgument;
		}

		let Some((ptr, len)) = self.with(id, |attribute| match attribute {
			Attribute::Binary(bytes) => Some((bytes.as_ptr(), bytes.len())),
			_ => None,
		}) else {
			return kInvalidArgument;
		};

		// SAFETY:
		// `data` is non-null. The pointer stays valid for as long as the attribute isn't
		// overwritten, which is what the interface promises.
		unsafe { *data = ptr.cast(); }

		// SAFETY:
		// `size_in_bytes` is non-null.
		unsafe { *size_in_bytes = len as uint32; }

		kResultOk
	}
}

/// The host's `IMessage`.
pub struct HostMessage {
	id: Mutex<CString>,
	attributes: ComWrapper<HostAttributeList>,
}

impl std::fmt::Debug for HostMessage {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("HostMessage")
			.field("id", &self.id)
			.finish_non_exhaustive()
	}
}

impl Default for HostMessage {
	fn default() -> Self {
		Self::new()
	}
}

impl HostMessage {
	#[must_use]
	pub fn new() -> Self {
		Self {
			id: Mutex::new(CString::default()),
			attributes: ComWrapper::new(HostAttributeList::default()),
		}
	}
}

impl Class for HostMessage {
	type Interfaces = (IMessage,);
}

impl IMessageTrait for HostMessage {
	unsafe fn getMessageID(&self) -> FIDString {
		// The returned pointer stays valid until the id is changed.
		self.id.lock().unwrap().as_ptr()
	}

	unsafe fn setMessageID(&self, id: FIDString) {
		if id.is_null() {
			*self.id.lock().unwrap() = CString::default();
			return;
		}

		// SAFETY:
		// Message ids are null-terminated strings, and we checked for null.
		let id = unsafe { CStr::from_ptr(id) }.to_owned();
		*self.id.lock().unwrap() = id;
	}

	unsafe fn getAttributes(&self) -> *mut IAttributeList {
		self.attributes
			.as_com_ref::<IAttributeList>()
			.map_or(std::ptr::null_mut(), |attributes| attributes.as_ptr())
	}
}
