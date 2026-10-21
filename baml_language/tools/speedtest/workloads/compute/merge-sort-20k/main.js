function lcgValues(n, seed) {
  const out = [];
  for (let i = 0; i < n; i++) { seed = seed * 16807 % 2147483647; out.push(seed % 100000); }
  return out;
}
function mergeSort(values) {
  let a = values.slice(), b = new Array(values.length).fill(0);
  let width = 1; const n = a.length;
  while (width < n) {
    let lo = 0;
    while (lo < n) {
      const mid = Math.min(lo + width, n), hi = Math.min(lo + 2 * width, n);
      let i = lo, j = mid, k = lo;
      while (k < hi) {
        if (i < mid && (j >= hi || a[i] <= a[j])) { b[k] = a[i]; i++; }
        else { b[k] = a[j]; j++; }
        k++;
      }
      lo += 2 * width;
    }
    const t = a; a = b; b = t;
    width *= 2;
  }
  return a;
}
function checksum(xs) {
  let h = 0;
  for (let i = 0; i < xs.length; i++) {
    if (i > 0 && xs[i - 1] > xs[i]) return -1;
    h = (h * 31 + xs[i]) % 1000000007;
  }
  return h;
}
function main() {
  const rand = lcgValues(20000, 7);
  const srt = mergeSort(rand);
  let total = 0;
  for (const inp of [rand, srt, srt.slice().reverse(), rand.map(v => v % 16)]) {
    total = (total + checksum(mergeSort(inp))) % 1000000007;
  }
  return total;
}
