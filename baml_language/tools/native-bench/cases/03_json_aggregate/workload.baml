class State { raw: string }
class Row { account: string, amount_cents: int | float, active: bool }
class Total { account: string, total_cents: int }
class Invalid { message: string }
function prepare(raw: string) -> State { State { raw: raw } }
function aggregate(raw: string) -> string {
    let rows = baml.json.deserialize<Row[]>(raw);
    let totals: map<string, int> = {};
    if (rows.length() > 1000000) { throw Invalid { message: "too many rows" }; }
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
function run(state: State) -> string {
    aggregate(state.raw) catch (_) { let _ => "{\"error\":\"invalid\"}" }
}
