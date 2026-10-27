class Window {
  constructor(values, pos, end) { this.values = values; this.pos = pos; this.end = end; }
  [Symbol.iterator]() { return this; }
  next() {
    if (this.pos >= this.end) return { done: true, value: undefined };
    const v = this.values[this.pos];
    this.pos += 1;
    return { done: false, value: v };
  }
}
function main() {
  const xs = [];
  let seed = 5;
  for (let i = 0; i < 2000; i++) { seed = seed * 16807 % 2147483647; xs.push(seed % 1000); }
  let acc = 0;
  for (let round = 0; round < 10; round++) {
    const skip = round % 3;
    acc += xs.filter((x) => x % 3 !== skip).map((x) => x * 2 + skip).reduce((a, x) => a + x, 0);
    for (const v of new Window(xs, round * 100, round * 100 + 1000)) acc += v % 17;
  }
  return acc;
}
