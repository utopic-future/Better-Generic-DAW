with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix 2: MIDI pattern note operations
old = """\tfn insert_note(&mut self, pattern: MidiPatternId, note: MidiNote, index: usize) -> usize {
\t\tself.midi_patterns
\t\t\t.get_mut(&pattern)
\t\t\t.unwrap()
\t\t\t.notes
\t\t\t.insert(index, note);
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Add(note));
\t\tindex
\t}"""

new = """\tfn insert_note(&mut self, pattern: MidiPatternId, note: MidiNote, index: usize) -> usize {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tp.notes.insert(index, note);
\t\t}
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Add(note));
\t\tindex
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2a: insert_note done!")
else:
    print("Fix 2a: OLD NOT FOUND!")

# Fix insert_note_at
old = """\tpub fn insert_note_at(&mut self, pattern: MidiPatternId, index: usize, note: MidiNote) {
\t\tlet notes = &mut self.midi_patterns.get_mut(&pattern).unwrap().notes;
\t\tlet index = index.min(notes.len());
\t\tnotes.insert(index, note);
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Add(note));
\t}"""

new = """\tpub fn insert_note_at(&mut self, pattern: MidiPatternId, index: usize, note: MidiNote) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tlet notes = &mut p.notes;
\t\t\tlet index = index.min(notes.len());
\t\t\tnotes.insert(index, note);
\t\t}
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Add(note));
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2b: insert_note_at done!")
else:
    print("Fix 2b: OLD NOT FOUND!")

# Fix replace_note
old = """\t/// Replaces the contents of a note.
\tpub fn replace_note(&mut self, pattern: MidiPatternId, note: MidiNote) {
\t\tif let Some(old) = self
\t\t\t.midi_patterns
\t\t\t.get_mut(&pattern)
\t\t\t.unwrap()
\t\t\t.notes
\t\t\t.iter_mut()
\t\t\t.find(|old| old.id == note.id)
\t\t\t&& *old != note
\t\t{
\t\t\t*old = note;
\t\t\tself.midi_pattern_action(pattern, MidiPatternAction::Replace(note));
\t\t}
\t}"""

