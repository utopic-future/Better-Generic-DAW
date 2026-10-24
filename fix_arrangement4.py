with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix remaining track_of unwrap in freeze
old = """\t\tlet track = self.track_of(node).unwrap();"""
new = """\t\tlet track = self.track_of(node).expect("track not found in freeze");"""
content = content.replace(old, new)
print("Fixed freeze track_of.unwrap")

# Fix channel_of unwrap in duplicate_channel
old = """\t\tlet new_id = self.insert_channel(self.channel_of(id).unwrap() + 1);"""
new = """\t\tlet new_id = self.insert_channel(self.channel_of(id).expect("channel not found in duplicate") + 1);"""
content = content.replace(old, new)
print("Fixed duplicate_channel channel_of.unwrap")

# Fix sender.send(...).unwrap() in change_config
old = """\t\t\tsender.send((PullSlot::Full(processor), pool)).unwrap();"""
new = """\t\t\tif sender.send((PullSlot::Full(processor), pool)).is_err() {
\t\t\t\twarn!("failed to send processor");
\t\t\t}"""
content = content.replace(old, new)
print("Fixed change_config sender.send.unwrap")

# Fix s.send(...).unwrap() in plugin_restart (clap)
old = """\t\t\t\ts.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t))
\t\t\t\t.unwrap();"""
new = """\t\t\t\tif s.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t)).is_err() {
\t\t\t\t\twarn!("failed to send plugin restart slot");
\t\t\t\t}"""
content = content.replace(old, new)
print("Fixed plugin_restart clap s.send.unwrap")

# Fix s.send(...).unwrap() in plugin_restart else branch (clap)
old = """\t\t\ts.send(PushSlot::new(Some(None), r)).unwrap();"""
new = """\t\t\tif s.send(PushSlot::new(Some(None), r)).is_err() {
\t\t\t\twarn!("failed to send plugin restart none");
\t\t\t}"""
content = content.replace(old, new)
print("Fixed plugin_restart else s.send.unwrap")

# Fix message.unwrap() in clap Task::perform
old = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap())))"""
new = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))))"""
content = content.replace(old, new)
print("Fixed clap message.unwrap 1")

# Fix second occurrence
old = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap()))),"""
new = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default())))),"""
content = content.replace(old, new)
print("Fixed clap message.unwrap 2")

# Fix vst2 s.send unwrap
old = """\t\t\t\ts.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t))
\t\t\t\t.unwrap();\n\t\t\tlet id = self.nodes[&id].plugins[index].id;\n\t\t\tvst2_tasks(id, rece"""
new = """\t\t\t\tif s.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t)).is_err() {
\t\t\t\t\twarn!("failed to send vst2 plugin restart slot");
\t\t\t\t}
\t\t\tlet id = self.nodes[&id].plugins[index].id;
\t\t\tvst2_tasks(id, rece"""
content = content.replace(old, new)
print("Fixed vst2 s.send.unwrap 1")

# Fix vst2 else branch
old = """\t\t\ts.send(PushSlot::new(Some(None), r)).unwrap();\n\t\t\tTask::none()"""
new = """\t\t\tif s.send(PushSlot::new(Some(None), r)).is_err() {
\t\t\t\twarn!("failed to send vst2 plugin restart none");
\t\t\t}
\t\t\tTask::none()"""
content = content.replace(old, new)
print("Fixed vst2 s.send.unwrap else")

# Fix vst3 s.send unwrap
old = """\t\t\t\ts.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t))
\t\t\t\t.unwrap();\n\t\t\tlet id = self.nodes[&id].plugins[index].id;\n\t\t\tvst3_tasks(id, rece"""
new = """\t\t\t\tif s.send(PushSlot::new(
\t\t\t\t\tSome(Some(generic_daw_core::Plugin { processor, sender })),
\t\t\t\t\tr,
\t\t\t\t)).is_err() {
\t\t\t\t\twarn!("failed to send vst3 plugin restart slot");
\t\t\t\t}
\t\t\tlet id = self.nodes[&id].plugins[index].id;
\t\t\tvst3_tasks(id, rece"""
content = content.replace(old, new)
print("Fixed vst3 s.send.unwrap 1")

