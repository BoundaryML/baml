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
function main() {
  const seq = dna(25000);
  let check = 0;
  for (const k of [1, 2, 3, 4, 6, 12, 18]) {
    const counts = count(seq, k);
    let most = 0;
    for (const v of counts.values()) if (v > most) most = v;
    check = (check * 31 + counts.size * 1000 + most) % 1000000007;
  }
  return check + (count(seq, 3).get("ggt") ?? 0) + (count(seq, 6).get("ggtatt") ?? 0);
}
