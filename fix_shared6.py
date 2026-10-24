with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix the macro - use raw string to avoid escaping issues
old = """\t\tmacro_rules! initialized {
\t\t\t($($ident:ident),*) => { \\$(
\t\t\t\tif let Some($ident) = instance.get_extension() {
\t\t\t\t\tself.ext.$ident.set(NoDebug($ident)).unwrap();
\t\t\t\t\tinfo!(concat!("{}: implements '", stringify!($ident), "'"), &self.descriptor);
\t\t\t\t}
\t\t\t)* };
\t\t}"""
new = """\t\tmacro_rules! initialized {
\t\t\t($($ident:ident),*) => { $(
\t\t\t\tif let Some($ident) = instance.get_extension() {
\t\t\t\t\tself.ext.$ident.set(NoDebug($ident)).unwrap();
\t\t\t\t\tinfo!(concat!("{}: implements '", stringify!($ident), "'"), &self.descriptor);
\t\t\t\t}
\t\t\t)* };
\t\t}"""
content = content.replace(old, new)
print("Fixed macro template")

# Also fix the Size::from_native parens issue
old2 = """		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {"""
new2 = """		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {"""
# This is actually correct - the tuple (width, height) needs the double parens
# The issue is elsewhere. Let me check if there's a missing paren in the file.
# For now, just ensure the macro is fixed.

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)