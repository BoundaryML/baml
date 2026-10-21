// Array traversal (from the native-bench array_traversal case): over the same
// 8,192 integers, sum with a for-in loop, sum by index, and rebuild the array
// element by element. setup() builds the array once, untimed.
function setup() -> int[] {
  let seed = 3;
  let out: int[] = [];
  for (let i = 0; i < 8192; i += 1) {
    seed = seed * 16807 % 2147483647;
    out.push(seed % 1000);
  }
  out
}

function iterate(xs: int[]) -> int {
  let total = 0;
  for (let v in xs) { total += v; }
  total
}

function indexed(xs: int[]) -> int {
  let total = 0;
  let i = 0;
  while (i < xs.length()) {
    total += xs[i];
    i += 1;
  }
  total
}

function rebuild(xs: int[]) -> int {
  let ys: int[] = [];
  let j = 0;
  while (j < xs.length()) {
    ys.push(xs[j]);
    j += 1;
  }
  iterate(ys)
}

function run(xs: int[]) -> int {
  iterate(xs) + indexed(xs) * 3 + rebuild(xs) * 7
}

function main() -> int {
  run(setup())
}
