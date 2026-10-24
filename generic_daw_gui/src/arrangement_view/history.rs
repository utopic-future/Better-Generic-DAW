//! Undo and redo for edits to clips and notes.

use super::arrangement::Arrangement;
use generic_daw_core::{Clip, MidiNote, MidiNoteId, MidiPatternId, NodeId};

/// How many steps back the user can go.
const MAX_STEPS: usize = 128;

/// Where a clip was in the arrangement.
#[derive(Clone, Copy, Debug)]
pub struct ClipSnapshot {
	pub track: NodeId,
	pub index: usize,
	pub clip: Clip,
}

impl ClipSnapshot {
	/// Takes a snapshot of a clip, if it is where it should be.
	#[must_use]
	pub fn of(arrangement: &Arrangement, track: usize, clip: usize) -> Option<Self> {
		Some(Self {
			track: arrangement.tracks().get(track)?.id,
			index: clip,
			clip: *arrangement.tracks().get(track)?.clips.get(clip)?,
		})
	}
}

/// Something that can be undone.
#[derive(Clone, Copy, Debug)]
pub enum Edit {
	/// A clip that was put into the arrangement.
	ClipInsert {
		track: NodeId,
		index: usize,
		clip: Clip,
		/// Whether the arrangement no longer has a reference to the clip, so this
		/// edit has to let go of the sample or pattern instead.
		detached: bool,
	},
	/// A clip that was taken out of the arrangement.
	ClipRemove {
		track: NodeId,
		index: usize,
		clip: Clip,
		/// Whether this edit has to let go of the sample or pattern.
		detached: bool,
	},
	/// A clip that changed.
	ClipModify {
		before: ClipSnapshot,
		after: ClipSnapshot,
	},
	/// A note that was put into a pattern.
	NoteInsert {
		pattern: MidiPatternId,
		index: usize,
		note: MidiNote,
	},
	/// A note that was taken out of a pattern.
	NoteRemove {
		pattern: MidiPatternId,
		index: usize,
		note: MidiNote,
	},
	/// A note that changed.
	NoteModify {
		pattern: MidiPatternId,
		before: MidiNote,
		after: MidiNote,
	},
}

impl Edit {
	/// Whether this edit and another one are two versions of the same change, so
	/// that they can be put together.
	fn merge(&mut self, other: &Self) -> bool {
		match (self, other) {
			(
				Self::ClipModify { after, .. },
				Self::ClipModify {
					after: new_after, ..
				},
			) if after.clip.id() == new_after.clip.id() => {
				*after = *new_after;
				true
			}
			(
				Self::NoteModify { after, .. },
				Self::NoteModify {
					after: new_after, ..
				},
			) if after.id == new_after.id => {
				*after = *new_after;
				true
			}
			_ => false,
		}
	}

	/// Undoes or redoes the edit.
	fn apply(self, arrangement: &mut Arrangement, redo: bool) -> Self {
		match self {
			Self::ClipInsert {
				track, index, clip, ..
			} => {
				if redo {
					let attached = arrangement
						.track_of(track)
						.map(|track| arrangement.attach_clip(track, index, clip))
						.is_some();

					Self::ClipInsert {
						track,
						index,
						clip,
						detached: !attached,
					}
				} else {
					if let Some((track, index)) = arrangement.clip_location(clip.id()) {
						arrangement.detach_clip(track, index);
					}

					Self::ClipInsert {
						track,
						index,
						clip,
						detached: true,
					}
				}
			}
			Self::ClipRemove {
				track, index, clip, ..
			} => {
				if redo {
					if let Some((track, index)) = arrangement.clip_location(clip.id()) {
						arrangement.detach_clip(track, index);
					}

					Self::ClipRemove {
						track,
						index,
						clip,
						detached: true,
					}
				} else {
					let attached = arrangement
						.track_of(track)
						.map(|track| arrangement.attach_clip(track, index, clip))
						.is_some();

					Self::ClipRemove {
						track,
						index,
						clip,
						detached: !attached,
					}
				}
			}
			Self::ClipModify { before, after } => {
				let snapshot = if redo { after } else { before };

				if let Some((track, clip)) = arrangement.clip_location(snapshot.clip.id())
					&& let Some(new_track) = arrangement.track_of(snapshot.track)
				{
					if track == new_track {
						arrangement.replace_clip(track, clip, snapshot.clip);
					} else {
						arrangement.detach_clip(track, clip);
						arrangement.attach_clip(new_track, snapshot.index, snapshot.clip);
					}
				}

				Self::ClipModify { before, after }
			}
			Self::NoteInsert {
				pattern,
				index,
				note,
			} => {
				if redo {
					arrangement.insert_note_at(pattern, index, note);
				} else if let Some(index) = arrangement.note_index(pattern, note.id) {
					arrangement.remove_note(pattern, index);
				}

				Self::NoteInsert {
					pattern,
					index,
					note,
				}
			}
			Self::NoteRemove {
				pattern,
				index,
				note,
			} => {
				if redo {
					if let Some(index) = arrangement.note_index(pattern, note.id) {
						arrangement.remove_note(pattern, index);
					}
				} else {
					arrangement.insert_note_at(pattern, index, note);
				}

				Self::NoteRemove {
					pattern,
					index,
					note,
				}
			}
			Self::NoteModify {
				pattern,
				before,
				after,
			} => {
				arrangement.replace_note(pattern, if redo { after } else { before });

				Self::NoteModify {
					pattern,
					before,
					after,
				}
			}
		}
	}

