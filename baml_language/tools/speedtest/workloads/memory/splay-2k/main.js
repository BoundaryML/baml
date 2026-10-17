class Payload { constructor(left, right, array, text) { this.left = left; this.right = right; this.array = array; this.text = text; } }
class Node { constructor(key, value) { this.key = key; this.value = value; this.left = null; this.right = null; } }
function payload(depth, tag) {
  if (depth === 0) return new Payload(null, null, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "String for key " + tag + " in leaf node");
  return new Payload(payload(depth - 1, tag), payload(depth - 1, tag), [], "");
}
class Tree {
  constructor() { this.root = null; this.seed = 49734321; }
  splay(key) {
    if (this.root === null) return;
    const dummy = new Node(0, new Payload(null, null, [], ""));
    let left = dummy, right = dummy, current = this.root;
    while (true) {
      if (key < current.key) {
        if (current.left === null) break;
        if (key < current.left.key) {
          const tmp = current.left; current.left = tmp.right; tmp.right = current; current = tmp;
          if (current.left === null) break;
        }
        right.left = current; right = current; current = current.left;
      } else if (key > current.key) {
        if (current.right === null) break;
        if (key > current.right.key) {
          const tmp = current.right; current.right = tmp.left; tmp.left = current; current = tmp;
          if (current.right === null) break;
        }
        left.right = current; left = current; current = current.right;
      } else break;
    }
    left.right = current.left; right.left = current.right;
    current.left = dummy.right; current.right = dummy.left;
    this.root = current;
  }
  insert(key, value) {
    if (this.root === null) { this.root = new Node(key, value); return true; }
    this.splay(key);
    const r = this.root;
    if (r.key === key) return false;
    const node = new Node(key, value);
    if (key > r.key) { node.left = r; node.right = r.right; r.right = null; }
    else { node.right = r; node.left = r.left; r.left = null; }
    this.root = node;
    return true;
  }
  remove(key) {
    this.splay(key);
    const r = this.root;
    if (r === null || r.key !== key) return;
    if (r.left === null) this.root = r.right;
    else { const rest = r.right; this.root = r.left; this.splay(key); this.root.right = rest; }
  }
  greatestLessThan(key) {
    this.splay(key);
    const r = this.root;
    if (r === null) return -1;
    if (r.key < key) return r.key;
    let cur = r.left;
    if (cur === null) return -1;
    while (cur.right !== null) cur = cur.right;
    return cur.key;
  }
  nextKey() { this.seed = this.seed * 16807 % 2147483647; return this.seed % 1000000; }
  insertNew() { while (true) { const key = this.nextKey(); if (this.insert(key, payload(4, key))) return key; } }
}
function main() {
  const tree = new Tree();
  for (let i = 0; i < 2000; i++) tree.insertNew();
  for (let i = 0; i < 4000; i++) {
    const key = tree.insertNew();
    const g = tree.greatestLessThan(key);
    tree.remove(g < 0 ? key : g);
  }
  let total = 0, count = 0;
  const stack = tree.root === null ? [] : [tree.root];
  while (stack.length > 0) {
    const n = stack.pop();
    count++;
    total = (total + n.key * 7 + n.value.text.length) % 1000000007;
    if (n.left !== null) stack.push(n.left);
    if (n.right !== null) stack.push(n.right);
  }
  return total * 10000 + count;
}
