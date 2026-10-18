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