	/// Lets go of the sample or pattern the edit is keeping alive, if it is.
	fn release(self, arrangement: &mut Arrangement) {
		match self {
			Self::ClipInsert {
				clip,
				detached: true,
				..
			}
			| Self::ClipRemove {
				clip,
				detached: true,
				..
			} => arrangement.release_clip(clip),
			_ => {}
		}
	}
}

/// The edits that can be undone and redone.
#[derive(Clone, Debug, Default)]
pub struct History {
	undo: Vec<Vec<Edit>>,
	redo: Vec<Vec<Edit>>,
	/// The edits of the gesture that is happening, if it hasn't ended.
	open: Option<Vec<Edit>>,
	/// How many updates deep the recording is.
	depth: usize,
}

impl History {
	/// Starts recording the edits of an update.
	pub fn begin(&mut self) {
		self.depth += 1;
	}

	/// Stops recording the edits of an update.
	///
	/// `commit` is whether this update ended the gesture, so that the edits can
	/// be undone as one step.
	pub fn end(&mut self, arrangement: &mut Arrangement, commit: bool) {
		self.depth = self.depth.saturating_sub(1);

		if self.depth > 0 || !commit {
			return;
		}

		self.commit(arrangement);
	}

	/// Records an edit.
	pub fn push(&mut self, arrangement: &mut Arrangement, edit: Edit) {
		if self.open.is_none() {
			self.clear_redo(arrangement);
			self.open = Some(Vec::new());
		}

		let edits = self.open.as_mut().unwrap();

		if let Some(last) = edits.last_mut()
			&& last.merge(&edit)
		{
			return;
		}

		edits.push(edit);
	}

	/// Undoes the last gesture, if there is one.
	pub fn undo(&mut self, arrangement: &mut Arrangement) -> bool {
		self.commit(arrangement);

		let Some(mut edits) = self.undo.pop() else {
			return false;
		};

		for edit in edits.iter_mut().rev() {
			*edit = edit.apply(arrangement, false);
		}

		self.redo.push(edits);
		true
	}

	/// Redoes the last undone gesture, if there is one.
	pub fn redo(&mut self, arrangement: &mut Arrangement) -> bool {
		self.commit(arrangement);

		let Some(mut edits) = self.redo.pop() else {
			return false;
		};

		for edit in &mut edits {
			*edit = edit.apply(arrangement, true);
		}

		self.undo.push(edits);
		true
	}

	/// Forgets everything, letting go of the samples and patterns that were only
	/// being kept for undo.
	pub fn clear(&mut self, arrangement: &mut Arrangement) {
		if let Some(edits) = self.open.take() {
			for edit in edits {
				edit.release(arrangement);
			}
		}

		for edits in self.undo.drain(..).chain(self.redo.drain(..)) {
			for edit in edits {
				edit.release(arrangement);
			}
		}
	}

	/// Puts all the edits into a step that can be undone.
	fn commit(&mut self, arrangement: &mut Arrangement) {
		let Some(edits) = self.open.take() else {
			return;
		};

		if edits.is_empty() {
			return;
		}

		self.undo.push(edits);

		while self.undo.len() > MAX_STEPS {
			let edits = self.undo.remove(0);

			for edit in edits {
				edit.release(arrangement);
			}
		}
	}

