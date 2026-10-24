with open('generic_daw_core/src/track.rs', 'r') as f:
    content = f.read()

# Replace all remaining Clip* actions that use unwrap/panic
old = """\t\t\tNodeAction::ClipFadeStartLen(id, len) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_start.len = len;
\t\t\t}
\t\t\tNodeAction::ClipFadeStartP(id, p) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_start.p = p;
\t\t\t}
\t\t\tNodeAction::ClipFadeStartToggleSymmetric(id) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_start.symmetric ^= true;
\t\t\t}
\t\t\tNodeAction::ClipFadeEndLen(id, len) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_end.len = len;
\t\t\t}
\t\t\tNodeAction::ClipFadeEndP(id, p) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_end.p = p;
\t\t\t}
\t\t\tNodeAction::ClipFadeEndToggleSymmetric(id) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.fade_end.symmetric ^= true;
\t\t\t}
\t\t\tNodeAction::ClipStretchStartTo(id, pos) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tlet len = clip.position.len() * clip.stretch.abs();
\t\t\t\tlet end = clip.position.end(&state.transport);
\t\t\t\tclip.position.move_to(pos.min(end - BeatTime::TICK));
\t\t\t\tclip.position.trim_end_to(end, &state.transport);
\t\t\t\tlet stretch = clip.stretch;
\t\t\t\tclip.stretch = (len / clip.position.len()).copysign(stretch);
\t\t\t\tclip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
\t\t\t\tclip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
\t\t\t}
\t\t\tNodeAction::ClipStretchEndTo(id, pos) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tlet len = clip.position.len() * clip.stretch.abs();
\t\t\t\tclip.position.trim_end_to(pos, &state.transport);
\t\t\t\tlet stretch = clip.stretch;
\t\t\t\tclip.stretch = (len / clip.position.len()).copysign(clip.stretch);
\t\t\t\tclip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
\t\t\t\tclip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
\t\t\t}
\t\t\tNodeAction::ClipReverse(id) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.stretch *= -1.0;
\t\t\t\tclip.position.reverse(
\t\t\t\t\tstate.samples[&clip.sample].len(&state.transport),
\t\t\t\t\tclip.stretch.abs(),
\t\t\t\t);
\t\t\t\t(clip.fade_start, clip.fade_end) = (clip.fade_end, clip.fade_start);
\t\t\t}
\t\t\tNodeAction::ClipSlipTo(id, pos) => {
\t\t\t\tself.clips
\t\t\t\t\t.get_mut(&id)
\t\t\t\t\t.unwrap()
\t\t\t\t\t.slip_to(pos, &state.transport);
\t\t\t}"""

new = """\t\t\tNodeAction::ClipFadeStartLen(id, len) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_start.len = len;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipFadeStartP(id, p) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_start.p = p;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipFadeStartToggleSymmetric(id) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_start.symmetric ^= true;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipFadeEndLen(id, len) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_end.len = len;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipFadeEndP(id, p) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_end.p = p;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipFadeEndToggleSymmetric(id) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.fade_end.symmetric ^= true;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipStretchStartTo(id, pos) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tlet len = clip.position.len() * clip.stretch.abs();
\t\t\t\t\tlet end = clip.position.end(&state.transport);
\t\t\t\t\tclip.position.move_to(pos.min(end - BeatTime::TICK));
\t\t\t\t\tclip.position.trim_end_to(end, &state.transport);
\t\t\t\t\tlet stretch = clip.stretch;
\t\t\t\t\tclip.stretch = (len / clip.position.len()).copysign(stretch);
\t\t\t\t\tclip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
\t\t\t\t\tclip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipStretchEndTo(id, pos) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tlet len = clip.position.len() * clip.stretch.abs();
\t\t\t\t\tclip.position.trim_end_to(pos, &state.transport);
\t\t\t\t\tlet stretch = clip.stretch;
\t\t\t\t\tclip.stretch = (len / clip.position.len()).copysign(clip.stretch);
\t\t\t\t\tclip.fade_start.len = clip.fade_start.len * stretch / clip.stretch;
\t\t\t\t\tclip.fade_end.len = clip.fade_end.len * stretch / clip.stretch;
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipReverse(id) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.stretch *= -1.0;
\t\t\t\t\tclip.position.reverse(
\t\t\t\t\t\tstate.samples[&clip.sample].len(&state.transport),
\t\t\t\t\t\tclip.stretch.abs(),
\t\t\t\t\t);
\t\t\t\t\t(clip.fade_start, clip.fade_end) = (clip.fade_end, clip.fade_start);
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipSlipTo(id, pos) => {
\t\t\t\tif let Some(clip) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.slip_to(pos, &state.transport);
\t\t\t\t}
\t\t\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Third replacement done!")
else:
    print("OLD NOT FOUND!")

with open('generic_daw_core/src/track.rs', 'w') as f:
    f.write(content)