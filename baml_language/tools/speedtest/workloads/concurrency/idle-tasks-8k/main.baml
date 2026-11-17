// The spawn tree with every leaf asleep for 100 ms: 8,192 sleeping tasks and
// 8,191 parents parked on them are all alive at the same moment. The time of
// a job is the sleep plus the cost of starting and joining the tree; the
// peak memory of the process is what 16,383 idle tasks cost.
function nap() -> int {
  baml.sys.sleep(baml.time.Duration.from_milliseconds(100n)) catch (e) { let e => 0 };
  1
}
function tree(depth: int) -> int {
  if (depth == 0) { return nap(); }
  let left = spawn { tree(depth - 1) };
  let right = spawn { tree(depth - 1) };
  (await left) + (await right) + 1
}
function main() -> int {
  tree(13)
}
