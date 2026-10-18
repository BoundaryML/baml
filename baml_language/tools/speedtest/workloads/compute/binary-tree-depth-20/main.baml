function tree_sum(depth: int) -> int {
  if depth <= 0 { 1 } else { tree_sum(depth - 1) + tree_sum(depth - 1) }
}
function main() -> int { tree_sum(20) }
