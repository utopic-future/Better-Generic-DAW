with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix Size::from_native - pass tuple as single argument
old = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native(
			(width as f32, height as f32)
		))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
			width as f32, height as f32,
		)))).is_err() {
			return Err(HostError::Unsupported);
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed Size::from_native tuple")

# Fix HostError::Denied -> HostError::Unsupported (or whatever variant exists)
# Also fix request_show and request_hide
old2 = """	return Err(HostError::Denied);"""
new2 = """	return Err(HostError::Unsupported);"""
content = content.replace(old2, new2)
print("Fixed HostError variant")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)