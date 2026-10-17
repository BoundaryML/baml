// A data-transformation job (from the native-bench json_aggregate case): build
// a 2,048-row JSON document, then 20 times decode it into typed classes,
// validate with a thrown error class, narrow an int | float union with match,
// group into a map, sort the keys and serialize the result.
class Row {
  account: string
  amount_cents: int | float
  active: bool
}

class Total {
  account: string
  total_cents: int
}

class Invalid {
  message: string
}

function rows_json(n: int) -> string {
  let seed = 11;
  let parts: string[] = [];
  for (let i = 0; i < n; i += 1) {
    seed = seed * 16807 % 2147483647;
    let account = (seed % 97).to_string();
    while (account.length() < 3) { account = "0" + account; }
    let amount = (seed % 100000).to_string();
    if (i % 5 == 0) { amount = amount + ".0"; }
    let active = if (seed % 7 != 0) { "true" } else { "false" };
    parts.push("{\"account\": \"acct-" + account + "\", \"amount_cents\": " + amount + ", \"active\": " + active + "}");
  }
  "[" + parts.join(", ") + "]"
}

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
  let raw = rows_json(2048);
  let check = 0;
  for (let i = 0; i < 20; i += 1) {
    let out = aggregate(raw) catch (_) { let _ => "invalid" };
    let h = 0;
    for (let c in out.to_code_points()) { h = (h * 31 + c) % 1000000007; }
    check = (check + h + out.length()) % 1000000007;
  }
  check
}
