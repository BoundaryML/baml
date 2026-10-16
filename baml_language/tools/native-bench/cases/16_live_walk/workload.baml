class TNode { value: int, parent: TNode | null, children: TNode[] }
class Config { nodes: int, walks: int }
class State { config: Config, root: TNode | null }
// The long-lived tree: `nodes` nodes in heap order, each with a parent
// pointer, built once and kept.
function prepare(raw: string) -> State {
    let c = baml.json.deserialize<Config>(raw);
    let nodes: TNode[] = [];
    let i = 0;
    while (i < c.nodes) {
        let parent: TNode | null = null;
        if (i > 0) { parent = nodes[(i - 1) / 2]; }
        let node = TNode { value: (i * 13) % 1000, parent: parent, children: [] };
        if (parent != null) { let p: TNode = parent; p.children.push(node); }
        nodes.push(node);
        i += 1;
    }
    let root: TNode | null = null;
    if (c.nodes > 0) { root = nodes[0]; }
    State { config: c, root: root }
}
function sum(node: TNode) -> int {
    let total = node.value;
    for (let child in node.children) { total += sum(child); }
    total
}
// `walks` read-only recursive walks of the live tree: no allocation of
// heap objects, no garbage.
function run(s: State) -> string {
    let checksum = 0;
    let w = 0;
    while (w < s.config.walks) {
        let root = s.root;
        if (root != null) {
            let r: TNode = root;
            checksum = (checksum * 31 + sum(r) + w) % 1000000007;
        }
        w += 1;
    }
    baml.json.to_string([s.config.nodes, checksum])
}
