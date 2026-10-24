with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix freeze Task::future - the closure needs to return the Message in success case
old = """		Task::batch([
			Task::future(unblock(move || {
				let (mut processor, mut pool) = match p_receiver.recv() {
					Ok(v) => v,
					Err(_) => {
						warn!("failed to receive processor for freeze");
						return;
					}
				};
				processor.render(
					&mut pool,
					node,
					beat_range,
					|samples| recording.recorded(samples),
					|f| { let _ = progress_sender.force_send(f); },
				);
				if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!("failed to send processor back"); }
				// The message is sent via the channel, not returned
				let _ = project; // suppress unused warning
			})),
			Task::run(progress_receiver, |progress| {
				daw::Message::Progress(progress as f32)
			})
			.chain(Task::done(daw::Message::RenderedFile)),
		])"""
new = """		Task::batch([
			Task::future(unblock(move || {
				let (mut processor, mut pool) = match p_receiver.recv() {
					Ok(v) => v,
					Err(_) => {
						warn!("failed to receive processor for freeze");
						return daw::Message::RenderedFile;
					}
				};
				processor.render(
					&mut pool,
					node,
					beat_range,
					|samples| recording.recorded(samples),
					|f| { let _ = progress_sender.force_send(f); },
				);
				if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!("failed to send processor back"); }

				daw::Message::Arrangement(
					project,
					arrangement_view::Message::FreezeDone(
						node,
						Box::new(recording.finalize().1),
						beat_range.start(),
					),
				)
			})),
			Task::run(progress_receiver, |progress| {
				daw::Message::Progress(progress as f32)
			})
			.chain(Task::done(daw::Message::RenderedFile)),
		])"""
content = content.replace(old, new)
print("Fixed freeze Task::future return type properly")

# Fix borrow checker issues in MIDI pattern functions
# The issue is holding &mut self.midi_patterns while calling self.midi_pattern_action
# We need to release the borrow before the call

# Fix note_change_velocity
old = """\tpub fn note_change_velocity(&mut self, pattern: MidiPatternId, note: usize, velocity: f32) {
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
new = """\tpub fn note_change_velocity(&mut self, pattern: MidiPatternId, note: usize, velocity: f32) {
\t\tlet mut note_id = None;
\t\tlet mut should_send = false;
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.velocity != velocity {
\t\t\t\t\tn.velocity = velocity;
\t\t\t\t\tnote_id = Some(n.id);
\t\t\t\t\tshould_send = true;
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif should_send {
\t\t\tif let Some(note_id) = note_id {
\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::ChangeVelocity(note_id, velocity),
\t\t\t\t);
\t\t\t}
\t\t}
\t}"""
content = content.replace(old, new)
print("Fixed note_change_velocity borrow")

# Fix note_change_key
old = """\tpub fn note_change_key(&mut self, pattern: MidiPatternId, note: usize, key: MidiKey) {
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
new = """\tpub fn note_change_key(&mut self, pattern: MidiPatternId, note: usize, key: MidiKey) {
\t\tlet mut note_id = None;
\t\tlet mut should_send = false;
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.key != key {
\t\t\t\t\tn.key = key;
\t\t\t\t\tnote_id = Some(n.id);
\t\t\t\t\tshould_send = true;
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif should_send {
\t\t\tif let Some(note_id) = note_id {
\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::ChangeKey(note_id, key),
\t\t\t\t);
\t\t\t}
\t\t}
\t}"""
content = content.replace(old, new)
print("Fixed note_change_key borrow")

# Fix note_move_to
old = """\tpub fn note_move_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
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
new = """\tpub fn note_move_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tlet mut note_id = None;
\t\tlet mut should_send = false;
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.start() != pos {
\t\t\t\t\tn.position.move_to(pos);
\t\t\t\t\tnote_id = Some(n.id);
\t\t\t\t\tshould_send = true;
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif should_send {
\t\t\tif let Some(note_id) = note_id {
\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::MoveTo(note_id, pos),
\t\t\t\t);
\t\t\t}
\t\t}
\t}"""
content = content.replace(old, new)
print("Fixed note_move_to borrow")

# Fix note_trim_start_to
old = """\tpub fn note_trim_start_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
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
new = """\tpub fn note_trim_start_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tlet mut note_id = None;
\t\tlet mut should_send = false;
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.start() != pos {
\t\t\t\t\tn.position.trim_start_to(pos);
\t\t\t\t\tnote_id = Some(n.id);
\t\t\t\t\tshould_send = true;
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif should_send {
\t\t\tif let Some(note_id) = note_id {
\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::TrimStartTo(note_id, pos),
\t\t\t\t);
\t\t\t}
\t\t}
\t}"""
content = content.replace(old, new)
print("Fixed note_trim_start_to borrow")

# Fix note_trim_end_to
old = """\tpub fn note_trim_end_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
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
new = """\tpub fn note_trim_end_to(&mut self, pattern: MidiPatternId, note: usize, pos: BeatTime) {
\t\tlet mut note_id = None;
\t\tlet mut should_send = false;
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tif let Some(n) = p.notes.get_mut(note) {
\t\t\t\tif n.position.end() != pos {
\t\t\t\t\tn.position.trim_end_to(pos);
\t\t\t\t\tnote_id = Some(n.id);
\t\t\t\t\tshould_send = true;
\t\t\t\t}
\t\t\t}
\t\t}
\t\tif should_send {
\t\t\tif let Some(note_id) = note_id {
\t\t\t\tself.midi_pattern_action(
\t\t\t\t\tpattern,
\t\t\t\t\tMidiPatternAction::TrimEndTo(note_id, pos),
\t\t\t\t);
\t\t\t}
\t\t}
\t}"""
content = content.replace(old, new)
print("Fixed note_trim_end_to borrow")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)