function classify(x: int) -> int {
  if x % 5 == 0 { 1 }
  else if x % 5 == 1 { 2 }
  else if x % 5 == 2 { 3 }
  else if x % 5 == 3 { 4 }
  else { 5 }
}
function main() -> int {
  let s = 0;
  for (let i = 0; i < 1000000; i += 1) { s += classify(i); };
  return s;
}
