with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix force_send closure syntax in render
old = """					|f| _ = let _ = progress_sender.force_send(f);,"""
new = """					|f| { let _ = progress_sender.force_send(f); },"""
content = content.replace(old, new)
print("Fixed force_send closure in render")

# Fix force_send closure syntax in freeze
old = """					|f| _ = let _ = progress_sender.force_send(f);,"""
new = """					|f| { let _ = progress_sender.force_send(f); },"""
content = content.replace(old, new)
print("Fixed force_send closure in freeze")

# Fix AudioThreadMessage default - need to check what the enum looks like
# For now, let's use a dummy message that won't be processed
# Actually, let's look at what message.unwrap() was doing - it's getting the message from oneshot
# If the oneshot is dropped, we should just not send anything
# Let's change the approach: instead of unwrap_or_else with default, just handle the error

# Fix clap message.unwrap_or_else
old = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))))"""
new = """clap_host::Message::AudioThread(id, NoClone(Box::new(match message {
				Ok(m) => m,
				Err(_) => {
					warn!("audio thread message channel closed");
					return;
				}
			})))"""
content = content.replace(old, new)
print("Fixed clap message 1")

old = """clap_host::Message::AudioThread(id, NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default())))),"""
new = """clap_host::Message::AudioThread(id, NoClone(Box::new(match message {
				Ok(m) => m,
				Err(_) => {
					warn!("audio thread message channel closed");
					return;
				}
			}))),"""
content = content.replace(old, new)
print("Fixed clap message 2")

# Fix vst2 message
old = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
				id,
				NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))),
			))"""
new = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
				id,
				NoClone(Box::new(match message {
					Ok(m) => m,
					Err(_) => {
						warn!("audio thread message channel closed");
						return;
					}
				})),
			))"""
content = content.replace(old, new)
print("Fixed vst2 message")

# Fix vst3 message
old = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
				id,
				NoClone(Box::new(message.unwrap_or_else(|_| generic_daw_core::AudioThreadMessage::default()))),
			))"""
new = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
				id,
				NoClone(Box::new(match message {
					Ok(m) => m,
					Err(_) => {
						warn!("audio thread message channel closed");
						return;
					}
				})),
			))"""
content = content.replace(old, new)
print("Fixed vst3 message")

# Fix the freeze match statement - it's missing braces
old = """			Task::future(unblock(move || {
				let (mut processor, mut pool) = match p_receiver.recv() {
				Ok(v) => v,
				Err(_) => {
					warn!("failed to receive processor");
					return;
				}
			};
				processor.render("""
new = """			Task::future(unblock(move || {
				let (mut processor, mut pool) = match p_receiver.recv() {
					Ok(v) => v,
					Err(_) => {
						warn!("failed to receive processor for freeze");
						return;
					}
				};
				processor.render("""
content = content.replace(old, new)
print("Fixed freeze match statement")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)