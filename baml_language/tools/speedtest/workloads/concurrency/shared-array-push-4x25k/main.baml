function push_n(arr: int[], start: int, n: int) -> int {
  let i = 0;
  while i < n {
    arr.push(start + i);
    i += 1;
  };
  0
}
function main() -> int {
  let arr = [];
  let a = spawn { push_n(arr, 0, 25000) };
  let b = spawn { push_n(arr, 25000, 25000) };
  let c = spawn { push_n(arr, 50000, 25000) };
  let d = spawn { push_n(arr, 75000, 25000) };
  (await a) + (await b) + (await c) + (await d);
  arr.length()
}
