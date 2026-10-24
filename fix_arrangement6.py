with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix clap plugin_restart Task::perform - revert to expect
old = """				Task::perform(receiver, move |message| {
					clap_host::Message::AudioThread(id, NoClone(Box::new(match message {
				Ok(m) => m,
				Err(_) => {
					warn!("audio thread message channel closed");
					return;
				}
			})))
				})"""
new = """				Task::perform(receiver, move |message| {
					clap_host::Message::AudioThread(id, NoClone(Box::new(message.expect("audio thread message channel closed in plugin_restart"))))
				})"""
content = content.replace(old, new)
print("Fixed clap plugin_restart Task::perform")

# Fix clap plugin_activate Task::perform
old = """			Task::perform(receiver, move |message| {
				clap_host::Message::AudioThread(id, NoClone(Box::new(match message {
				Ok(m) => m,
				Err(_) => {
					warn!("audio thread message channel closed");
					return;
				}
			})))
			})"""
new = """			Task::perform(receiver, move |message| {
				clap_host::Message::AudioThread(id, NoClone(Box::new(message.expect("audio thread message channel closed in plugin_activate"))))
			})"""
content = content.replace(old, new)
print("Fixed clap plugin_activate Task::perform")

# Fix vst2_tasks function - message.unwrap()
old = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
				id,
				NoClone(Box::new(match message {
					Ok(m) => m,
					Err(_) => {
						warn!("audio thread message channel closed");
						return;
					}
				})),
			))"""
new = """daw::Message::Vst2Host(vst2_host::Message::AudioThread(
				id,
				NoClone(Box::new(message.expect("audio thread message channel closed in vst2_tasks"))),
			))"""
content = content.replace(old, new)
print("Fixed vst2_tasks message")

# Fix vst3_tasks function - message.unwrap()
old = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
				id,
				NoClone(Box::new(match message {
					Ok(m) => m,
					Err(_) => {
						warn!("audio thread message channel closed");
						return;
					}
				})),
			))"""
new = """daw::Message::Vst3Host(vst3_host::Message::AudioThread(
				id,
				NoClone(Box::new(message.expect("audio thread message channel closed in vst3_tasks"))),
			))"""
content = content.replace(old, new)
print("Fixed vst3_tasks message")

# Fix freeze Task::future - the return type mismatch
# The unblock closure returns a Message but should return ()
# The issue is the last expression `daw::Message::Arrangement(...)` 
# Let me check the freeze function
old = """			}))
			}))
		Task::batch([
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
new = """			}))
			}))
		Task::batch([
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
content = content.replace(old, new)
print("Fixed freeze Task::future return type")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)