function sum_range(start: int, end: int) -> int {
  let s = 0;
  let i = start;
  while i < end {
    s += i;
    i += 1;
  };
  s
}
function main() -> int {
  let a = spawn { sum_range(0, 250000) };
  let b = spawn { sum_range(250000, 500000) };
  let c = spawn { sum_range(500000, 750000) };
  let d = spawn { sum_range(750000, 1000000) };
  (await a) + (await b) + (await c) + (await d)
}
