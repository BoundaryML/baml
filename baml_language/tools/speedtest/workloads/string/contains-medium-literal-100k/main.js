function main() {
  const s = "the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow|the quick brown fox jumps over the lazy dog and runs through the meadow";
  const n = "meadow";
  let c = 0;
  for(let i=0;i<100000;i++) if(s.includes(n)) c += 1;
  return c;
}
