function fib(n: int) -> int {
  let a = 0;
  let b = 1;
  for (let i = 0; i < n; i += 1) {
    let tmp = b;
    b = a + b;
    a = tmp;
  };
  return a;
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 100000; i += 1) { s += fib(50); };
  return s;
}
