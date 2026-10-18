function validate(x: int) -> int {
  if x < 0 { return -1; };
  if x > 1000000 { return -2; };
  if x % 7 == 0 { return x * 2; };
  if x % 3 == 0 { return x + 1; };
  return x;
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += validate(i); };
  return s;
}
