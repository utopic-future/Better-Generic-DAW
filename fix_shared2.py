with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix the macro - line 84 has extra paren
old = """\t\t\t($($ident:ident),*) => { $("""
new = """\t\t\t($($ident:ident),*) => {"""
content = content.replace(old, new)
print("Fixed macro paren")

# Fix the Size::from_native parens - it has double (( but should be single (
old = """		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {"""
new = """		if self.sender.send(MainThreadMessage::GuiRequestResize(Size::from_native((
				width as f32,
				height as f32,
			))).is_err() {"""
content = content.replace(old, new)
print("Checked Size::from_native")

# Actually the Size::from_native is correct - it's a tuple (width, height) passed to the function
# The issue is that the macro on line 84 has an extra paren that was never closed
# Let me verify the paren count after fixing the macro

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)