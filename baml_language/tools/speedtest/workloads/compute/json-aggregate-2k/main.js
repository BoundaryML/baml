function rowsJson(n) {
  let seed = 11;
  const parts = [];
  for (let i = 0; i < n; i++) {
    seed = seed * 16807 % 2147483647;
    const account = String(seed % 97).padStart(3, "0");
    const amount = String(seed % 100000) + (i % 5 === 0 ? ".0" : "");
    const active = seed % 7 !== 0 ? "true" : "false";
    parts.push(`{"account": "acct-${account}", "amount_cents": ${amount}, "active": ${active}}`);
  }
  return "[" + parts.join(", ") + "]";
}

function aggregate(raw) {
  const totals = new Map();
  for (const row of JSON.parse(raw)) {
    if (!/^[\x00-\x7f]*$/.test(row.account)) throw new Error("non-ASCII account");
    const a = row.amount_cents;
    if (a < 0 || a > 1000000 || a !== Math.floor(a)) throw new Error("invalid amount");
    if (row.active) totals.set(row.account, (totals.get(row.account) ?? 0) + a);
  }
  const keys = [...totals.keys()].sort();
  return JSON.stringify(keys.map(k => ({ account: k, total_cents: totals.get(k) })));
}

function main() {
  const raw = rowsJson(2048);
  let check = 0;
  for (let i = 0; i < 20; i++) {
    let out;
    try { out = aggregate(raw); } catch (e) { out = "invalid"; }
    let h = 0;
    for (let j = 0; j < out.length; j++) h = (h * 31 + out.charCodeAt(j)) % 1000000007;
    check = (check + h + out.length) % 1000000007;
  }
  return check;
}
