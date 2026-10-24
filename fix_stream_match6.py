with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix the match arm with exact whitespace (6 tabs = \t\t\t\t\t\t)
old = "\t\t\t\t\t\tmatch data.as_slice_mut() {\n\t\t\t\t\t\t\tflush_denormals(slice),\n\t\t\t\t\t\t}"
new = "\t\t\t\t\t\tmatch data.as_slice_mut() {\n\t\t\t\t\t\t\tOk(slice) => flush_denormals(slice),\n\t\t\t\t\t\t\tErr(_) => {}\n\t\t\t\t\t\t}"
if old in content:
    content = content.replace(old, new, 1)
    print("Fixed match arm")
else:
    print("Still not found")

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)