use crate::{
	EventImpl as _, Injector, NodeId, NodeImpl, ThreadPool,
	entry::{Entry, Incoming},
	node_impl::Inject,
};
use crossbeam_queue::ArrayQueue;
use std::{
	collections::{HashMap, hash_map},
	num::NonZero,
	sync::{RwLockWriteGuard, atomic::Ordering::Relaxed},
};
use thread_pool::WorkList;

#[cfg(feature = "simd")]
use crate::simd;

impl<Node: NodeImpl> WorkList for AudioGraph<Node> {
	type Item = NodeId;
	type Scratch = Node::Scratch;
	type Inject = Inject<Node>;

	fn next_item(&self) -> Option<Self::Item> {
		self.queue.pop()
	}

	fn do_work(
		&self,
		item: Self::Item,
		scratch: &mut Self::Scratch,
		injector: &Injector<Node>,
	) -> Option<Self::Item> {
		self.process_node(&self.graph[&item], scratch, injector)
	}
}

#[derive(Debug)]
pub struct AudioGraph<Node: NodeImpl> {
	state: Node::State,
	graph: HashMap<NodeId, Entry<Node>>,
	queue: ArrayQueue<NodeId>,
	max_frames: NonZero<u32>,
	curr_len: usize,
	needs_reset: bool,
}

impl<Node: NodeImpl> AudioGraph<Node> {
	#[must_use]
	pub fn new(state: Node::State, max_frames: NonZero<u32>) -> Self {
		Self {
			state,
			graph: HashMap::new(),
			queue: ArrayQueue::new(64),
			max_frames,
			curr_len: 0,
			needs_reset: false,
		}
	}

	pub fn change_max_frames(&mut self, max_frames: NonZero<u32>) {
		if self.max_frames == max_frames {
			return;
		}

		for entry in self.graph.values_mut() {
			entry.change_max_frames(max_frames);
		}

		self.max_frames = max_frames;
	}

	pub fn process_all(&mut self, pool: &mut ThreadPool<Node>, len: usize) {
		self.curr_len = len;

		for (&id, entry) in &mut self.graph {
			*entry.indegree.get_mut() = entry.buffers().incoming.len();
			if *entry.indegree.get_mut() == 0 {
				self.queue.push_mut(id).unwrap();
			}
		}

		pool.run(
			self,
			self.graph.len(),
			NonZero::new(self.queue.len()).unwrap(),
		);

		self.needs_reset = false;

		for entry in self.graph.values_mut() {
			debug_assert_eq!(*entry.indegree.get_mut(), 0);
		}
	}

	pub fn process_subtree(
		&mut self,
		pool: &mut ThreadPool<Node>,
		node: NodeId,
		audio: &mut [[f32; 2]],
	) {
		fn visit<Node: NodeImpl>(this: &AudioGraph<Node>, entry: &Entry<Node>) {
			let buffers = entry.read_buffers_uncontended();
			if entry.indegree.swap(buffers.incoming.len(), Relaxed) == usize::MAX {
				buffers
					.incoming
					.keys()
					.map(|node| &this.graph[node])
					.for_each(|dep| visit(this, dep));
			}
		}

		self.curr_len = audio.len();

		for entry in self.graph.values_mut() {
			*entry.indegree.get_mut() = usize::MAX;
		}

		visit(self, &self.graph[&node]);

		let mut count = 0;
		for (&id, entry) in &mut self.graph {
			if *entry.indegree.get_mut() == usize::MAX {
				continue;
			}

			count += 1;
			if *entry.indegree.get_mut() == 0 {
				self.queue.push_mut(id).unwrap();
			}
		}

		pool.run(self, count, NonZero::new(self.queue.len()).unwrap());

		self.needs_reset = false;

		audio.copy_from_slice(&self.entry_mut(node).buffers().audio[..audio.len()]);
	}

