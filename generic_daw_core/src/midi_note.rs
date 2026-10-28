use crate::time::{BeatRange, BeatTime};
use std::fmt::{Debug, Display, Formatter};
use utils::{unique_id, variants};

unique_id!(MidiNoteId);

variants! {
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Key {
	C,
	CSharp,
	D,
	DSharp,
	E,
	F,
	FSharp,
	G,
	GSharp,
	A,
	ASharp,
	B,
}
}

impl Key {
	#[must_use]
	pub const fn is_black(self) -> bool {
		matches!(
			self,
			Self::CSharp | Self::DSharp | Self::FSharp | Self::GSharp | Self::ASharp
		)
	}
}

impl Display for Key {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		f.write_str(match self {
			Self::C => "C",
			Self::CSharp => "C#",
			Self::D => "D",
			Self::DSharp => "D#",
			Self::E => "E",
			Self::F => "F",
			Self::FSharp => "F#",
			Self::G => "G",
			Self::GSharp => "G#",
			Self::A => "A",
			Self::ASharp => "A#",
			Self::B => "B",
		})
	}
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MidiKey(pub u8);

impl MidiKey {
	#[must_use]
	pub fn key(self) -> Key {
		Key::VARIANTS[usize::from(self.0) % 12]
	}

	#[must_use]
	pub const fn octave(self) -> i8 {
		self.0.cast_signed() / 12 - 2
	}
}

impl Display for MidiKey {
	fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}{}", self.key(), self.octave())
	}
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiNote {
	pub id: MidiNoteId,
	pub key: MidiKey,
	pub velocity: f32,
	pub position: BeatRange,
}


/// A sparse, note-relative point in an MPE expression curve.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MidiExpressionPoint {
	/// The time from the note's start.
	pub position: BeatTime,
	/// Pitch bend uses -1.0..=1.0; pressure and timbre use 0.0..=1.0.
	pub value: f32,
}

/// The MPE expression dimension represented by a curve.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum MidiExpressionKind {
	#[default]
	PitchBend,
	Pressure,
	Timbre,
}

/// Per-note MPE expression curves.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct MidiNoteExpression {
	pub pitch_bend: Vec<MidiExpressionPoint>,
	pub pressure: Vec<MidiExpressionPoint>,
	pub timbre: Vec<MidiExpressionPoint>,
}


impl MidiNoteExpression {
	#[must_use]
	pub fn curve(&self, kind: MidiExpressionKind) -> &[MidiExpressionPoint] {
		match kind {
			MidiExpressionKind::PitchBend => &self.pitch_bend,
			MidiExpressionKind::Pressure => &self.pressure,
			MidiExpressionKind::Timbre => &self.timbre,
		}
	}

	pub fn curve_mut(&mut self, kind: MidiExpressionKind) -> &mut Vec<MidiExpressionPoint> {
		match kind {
			MidiExpressionKind::PitchBend => &mut self.pitch_bend,
			MidiExpressionKind::Pressure => &mut self.pressure,
			MidiExpressionKind::Timbre => &mut self.timbre,
		}
	}
}
