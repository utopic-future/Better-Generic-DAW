with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix the early return in freeze - line 1790
old = """					Err(_) => {
						warn!("failed to receive processor for freeze");
						return;
					}"""
new = """					Err(_) => {
						warn!("failed to receive processor for freeze");
						return daw::Message::RenderedFile;
					}"""
content = content.replace(old, new)
print("Fixed freeze early return")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)