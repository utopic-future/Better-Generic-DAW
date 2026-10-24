with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize - tuple on one line, HostError::Message
old = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native(
			(width as f32, height as f32)
		))).is_err() {
			return Err(HostError::VersionMismatch);
		}
		Ok(())
	}"""
new = """	fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((width as f32, height as f32)))).is_err() {
			return Err(HostError::Message("failed to send gui request resize"));
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_resize")

# Fix request_show
old2 = """	fn request_show(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestShow).is_err() {
			return Err(HostError::VersionMismatch);
		}
		Ok(())
	}"""
new2 = """	fn request_show(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestShow).is_err() {
			return Err(HostError::Message("failed to send gui request show"));
		}
		Ok(())
	}"""
content = content.replace(old2, new2)
print("Fixed request_show")

# Fix request_hide
old3 = """	fn request_hide(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestHide).is_err() {
			return Err(HostError::VersionMismatch);
		}
		Ok(())
	}"""
new3 = """	fn request_hide(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestHide).is_err() {
			return Err(HostError::Message("failed to send gui request hide"));
		}
		Ok(())
	}"""
content = content.replace(old3, new3)
print("Fixed request_hide")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)