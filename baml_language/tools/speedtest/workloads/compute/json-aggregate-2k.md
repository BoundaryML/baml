# compute::json aggregate 2k

## eval-setup
```py
import json
seed = 11
rows = []
for i in range(2048):
    seed = seed * 16807 % 2147483647
    amount = seed % 100000
    rows.append({
        "account": "acct-%03d" % (seed % 97),
        "amount_cents": float(amount) if i % 5 == 0 else amount,
        "active": seed % 7 != 0,
    })
raw = json.dumps(rows)
baml_raw = json.dumps(raw)
py_raw = repr(raw)
js_raw = json.dumps(raw)
```

## BAML
```baml
// A data-transformation job (ported from the native-bench json_aggregate case):
// typed JSON deserialize of 2,048 rows, validation with a thrown error class,
// an int | float union narrowed with match, grouping into a map, sorting the
// keys and serializing the result. Run 20 times.
class Row { account: string, amount_cents: int | float, active: bool }
class Total { account: string, total_cents: int }
class Invalid { message: string }
function aggregate(raw: string) -> string {
  let rows = baml.json.deserialize<Row[]>(raw);
  let totals: map<string, int> = {};
  for (let row in rows) {
    if (!row.account.is_ascii()) { throw Invalid { message: "non-ASCII account" }; }
    let amount = match (row.amount_cents) {
      let n: int => n,
      let f: float => {
        if (f != f.floor() || f < 0.0 || f > 1000000.0) {
          throw Invalid { message: "invalid amount" };
        }
        f.itrunc()
      }
    };
    if (amount < 0 || amount > 1000000) { throw Invalid { message: "invalid amount" }; }
    if (row.active) { totals[row.account] = (totals.get(row.account) ?? 0) + amount; }
  }
  let accounts = totals.keys();
  accounts.sort();
  let output: Total[] = [];
  for (let account in accounts) {
    output.push(Total { account: account, total_cents: totals.get(account) ?? 0 });
  }
  baml.json.to_string(output)
}
function main() -> int {
  let raw = $$baml_raw;
  let check = 0;
  for (let i = 0; i < 20; i += 1) {
    let out = aggregate(raw) catch (_) { let _ => "invalid" };
    let h = 0;
    for (let c in out.to_code_points()) { h = (h * 31 + c) % 1000000007; }
    check = (check + h + out.length()) % 1000000007;
  }
  check
}
```

## Python
```python
import json
class Invalid(Exception): pass
def aggregate(raw):
    rows = json.loads(raw)
    totals = {}
    for row in rows:
        if not row["account"].isascii(): raise Invalid()
        a = row["amount_cents"]
        if isinstance(a, float):
            if a != int(a) or a < 0 or a > 1000000: raise Invalid()
            a = int(a)
        if a < 0 or a > 1000000: raise Invalid()
        if row["active"]: totals[row["account"]] = totals.get(row["account"], 0) + a
    return json.dumps([{"account": k, "total_cents": totals[k]} for k in sorted(totals)], separators=(",", ":"))
raw = $$py_raw
check = 0
for _ in range(20):
    try:
        out = aggregate(raw)
    except Invalid:
        out = "invalid"
    h = 0
    for ch in out: h = (h * 31 + ord(ch)) % 1000000007
    check = (check + h + len(out)) % 1000000007
print(check)
```

## Typescript
```ts
function aggregate(raw) {
  const rows = JSON.parse(raw);
  const totals = new Map();
  for (const row of rows) {
    if (!/^[\x00-\x7f]*$/.test(row.account)) throw new Error("non-ASCII account");
    const a = row.amount_cents;
    if (a < 0 || a > 1000000 || a !== Math.floor(a)) throw new Error("invalid amount");
    if (row.active) totals.set(row.account, (totals.get(row.account) ?? 0) + a);
  }
  const keys = [...totals.keys()].sort();
  return JSON.stringify(keys.map(k => ({ account: k, total_cents: totals.get(k) })));
}
const raw = $$js_raw;
let check = 0;
for (let i = 0; i < 20; i++) {
  let out;
  try { out = aggregate(raw); } catch (e) { out = "invalid"; }
  let h = 0;
  for (let j = 0; j < out.length; j++) h = (h * 31 + out.charCodeAt(j)) % 1000000007;
  check = (check + h + out.length) % 1000000007;
}
console.log(check);
```
