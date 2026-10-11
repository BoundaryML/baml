class Node { id: int, value: int, edges: Node[], parent: Node | null }
class State { requests: int, nodes: int }
function prepare(raw: string) -> State { baml.json.deserialize<State>(raw) }
// One request: a graph of `n` nodes, each with a ring edge, a chord and a
// parent pointer, so every node is on a cycle; walked once and dropped.
function request(r: int, n: int) -> int {
    let nodes: Node[] = [];
    let i = 0;
    while (i < n) {
        nodes.push(Node { id: i, value: (r * 31 + i * 17) % 1000, edges: [], parent: null });
        i += 1;
    }
    for (let node in nodes) {
        node.edges.push(nodes[(node.id + 1) % n]);
        node.edges.push(nodes[(node.id * 7 + r) % n]);
        node.parent = nodes[node.id / 2];
    }
    let sum = 0;
    for (let node in nodes) {
        for (let edge in node.edges) { sum += edge.value; }
        let parent = node.parent;
        if (parent != null) { let p: Node = parent; sum += p.id; }
    }
    sum
}
function run(s: State) -> string {
    let checksum = 0;
    let r = 0;
    while (r < s.requests) {
        if (s.nodes > 0) { checksum = (checksum * 31 + request(r, s.nodes)) % 1000000007; }
        r += 1;
    }
    baml.json.to_string([s.requests * s.nodes, checksum])
}
