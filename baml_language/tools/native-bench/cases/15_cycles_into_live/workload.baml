class Node { id: int, value: int, edges: Node[] }
class Config { live: int, requests: int }
class State { config: Config, live: Node[] }
// The long-lived graph: a ring of `live` nodes, built once and kept.
function prepare(raw: string) -> State {
    let c = baml.json.deserialize<Config>(raw);
    let live: Node[] = [];
    let i = 0;
    while (i < c.live) {
        live.push(Node { id: i, value: (i * 7) % 1000, edges: [] });
        i += 1;
    }
    for (let node in live) { node.edges.push(live[(node.id + 1) % c.live]); }
    State { config: c, live: live }
}
// `requests` request-scoped two-node cycles, each also pointing at one node
// of the live ring: walked once and dropped.
function run(s: State) -> string {
    let live = s.live;
    let n = live.length();
    let checksum = 0;
    let r = 0;
    while (r < s.config.requests) {
        let a = Node { id: r, value: r % 1000, edges: [] };
        let b = Node { id: r + 1, value: 0, edges: [a] };
        if (n > 0) { b.edges.push(live[(r * 7919) % n]); }
        a.edges.push(b);
        let sum = 0;
        for (let edge in b.edges) { sum += edge.value; }
        checksum = (checksum * 31 + sum) % 1000000007;
        r += 1;
    }
    baml.json.to_string([n, checksum])
}
