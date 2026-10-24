with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize - Size::from_native should have single paren, tuple provides the second
old = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
			(width as f32, height as f32)
		))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
			width as f32, height as f32
		))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_resize - removed extra parens around tuple")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)