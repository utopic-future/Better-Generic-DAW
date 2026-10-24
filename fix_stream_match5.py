with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix the match arm with exact whitespace
old = """match data.as_slice_mut() {
				flush_denormals(slice),
			}"""
new = """match data.as_slice_mut() {
				Ok(slice) => flush_denormals(slice),
				Err(_) => {}
			}"""
if old in content:
    content = content.replace(old, new, 1)
    print("Fixed match arm")
else:
    print("Not found - checking actual content:")
    # Print exact lines
    lines = content.split('\n')
    for i, line in enumerate(lines):
        if 'match data.as_slice_mut' in line:
            print(f"Line {i}: {repr(line)}")
            for j in range(1, 5):
                print(f"Line {i+j}: {repr(lines[i+j])}")
            break

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)