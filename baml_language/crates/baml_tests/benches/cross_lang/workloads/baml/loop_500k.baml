function main() -> int {
  let sum = 0;
  for (let i = 0; i < 500000; i += 1) {
    sum += i;
  };
  return sum;
}
