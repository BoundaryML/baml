function main() -> int {
  let s = "the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow";
  let delim = "|";
  let count = 0;
  for (let i = 0; i < 10000; i += 1) {
    let parts = s.split(delim);
    count += parts.length();
  };
  return count;
}
