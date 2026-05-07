function main() -> int {
  let sum = 0;
  for (let i = 0; i < 200; i += 1) {
    for (let j = 0; j < 200; j += 1) {
      sum += i * j;
    };
  };
  return sum;
}
