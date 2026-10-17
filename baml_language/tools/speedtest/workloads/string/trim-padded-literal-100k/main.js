function main() {
  const s = "                              the quick brown fox jumps over the lazy dog and runs through the meadow with great enthusiasm                              ";
  let t = 0;
  for(let i=0;i<100000;i++) t += s.trim().length;
  return t;
}
