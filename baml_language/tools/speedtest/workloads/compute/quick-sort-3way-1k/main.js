function setup() {
  let seed = 7;
  const random = [], dups = [], sorted = [], reversed = [];
  for (let i = 0; i < 1024; i++) { seed = seed * 16807 % 2147483647; random.push(seed % 100000); dups.push(seed % 16); }
  for (let i = 0; i < 1024; i++) { sorted.push(i * 3); reversed.push((1023 - i) * 3); }
  return [random, sorted, reversed, dups];
}
function quickSort(values) {
  const a = values.slice();
  const lows = [0], highs = [a.length - 1];
  let top = 1;
  while (top > 0) {
    top--;
    const lo = lows[top], hi = highs[top];
    if (lo < hi) {
      const pivot = a[lo + Math.floor((hi - lo) / 2)];
      let lt = lo, i = lo, gt = hi;
      while (i <= gt) {
        if (a[i] < pivot) { const v = a[lt]; a[lt] = a[i]; a[i] = v; lt++; i++; }
        else if (a[i] > pivot) { const v = a[gt]; a[gt] = a[i]; a[i] = v; gt--; }
        else i++;
      }
      if (top === lows.length) { lows.push(lo); highs.push(lt - 1); } else { lows[top] = lo; highs[top] = lt - 1; }
      top++;
      if (top === lows.length) { lows.push(gt + 1); highs.push(hi); } else { lows[top] = gt + 1; highs[top] = hi; }
      top++;
    }
  }
  return a;
}
function run(inputs) {
  let check = 0;
  for (const values of inputs) {
    const out = JSON.stringify(quickSort(values));
    let h = 0;
    for (let j = 0; j < out.length; j++) h = (h * 31 + out.charCodeAt(j)) % 1000000007;
    check = (check + h + out.length) % 1000000007;
  }
  return check;
}
function main() { return run(setup()); }
