function main() -> int {
  let s = 0;
  for (let i = 0; i < 10000; i += 1) {
    s += await spawn { 1 };
  };
  s
}
