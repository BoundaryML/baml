function setup() {
  let seed = 3;
  const out = [];
  for (let i = 0; i < 8192; i++) { seed = seed * 16807 % 2147483647; out.push(seed % 1000); }
  return out;
}
function iterate(xs) { let total = 0; for (const v of xs) total += v; return total; }
function indexed(xs) { let total = 0; let i = 0; while (i < xs.length) { total += xs[i]; i++; } return total; }
function rebuild(xs) { const ys = []; let j = 0; while (j < xs.length) { ys.push(xs[j]); j++; } return iterate(ys); }
function run(xs) { return iterate(xs) + indexed(xs) * 3 + rebuild(xs) * 7; }
function main() { return run(setup()); }