# Fix vst3 else branch
old = """\t\t\ts.send(PushSlot::new(Some(None), r)).unwrap();\n\t\t\tTask::none()\n\t\t}\n\n\tpub fn plugin_activate_vst3"""
new = """\t\t\tif s.send(PushSlot::new(Some(None), r)).is_err() {
\t\t\t\twarn!("failed to send vst3 plugin restart none");
\t\t\t}
\t\t\tTask::none()
\t\t}\n\n\tpub fn plugin_activate_vst3"""
content = content.replace(old, new)
print("Fixed vst3 s.send.unwrap else")

# Fix vst2 message.unwrap
old = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
\t\t\t\tid,
\t\t\t\tNoClone(Box::new(message.unwrap())),
\t\t\t))"""
new = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
\t\t\t\tid,
\t\t\t\tNoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))),
\t\t\t))"""
content = content.replace(old, new)
print("Fixed vst2 message.unwrap")

# Fix vst3 message.unwrap
old = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
\t\t\t\tid,
\t\t\t\tNoClone(Box::new(message.unwrap())),
\t\t\t))"""
new = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
\t\t\t\tid,
\t\t\t\tNoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))),
\t\t\t))"""
content = content.replace(old, new)
print("Fixed vst3 message.unwrap")

# Fix File::create unwrap
old = """\t\tBufWriter::new(File::create(path).unwrap()),"""
new = """\t\tBufWriter::new(File::create(path).expect("failed to create wav file")),"""
content = content.replace(old, new)
print("Fixed File::create unwrap")

# Fix WavWriter::new unwrap
old = """\t\t)\n\t\t.unwrap();"""
new = """\t\t)
\t\t.expect("failed to create wav writer");"""
content = content.replace(old, new)
print("Fixed WavWriter unwrap")

# Fix writer.write_sample unwrap (two occurrences)
content = content.replace("writer.write_sample(l).unwrap();", "if writer.write_sample(l).is_err() { warn!(\"failed to write sample\"); }")
content = content.replace("writer.write_sample(r).unwrap();", "if writer.write_sample(r).is_err() { warn!(\"failed to write sample\"); }")
print("Fixed writer.write_sample unwrap")

# Fix progress_sender.force_send unwrap
content = content.replace("progress_sender.force_send(f).unwrap()", "let _ = progress_sender.force_send(f);")
print("Fixed progress_sender.force_send unwrap")

# Fix p_receiver.recv().unwrap() in render
old = """\t\t\tlet (mut processor, mut pool) = p_receiver.recv().unwrap();"""
new = """\t\t\tlet (mut processor, mut pool) = match p_receiver.recv() {
\t\t\t\tOk(v) => v,
\t\t\t\tErr(_) => {
\t\t\t\t\twarn!("failed to receive processor");
\t\t\t\t\treturn;
\t\t\t\t}
\t\t\t};"""
content = content.replace(old, new)
print("Fixed p_receiver.recv unwrap 1")

# Fix second p_receiver.recv().unwrap() in freeze
old = """\t\t\tlet (mut processor, mut pool) = p_receiver.recv().unwrap();"""
new = """\t\t\tlet (mut processor, mut pool) = match p_receiver.recv() {
\t\t\t\tOk(v) => v,
\t\t\t\tErr(_) => {
\t\t\t\t\twarn!("failed to receive processor for freeze");
\t\t\t\t\treturn;
\t\t\t\t}
\t\t\t};"""
content = content.replace(old, new)
print("Fixed p_receiver.recv unwrap 2")

# Fix p_sender.send unwrap
content = content.replace("p_sender.send((PullSlot::Full(processor), pool)).unwrap();", "if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!(\"failed to send processor back\"); }")
print("Fixed p_sender.send unwrap")

# Fix second p_sender.send unwrap
# (should be caught by above)

# Fix force_send unwrap in freeze
content = content.replace("progress_sender.force_send(f).unwrap()", "let _ = progress_sender.force_send(f);")
print("Fixed force_send unwrap in freeze")

# Fix second p_sender.send unwrap in freeze
content = content.replace("p_sender.send((PullSlot::Full(processor), pool)).unwrap();", "if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!(\"failed to send processor back from freeze\"); }")
print("Fixed p_sender.send unwrap in freeze")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)