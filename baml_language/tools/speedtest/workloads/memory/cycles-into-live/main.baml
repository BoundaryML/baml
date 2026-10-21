// Cycles into a live graph (from the native-bench cycles_into_live case):
// setup() builds a ring of 500,000 nodes, once and untimed. Each job builds
// 65,536 request-scoped two-node cycles that each also point at one node of
// the ring, walks them once and drops them. A collector must reclaim the
// cycles without disturbing the ring. (The native-bench case used a ring of 1,000,000.)
class Node {
  id: int
  value: int
  edges: Node[]
}

function setup() -> Node[] {
  let live: Node[] = [];
  let i = 0;
  while (i < 500000) {
    live.push(Node { id: i, value: (i * 7) % 1000, edges: [] });
    i += 1;
  }
  for (let node in live) { node.edges.push(live[(node.id + 1) % 500000]); }
  live
}

function run(live: Node[]) -> string {
  let n = live.length();
  let checksum = 0;
  for (let r = 0; r < 65536; r += 1) {
    let a = Node { id: r, value: r % 1000, edges: [] };
    let b = Node { id: r + 1, value: 0, edges: [a] };
    b.edges.push(live[(r * 7919) % n]);
    a.edges.push(b);
    let sum = 0;
    for (let edge in b.edges) { sum += edge.value; }
    checksum = (checksum * 31 + sum) % 1000000007;
  }
  n.to_string() + "," + checksum.to_string()
}

function main() -> string {
  run(setup())
}
