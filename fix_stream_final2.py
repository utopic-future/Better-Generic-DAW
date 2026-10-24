with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix input callback - remove the denormal flush for input since data is &Data
old = """SampleFormat::F32 => {
				callback(data.as_slice().unwrap());
				// Flush denormals on input too
				if let Some(slice) = data.as_slice_mut() {
					flush_denormals(slice);
				}
			}"""
new = """SampleFormat::F32 => {
				callback(data.as_slice().unwrap());
				// Input buffer flush skipped - data is &Data, not &mut Data
			}"""
if old in content:
    content = content.replace(old, new, 1)
    print("Fixed input callback")
else:
    print("Not found")

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)