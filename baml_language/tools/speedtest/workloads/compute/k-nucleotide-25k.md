# compute::k-nucleotide 25k

## BAML
```baml
// Benchmarks Game k-nucleotide: count every k-mer of a DNA sequence in a hash
// map for k = 1, 2, 3, 4, 6, 12, 18. The sequence comes from the fasta
// benchmark's LCG with integer thresholds. Map-heavy: string slicing, hashing,
// get/insert of short string keys.
function dna(n: int) -> string {
  let seed = 42;
  let out: string[] = [];
  for (let i = 0; i < n; i += 1) {
    seed = (seed * 3877 + 29573) % 139968;
    if (seed < 42000) { out.push("a"); }
    else if (seed < 70000) { out.push("c"); }
    else if (seed < 98000) { out.push("g"); }
    else { out.push("t"); }
  }
  out.join("")
}
function count(seq: string, k: int) -> map<string, int> {
  let counts: map<string, int> = {};
  let last = seq.length() - k;
  for (let i = 0; i <= last; i += 1) {
    let key = seq.slice(i, i + k);
    counts[key] = (counts.get(key) ?? 0) + 1;
  }
  counts
}
function main() -> int {
  let seq = dna(25000);
  let check = 0;
  for (let k in [1, 2, 3, 4, 6, 12, 18]) {
    let counts = count(seq, k);
    let most = 0;
    for (let v in counts.values()) {
      if (v > most) { most = v; }
    }
    check = check * 31 + counts.length() * 1000 + most;
    check = check % 1000000007;
  }
  let probe = count(seq, 3);
  check + (probe.get("ggt") ?? 0) + (count(seq, 6).get("ggtatt") ?? 0)
}
```

## Python
```python
def dna(n):
    seed = 42
    out = []
    for _ in range(n):
        seed = (seed * 3877 + 29573) % 139968
        if seed < 42000: out.append("a")
        elif seed < 70000: out.append("c")
        elif seed < 98000: out.append("g")
        else: out.append("t")
    return "".join(out)
def count(seq, k):
    counts = {}
    for i in range(len(seq) - k + 1):
        key = seq[i:i + k]
        counts[key] = counts.get(key, 0) + 1
    return counts
seq = dna(25000)
check = 0
for k in [1, 2, 3, 4, 6, 12, 18]:
    counts = count(seq, k)
    most = max(counts.values())
    check = (check * 31 + len(counts) * 1000 + most) % 1000000007
print(check + count(seq, 3).get("ggt", 0) + count(seq, 6).get("ggtatt", 0))
```

## Typescript
```ts
function dna(n) {
  let seed = 42;
  const out = [];
  for (let i = 0; i < n; i++) {
    seed = (seed * 3877 + 29573) % 139968;
    if (seed < 42000) out.push("a");
    else if (seed < 70000) out.push("c");
    else if (seed < 98000) out.push("g");
    else out.push("t");
  }
  return out.join("");
}
function count(seq, k) {
  const counts = new Map();
  for (let i = 0; i <= seq.length - k; i++) {
    const key = seq.slice(i, i + k);
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}
const seq = dna(25000);
let check = 0;
for (const k of [1, 2, 3, 4, 6, 12, 18]) {
  const counts = count(seq, k);
  let most = 0;
  for (const v of counts.values()) if (v > most) most = v;
  check = (check * 31 + counts.size * 1000 + most) % 1000000007;
}
console.log(check + (count(seq, 3).get("ggt") ?? 0) + (count(seq, 6).get("ggtatt") ?? 0));
```
