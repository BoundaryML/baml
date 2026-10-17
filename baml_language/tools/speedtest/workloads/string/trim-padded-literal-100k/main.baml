function main() -> int {
  let s = "                              the quick brown fox jumps over the lazy dog and runs through the meadow with great enthusiasm                              ";
  let total = 0;
  for (let i = 0; i < 100000; i += 1) {
    let t = s.trim();
    total += t.length();
  };
  return total;
}
