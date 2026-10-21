function main() {
  const s = "the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow";
  const delim = "|";
  let c = 0;
  for(let i=0;i<10000;i++) c += s.split(delim).length;
  return c;
}
