class TreeNode { constructor(left, right) { this.left = left; this.right = right; } }
function bottomUp(d) { return d <= 0 ? new TreeNode(null, null) : new TreeNode(bottomUp(d - 1), bottomUp(d - 1)); }
function check(n) { return n.left === null || n.right === null ? 1 : 1 + check(n.left) + check(n.right); }
function main() {
  const maxDepth = 14;
  let total = check(bottomUp(maxDepth + 1));
  const longLived = bottomUp(maxDepth);
  for (let depth = 4; depth <= maxDepth; depth += 2) {
    const iterations = 1 << (maxDepth - depth + 4);
    let sum = 0;
    for (let i = 0; i < iterations; i++) sum += check(bottomUp(depth));
    total += sum;
  }
  return total + check(longLived);
}
