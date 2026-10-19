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
