use std::cell::{Cell, RefCell};
use vst3::{
	Class, ComWrapper,
	Steinberg::{
		Vst::{
			IParamValueQueue, IParamValueQueueTrait, IParameterChanges, IParameterChangesTrait,
			ParamID, ParamValue,
		},
		int32, kInvalidArgument, kResultFalse, kResultOk, tresult,
	},
};

/// The host's `IParamValueQueue`: the changes of one parameter, ordered by time.
pub struct ParamValueQueue {
	id: Cell<u32>,
	points: RefCell<Vec<(i32, f64)>>,
	capacity: usize,
}

impl ParamValueQueue {
	fn new(capacity: usize) -> Self {
		Self {
			id: Cell::new(0),
			points: RefCell::new(Vec::with_capacity(capacity)),
			capacity,
		}
	}

	fn reset(&self, id: u32) {
		self.id.set(id);
		self.points.borrow_mut().clear();
	}

	fn push(&self, sample_offset: i32, value: f64) -> Option<usize> {
		let mut points = self.points.borrow_mut();

		if points.len() >= self.capacity {
			return None;
		}

		points.push((sample_offset, value));
		Some(points.len() - 1)
	}
}

impl Class for ParamValueQueue {
	type Interfaces = (IParamValueQueue,);
}

#[expect(clippy::renamed_function_params, reason = "the names of the trait's parameters aren't snake case")]
impl IParamValueQueueTrait for ParamValueQueue {
	unsafe fn getParameterId(&self) -> ParamID {
		self.id.get()
	}

	unsafe fn getPointCount(&self) -> int32 {
		self.points.borrow().len() as int32
	}

	unsafe fn getPoint(
		&self,
		index: int32,
		sample_offset: *mut int32,
		value: *mut ParamValue,
	) -> tresult {
		if sample_offset.is_null() || value.is_null() {
			return kInvalidArgument;
		}

		let Some((offset, point)) = usize::try_from(index)
			.ok()
			.and_then(|index| self.points.borrow().get(index).copied())
		else {
			return kInvalidArgument;
		};

		// SAFETY:
		// Both pointers are non-null.
		unsafe { *sample_offset = offset; }

		// SAFETY:
		// Both pointers are non-null.
		unsafe { *value = point; }

		kResultOk
	}

	unsafe fn addPoint(
		&self,
		sample_offset: int32,
		value: ParamValue,
		index: *mut int32,
	) -> tresult {
		let Some(position) = self.push(sample_offset, value) else {
			return kResultFalse;
		};

		if !index.is_null() {
			// SAFETY:
			// `index` is non-null.
			unsafe { *index = position as int32; }
		}

		kResultOk
	}
}

/// The host's `IParameterChanges`: one queue per changed parameter.
///
/// All queues are created up front, so using the list doesn't allocate.
pub struct ParameterChanges {
	queues: Box<[ComWrapper<ParamValueQueue>]>,
	used: Cell<usize>,
}

impl ParameterChanges {
	#[must_use]
	pub fn new(queues: usize, points_per_queue: usize) -> Self {
		Self {
			queues: (0..queues)
				.map(|_| ComWrapper::new(ParamValueQueue::new(points_per_queue)))
				.collect(),
			used: Cell::new(0),
		}
	}

	pub fn clear(&self) {
		self.used.set(0);
	}

	/// The position of the queue for `id`, which is created if there isn't one yet.
	fn queue_for(&self, id: u32) -> Option<usize> {
		let used = self.used.get();

		if let Some(position) = self.queues[..used].iter().position(|queue| queue.id.get() == id) {
			return Some(position);
		}

		if used == self.queues.len() {
			return None;
		}

		self.queues[used].reset(id);
		self.used.set(used + 1);
		Some(used)
	}

	/// Adds a change, which has to be at or after the last change of the same parameter.
	/// Returns `false` if there's no room.
	pub fn add_point(&self, id: u32, sample_offset: i32, value: f64) -> bool {
		self.queue_for(id)
			.is_some_and(|position| self.queues[position].push(sample_offset, value).is_some())
	}

	/// Calls `f` with the parameter id, sample offset and value of every change.
	pub fn for_each(&self, mut f: impl FnMut(u32, i32, f64)) {
		for queue in &self.queues[..self.used.get()] {
			for &(offset, value) in queue.points.borrow().iter() {
				f(queue.id.get(), offset, value);
			}
		}
	}
}

impl Class for ParameterChanges {
	type Interfaces = (IParameterChanges,);
}

impl IParameterChangesTrait for ParameterChanges {
	unsafe fn getParameterCount(&self) -> int32 {
		self.used.get() as int32
	}

	unsafe fn getParameterData(&self, index: int32) -> *mut IParamValueQueue {
		usize::try_from(index)
			.ok()
			.filter(|&index| index < self.used.get())
			.and_then(|index| self.queues[index].as_com_ref::<IParamValueQueue>())
			.map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
	}

	unsafe fn addParameterData(&self, id: *const ParamID, index: *mut int32) -> *mut IParamValueQueue {
		if id.is_null() {
			return std::ptr::null_mut();
		}

		// SAFETY:
		// `id` is non-null.
		let id = unsafe { *id };

		let Some(position) = self.queue_for(id) else {
			return std::ptr::null_mut();
		};

		if !index.is_null() {
			// SAFETY:
			// `index` is non-null.
			unsafe { *index = position as int32; }
		}

		self.queues[position]
			.as_com_ref::<IParamValueQueue>()
			.map_or(std::ptr::null_mut(), |queue| queue.as_ptr())
	}
}
