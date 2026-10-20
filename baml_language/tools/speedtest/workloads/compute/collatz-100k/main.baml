function collatz_steps(n: int) -> int {
  let steps = 0;
  let x = n;
  while x != 1 {
    if x % 2 == 0 { x = x / 2; }
    else { x = 3 * x + 1; };
    steps += 1;
  };
  return steps;
}
function main() -> int {
  let s = 0;
  for (let i = 1; i < 100000; i += 1) { s += collatz_steps(i); };
  return s;
}
