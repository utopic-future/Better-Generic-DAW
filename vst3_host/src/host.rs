use crate::{
	message::{HostAttributeList, HostMessage},
	module::tuid_to_bytes,
};
use std::{
	ffi::c_void,
	sync::atomic::{AtomicUsize, Ordering::Relaxed},
};
use vst3::{
	Class, ComWrapper, Interface as _,
	Steinberg::{
		FUnknown, TUID,
		Vst::{IAttributeList, IHostApplication, IHostApplicationTrait, IMessage, String128},
		kInvalidArgument, kResultOk, tresult,
	},
};

/// How many host-side COM objects (`HostApplication` and `ComponentHandler`) are alive.
pub static LIVE_HOST_OBJECTS: AtomicUsize = AtomicUsize::new(0);

/// The number of host objects that are currently alive, for leak tests.
#[doc(hidden)]
#[must_use]
pub fn live_host_objects() -> usize {
	LIVE_HOST_OBJECTS.load(Relaxed)
}

/// The name plugins see through `IHostApplication::getName`.
pub const HOST_NAME: &str = "Generic DAW";

/// The host context handed to plugins in `initialize`.
#[derive(Debug)]
pub struct HostApplication;

impl HostApplication {
	#[must_use]
	pub fn new() -> Self {
		LIVE_HOST_OBJECTS.fetch_add(1, Relaxed);
		Self
	}
}

impl Default for HostApplication {
	fn default() -> Self {
		Self::new()
	}
}

impl Drop for HostApplication {
	fn drop(&mut self) {
		LIVE_HOST_OBJECTS.fetch_sub(1, Relaxed);
	}
}

impl Class for HostApplication {
	type Interfaces = (IHostApplication,);
}

impl IHostApplicationTrait for HostApplication {
	unsafe fn getName(&self, name: *mut String128) -> tresult {
		if name.is_null() {
			return kInvalidArgument;
		}

		// SAFETY:
		// `name` is non-null and points to a `String128`.
		let name = unsafe { &mut *name };

		let mut len = 0;
		for (dst, src) in name.iter_mut().zip(HOST_NAME.encode_utf16()) {
			*dst = src;
			len += 1;
		}
		name[len] = 0;

		kResultOk
	}

	unsafe fn createInstance(
		&self,
		cid: *mut TUID,
		iid: *mut TUID,
		obj: *mut *mut c_void,
	) -> tresult {
		if cid.is_null() || iid.is_null() || obj.is_null() {
			return kInvalidArgument;
		}

		// SAFETY:
		// `cid` is non-null and points to a `TUID`.
		let cid = tuid_to_bytes(unsafe { &*cid });

		// The VST3 SDK asks for messages and attribute lists by passing the interface id as the
		// class id.
		let instance = if cid == IMessage::IID {
			ComWrapper::new(HostMessage::new()).to_com_ptr::<FUnknown>()
		} else if cid == IAttributeList::IID {
			ComWrapper::new(HostAttributeList::default()).to_com_ptr::<FUnknown>()
		} else {
			None
		};

		let Some(instance) = instance else {
			return kInvalidArgument;
		};

		let ptr = instance.as_ptr();

		// SAFETY:
		// `ptr` is a valid `FUnknown`, and the caller guarantees `iid` and `obj` are valid.
		let vtbl = unsafe { (*ptr).vtbl };

		// SAFETY:
		// `vtbl` is the valid vtable of `ptr`.
		let query_interface = unsafe { (*vtbl).queryInterface };

		// SAFETY:
		// `query_interface` belongs to `ptr`, and the caller guarantees `iid` and `obj` are valid.
		unsafe { query_interface(ptr, iid, obj) }
	}
}
