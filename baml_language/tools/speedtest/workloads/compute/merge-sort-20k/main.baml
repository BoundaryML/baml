// Bottom-up merge sort written in user code (ported from the native-bench
// merge_sort case): index reads and writes, branches and buffer swaps in tight
// loops over a 20,000-element int array, sorted from four input orders.
function lcg_values(n: int, seed0: int) -> int[] {
  let seed = seed0;
  let out: int[] = [];
  for (let i = 0; i < n; i += 1) {
    seed = seed * 16807 % 2147483647;
    out.push(seed % 100000);
  }
  out
}
function merge_sort(values: int[]) -> int[] {
  let a: int[] = [];
  let b: int[] = [];
  for (let value in values) { a.push(value); b.push(0); }
  let width = 1;
  let n = a.length();
  while (width < n) {
    let lo = 0;
    while (lo < n) {
      let mid = lo + width;
      if (mid > n) { mid = n; }
      let hi = lo + 2 * width;
      if (hi > n) { hi = n; }
      let i = lo;
      let j = mid;
      let k = lo;
      while (k < hi) {
        if (i < mid && (j >= hi || a[i] <= a[j])) {
          b[k] = a[i]; i += 1;
        } else { b[k] = a[j]; j += 1; }
        k += 1;
      }
      lo += 2 * width;
    }
    let tmp = a; a = b; b = tmp;
    width *= 2;
  }
  a
}
function checksum(xs: int[]) -> int {
  let h = 0;
  for (let i = 0; i < xs.length(); i += 1) {
    if (i > 0 && xs[i - 1] > xs[i]) { return -1; }
    h = (h * 31 + xs[i]) % 1000000007;
  }
  h
}
function main() -> int {
  let random = lcg_values(20000, 7);
  let sorted = merge_sort(random);
  let reversed = sorted.reverse();
  let dups: int[] = [];
  for (let v in random) { dups.push(v % 16); }
  let total = 0;
  for (let input in [random, sorted, reversed, dups]) {
    total = (total + checksum(merge_sort(input))) % 1000000007;
  }
  total
}
