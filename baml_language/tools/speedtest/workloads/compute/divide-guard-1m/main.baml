function maybe_divide(a: int, b: int) -> int {
  if b == 0 { 0 } else { a / b }
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += maybe_divide(i, i + 1); };
  return s;
}
