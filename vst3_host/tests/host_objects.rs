#![expect(
	clippy::multiple_unsafe_ops_per_block,
	reason = "these tests call into COM interfaces, where every call is unsafe"
)]

mod common;

use dsp as _;
use libloading as _;
use raw_window_handle as _;
use log as _;
use std::{
	ffi::{CStr, c_void},
	ptr::{null, null_mut},
	sync::mpsc,
};
use vst3::{
	ComPtr, ComRef, ComWrapper, Interface as _,
	Steinberg::{
		Vst::{
			IAttributeList, IAttributeListTrait as _, IComponentHandler,
			IComponentHandlerTrait as _, IHostApplication, IHostApplicationTrait as _, IMessage,
			IMessageTrait as _, String128,
		},
		kInvalidArgument, kResultOk,
	},
};
use vst3_host::{
	ComponentHandler, HOST_NAME, HostApplication, MainThreadMessage, RestartFlags,
};
use walkdir as _;

fn host() -> ComPtr<IHostApplication> {
	ComWrapper::new(HostApplication::new())
		.to_com_ptr::<IHostApplication>()
		.unwrap()
}

fn create_message(host: &ComPtr<IHostApplication>) -> ComPtr<IMessage> {
	let mut cid = IMessage::IID.map(|b| b as _);
	let mut iid = IMessage::IID.map(|b| b as _);
	let mut obj = null_mut::<c_void>();

	// SAFETY:
	// All pointers are valid for the duration of the call.
	let result = unsafe { host.createInstance(&raw mut cid, &raw mut iid, &raw mut obj) };
	assert_eq!(result, kResultOk);

	// SAFETY:
	// A successful `createInstance` returns an owned `IMessage`.
	unsafe { ComPtr::from_raw(obj.cast::<IMessage>()) }.unwrap()
}

fn utf16(s: &str) -> Vec<u16> {
	s.encode_utf16().chain([0]).collect()
}

#[test]
fn host_name() {
	let host = host();
	let mut name: String128 = [0; 128];

	// SAFETY:
	// `name` is a valid `String128`.
	assert_eq!(unsafe { host.getName(&raw mut name) }, kResultOk);

	let len = name.iter().position(|&c| c == 0).unwrap();
	assert_eq!(String::from_utf16(&name[..len]).unwrap(), HOST_NAME);
}

#[test]
fn unknown_classes_are_refused() {
	let host = host();
	let mut cid = [7; 16];
	let mut iid = IMessage::IID.map(|b| b as _);
	let mut obj = null_mut::<c_void>();

	// SAFETY:
	// All pointers are valid for the duration of the call.
	let result = unsafe { host.createInstance(&raw mut cid, &raw mut iid, &raw mut obj) };

	assert_eq!(result, kInvalidArgument);
	assert!(obj.is_null());
}

#[test]
fn message_id_and_attributes_round_trip() {
	let host = host();
	let message = create_message(&host);

	// SAFETY:
	// `c"hello"` is a valid, null-terminated string.
	unsafe { message.setMessageID(c"hello".as_ptr()); }

	// SAFETY:
	// The message id is a valid, null-terminated string.
	let id = unsafe { CStr::from_ptr(message.getMessageID()) };
	assert_eq!(id.to_str().unwrap(), "hello");

	// SAFETY:
	// `getAttributes` returns a valid interface pointer owned by the message.
	let attributes: ComRef<'_, IAttributeList> =
		unsafe { ComRef::from_raw(message.getAttributes()) }.unwrap();

	// SAFETY:
	// All pointers passed below are valid for the duration of each call.
	unsafe {
		assert_eq!(attributes.setInt(c"int".as_ptr(), -42), kResultOk);
		assert_eq!(attributes.setFloat(c"float".as_ptr(), 1.5), kResultOk);

		let string = utf16("héllo ✓");
		assert_eq!(attributes.setString(c"string".as_ptr(), string.as_ptr()), kResultOk);

		let data = [1u8, 2, 3, 4, 5];
		assert_eq!(
			attributes.setBinary(c"binary".as_ptr(), data.as_ptr().cast(), 5),
			kResultOk
		);

		let mut int = 0;
		assert_eq!(attributes.getInt(c"int".as_ptr(), &raw mut int), kResultOk);
		assert_eq!(int, -42);

		let mut float = 0.0;
		assert_eq!(attributes.getFloat(c"float".as_ptr(), &raw mut float), kResultOk);
		assert_eq!(float, 1.5);

		let mut buffer = [0u16; 32];
		assert_eq!(
			attributes.getString(c"string".as_ptr(), buffer.as_mut_ptr(), 64),
			kResultOk
		);
		let len = buffer.iter().position(|&c| c == 0).unwrap();
		assert_eq!(String::from_utf16(&buffer[..len]).unwrap(), "héllo ✓");

		// a too small buffer truncates, but stays terminated
		let mut small = [0xFFFFu16; 4];
		assert_eq!(
			attributes.getString(c"string".as_ptr(), small.as_mut_ptr(), 8),
			kResultOk
		);
		assert_eq!(small[3], 0);
		assert_eq!(String::from_utf16(&small[..3]).unwrap(), "hél");

		let mut ptr = null::<c_void>();
		let mut size = 0;
		assert_eq!(
			attributes.getBinary(c"binary".as_ptr(), &raw mut ptr, &raw mut size),
			kResultOk
		);
		assert_eq!(std::slice::from_raw_parts(ptr.cast::<u8>(), size as usize), &[1, 2, 3, 4, 5]);

		// asking for the wrong type, or for something that isn't there, fails
		assert_eq!(attributes.getFloat(c"int".as_ptr(), &raw mut float), kInvalidArgument);
		assert_eq!(attributes.getInt(c"missing".as_ptr(), &raw mut int), kInvalidArgument);
		assert_eq!(attributes.setInt(null(), 1), kInvalidArgument);
	}
}

#[test]
fn component_handler_forwards_to_the_channel() {
	let (sender, receiver) = mpsc::channel();
	let handler = ComWrapper::new(ComponentHandler::new(sender))
		.to_com_ptr::<IComponentHandler>()
		.unwrap();

	// SAFETY:
	// Plain value arguments on a valid handler.
	unsafe {
		assert_eq!(handler.beginEdit(7), kResultOk);
		assert_eq!(handler.performEdit(7, 0.25), kResultOk);
		assert_eq!(handler.endEdit(7), kResultOk);
		assert_eq!(
			handler.restartComponent(RestartFlags::LATENCY_CHANGED | RestartFlags::IO_CHANGED),
			kResultOk
		);
	}

	assert_eq!(receiver.try_recv(), Ok(MainThreadMessage::BeginEdit(7)));
	assert_eq!(receiver.try_recv(), Ok(MainThreadMessage::PerformEdit(7, 0.25)));
	assert_eq!(receiver.try_recv(), Ok(MainThreadMessage::EndEdit(7)));

	let Ok(MainThreadMessage::Restart(flags)) = receiver.try_recv() else {
		panic!("expected a restart message");
	};
	assert!(flags.contains(RestartFlags::LATENCY_CHANGED));
	assert!(flags.needs_restart());
	assert!(!flags.contains(RestartFlags::RELOAD_COMPONENT));

	// a plugin that outlives the receiver must not make the handler fail
	drop(receiver);
	// SAFETY:
	// Plain value arguments on a valid handler.
	assert_eq!(unsafe { handler.beginEdit(1) }, kResultOk);
}
