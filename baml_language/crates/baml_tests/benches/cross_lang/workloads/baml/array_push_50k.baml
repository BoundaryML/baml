function main() -> int {
  let arr: int[] = [];
  for (let i = 0; i < 50000; i += 1) {
    arr.push(i);
  };
  return arr.length();
}
