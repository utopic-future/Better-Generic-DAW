use generic_daw_core::{
	MidiExpressionPoint, MidiNote, MidiNoteExpression, MidiNoteId, MidiPatternId, Transport,
	time::BeatTime,
};
use std::{collections::HashMap, path::Path, sync::Arc};

#[derive(Clone, Debug)]
pub struct MidiPattern {
	pub id: MidiPatternId,
	pub name: Arc<str>,
	pub notes: Vec<MidiNote>,
	pub expression: HashMap<MidiNoteId, MidiNoteExpression>,
	pub refs: usize,
}

impl MidiPattern {
	pub fn len(&self) -> BeatTime {
		self.notes
			.iter()
			.map(|note| note.position.end())
			.max()
			.unwrap_or_default()
	}
}

#[derive(Clone, Debug)]
pub struct MidiPatternPair {
	pub core: generic_daw_core::MidiPattern,
	pub gui: MidiPattern,
}

impl MidiPatternPair {
	pub fn from_notes(notes: Vec<MidiNote>, name: &str) -> Self {
		Self::from_notes_with_expression(notes, HashMap::new(), name)
	}

	pub fn from_notes_with_expression(
		notes: Vec<MidiNote>,
		expression: HashMap<MidiNoteId, MidiNoteExpression>,
		name: &str,
	) -> Self {
		let mut core = generic_daw_core::MidiPattern::from_notes(&notes);
		core.expression = expression.clone();
		let gui = MidiPattern {
			id: core.id,
			name: name.into(),
			notes,
			expression,
			refs: 0,
		};
		Self { core, gui }
	}

	pub fn from_midi(path: &Path, transport: &Transport) -> Option<Self> {
		let name = path.file_name()?.to_str()?;
		let notes =
			generic_daw_core::MidiPattern::parse_midi(&std::fs::read(path).ok()?, transport)?;
		Some(Self::from_notes(notes, name))
	}
}