	/// Forgets the edits that could be redone.
	fn clear_redo(&mut self, arrangement: &mut Arrangement) {
		for edits in self.redo.drain(..) {
			for edit in edits {
				edit.release(arrangement);
			}
		}
	}
}

/// Records that a clip appeared or moved.
pub fn record_clip_change(
	arrangement: &mut Arrangement,
	history: &mut History,
	before: ClipSnapshot,
) {
	let Some((track, index)) = arrangement.clip_location(before.clip.id()) else {
		return;
	};

	let after = ClipSnapshot {
		track: arrangement.tracks()[track].id,
		index,
		clip: arrangement.tracks()[track].clips[index],
	};

	if before.clip != after.clip || before.track != after.track || before.index != after.index {
		history.push(arrangement, Edit::ClipModify { before, after });
	}
}

/// Records that a clip was added at an index of a track.
pub fn record_clip_add(
	arrangement: &mut Arrangement,
	history: &mut History,
	track: usize,
	index: usize,
) {
	let Some(snapshot) = ClipSnapshot::of(arrangement, track, index) else {
		return;
	};

	history.push(
		arrangement,
		Edit::ClipInsert {
			track: snapshot.track,
			index: snapshot.index,
			clip: snapshot.clip,
			detached: false,
		},
	);
}

/// Records that a clip was taken out of a track.
pub fn record_clip_remove(
	arrangement: &mut Arrangement,
	history: &mut History,
	track: NodeId,
	index: usize,
	clip: Clip,
) {
	history.push(
		arrangement,
		Edit::ClipRemove {
			track,
			index,
			clip,
			detached: true,
		},
	);
}

/// Records that a note appeared.
pub fn record_note_add(
	arrangement: &mut Arrangement,
	history: &mut History,
	pattern: MidiPatternId,
	index: usize,
) {
	let Some(note) = arrangement
		.midi_patterns()
		.get(&pattern)
		.and_then(|pattern| pattern.notes.get(index))
		.copied()
	else {
		return;
	};

	history.push(
		arrangement,
		Edit::NoteInsert {
			pattern,
			index,
			note,
		},
	);
}

/// Records that a note changed.
pub fn record_note_change(
	arrangement: &mut Arrangement,
	history: &mut History,
	pattern: MidiPatternId,
	before: MidiNote,
) {
	let Some(index) = arrangement.note_index(pattern, before.id) else {
		return;
	};

	let after = arrangement.midi_patterns()[&pattern].notes[index];

	if after != before {
		history.push(
			arrangement,
			Edit::NoteModify {
				pattern,
				before,
				after,
			},
		);
	}
}

/// Records that a note was taken out of a pattern.
pub fn record_note_remove(
	arrangement: &mut Arrangement,
	history: &mut History,
	pattern: MidiPatternId,
	index: usize,
	note: MidiNote,
) {
	history.push(
		arrangement,
		Edit::NoteRemove {
			pattern,
			index,
			note,
		},
	);
}

