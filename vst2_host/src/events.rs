/// Something that happens at a point in a block, in either direction.
///
/// `time` is in frames from the start of the block, for events sent to the plugin the
/// block that is processed next, and for events from the plugin the block that was just
/// processed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Event {
	/// A note starts.
	NoteOn {
		time: u32,
		/// The MIDI note number, `60` is middle C.
		key: u8,
		/// The velocity of the note, `0.0..=1.0`.
		velocity: f32,
	},
	/// A note ends.
	NoteOff {
		time: u32,
		key: u8,
		velocity: f32,
	},
	/// Sets a parameter to a normalized value (`0.0..=1.0`).
	ParamValue { time: u32, id: u32, value: f32 },
}

impl Event {
	/// The time of the event, in frames from the start of the block.
	#[must_use]
	pub const fn time(&self) -> u32 {
		match *self {
			Self::NoteOn { time, .. }
			| Self::NoteOff { time, .. }
			| Self::ParamValue { time, .. } => time,
		}
	}

	/// The MIDI bytes of the event, if it's a note.
	#[must_use]
	pub const fn to_midi(self) -> Option<[u8; 3]> {
		match self {
			Self::NoteOn { key, velocity, .. } => {
				Some([0x90, key, (velocity * 127.0).round().max(1.0) as u8])
			}
			Self::NoteOff { key, velocity, .. } => {
				Some([0x80, key, (velocity * 127.0).round() as u8])
			}
			Self::ParamValue { .. } => None,
		}
	}
}
