function main() -> int {
  let s = 0;
  for (let i = 0; i < 500; i += 1) {
    for (let j = 0; j < 500; j += 1) { s += i * j; };
  };
  return s;
}
