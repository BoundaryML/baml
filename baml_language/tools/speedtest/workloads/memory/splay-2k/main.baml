// Octane Splay: a top-down splay tree of 2,000 nodes, each carrying a payload
// tree of 31 objects (int arrays and strings), under 4,000 insert/remove
// modifications. A large live heap with steady churn, plus pointer-heavy
// tree rotations.
class Payload { left: Payload?, right: Payload?, array: int[], text: string }
class SplayNode { key: int, value: Payload, left: SplayNode?, right: SplayNode? }
class SplayTree { top: SplayNode?, seed: int }
function payload(depth: int, tag: int) -> Payload {
  if (depth == 0) {
    return Payload { left: null, right: null, array: [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], text: "String for key " + tag.to_string() + " in leaf node" };
  }
  Payload { left: payload(depth - 1, tag), right: payload(depth - 1, tag), array: [], text: "" }
}
function splay(tree: SplayTree, key: int) -> int {
  let start = tree.top;
  if (start == null) { return 0; }
  let dummy = SplayNode { key: 0, value: Payload { left: null, right: null, array: [], text: "" }, left: null, right: null };
  let left = dummy;
  let right = dummy;
  let current = start;
  while (true) {
    if (key < current.key) {
      let cl = current.left;
      if (cl == null) { break; }
      if (key < cl.key) {
        current.left = cl.right;
        cl.right = current;
        current = cl;
        let next = current.left;
        if (next == null) { break; }
      }
      right.left = current;
      right = current;
      let next = current.left;
      if (next == null) { break; }
      current = next;
    } else if (key > current.key) {
      let cr = current.right;
      if (cr == null) { break; }
      if (key > cr.key) {
        current.right = cr.left;
        cr.left = current;
        current = cr;
        let next = current.right;
        if (next == null) { break; }
      }
      left.right = current;
      left = current;
      let next = current.right;
      if (next == null) { break; }
      current = next;
    } else {
      break;
    }
  }
  left.right = current.left;
  right.left = current.right;
  current.left = dummy.right;
  current.right = dummy.left;
  tree.top = current;
  0
}
function insert(tree: SplayTree, key: int, value: Payload) -> bool {
  let t = tree.top;
  if (t == null) {
    tree.top = SplayNode { key: key, value: value, left: null, right: null };
    return true;
  }
  splay(tree, key);
  let r = tree.top;
  if (r == null || r.key == key) { return false; }
  let node = SplayNode { key: key, value: value, left: null, right: null };
  if (key > r.key) {
    node.left = r;
    node.right = r.right;
    r.right = null;
  } else {
    node.right = r;
    node.left = r.left;
    r.left = null;
  }
  tree.top = node;
  true
}
function remove(tree: SplayTree, key: int) -> int {
  splay(tree, key);
  let r = tree.top;
  if (r == null || r.key != key) { return 0; }
  let l = r.left;
  if (l == null) {
    tree.top = r.right;
  } else {
    let rest = r.right;
    tree.top = l;
    splay(tree, key);
    let nr = tree.top;
    if (nr != null) { nr.right = rest; }
  }
  1
}
function greatest_less_than(tree: SplayTree, key: int) -> int {
  splay(tree, key);
  let r = tree.top;
  if (r == null) { return -1; }
  if (r.key < key) { return r.key; }
  let l = r.left;
  if (l == null) { return -1; }
  let cur: SplayNode = l;
  while (true) {
    let next = cur.right;
    if (next == null) { break; }
    cur = next;
  }
  cur.key
}
function next_key(tree: SplayTree) -> int {
  tree.seed = tree.seed * 16807 % 2147483647;
  tree.seed % 1000000
}
function insert_new(tree: SplayTree) -> int {
  while (true) {
    let key = next_key(tree);
    if (insert(tree, key, payload(4, key))) { return key; }
  }
  0
}
function main() -> int {
  let tree = SplayTree { top: null, seed: 49734321 };
  for (let i = 0; i < 2000; i += 1) { insert_new(tree); }
  for (let i = 0; i < 4000; i += 1) {
    let key = insert_new(tree);
    let g = greatest_less_than(tree, key);
    if (g < 0) { remove(tree, key); } else { remove(tree, g); }
  }
  let sum = 0;
  let count = 0;
  let stack: SplayNode[] = [];
  let t = tree.top;
  if (t != null) { stack.push(t); }
  while (stack.length() > 0) {
    let n = stack.pop();
    if (n == null) { break; }
    count += 1;
    sum = (sum + n.key * 7 + n.value.text.length()) % 1000000007;
    let l = n.left;
    if (l != null) { stack.push(l); }
    let r = n.right;
    if (r != null) { stack.push(r); }
  }
  sum * 10000 + count
}
