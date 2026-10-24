with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'r') as f:
    content = f.read()

# Fix 3: node_mut - change to return Option
old = """\tfn node_mut(&mut self, id: NodeId) -> &mut Node {
\t\tself.nodes.get_mut(&id).unwrap()
\t}"""

new = """\tfn node_mut(&mut self, id: NodeId) -> Option<&mut Node> {
\t\tself.nodes.get_mut(&id)
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 3a: node_mut done!")
else:
    print("Fix 3a: OLD NOT FOUND!")

# Fix remove
old = """\tfn remove(&mut self, id: NodeId) -> Node {
\t\tfor node in self.nodes.values_mut() {
\t\t\tnode.outgoing.remove(&id);
\t\t}
\t\tif self.solo == Some(id) {
\t\t\tself.track_toggle_solo(id);
\t\t}
\t\tself.send(Message::NodeRemove(id, Box::new(MaybeUninit::uninit())));
\t\tself.nodes.remove(&id).unwrap()
\t}"""

new = """\tfn remove(&mut self, id: NodeId) -> Option<Node> {
\t\tfor node in self.nodes.values_mut() {
\t\t\tnode.outgoing.remove(&id);
\t\t}
\t\tif self.solo == Some(id) {
\t\t\tself.track_toggle_solo(id);
\t\t}
\t\tself.send(Message::NodeRemove(id, Box::new(MaybeUninit::uninit())));
\t\tself.nodes.remove(&id)
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 3b: remove done!")
else:
    print("Fix 3b: OLD NOT FOUND!")

# Fix set_mix
old = """\tpub fn set_mix(&mut self, from: NodeId, to: NodeId, mix: f32) {
\t\tif self.node(from).outgoing[&to] != mix {
\t\t\t*self.node_mut(from).outgoing.get_mut(&to).unwrap() = mix;
\t\t\tself.send(Message::NodeSetMix(from, to, mix));
\t\t}
\t}"""

new = """\tpub fn set_mix(&mut self, from: NodeId, to: NodeId, mix: f32) {
\t\tif let Some(node) = self.node_mut(from) {
\t\t\tif node.outgoing.get(&to) != Some(&mix) {
\t\t\t\tif let Some(m) = node.outgoing.get_mut(&to) {
\t\t\t\t\t*m = mix;
\t\t\t\t\tself.send(Message::NodeSetMix(from, to, mix));
\t\t\t\t}
\t\t\t}
\t\t}
\t}"""

if old in content:
    content = content.replace(old, new, 1)
    print("Fix 3c: set_mix done!")
else:
    print("Fix 3c: OLD NOT FOUND!")

with open('generic_daw_gui/src/arrangement_view/arrangement.rs', 'w') as f:
    f.write(content)