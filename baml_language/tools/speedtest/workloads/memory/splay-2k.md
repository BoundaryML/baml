# memory::splay 2k

## BAML
```baml
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
```

## Python
```python
class Payload:
    __slots__ = ('left', 'right', 'array', 'text')
    def __init__(self, left, right, array, text):
        self.left = left; self.right = right; self.array = array; self.text = text
class Node:
    __slots__ = ('key', 'value', 'left', 'right')
    def __init__(self, key, value):
        self.key = key; self.value = value; self.left = None; self.right = None
def payload(depth, tag):
    if depth == 0:
        return Payload(None, None, [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], "String for key " + str(tag) + " in leaf node")
    return Payload(payload(depth - 1, tag), payload(depth - 1, tag), [], "")
class Tree:
    def __init__(self):
        self.root = None; self.seed = 49734321
    def splay(self, key):
        if self.root is None: return
        dummy = Node(0, Payload(None, None, [], ""))
        left = right = dummy
        current = self.root
        while True:
            if key < current.key:
                if current.left is None: break
                if key < current.left.key:
                    tmp = current.left; current.left = tmp.right; tmp.right = current; current = tmp
                    if current.left is None: break
                right.left = current; right = current; current = current.left
            elif key > current.key:
                if current.right is None: break
                if key > current.right.key:
                    tmp = current.right; current.right = tmp.left; tmp.left = current; current = tmp
                    if current.right is None: break
                left.right = current; left = current; current = current.right
            else:
                break
        left.right = current.left; right.left = current.right
        current.left = dummy.right; current.right = dummy.left
        self.root = current
    def insert(self, key, value):
        if self.root is None:
            self.root = Node(key, value); return True
        self.splay(key)
        r = self.root
        if r.key == key: return False
        node = Node(key, value)
        if key > r.key:
            node.left = r; node.right = r.right; r.right = None
        else:
            node.right = r; node.left = r.left; r.left = None
        self.root = node
        return True
    def remove(self, key):
        self.splay(key)
        r = self.root
        if r is None or r.key != key: return
        if r.left is None:
            self.root = r.right
        else:
            rest = r.right; self.root = r.left; self.splay(key); self.root.right = rest
    def greatest_less_than(self, key):
        self.splay(key)
        r = self.root
        if r is None: return -1
        if r.key < key: return r.key
        cur = r.left
        if cur is None: return -1
        while cur.right is not None: cur = cur.right
        return cur.key
    def next_key(self):
        self.seed = self.seed * 16807 % 2147483647
        return self.seed % 1000000
    def insert_new(self):
        while True:
            key = self.next_key()
            if self.insert(key, payload(4, key)): return key
tree = Tree()
for _ in range(2000): tree.insert_new()
for _ in range(4000):
    key = tree.insert_new()
    g = tree.greatest_less_than(key)
    tree.remove(key if g < 0 else g)
total = 0; count = 0
stack = [tree.root] if tree.root is not None else []
while stack:
    n = stack.pop()
    count += 1
    total = (total + n.key * 7 + len(n.value.text)) % 1000000007
    if n.left is not None: stack.append(n.left)
    if n.right is not None: stack.append(n.right)
print(total * 10000 + count)
```

## Typescript
```ts
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
console.log(total * 10000 + count);
```