/// Records everything that changed in a pattern.
pub fn record_pattern_changes(
	arrangement: &mut Arrangement,
	history: &mut History,
	pattern: MidiPatternId,
	before: &[MidiNote],
) {
	let Some(after) = arrangement.midi_patterns().get(&pattern) else {
		return;
	};

	let after = after.notes.clone();
	let find_before = |id: MidiNoteId| before.iter().find(|note| note.id == id).copied();

	// Recorded from the end so that undoing them puts the notes back in order.
	for (index, note) in before.iter().enumerate().rev() {
		if after.iter().all(|new| new.id != note.id) {
			history.push(
				arrangement,
				Edit::NoteRemove {
					pattern,
					index,
					note: *note,
				},
			);
		}
	}

	for (index, note) in after.iter().enumerate() {
		match find_before(note.id) {
			Some(old) if old != *note => history.push(
				arrangement,
				Edit::NoteModify {
					pattern,
					before: old,
					after: *note,
				},
			),
			None => history.push(
				arrangement,
				Edit::NoteInsert {
					pattern,
					index,
					note: *note,
				},
			),
			_ => {}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::arrangement_view::midi_pattern::MidiPatternPair;
	use generic_daw_core::{
		MidiKey,
		time::{BeatRange, BeatTime},
	};
	use std::num::NonZero;

	fn note(key: u8, start: u64, end: u64) -> MidiNote {
		MidiNote {
			id: MidiNoteId::unique(),
			key: MidiKey(key),
			velocity: 1.0,
			position: BeatRange::new(BeatTime::new(start, 0), BeatTime::new(end, 0)),
		}
	}

	fn arrangement_with_pattern(notes: Vec<MidiNote>) -> (Arrangement, MidiPatternId) {
		let (mut arrangement, processor, task) = Arrangement::create(
			0,
			NonZero::new(2).unwrap(),
			NonZero::new(48_000).unwrap(),
			NonZero::new(512).unwrap(),
		);
		drop(processor);
		drop(task);

		arrangement.insert_track(0);

		let pattern = MidiPatternPair::from_notes(notes, "Test");
		let id = pattern.gui.id;
		arrangement.add_midi_pattern(pattern);
		arrangement.add_midi_clip_from_pattern(0, BeatTime::ZERO, id);

		(arrangement, id)
	}

	#[test]
	fn undoing_a_deletion_puts_the_clips_back_in_order() {
		let (mut arrangement, pattern) = arrangement_with_pattern(vec![note(60, 0, 1)]);
		arrangement.add_midi_clip_from_pattern(0, BeatTime::ZERO, pattern);
		arrangement.add_midi_clip_from_pattern(0, BeatTime::ZERO, pattern);

		let mut history = History::default();
		let before = arrangement.tracks()[0].clips.clone();

		for clip in [2, 1] {
			let track = arrangement.tracks()[0].id;
			let removed = arrangement.detach_clip(0, clip);
			record_clip_remove(&mut arrangement, &mut history, track, clip, removed);
		}

		assert_eq!(arrangement.tracks()[0].clips, before[..1].to_vec());
		// The history keeps the removed clips' pattern alive for undo.
		assert_eq!(arrangement.midi_patterns()[&pattern].refs, 3);

		assert!(history.undo(&mut arrangement));
		assert_eq!(arrangement.tracks()[0].clips, before);
		assert_eq!(arrangement.midi_patterns()[&pattern].refs, 3);

		assert!(history.redo(&mut arrangement));
		assert_eq!(arrangement.tracks()[0].clips, before[..1].to_vec());
		assert_eq!(arrangement.midi_patterns()[&pattern].refs, 3);

		history.clear(&mut arrangement);
		assert_eq!(arrangement.midi_patterns()[&pattern].refs, 1);
	}

	#[test]
	fn undoing_a_split_removes_the_new_clip() {
		let (mut arrangement, _) = arrangement_with_pattern(vec![note(60, 0, 2)]);
		let mut history = History::default();
		let original = arrangement.tracks()[0].clips[0];

		let before = ClipSnapshot::of(&arrangement, 0, 0).unwrap();
		let rhs = arrangement.duplicate_clip(0, 0);
		arrangement.clip_trim_end_to(0, 0, BeatTime::BEAT);
		arrangement.clip_trim_start_to(0, rhs, BeatTime::BEAT);
		record_clip_add(&mut arrangement, &mut history, 0, rhs);
		record_clip_change(&mut arrangement, &mut history, before);

		assert_eq!(arrangement.tracks()[0].clips.len(), 2);

		assert!(history.undo(&mut arrangement));
		assert_eq!(arrangement.tracks()[0].clips, vec![original]);

		assert!(history.redo(&mut arrangement));
		let clips = &arrangement.tracks()[0].clips;
		assert_eq!(clips.len(), 2);
		assert_eq!(clips[0].start(), BeatTime::ZERO);
		assert_eq!(clips[0].end(arrangement.transport()), BeatTime::BEAT);
		assert_eq!(clips[1].start(), BeatTime::BEAT);
		assert_eq!(clips[1].end(arrangement.transport()), BeatTime::new(2, 0));
	}

	#[test]
	fn undoing_note_removals_restores_their_order() {
		let notes = vec![note(60, 0, 1), note(62, 1, 2), note(64, 2, 3)];
		let (mut arrangement, pattern) = arrangement_with_pattern(notes.clone());
		let mut history = History::default();

		// The piano roll removes the selected notes from the end.
		arrangement.remove_note(pattern, 1);
		arrangement.remove_note(pattern, 0);
		record_pattern_changes(&mut arrangement, &mut history, pattern, &notes);

		assert_eq!(arrangement.midi_patterns()[&pattern].notes, vec![notes[2]]);

		assert!(history.undo(&mut arrangement));
		assert_eq!(arrangement.midi_patterns()[&pattern].notes, notes);

		assert!(history.redo(&mut arrangement));
		assert_eq!(arrangement.midi_patterns()[&pattern].notes, vec![notes[2]]);
	}
}