new = """\t/// Replaces the contents of a note.
\tpub fn replace_note(&mut self, pattern: MidiPatternId, note: MidiNote) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(old) = p.notes.iter_mut().find(|old| old.id == note.id)
\t\t\t\t&& *old != note
\t\t\t{
\t\t\t\t*old = note;
\t\t\t\tself.midi_pattern_action(pattern, MidiPatternAction::Replace(note));
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2c: replace_note done!")
else:
    print("Fix 2c: OLD NOT FOUND!")

# Fix remove_note
old = """\tpub fn remove_note(&mut self, pattern: MidiPatternId, note: usize) -> MidiNote {
\t\tlet note = self
\t\t\t.midi_patterns
\t\t\t.get_mut(&pattern)
\t\t\t.unwrap()
\t\t\t.notes
\t\t\t.remove(note);
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Remove(note.id));
\t\tnote
\t}"""

new = """\tpub fn remove_note(&mut self, pattern: MidiPatternId, note: usize) -> Option<MidiNote> {
\t\tlet note = self
\t\t\t.midi_patterns
\t\t\t.get_mut(&pattern)?
\t\t\t.notes
\t\t\t.remove(note);
\t\tself.midi_pattern_action(pattern, MidiPatternAction::Remove(note.id));
\t\tSome(note)
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2d: remove_note done!")
else:
    print("Fix 2d: OLD NOT FOUND!")

# Fix note_change_velocity
old = """\tpub fn note_change_velocity(&mut self, pattern: MidiPatternId, note: usize, velocity: f32) {
\t\tif self.midi_patterns[&pattern].notes[note].velocity != velocity {
\t\t\tself.midi_patterns.get_mut(&pattern).unwrap().notes[note].velocity = velocity;
\t\t\tself.midi_pattern_action(
\t\t\t\tpattern,
\t\t\t\tMidiPatternAction::ChangeVelocity(
\t\t\t\t\tself.midi_patterns[&pattern].notes[note].id,
\t\t\t\t\tvelocity,
\t\t\t\t),
\t\t\t);
\t\t}
\t}"""

new = """\tpub fn note_change_velocity(&mut self, pattern: MidiPatternId, note: usize, velocity: f32) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.velocity != velocity {
\t\t\t\t\tn.velocity = velocity;
\t\t\t\t\tself.midi_pattern_action(
\t\t\t\t\t\tpattern,
\t\t\t\t\t\tMidiPatternAction::ChangeVelocity(n.id, velocity),
\t\t\t\t\t);
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2e: note_change_velocity done!")
else:
    print("Fix 2e: OLD NOT FOUND!")

# Fix note_change_key
old = """\tpub fn note_change_key(&mut self, pattern: MidiPatternId, note: usize, key: MidiKey) {
\t\tif self.midi_patterns[&pattern].notes[note].key != key {
\t\t\tself.midi_patterns.get_mut(&pattern).unwrap().notes[note].key = key;
\t\t\tself.midi_pattern_action(
\t\t\t\tpattern,
\t\t\t\tMidiPatternAction::ChangeKey(self.midi_patterns[&pattern].notes[note].id, key),
\t\t\t);
\t\t}
\t}"""

new = """\tpub fn note_change_key(&mut self, pattern: MidiPatternId, note: usize, key: MidiKey) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.key != key {
\t\t\t\t\tn.key = key;
\t\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::ChangeKey(n.id, key),
\t\t\t\t\t);
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2f: note_change_key done!")
else:
    print("Fix 2f: OLD NOT FOUND!")

# Fix note_move_to
old = """\tpub fn note_move_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif self.midi_patterns[&pattern].notes[note].position.start() != pos {
\t\t\tself.midi_patterns.get_mut(&pattern).unwrap().notes[note]
\t\t\t\t.position
\t\t\t\t.move_to(pos);
\t\t\tself.midi_pattern_action(
\t\t\t\tpattern,
\t\t\t\tMidiPatternAction::MoveTo(self.midi_patterns[&pattern].notes[note].id, pos),
\t\t\t);
\t\t}
\t}"""

new = """\tpub fn note_move_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.start() != pos {
\t\t\t\t\tn.position.move_to(pos);
\t\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::MoveTo(n.id, pos),
\t\t\t\t\t);
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2g: note_move_to done!")
else:
    print("Fix 2g: OLD NOT FOUND!")

# Fix note_trim_start_to
old = """\tpub fn note_trim_start_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif self.midi_patterns[&pattern].notes[note].position.start() != pos {
\t\t\tself.midi_patterns.get_mut(&pattern).unwrap().notes[note]
\t\t\t\t.position
\t\t\t\t.trim_start_to(pos);
\t\t\tself.midi_pattern_action(
\t\t\t\tpattern,
\t\t\t\tMidiPatternAction::TrimStartTo(self.midi_patterns[&pattern].notes[note].id, pos),
\t\t\t);
\t\t}
\t}"""

new = """\tpub fn note_trim_start_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.start() != pos {
\t\t\t\t\tn.position.trim_start_to(pos);
\t\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::TrimStartTo(n.id, pos),
\t\t\t\t\t);
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2h: note_trim_start_to done!")
else:
    print("Fix 2h: OLD NOT FOUND!")

# Fix note_trim_end_to
old = """\tpub fn note_trim_end_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif self.midi_patterns[&pattern].notes[note].position.end() != pos {
\t\t\tself.midi_patterns.get_mut(&pattern).unwrap().notes[note]
\t\t\t\t.position
\t\t\t\t.trim_end_to(pos);
\t\t\tself.midi_pattern_action(
\t\t\t\tpattern,
\t\t\t\tMidiPatternAction::TrimEndTo(self.midi_patterns[&pattern].notes[note].id, pos),
\t\t\t);
\t\t}
\t}"""

new = """\tpub fn note_trim_end_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.end() != pos {
\t\t\t\t\tn.position.trim_end_to(pos);
\t\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::TrimEndTo(n.id, pos),
\t\t\t\t\t);
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 2i: note_trim_end_to done!")
else:
    print("Fix 2i: OLD NOT FOUND!")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)