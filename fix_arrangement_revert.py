with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Revert node_mut to use expect with message
old = """\tfn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
\t\tself.nodes.get_mut(&id)
\t}"""

new = """\tfn node_mut(&mut self, id: NodeId) -> &mut Node {
\t\tself.nodes.get_mut(&id).expect("node not found")
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Revert node_mut done!")
else:
    print("Revert node_mut: NOT FOUND")

# Revert remove to use expect with message
old = """\tfn remove(&mut self, id: NodeId) -> Option<Node> {
\t\tfor node in self.nodes.values_mut() {
\t\t\tnode.outgoing.remove(&id);
\t\t}
\t\tif self.solo == Some(id) {
\t\t\tself.track_toggle_solo(id);
\t\t}
\t\tself.send(Message::NodeRemove(id, Box::new(MaybeUninit::uninit())));
\t\tself.nodes.remove(&id)
\t}"""

new = """\tfn remove(&mut self, id: NodeId) -> Node {
\t\tfor node in self.nodes.values_mut() {
\t\t\tnode.outgoing.remove(&id);
\t\t}
\t\tif self.solo == Some(id) {
\t\t\tself.track_toggle_solo(id);
\t\t}
\t\tself.send(Message::NodeRemove(id, Box::new(MaybeUninit::uninit())));
\t\tself.nodes.remove(&id).expect("node not found")
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Revert remove done!")
else:
    print("Revert remove: NOT FOUND")

# Fix set_mix to use expect
old = """\tpub fn set_mix(&mut self, from: NodeId, to: NodeId, mix: f32) {
\t\tif let Some(node) = self.node_mut(from) {
\t\t\tif node.outgoing.get(&to) != Some(&mix) {
\t\t\t\tif let Some(m) = node.outgoing.get_mut(&to) {
\t\t\t\t\t*m = mix;
\t\t\t\t\tself.send(Message::NodeSetMix(from, to, mix));
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

new = """\tpub fn set_mix(&mut self, from: NodeId, to: NodeId, mix: f32) {
\t\tif self.node(from).outgoing[&to] != mix {
\t\t\t*self.node_mut(from).outgoing.get_mut(&to).expect("mix not found") = mix;
\t\t\tself.send(Message::NodeSetMix(from, to, mix));
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix set_mix done!")
else:
    print("Fix set_mix: NOT FOUND")

# Fix channel_of/unwrap -> expect
old = """\t\tlet index = self.channel_of(id).unwrap();"""
new = """\t\tlet index = self.channel_of(id).expect("channel not found");"""
content = content.replace(old, new)
print("Fixed channel_of.unwrap -> expect")

# Fix track_of/unwrap -> expect
old = """\t\tlet index = self.track_of(id).unwrap();"""
new = """\t\tlet index = self.track_of(id).expect("track not found");"""
content = content.replace(old, new)
print("Fixed track_of.unwrap -> expect")

old = """\t\tlet track = self.track_of(id).unwrap();"""
new = """\t\tlet track = self.track_of(id).expect("track not found");"""
content = content.replace(old, new)
print("Fixed track_of.unwrap (let track) -> expect")

old = """\tlet track = self.track_of(id).unwrap();"""
new = """\tlet track = self.track_of(id).expect("track not found");"""
content = content.replace(old, new)
print("Fixed track_of.unwrap (let track 2) -> expect")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)