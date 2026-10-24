with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix using match statement
old = """if let Ok(slice) = data.as_slice_mut() {
					flush_denormals(slice);
				}
			}
			SampleFormat::F64 => {"""
new = """match data.as_slice_mut() {
					Ok(slice) => flush_denormals(slice),
					Err(_) => {}
				}
			}
			SampleFormat::F64 => {"""
if old in content:
    content = content.replace(old, new, 1)
    print("Fixed with match")
else:
    print("Not found")

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)