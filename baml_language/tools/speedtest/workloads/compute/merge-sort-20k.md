# compute::merge sort 20k

## BAML
```baml
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
```

## Python
```python
def lcg_values(n, seed):
    out = []
    for _ in range(n):
        seed = seed * 16807 % 2147483647
        out.append(seed % 100000)
    return out
def merge_sort(values):
    a = list(values); b = [0] * len(values)
    width = 1; n = len(a)
    while width < n:
        lo = 0
        while lo < n:
            mid = min(lo + width, n); hi = min(lo + 2 * width, n)
            i = lo; j = mid; k = lo
            while k < hi:
                if i < mid and (j >= hi or a[i] <= a[j]):
                    b[k] = a[i]; i += 1
                else:
                    b[k] = a[j]; j += 1
                k += 1
            lo += 2 * width
        a, b = b, a
        width *= 2
    return a
def checksum(xs):
    h = 0
    for i, x in enumerate(xs):
        if i > 0 and xs[i - 1] > x: return -1
        h = (h * 31 + x) % 1000000007
    return h
rand = lcg_values(20000, 7)
srt = merge_sort(rand)
total = 0
for inp in [rand, srt, srt[::-1], [v % 16 for v in rand]]:
    total = (total + checksum(merge_sort(inp))) % 1000000007
print(total)
```

## Typescript
```ts
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
const rand = lcgValues(20000, 7);
const srt = mergeSort(rand);
let total = 0;
for (const inp of [rand, srt, srt.slice().reverse(), rand.map(v => v % 16)]) {
  total = (total + checksum(mergeSort(inp))) % 1000000007;
}
console.log(total);
```
