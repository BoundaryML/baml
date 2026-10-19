// Cyclic churn (from the native-bench cyclic_churn case): 64 requests, each
// building a graph of 256 nodes with a ring edge, a chord and a parent
// pointer, so every node is on a cycle; walked once and dropped. Under
// reference counting this garbage is reclaimed only by cycle collection.
class Node {
  id: int
  value: int
  edges: Node[]
  parent: Node | null
}

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
    if (parent != null) {
      let p: Node = parent;
      sum += p.id;
    }
  }
  sum
}

function main() -> string {
  let requests = 64;
  let nodes = 256;
  let checksum = 0;
  for (let r = 0; r < requests; r += 1) {
    checksum = (checksum * 31 + request(r, nodes)) % 1000000007;
  }
  (requests * nodes).to_string() + "," + checksum.to_string()
}
