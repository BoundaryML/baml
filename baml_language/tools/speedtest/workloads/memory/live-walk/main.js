class TNode { constructor(value, parent) { this.value = value; this.parent = parent; this.children = []; } }
function setup() {
  const nodes = [];
  for (let i = 0; i < 262144; i++) {
    const parent = i > 0 ? nodes[Math.floor((i - 1) / 2)] : null;
    const node = new TNode((i * 13) % 1000, parent);
    if (parent !== null) parent.children.push(node);
    nodes.push(node);
  }
  return nodes[0];
}
function sum(node) { let total = node.value; for (const child of node.children) total += sum(child); return total; }
function run(tree) { return sum(tree); }
function main() { return run(setup()); }
