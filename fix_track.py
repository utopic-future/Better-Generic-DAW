with open('generic_daw_core/src/track.rs', 'r') as f:
    content = f.read()

# Replace the first section
old = """\t\t\tNodeAction::ClipMoveTo(id, pos) => self.clips.get_mut(&id).unwrap().move_to(pos),
\t\t\tNodeAction::ClipTrimStartTo(id, pos) => {
\t\t\t\tlet clip = self.clips.get_mut(&id).unwrap();
\t\t\t\tclip.trim_start_to(pos, &state.transport);
\t\t\t\tif let Clip::Audio(audio) = clip {
\t\t\t\t\taudio.fade_start.len = audio.fade_start.len.min(audio.position.len());
\t\t\t\t\taudio.fade_end.len = audio
\t\t\t\t\t\t.fade_end
\t\t\t\t\t\t.len
\t\t\t\t\t\t.min(audio.position.len() - audio.fade_start.len);
\t\t\t\t}
\t\t\t}"""

new = """\t\t\tNodeAction::ClipMoveTo(id, pos) => {
\t\t\t\tif let Some(clip) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.move_to(pos);
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipTrimStartTo(id, pos) => {
\t\t\t\tif let Some(clip) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.trim_start_to(pos, &state.transport);
\t\t\t\t\tif let Clip::Audio(audio) = clip {
\t\t\t\t\t\taudio.fade_start.len = audio.fade_start.len.min(audio.position.len());
\t\t\t\t\t\taudio.fade_end.len = audio
\t\t\t\t\t\t\t.fade_end
\t\t\t\t\t\t\t.len
\t\t\t\t\t\t\t.min(audio.position.len() - audio.fade_start.len);
\t\t\t\t\t}
\t\t\t\t}
\t\t\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("First replacement done!")
else:
    print("OLD NOT FOUND!")
    idx = content.find("\t\t\tNodeAction::ClipMoveTo")
    if idx >= 0:
        print(repr(content[idx:idx+500]))

with open('generic_daw_core/src/track.rs', 'w') as f:
    f.write(content)