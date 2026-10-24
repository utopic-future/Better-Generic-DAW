with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix request_resize - put tuple on same line as function call
old = """\tfn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
\t\tif self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native(
\t\t\twidth as f32, height as f32
\t\t))).is_err() {
\t\t\treturn Err(HostError::VersionMismatch);
\t\t}
\t\tOk(())
\t}"""
new = """\tfn request_resize(&self, GuiSize { width, height }: GuiSize) -> Result<(), HostError> {
\t\tif self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((width as f32, height as f32)))).is_err() {
\t\t\treturn Err(HostError::Message("failed to send gui request resize"));
\t\t}
\t\tOk(())
\t}"""
content = content.replace(old, new)
print("Fixed request_resize")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)