with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize
old = """fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		self.sender
			.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))))
			.unwrap();

		Ok(())
	}"""
new = """fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		self.sender
			.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))))
			.map_err(|_| HostError::Denied)?;

		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_resize")

# Fix request_show
old = """fn request_show(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestShow).unwrap();

		Ok(())
	}"""
new = """fn request_show(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestShow).map_err(|_| HostError::Denied)?;

		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_show")

# Fix request_hide
old = """fn request_hide(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestHide).unwrap();

		Ok(())
	}"""
new = """fn request_hide(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestHide).map_err(|_| HostError::Denied)?;

		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed request_hide")

# Fix closed
old = """fn closed(&self, _was_destroyed: bool) {
		self.sender.send(MainThreadMessage::GuiClosed).unwrap();
	}"""
new = """fn closed(&self, _was_destroyed: bool) {
		if let Err(e) = self.sender.send(MainThreadMessage::GuiClosed) {
			warn!("failed to send gui closed: {e}");
		}
	}"""
content = content.replace(old, new)
print("Fixed closed")

# Fix request_flush
old = """fn request_flush(&self) {
		if !self.request_flush.swap(true, Relaxed) && !self.active.load(Relaxed) {
			self.sender.send(MainThreadMessage::RequestFlush).unwrap();
		}
	}"""
new = """fn request_flush(&self) {
		if !self.request_flush.swap(true, Relaxed) && !self.active.load(Relaxed) {
			if let Err(e) = self.sender.send(MainThreadMessage::RequestFlush) {
				warn!("failed to send request_flush: {e}");
			}
		}
	}"""
content = content.replace(old, new)
print("Fixed request_flush")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)