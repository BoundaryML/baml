// A backtick template whose holes are strings: `${a}/${b}`, 10,000 times.
// The same string as `a + "/" + b`.
function main() {
  const names = ["apple", "pear", "plum", "fig", "kiwi"];
  const families = ["FR", "VG", "DY"];
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i++) {
    seed = seed * 16807 % 2147483647;
    const a = names[seed % 5];
    const b = families[i % 3];
    const s = `${a}/${b}`;
    total += s.length;
  }
  return total;
}
