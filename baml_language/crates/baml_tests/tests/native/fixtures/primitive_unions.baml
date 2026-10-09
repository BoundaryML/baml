// Native backend fixture: closed unions of primitives as generated enums.

class Row { amount: int | float, note: string | null }

function half(n: int) -> float {
    let f = 0.0;
    let i = 0;
    while (i < n) {
        f = f + 0.5;
        i = i + 1;
    }
    f
}

function make(n: int) -> int | float {
    if (n % 2 == 0) { n } else { half(n) }
}

function make_null(n: int) -> int | float | null {
    if (n == 0) { null } else { make(n) }
}

function reassign(n: int) -> int | float {
    let v: int | float = 3;
    if (n > 0) { v = 2.5; }
    if (n > 10) { v = n; }
    v
}

function kind(x: int | float) -> string {
    match (x) {
        int => "int",
        float => "float",
    }
}

function bound(x: int | float) -> int {
    match (x) {
        let n: int => n,
        let f: float => f.itrunc(),
    }
}

function tested(x: int | float) -> int {
    if (x is int) { let y: int = x; y + 1 } else { 0 }
}

function narrowed(x: int | float) -> float {
    if (x is float) { x * 2.0 } else { half(x) }
}

function is_number(x: int | float | string) -> bool {
    x is int | float
}

function four(x: int | float | string | bool) -> int {
    match (x) {
        int => 1,
        float => 2,
        string => 3,
        bool => 4,
    }
}

function four_of(n: int) -> int {
    let v: int | float | string | bool = if (n == 0) { "s" } else if (n == 1) { true } else if (n == 2) { 1.5 } else { n };
    four(v)
}

function wide_match(n: int) -> string {
    let v: int | float | string | bool = if (n == 0) { "s" } else if (n == 1) { true } else if (n == 2) { 1.5 } else { n };
    match (v) {
        let i: int => "i" + i.to_string(),
        let f: float => "f" + f.to_string(),
        let s: string => "s" + s,
        let b: bool => "b" + b.to_string(),
    }
}

function nullable_kind(n: int) -> string {
    let v = make_null(n);
    if (v == null) { "null" } else { match (v) { int => "int", float => "float" } }
}

function nullable_narrow(n: int) -> int {
    let v = make_null(n);
    if (v != null && v is int) { v + 100 } else { -1 }
}

function same(a: int, b: int) -> bool {
    make(a) == make(b)
}

function differ(a: int, b: int) -> bool {
    make(a) != make(b)
}

function is_three(n: int) -> bool {
    make(n) == 3
}

function is_half(n: int) -> bool {
    1.5 == make(n)
}

function nullable_same(a: int, b: int) -> bool {
    make_null(a) == make_null(b)
}

function render(n: int) -> string {
    let r = Row { amount: make(n), note: null };
    make(n).to_string() + " " + r.to_string() + " " + [make(n), make(n + 1)].to_string()
}

function json_out(n: int) -> string {
    baml.json.to_string(make(n)) + " " + baml.json.to_string(Row { amount: make(n), note: "x" })
}

function json_in(raw: string) -> string {
    let v = baml.json.deserialize<int | float>(raw);
    kind(v) + ":" + v.to_string()
}

function json_rows(raw: string) -> string {
    let rows = baml.json.deserialize<Row[]>(raw);
    let total = 0;
    for (let row in rows) {
        total = total + bound(row.amount);
    }
    total.to_string()
}

function sum(n: int) -> int {
    let xs: (int | float)[] = [];
    let i = 0;
    while (i < n) {
        xs.push(make(i));
        i = i + 1;
    }
    let total = 0;
    for (let x in xs) {
        total = total + bound(x);
    }
    total
}

function double(x: int | float) -> int | float {
    match (x) {
        let n: int => n * 2,
        let f: float => f * 2.0,
    }
}

function doubled(n: int) -> string {
    double(make(n)).to_string()
}

function coalesce(n: int) -> int {
    let m: map<string, int> = { "a": 1 };
    let k = if (n > 0) { "a" } else { "b" };
    (m.get(k) ?? n) + 1
}

function coalesce_union(n: int) -> string {
    (make_null(n) ?? 0).to_string()
}

function big(n: int) -> string {
    let v: int | bigint = if (n > 0) { 1n << 70 } else { n };
    match (v) {
        let i: int => "int " + i.to_string(),
        let b: bigint => "bigint " + b.to_string(),
    }
}

function flag(n: int) -> string {
    let v: bool | int = if (n > 0) { true } else { n };
    match (v) {
        let b: bool => if (b) { "yes" } else { "no" },
        let i: int => i.to_string(),
    }
}

function plain_tests(n: int) -> int {
    let xs = [n];
    let total = 0;
    if (n is int) { total = total + 1; }
    if (xs is int[]) { total = total + 10; }
    match (xs) {
        int[] => { total = total + 1000; }
    }
    total
}