	#[expect(clippy::significant_drop_tightening)]
	fn process_node(
		&self,
		entry: &Entry<Node>,
		scratch: &mut Node::Scratch,
		injector: &Injector<Node>,
	) -> Option<NodeId> {
		debug_assert_eq!(entry.indegree.load(Relaxed), 0);

		let mut node = entry.node_uncontended();

		let mut buffers_lock = entry.write_buffers_uncontended();
		let buffers = &mut *buffers_lock;

		if self.needs_reset {
			node.reset();

			for Incoming {
				delay_line, events, ..
			} in buffers.incoming.values_mut()
			{
				delay_line.reset();
				events.clear();
			}
		}

		let mut buffers_init = false;
		buffers.events.clear();

		let max_latency = buffers
			.incoming
			.keys()
			.map(|node| self.graph[node].latency.load(Relaxed))
			.max()
			.unwrap_or_default();

		for (
			dep,
			Incoming {
				delay_line,
				events,
				mix,
			},
		) in &mut buffers.incoming
		{
			let dep_entry = &self.graph[dep];
			let dep_buffers = dep_entry.read_buffers_uncontended();
			let latency_diff = max_latency - dep_entry.latency.load(Relaxed);
			delay_line.resize(latency_diff);

			let dst = &mut buffers.audio[..self.curr_len];
			let mix_val = *mix;

			if latency_diff == 0 {
				let src = &dep_buffers.audio[..self.curr_len];
				
				if buffers_init {
					// Mix: dst = dst + src * mix
					#[cfg(feature = "simd")]
					simd::mix_stereo_sse2(dst, src, mix_val);
					#[cfg(not(feature = "simd"))]
					for i in 0..self.curr_len {
						dst[i][0] += src[i][0] * mix_val;
						dst[i][1] += src[i][1] * mix_val;
					}
				} else {
					// First input: dst = src * mix
					#[cfg(feature = "simd")]
					simd::copy_stereo_sse2(dst, src, mix_val);
					#[cfg(not(feature = "simd"))]
					for i in 0..self.curr_len {
						dst[i][0] = src[i][0] * mix_val;
						dst[i][1] = src[i][1] * mix_val;
					}
					buffers_init = true;
				}
			} else {
				// Need delay line - use scratch buffer
				let audio = Node::get_scratch_audio(scratch);
				let src_scratch = &mut audio[..self.curr_len];
				
				// Copy to scratch
				#[cfg(feature = "simd")]
				simd::copy_to_scratch_sse2(src_scratch, &dep_buffers.audio[..self.curr_len]);
				#[cfg(not(feature = "simd"))]
				for i in 0..self.curr_len {
					src_scratch[i][0] = dep_buffers.audio[i][0];
					src_scratch[i][1] = dep_buffers.audio[i][1];
				}
				delay_line.advance_mut(src_scratch);
				
				if buffers_init {
					#[cfg(feature = "simd")]
					simd::mix_scratch_sse2(dst, src_scratch, mix_val);
					#[cfg(not(feature = "simd"))]
					for i in 0..self.curr_len {
						dst[i][0] += src_scratch[i][0] * mix_val;
						dst[i][1] += src_scratch[i][1] * mix_val;
					}
				} else {
					#[cfg(feature = "simd")]
					simd::copy_stereo_sse2(dst, src_scratch, mix_val);
					#[cfg(not(feature = "simd"))]
					for i in 0..self.curr_len {
						dst[i][0] = src_scratch[i][0] * mix_val;
						dst[i][1] = src_scratch[i][1] * mix_val;
					}
					buffers_init = true;
				}
			}

			// Events: avoid allocations by using manual loop
			for e in &dep_buffers.events {
				let adjusted = e.at(e.time() + latency_diff);
				if adjusted.time() < self.curr_len {
					events.push(adjusted);
				}
			}

			// Filter events in place - use retain for in-place filtering
			events.retain(|e| e.time() < self.curr_len);
		}

		if !buffers_init {
			for i in 0..self.curr_len {
				buffers.audio[i][0] = 0.0;
				buffers.audio[i][1] = 0.0;
			}
		}

		entry.latency.store(
			node.process(
				&self.state,
				&mut buffers.audio[..self.curr_len],
				&mut buffers.events,
				scratch,
				injector,
			) + max_latency,
			Relaxed,
		);

		drop(node);

		let buffers_lock = RwLockWriteGuard::downgrade(buffers_lock);

		let mut iter = buffers_lock
			.outgoing
			.iter()
			.copied()
			.filter(|node| self.graph[node].indegree.fetch_sub(1, Relaxed) == 1);

		let inline = iter.next();

		for node in iter {
			self.queue.push(node).unwrap();
		}

		inline
	}

	pub fn reset(&mut self) {
		self.needs_reset = true;
	}

	pub fn state(&self) -> &Node::State {
		&self.state
	}

	pub fn state_mut(&mut self) -> &mut Node::State {
		&mut self.state
	}

	fn entry_mut(&mut self, node: NodeId) -> &mut Entry<Node> {
		self.graph.get_mut(&node).unwrap()
	}

	pub fn for_node<T>(
		&mut self,
		node: NodeId,
		f: impl FnOnce(&Node, &Node::State, &[[f32; 2]], &[Node::Event]) -> T,
	) -> T {
		let entry = self.graph.get_mut(&node).unwrap();
		let node = &*entry.node_uncontended();
		let buffers = &*entry.read_buffers_uncontended();
		f(node, &self.state, &buffers.audio, &buffers.events)
	}

