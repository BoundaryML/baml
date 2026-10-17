class Node { constructor(id, value, edges) { this.id = id; this.value = value; this.edges = edges; } }
function setup() {
  const live = [];
  for (let i = 0; i < 500000; i++) live.push(new Node(i, (i * 7) % 1000, []));
  for (const node of live) node.edges.push(live[(node.id + 1) % 500000]);
  return live;
}
function run(live) {
  const n = live.length;
  let checksum = 0;
  for (let r = 0; r < 65536; r++) {
    const a = new Node(r, r % 1000, []);
    const b = new Node(r + 1, 0, [a]);
    b.edges.push(live[(r * 7919) % n]);
    a.edges.push(b);
    let sum = 0;
    for (const edge of b.edges) sum += edge.value;
    checksum = (checksum * 31 + sum) % 1000000007;
  }
  return `${n},${checksum}`;
}
function main() { return run(setup()); }
