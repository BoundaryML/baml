class Node { constructor(id, value) { this.id = id; this.value = value; this.edges = []; this.parent = null; } }
function request(r, n) {
  const nodes = [];
  for (let i = 0; i < n; i++) nodes.push(new Node(i, (r * 31 + i * 17) % 1000));
  for (const node of nodes) {
    node.edges.push(nodes[(node.id + 1) % n]);
    node.edges.push(nodes[(node.id * 7 + r) % n]);
    node.parent = nodes[Math.floor(node.id / 2)];
  }
  let sum = 0;
  for (const node of nodes) {
    for (const edge of node.edges) sum += edge.value;
    if (node.parent !== null) sum += node.parent.id;
  }
  return sum;
}
function main() {
  const requests = 64, nodes = 256;
  let checksum = 0;
  for (let r = 0; r < requests; r++) checksum = (checksum * 31 + request(r, nodes)) % 1000000007;
  return `${requests * nodes},${checksum}`;
}
