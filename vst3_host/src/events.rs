use std::mem::zeroed;
use vst3::Steinberg::Vst::{Event as RawEvent, Event_::EventTypes_, NoteOffEvent, NoteOnEvent};

/// Something that happens at a point in a block, in either direction.
///
/// `time` is in frames from the start of the block, for events sent to the plugin the block that
/// is processed next, and for events from the plugin the block that was just processed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
	NoteOn {
		time: u32,
		channel: i16,
		/// The MIDI note number, `60` is middle C.
		pitch: i16,
		velocity: f32,
		/// Identifies the note, so that the matching note off (and per-note automation) finds it.
		note_id: Option<i32>,
	},
	NoteOff {
		time: u32,
		channel: i16,
		pitch: i16,
		velocity: f32,
		note_id: Option<i32>,
	},
	/// Sets a parameter to a normalized value (`0.0..=1.0`).
	ParamValue { time: u32, id: u32, value: f64 },
}

impl Event {
	#[must_use]
	pub const fn time(&self) -> u32 {
		match *self {
			Self::NoteOn { time, .. }
			| Self::NoteOff { time, .. }
			| Self::ParamValue { time, .. } => time,
		}
	}

	/// The VST3 form of a note event. Parameter changes don't travel in event lists.
	pub(crate) fn to_raw(self, sample_offset: i32) -> Option<RawEvent> {
		// SAFETY:
		// `Event` is plain old data, all zeroes is a valid value.
		let mut raw = unsafe { zeroed::<RawEvent>() };
		raw.sampleOffset = sample_offset;

		match self {
			Self::NoteOn {
				channel,
				pitch,
				velocity,
				note_id,
				..
			} => {
				raw.r#type = EventTypes_::kNoteOnEvent as u16;
				raw.__field0.noteOn = NoteOnEvent {
					channel,
					pitch,
					tuning: 0.0,
					velocity,
					length: 0,
					noteId: note_id.unwrap_or(-1),
				};
			}
			Self::NoteOff {
				channel,
				pitch,
				velocity,
				note_id,
				..
			} => {
				raw.r#type = EventTypes_::kNoteOffEvent as u16;
				raw.__field0.noteOff = NoteOffEvent {
					channel,
					pitch,
					velocity,
					noteId: note_id.unwrap_or(-1),
					tuning: 0.0,
				};
			}
			Self::ParamValue { .. } => return None,
		}

		Some(raw)
	}

	/// Converts a note event from a plugin, other kinds of events are ignored.
	pub(crate) fn from_raw(raw: &RawEvent, time: u32) -> Option<Self> {
		let note_id = |id: i32| (id != -1).then_some(id);

		if i64::from(raw.r#type) == i64::from(EventTypes_::kNoteOnEvent) {
			// SAFETY:
			// The event type says that `noteOn` is the active field.
			let note = unsafe { raw.__field0.noteOn };

			Some(Self::NoteOn {
				time,
				channel: note.channel,
				pitch: note.pitch,
				velocity: note.velocity,
				note_id: note_id(note.noteId),
			})
		} else if i64::from(raw.r#type) == i64::from(EventTypes_::kNoteOffEvent) {
			// SAFETY:
			// The event type says that `noteOff` is the active field.
			let note = unsafe { raw.__field0.noteOff };

			Some(Self::NoteOff {
				time,
				channel: note.channel,
				pitch: note.pitch,
				velocity: note.velocity,
				note_id: note_id(note.noteId),
			})
		} else {
			None
		}
	}
}
