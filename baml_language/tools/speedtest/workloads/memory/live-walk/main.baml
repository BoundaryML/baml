// Reading a large live structure (from the native-bench live_walk case):
// setup() builds a tree of 262,144 nodes in heap order, each with a parent
// pointer and a children array, once and untimed. Each job walks it
// recursively once: no allocation, no garbage. (The native-bench case used
// 1,048,576 nodes and four walks; one VM walk of that size takes seconds.)
class TNode {
  value: int
  parent: TNode | null
  children: TNode[]
}

function setup() -> TNode {
  let nodes: TNode[] = [];
  let i = 0;
  while (i < 262144) {
    let parent: TNode | null = null;
    if (i > 0) { parent = nodes[(i - 1) / 2]; }
    let node = TNode { value: (i * 13) % 1000, parent: parent, children: [] };
    if (parent != null) {
      let p: TNode = parent;
      p.children.push(node);
    }
    nodes.push(node);
    i += 1;
  }
  nodes[0]
}

function sum(node: TNode) -> int {
  let total = node.value;
  for (let child in node.children) { total += sum(child); }
  total
}

function run(tree: TNode) -> int {
  sum(tree)
}

function main() -> int {
  run(setup())
}
