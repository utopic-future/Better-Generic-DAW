use std::cell::RefCell;
use vst3::{
	Class,
	Steinberg::{
		Vst::{Event, IEventList, IEventListTrait},
		int32, kInvalidArgument, kResultFalse, kResultOk, tresult,
	},
};

/// The host's `IEventList`. It's filled by the host for the plugin to read, or by the plugin for
/// the host to read, and never grows beyond its capacity, so it doesn't allocate while processing.
pub struct EventList {
	events: RefCell<Vec<Event>>,
	capacity: usize,
}

impl EventList {
	#[must_use]
	pub fn new(capacity: usize) -> Self {
		Self {
			events: RefCell::new(Vec::with_capacity(capacity)),
			capacity,
		}
	}

	pub fn clear(&self) {
		self.events.borrow_mut().clear();
	}

	/// Returns `false` if the list is full.
	pub fn push(&self, event: Event) -> bool {
		let mut events = self.events.borrow_mut();

		if events.len() >= self.capacity {
			return false;
		}

		events.push(event);
		true
	}

	pub fn for_each(&self, mut f: impl FnMut(&Event)) {
		self.events.borrow().iter().for_each(&mut f);
	}
}

impl Class for EventList {
	type Interfaces = (IEventList,);
}

impl IEventListTrait for EventList {
	unsafe fn getEventCount(&self) -> int32 {
		self.events.borrow().len() as int32
	}

	unsafe fn getEvent(&self, index: int32, e: *mut Event) -> tresult {
		if e.is_null() {
			return kInvalidArgument;
		}

		let Some(event) = usize::try_from(index)
			.ok()
			.and_then(|index| self.events.borrow().get(index).copied())
		else {
			return kInvalidArgument;
		};

		// SAFETY:
		// `e` is non-null and points to an `Event`.
		unsafe {
			*e = event;
		}
		kResultOk
	}

	unsafe fn addEvent(&self, e: *mut Event) -> tresult {
		if e.is_null() {
			return kInvalidArgument;
		}

		// SAFETY:
		// `e` is non-null and points to an `Event`.
		let event = unsafe { *e };

		if self.push(event) {
			kResultOk
		} else {
			kResultFalse
		}
	}
}
