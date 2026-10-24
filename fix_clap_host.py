with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# The HostError from clack-host doesn't have a Denied variant, let's use the simpler approach
# Fix request_resize - use is_ok()
old = """fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		self.sender
			.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))))
			.map_err(|_| HostError::Denied)?;

		Ok(())
	}

	fn request_show(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestShow).map_err(|_| HostError::Denied)?;

		Ok(())
	}

	fn request_hide(&self) -> Result<(), HostError> {
		self.sender.send(MainThreadMessage::GuiRequestHide).map_err(|_| HostError::Denied)?;

		Ok(())
	}"""
new = """fn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}

	fn request_show(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestShow).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}

	fn request_hide(&self) -> Result<(), HostError> {
		if self.sender.send(MainThreadMessage::GuiRequestHide).is_err() {
			return Err(HostError::Denied);
		}
		Ok(())
	}"""
content = content.replace(old, new)
print("Fixed HostError issues")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)