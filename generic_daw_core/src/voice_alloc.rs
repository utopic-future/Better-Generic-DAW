use std::{cmp::Ordering, fmt::Debug, num::NonZero};

/// Note ID ranges:
/// - Voice allocator (clip notes): 0 ..= i32::MAX - 32768
/// - Live MIDI input: i32::MAX - 32767 ..= i32::MAX - 1
/// This ensures no collision between clip-generated and live notes.
const VOICE_ALLOC_MAX_NOTE_ID: u32 = (i32::MAX - 32768) as u32;

#[derive(Clone, Copy, Debug)]
pub struct Voice<Id, Info> {
	pub id: Id,
	pub info: Info,
	pub note_id: u32,
	pub active: bool,
}

#[derive(Debug)]
pub struct VoiceAlloc<Id, Info> {
	voices: Vec<Voice<Id, Info>>,
	next_note_id: u32,
}

impl<Id: Copy + Eq, Info: Copy> Default for VoiceAlloc<Id, Info> {
	fn default() -> Self {
		Self::new(NonZero::new(128).unwrap())
	}
}

impl<Id: Copy + Eq, Info: Copy> VoiceAlloc<Id, Info> {
	pub fn new(count: NonZero<usize>) -> Self {
		Self {
			voices: Vec::with_capacity(count.get()),
			next_note_id: 0,
		}
	}

	pub fn current_polyphony(&self) -> usize {
		self.voices.len()
	}

	pub fn deactivate_all(&mut self) {
		for voice in &mut self.voices {
			voice.active = false;
		}
	}

	pub fn drain_inactive(&mut self) -> impl Iterator<Item = Voice<Id, Info>> {
		self.voices.extract_if(.., |voice| !voice.active)
	}

	pub fn activate(&mut self, id: Id) -> bool {
		self.voices
			.iter_mut()
			.find_map(|voice| (voice.id == id).then(|| voice.active = true))
			.is_some()
	}

	pub fn alloc(&mut self, id: Id, info: Info) -> Option<Voice<Id, Info>> {
		if self.voices.len() == self.voices.capacity() {
			return None;
		}

		let voice = self.new_voice(id, info);

		self.voices.push(voice);

		Some(voice)
	}

	pub fn steal(
		&mut self,
		id: Id,
		info: Info,
		mut f: impl FnMut(&Voice<Id, Info>, &Voice<Id, Info>) -> Ordering,
	) -> (Voice<Id, Info>, Voice<Id, Info>) {
		let voice = self.new_voice(id, info);

		// Find the index of the voice to steal (avoid UB from mem::replace on reference)
		let min_idx = self
			.voices
			.iter()
			.enumerate()
			.min_by(|(_, l), (_, r)| f(l, r))
			.map(|(i, _)| i)
			.unwrap();
		let old_voice = std::mem::replace(&mut self.voices[min_idx], voice);

		(voice, old_voice)
	}

	fn new_voice(&mut self, id: Id, info: Info) -> Voice<Id, Info> {
		let note_id = self.next_note_id;
		// Wrap within the voice allocator range to avoid collision with live MIDI notes
		self.next_note_id = (self.next_note_id + 1) % VOICE_ALLOC_MAX_NOTE_ID;

		Voice {
			id,
			info,
			note_id,
			active: true,
		}
	}

	pub fn dealloc(&mut self, id: Id) -> Option<Voice<Id, Info>> {
		self.voices
			.iter()
			.position(|voice| voice.id == id)
			.map(|i| self.voices.swap_remove(i))
	}
}
