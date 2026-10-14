// Native backend fixture: maps keyed by string, int and bool.

class Inventory { stock: map<string, int>, name: string }

function tally(n: int) -> string {
    let m: map<string, int> = { "a": n, "b": n + 1 };
    m["c"] = n + 2;
    m.set("a", m["a"] * 2);
    m.delete("b");
    let total = 0;
    for (let k in m.keys()) {
        total = total + m[k];
    }
    m.to_string() + " " + total.to_string() + " " + m.length().to_string()
}

function lookup(n: int) -> int {
    let m: map<int, string> = { 1: "one", 2: "two", -1: "minus" };
    m[n].length()
}

function flags(b: bool) -> string {
    let m: map<bool, int> = { true: 1 };
    let v = m.get(b);
    let found = if (v == null) { "missing" } else { "found" };
    found + " " + m.has(b).to_string() + " " + m.to_string()
}

function json_out(n: int) -> string {
    let inv = Inventory { stock: { "x": n }, name: "s" };
    inv.stock["y"] = n * 2;
    baml.json.to_string(inv) + " " + baml.json.to_string(inv.stock)
}

function json_in(raw: string) -> string {
    let inv = baml.json.deserialize<Inventory>(raw);
    inv.name + " " + inv.stock.to_string() + " " + inv.stock.length().to_string()
}

function json_int_keys(n: int) -> string {
    let m: map<int, int> = { n: n };
    baml.json.to_string(m)
}

function json_int_keys_in(raw: string) -> int {
    let m = baml.json.deserialize<map<int, int>>(raw);
    m.length()
}

function get_or_insert(n: int) -> string {
    let m: map<string, int> = {};
    let a = m.get_or_insert("k", n);
    let b = m.get_or_insert("k", n + 1);
    let vs = m.values();
    m.clear();
    a.to_string() + " " + b.to_string() + " " + vs.to_string() + " " + m.length().to_string()
}

function alias(n: int) -> int {
    let a: map<string, int> = { "x": n };
    let b = a;
    b["x"] = n + 1;
    a["x"]
}
