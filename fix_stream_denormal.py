with open('generic_daw_core/src/stream.rs', 'r') as f:
    content = f.read()

# Fix input callback - SampleFormat::F32 (exact format from file)
lines = content.split('\n')
new_lines = []
i = 0
while i < len(lines):
    line = lines[i]
    if line.strip() == 'SampleFormat::F32 => callback(data.as_slice().unwrap()),':
        # Replace this line and next 2 lines
        new_lines.append('\t\t\t\t\tSampleFormat::F32 => {')
        new_lines.append('\t\t\t\t\t\tcallback(data.as_slice().unwrap());')
        new_lines.append('\t\t\t\t\t\t// Flush denormals on input too')
        new_lines.append('\t\t\t\t\t\tif let Ok(slice) = data.as_slice_mut() {')
        new_lines.append('\t\t\t\t\t\t\tflush_denormals(slice);')
        new_lines.append('\t\t\t\t\t\t}')
        new_lines.append('\t\t\t\t\t}')
        new_lines.append('\t\t\t\t\tSampleFormat::F64 => {')
        new_lines.append('\t\t\t\t\t\tbridge!(f64);')
        new_lines.append('\t\t\t\t\t}')
        new_lines.append('\t\t\t\t\tsample_format => panic!("unsupported sample format {sample_format}"),')
        # Skip the next 2 lines (F64 and panic)
        i += 3
    else:
        new_lines.append(line)
        i += 1

content = '\n'.join(new_lines)
print("Fixed input callback")

with open('generic_daw_core/src/stream.rs', 'w') as f:
    f.write(content)