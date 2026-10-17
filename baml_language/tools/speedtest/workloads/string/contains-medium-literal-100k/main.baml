function main() -> int {
  let s = "the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow";
  let needle = "meadow";
  let count = 0;
  for (let i = 0; i < 100000; i += 1) {
    if s.includes(needle) { count += 1; };
  };
  return count;
}
