with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix the as_slice_mut() issue - use match instead
old = """SampleFormat::F32 => {
					callback(data.as_slice().unwrap());
					// Flush denormals on input too
					if let Ok(slice) = data.as_slice_mut() {
						flush_denormals(slice);
					}
				}"""
new = """SampleFormat::F32 => {
					callback(data.as_slice().unwrap());
					// Flush denormals on input too
					if let Ok(slice) = data.as_slice_mut() {
						flush_denormals(slice);
					}
				}"""
# The code should work, but let's try a different approach - use match
new = """SampleFormat::F32 => {
					callback(data.as_slice().unwrap());
					// Flush denormals on input too
					match data.as_slice_mut() {
						Ok(slice) => flush_denormals(slice),
						Err(_) => {}
					}
				}"""
if old in content:
    content = content.replace(old, new, 1)
    print("Fixed with match")
else:
    print("Not found")

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)