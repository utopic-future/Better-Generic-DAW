with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize - put tuple on same line to avoid paren confusion
old = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(
			Size::from_native(Size::from_native((width as f32, height as f32)))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
# Wait, that's wrong - double Size::from_native. Let me think...

# Actually, the original code in the repository probably worked. Let me check what the original looked like.
# The issue might be that Size::from_native expects a tuple, and the double parens are correct.
# But the Rust parser is confused by the multi-line formatting.
# Let me try putting the tuple on one line.

new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
			width as f32, height as f32
		))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_resize formatting")

# Also fix request_show and request_hide to use the same pattern for consistency
old2 = """	fn request_show(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestShow).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
# This one is fine - no Size::from_native

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)