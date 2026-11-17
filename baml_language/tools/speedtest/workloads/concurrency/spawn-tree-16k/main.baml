// A binary tree of tasks: every task above the leaves spawns two children
// and awaits both, so 16,383 tasks exist in one job and a parent stays parked
// while its subtree runs. Measures spawning, parking and resuming tasks when
// thousands are alive at once.
function tree(depth: int) -> int {
  if (depth == 0) { return 1; }
  let left = spawn { tree(depth - 1) };
  let right = spawn { tree(depth - 1) };
  (await left) + (await right) + 1
}
function main() -> int {
  tree(13)
}
