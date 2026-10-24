with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize - put tuple on same line
old = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native(
			(width as f32, height as f32)
		))).is_err() {
			return Err(HostError::Unsupported);
		}
		Ok(())
	}"""
new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((width as f32, height as f32)))).is_err() {
			return Err(HostError::VersionMismatch);
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_resize - tuple on one line, HostError variant")

# Fix request_show and request_hide
old2 = """	return Err(HostError::Unsupported);"""
new2 = """	return Err(HostError::VersionMismatch);"""
content = content.replace(old2, new2)
print("Fixed HostError variant to VersionMismatch")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)