// A backtick template whose holes are strings: `${a}/${b}`, 10,000 times.
// The same string as `a + "/" + b`.
function main() -> int {
  let names = ["apple", "pear", "plum", "fig", "kiwi"];
  let families = ["FR", "VG", "DY"];
  let seed = 7;
  let total = 0;
  for (let i = 0; i < 10000; i += 1) {
    seed = seed * 16807 % 2147483647;
    let a = names[seed % 5];
    let b = families[i % 3];
    let s = `${a}/${b}`;
    total += s.length();
  }
  total
}
