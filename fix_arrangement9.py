with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix render Task::future - the closure needs to return a Message at the end
old = """				processor.render(
					&mut pool,
					master,
					beat_range,
					|samples| {
						for &[l, r] in samples {
							if writer.write_sample(l).is_err() { warn!("failed to write sample"); }
							if writer.write_sample(r).is_err() { warn!("failed to write sample"); }
						}
					},
					|f| { let _ = progress_sender.force_send(f); },
				);
				if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!("failed to send processor back"); }
			}))
			.discard()"""
new = """				processor.render(
					&mut pool,
					master,
					beat_range,
					|samples| {
						for &[l, r] in samples {
							if writer.write_sample(l).is_err() { warn!("failed to write sample"); }
							if writer.write_sample(r).is_err() { warn!("failed to write sample"); }
						}
					},
					|f| { let _ = progress_sender.force_send(f); },
				);
				if p_sender.send((PullSlot::Full(processor), pool)).is_err() { warn!("failed to send processor back"); }
				daw::Message::RenderedFile
			}))
			.discard()"""
content = content.replace(old, new)
print("Fixed render Task::future return type")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)