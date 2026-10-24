with open('generic_daw_core/src/track.rs', 'r') as f:
    content = f.read()

# Replace the second section
old = """\t\t\tNodeAction::ClipTrimEndTo(id, pos) => {
\t\t\t\tlet clip = self.clips.get_mut(&id).unwrap();
\t\t\t\tclip.trim_end_to(pos, &state.transport);
\t\t\t\tif let Clip::Audio(audio) = clip {
\t\t\t\t\taudio.fade_end.len = audio.fade_end.len.min(audio.position.len());
\t\t\t\t\taudio.fade_start.len = audio
\t\t\t\t\t\t.fade_start
\t\t\t\t\t\t.len
\t\t\t\t\t\t.min(audio.position.len() - audio.fade_end.len);
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipVolumeChanged(id, volume) => {
\t\t\t\tlet Clip::Audio(clip) = self.clips.get_mut(&id).unwrap() else {
\t\t\t\t\tpanic!();
\t\t\t\t};
\t\t\t\tclip.volume = volume;
\t\t\t}"""

new = """\t\t\tNodeAction::ClipTrimEndTo(id, pos) => {
\t\t\t\tif let Some(clip) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.trim_end_to(pos, &state.transport);
\t\t\t\t\tif let Clip::Audio(audio) = clip {
\t\t\t\t\t\taudio.fade_end.len = audio.fade_end.len.min(audio.position.len());
\t\t\t\t\t\taudio.fade_start.len = audio
\t\t\t\t\t\t\t.fade_start
\t\t\t\t\t\t\t.len
\t\t\t\t\t\t\t.min(audio.position.len() - audio.fade_end.len);
\t\t\t\t\t}
\t\t\t\t}
\t\t\t}
\t\t\tNodeAction::ClipVolumeChanged(id, volume) => {
\t\t\t\tif let Some(Clip::Audio(clip)) = self.clips.get_mut(&id) {
\t\t\t\t\tclip.volume = volume;
\t\t\t\t}
\t\t\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Second replacement done!")
else:
    print("OLD NOT FOUND!")

with open('generic_daw_core/src/track.rs', 'w') as f:
    f.write(content)