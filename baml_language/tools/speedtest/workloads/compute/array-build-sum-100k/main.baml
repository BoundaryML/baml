function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 100000; i += 1) { arr.push(i); };
  let s = 0;
  for (let x in arr) { s += x; };
  return s;
}
