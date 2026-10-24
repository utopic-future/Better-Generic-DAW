with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix 1: ref counting functions
old = """\tfn add_ref_sample(&mut self, sample: SampleId) {
\t\tself.samples.get_mut(&sample).unwrap().refs += 1;
\t}

\tfn remove_ref_sample(&mut self, sample: SampleId) {
\t\tself.samples.get_mut(&sample).unwrap().refs -= 1;
\t}

\tfn gc_sample(&mut self, sample: SampleId) {
\t\tif self.samples[&sample].refs == 0 {
\t\t\tself.samples.remove(&sample);
\t\t\tself.send(Message::SampleRemove(
\t\t\t\tsample,
\t\t\t\tBox::new(MaybeUninit::uninit()),
\t\t\t));
\t\t}
\t}

\tfn add_ref_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tself.midi_patterns.get_mut(&pattern).unwrap().refs += 1;
\t}

\tfn remove_ref_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tself.midi_patterns.get_mut(&pattern).unwrap().refs -= 1;
\t}

\tfn gc_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tif self.midi_patterns[&pattern].refs == 0 {
\t\t\tself.midi_patterns.remove(&pattern);
\t\t\tself.send(Message::MidiPatternRemove(
\t\t\t\tpattern,
\t\t\t\tBox::new(MaybeUninit::uninit()),
\t\t\t));
\t\t}
\t}"""

new = """\tfn add_ref_sample(&mut self, sample: SampleId) {
\t\tif let Some(s) = self.samples.get_mut(&sample) {
\t\t\ts.refs += 1;
\t\t}
\t}

\tfn remove_ref_sample(&mut self, sample: SampleId) {
\t\tif let Some(s) = self.samples.get_mut(&sample) {
\t\t\ts.refs -= 1;
\t\t}
\t}

\tfn gc_sample(&mut self, sample: SampleId) {
\t\tif let Some(s) = self.samples.get(&sample) {
\t\t\tif s.refs == 0 {
\t\t\t\tself.samples.remove(&sample);
\t\t\t\tself.send(Message::SampleRemove(
\t\t\t\t\tsample,
\t\t\t\t\tBox::new(MaybeUninit::uninit()),
\t\t\t\t));
\t\t\t}
\t\t}
\t}

\tfn add_ref_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tp.refs += 1;
\t\t}
\t}

\tfn remove_ref_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tif let Some(p) = self.midi_patterns.get_mut(&pattern) {
\t\t\tp.refs -= 1;
\t\t}
\t}

\tfn gc_midi_pattern(&mut self, pattern: MidiPatternId) {
\t\tif let Some(p) = self.midi_patterns.get(&pattern) {
\t\t\tif p.refs == 0 {
\t\t\t\tself.midi_patterns.remove(&pattern);
\t\t\t\tself.send(Message::MidiPatternRemove(
\t\t\t\t\tpattern,
\t\t\t\t\tBox::new(MaybeUninit::uninit()),
\t\t\t\t));
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 1: ref counting functions done!")
else:
    print("Fix 1: OLD NOT FOUND!")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)