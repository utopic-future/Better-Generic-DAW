with open('clap_host/src/shared.rs', 'r') as f:
    content = f.read()

# Fix the macro - the template needs $( immediately after => {
old = """\t\tmacro_rules! initialized {
\t\t\t($($ident:ident),*) => {
\t\t\t\tif let Some($ident) = instance.get_extension() {
\t\t\t\t\tself.ext.$ident.set(NoDebug($ident)).unwrap();
\t\t\t\t\tinfo!(concat!("{}: implements '", stringify!($ident), "'"), &self.descriptor);
\t\t\t\t}
\t\t\t)* };
\t\t}"""
new = """\t\tmacro_rules! initialized {
\t\t\t($($ident:ident),*) => { \$(
\t\t\t\tif let Some($ident) = instance.get_extension() {
\t\t\t\t\tself.ext.$ident.set(NoDebug($ident)).unwrap();
\t\t\t\t\tinfo!(concat!("{}: implements '", stringify!($ident), "'"), &self.descriptor);
\t\t\t\t}
\t\t\t)* };
\t\t}"""
content = content.replace(old, new)
print("Fixed macro template")

with open('clap_host/src/shared.rs', 'w') as f:
    f.write(content)