	pub fn for_node_mut<T>(
		&mut self,
		node: NodeId,
		f: impl FnOnce(&mut Node, &mut Node::State, &mut [[f32; 2]], &mut [Node::Event]) -> T,
	) -> T {
		let entry = self.graph.get_mut(&node).unwrap();
		let node = &mut *entry.node_uncontended();
		let buffers = &mut *entry.write_buffers_uncontended();
		f(
			node,
			&mut self.state,
			&mut buffers.audio,
			&mut buffers.events,
		)
	}

	pub fn for_each_node(
		&mut self,
		mut f: impl FnMut(&Node, &Node::State, &[[f32; 2]], &[Node::Event]),
	) {
		for entry in self.graph.values_mut() {
			let node = &*entry.node_uncontended();
			let buffers = &*entry.read_buffers_uncontended();
			f(node, &self.state, &buffers.audio, &buffers.events);
		}
	}

	pub fn for_each_node_mut(
		&mut self,
		mut f: impl FnMut(&mut Node, &mut Node::State, &mut [[f32; 2]], &mut [Node::Event]),
	) {
		for entry in self.graph.values_mut() {
			let node = &mut *entry.node_uncontended();
			let buffers = &mut *entry.write_buffers_uncontended();
			f(
				node,
				&mut self.state,
				&mut buffers.audio,
				&mut buffers.events,
			);
		}
	}

	pub fn latency(&self, node: NodeId) -> usize {
		self.graph[&node].latency.load(Relaxed)
	}

	pub fn connect(&mut self, from: NodeId, to: NodeId) -> bool {
		if !self.graph.contains_key(&from) || !self.graph.contains_key(&to) {
			return false;
		}

		if !self.entry_mut(from).buffers().outgoing.insert(to) {
			return true;
		}

		self.entry_mut(to)
			.buffers()
			.incoming
			.insert(from, Incoming::default());

		if self.has_cycle() {
			self.entry_mut(from).buffers().outgoing.remove(&to);
			self.entry_mut(to).buffers().incoming.remove(&from);
			return false;
		}

		true
	}

	pub fn set_mix(&mut self, from: NodeId, to: NodeId, mix: f32) {
		self.entry_mut(to)
			.buffers()
			.incoming
			.get_mut(&from)
			.unwrap()
			.mix = mix;
	}

	pub fn disconnect(&mut self, from: NodeId, to: NodeId) {
		self.entry_mut(from).buffers().outgoing.remove(&to);
		self.entry_mut(to).buffers().incoming.remove(&from);
	}

	pub fn insert(&mut self, node: Node) -> Option<Node> {
		match self.graph.entry(node.id()) {
			hash_map::Entry::Occupied(mut entry) => {
				Some(std::mem::replace(entry.get_mut().node(), node))
			}
			hash_map::Entry::Vacant(entry) => {
				entry.insert(Entry::new(node, self.max_frames));
				if self.queue.capacity() < self.graph.len() {
					let mut new_queue = ArrayQueue::new(2 * self.queue.capacity());
					// Drain old queue into new queue to avoid losing items
					while let Some(item) = self.queue.pop() {
						let _ = new_queue.push(item);
					}
					self.queue = new_queue;
				}
				None
			}
		}
	}

	pub fn remove(&mut self, node: NodeId) -> Option<Node> {
		let mut entry = self.graph.remove(&node)?;

		for &incoming in entry.buffers().incoming.keys() {
			self.entry_mut(incoming).buffers().outgoing.remove(&node);
		}

		for &outgoing in &entry.buffers().outgoing {
			self.entry_mut(outgoing).buffers().incoming.remove(&node);
		}

		Some(entry.into_node())
	}

	fn has_cycle(&mut self) -> bool {
		fn visit<Node: NodeImpl>(this: &AudioGraph<Node>, entry: &Entry<Node>) {
			debug_assert_eq!(entry.indegree.load(Relaxed), usize::MAX);

			entry
				.read_buffers_uncontended()
				.outgoing
				.iter()
				.map(|node| &this.graph[node])
				.filter(|dep| dep.indegree.fetch_sub(1, Relaxed) == 0)
				.for_each(|dep| visit(this, dep));
		}

		for entry in self.graph.values_mut() {
			*entry.indegree.get_mut() = entry.buffers().incoming.len();
		}

		self.graph
			.values()
			.filter(|entry| entry.indegree.fetch_sub(1, Relaxed) == 0)
			.for_each(|entry| visit(self, entry));

		self.graph
			.values_mut()
			.any(|entry| *entry.indegree.get_mut() != usize::MAX)
	}
}
