use std::cell::UnsafeCell;
use vst3::{
	Class,
	Steinberg::{
		Vst::{Event, IEventList, IEventListTrait},
		int32, kInvalidArgument, kResultFalse, kResultOk, tresult,
	},
};

/// The host's `IEventList`. It's filled by the host for the plugin to read, or by the plugin for
/// the host to read, and never grows beyond its capacity, so it doesn't allocate while processing.
///
/// Uses UnsafeCell instead of RefCell for better real-time performance - caller must ensure
/// single-threaded access according to VST3 spec.
pub struct EventList {
	/// Unsafe storage - must only be accessed from single thread (VST3 spec guarantees this)
	events: UnsafeCell<Vec<Event>>,
	capacity: usize,
}

impl EventList {
	#[must_use]
	pub fn new(capacity: usize) -> Self {
		Self {
			events: UnsafeCell::new(Vec::with_capacity(capacity)),
			capacity,
		}
	}

	/// Clear events - real-time safe
	#[inline]
	pub fn clear(&self) {
		// SAFETY: Single-threaded access per VST3 spec
		let events = unsafe { &mut *self.events.get() };
		events.clear();
	}

	/// Returns `false` if the list is full.
	#[inline]
	pub fn push(&self, event: Event) -> bool {
		// SAFETY: Single-threaded access per VST3 spec
		let events = unsafe { &mut *self.events.get() };

		if events.len() >= self.capacity {
			return false;
		}

		events.push(event);
		true
	}

	#[inline]
	pub fn for_each(&self, mut f: impl FnMut(&Event)) {
		// SAFETY: Single-threaded access per VST3 spec
		let events = unsafe { &*self.events.get() };
		events.iter().for_each(&mut f);
	}

	/// Get number of events
	#[inline]
	pub fn len(&self) -> usize {
		// SAFETY: Single-threaded access per VST3 spec
		unsafe { (&*self.events.get()).len() }
	}

	/// Check if empty
	#[inline]
	pub fn is_empty(&self) -> bool {
		self.len() == 0
	}
}

// SAFETY: the events are only used by the thread that has the list, and the VST3 spec
// says that a list is only used by one thread at a time.
unsafe impl Send for EventList {}

impl Class for EventList {
	type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
	unsafe fn getEventCount(&self) -> int32 {
		self.len() as int32
	}

	unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
		if e.is_null() {
			return kInvalidArgument;
		}

		// Convert index and check bounds
		let Ok(idx) = usize::try_from(index) else {
			return kInvalidArgument;
		};
		if idx >= self.len() {
			return kInvalidArgument;
		}

		// SAFETY: Single-threaded access per VST3 spec, e is non-null and points to Event
		let event = unsafe { (&*self.events.get())[idx] };

		// SAFETY: e is non-null and points to an Event.
		unsafe { *e = event; }
		kResultOk
	}

	unsafe fn addEvent(&self, e: *mut Event) -> tresult {
		if e.is_null() {
			return kInvalidArgument;
		}

		// SAFETY: e is non-null and points to Event, and single-threaded access is guaranteed
		let event = unsafe { *e };

		if self.push(event) {
			kResultOk
		} else {
			kResultFalse
		}
	}
}