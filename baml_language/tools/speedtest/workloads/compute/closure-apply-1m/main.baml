function apply(f: (int) -> int, x: int) -> int { f(x) }
function main() -> int {
  let double = (x: int) -> int { x * 2 };
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += apply(double, i); };
  return s;
}
