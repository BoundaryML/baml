# memory::binary trees 14

## BAML
```baml
// Benchmarks Game binary-trees: one long-lived tree of depth 14 stays live
// while many short-lived trees of depths 4..14 are built and walked.
// Allocation rate and reclamation of short-lived objects under a live heap.
class TreeNode { left: TreeNode?, right: TreeNode? }
function bottom_up(depth: int) -> TreeNode {
  if (depth <= 0) { return TreeNode { left: null, right: null }; }
  TreeNode { left: bottom_up(depth - 1), right: bottom_up(depth - 1) }
}
function check(node: TreeNode) -> int {
  let l = node.left;
  let r = node.right;
  if (l == null || r == null) { return 1; }
  1 + check(l) + check(r)
}
function main() -> int {
  let max_depth = 14;
  let total = check(bottom_up(max_depth + 1));
  let long_lived = bottom_up(max_depth);
  for (let depth = 4; depth <= max_depth; depth += 2) {
    let iterations = 1 << (max_depth - depth + 4);
    let sum = 0;
    for (let i = 0; i < iterations; i += 1) {
      sum += check(bottom_up(depth));
    }
    total += sum;
  }
  total + check(long_lived)
}
```

## Python
```python
import sys
sys.setrecursionlimit(10000)
class TreeNode:
    __slots__ = ('left', 'right')
    def __init__(self, left, right): self.left = left; self.right = right
def bottom_up(d):
    if d <= 0: return TreeNode(None, None)
    return TreeNode(bottom_up(d - 1), bottom_up(d - 1))
def check(n):
    if n.left is None or n.right is None: return 1
    return 1 + check(n.left) + check(n.right)
max_depth = 14
total = check(bottom_up(max_depth + 1))
long_lived = bottom_up(max_depth)
for depth in range(4, max_depth + 1, 2):
    iterations = 1 << (max_depth - depth + 4)
    total += sum(check(bottom_up(depth)) for _ in range(iterations))
print(total + check(long_lived))
```

## Typescript
```ts
class TreeNode { constructor(left, right) { this.left = left; this.right = right; } }
function bottomUp(d) { return d <= 0 ? new TreeNode(null, null) : new TreeNode(bottomUp(d - 1), bottomUp(d - 1)); }
function check(n) { return n.left === null || n.right === null ? 1 : 1 + check(n.left) + check(n.right); }
const maxDepth = 14;
let total = check(bottomUp(maxDepth + 1));
const longLived = bottomUp(maxDepth);
for (let depth = 4; depth <= maxDepth; depth += 2) {
  const iterations = 1 << (maxDepth - depth + 4);
  let sum = 0;
  for (let i = 0; i < iterations; i++) sum += check(bottomUp(depth));
  total += sum;
}
console.log(total + check(longLived));
```
