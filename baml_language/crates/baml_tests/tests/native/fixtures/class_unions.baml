// Native backend fixture: closed unions of classes and enums.

class Ok { value: int }
class Err { message: string }
class Warn { code: int, note: string }
class Skip { }
enum Color { Red, Green, Blue }
enum Size { Small, Big }
class Report { outcome: Ok | Err, label: Color | Size }
class Carrier { payload: Ok | Err | null }
class Reversed { outcome: Err | Ok }

function outcome(n: int) -> Ok | Err {
    if (n >= 0) { Ok { value: n } } else { Err { message: "negative" } }
}

function describe(n: int) -> string {
    match (outcome(n)) {
        let ok: Ok => "ok " + ok.value.to_string(),
        let err: Err => "err " + err.message,
    }
}

function is_ok(n: int) -> bool {
    outcome(n) is Ok
}

function tested(n: int) -> int {
    let o = outcome(n);
    if (o is Ok) { let ok: Ok = o; ok.value * 2 } else { -1 }
}

function wide(n: int) -> Ok | Err | Warn | Skip {
    if (n == 0) { Skip { } } else if (n == 1) { Warn { code: 1, note: "w" } } else if (n > 1) { Ok { value: n } } else { Err { message: "neg" } }
}

function classify(n: int) -> int {
    match (wide(n)) {
        Ok => 1,
        Err => 2,
        Warn => 3,
        Skip => 4,
    }
}

function fields(n: int) -> string {
    match (wide(n)) {
        let ok: Ok => ok.value.to_string(),
        let err: Err => err.message,
        let warn: Warn => warn.code.to_string() + warn.note,
        let skip: Skip => "skip",
    }
}

function narrowed_arg(n: int) -> int {
    let o = outcome(n);
    if (o is Ok) { value_of(o) } else { 0 }
}

function value_of(ok: Ok) -> int {
    ok.value
}

function label(n: int) -> Color | Size {
    if (n < 0) { Size.Small } else if (n == 0) { Size.Big } else if (n == 1) { Color.Red } else { Color.Blue }
}

function label_name(n: int) -> string {
    match (label(n)) {
        let c: Color => "color " + c.to_string(),
        let s: Size => "size " + s.to_string(),
    }
}

function same_label(a: int, b: int) -> bool {
    label(a) == label(b)
}

function is_red(n: int) -> bool {
    label(n) == Color.Red
}

function not_big(n: int) -> bool {
    label(n) != Size.Big
}

function variant_arm(n: int) -> int {
    let x: int | Color = if (n > 0) { Color.Red } else if (n == 0) { Color.Green } else { n };
    match (x) {
        Color.Red => 1,
        let i: int => i,
        _ => 0,
    }
}

function literal_arm(n: int) -> string {
    let v: int | string = if (n == 0) { "a" } else if (n == 1) { 1 } else if (n == 2) { "b" } else { n };
    match (v) {
        1 => "one",
        "a" => "letter",
        _ => "other",
    }
}

function render(n: int) -> string {
    let r = Report { outcome: outcome(n), label: label(n) };
    outcome(n).to_string() + " | " + r.to_string() + " | " + [outcome(n), outcome(0 - n)].to_string()
}

function json_out(n: int) -> string {
    baml.json.to_string(outcome(n)) + " " + baml.json.to_string(Report { outcome: outcome(n), label: label(n) })
}

function json_in(raw: string) -> string {
    let o = baml.json.deserialize<Ok | Err>(raw);
    describe_outcome(o)
}

function describe_outcome(o: Ok | Err) -> string {
    match (o) {
        let ok: Ok => "ok " + ok.value.to_string(),
        let err: Err => "err " + err.message,
    }
}

function json_in_reversed(raw: string) -> string {
    let o = baml.json.deserialize<Err | Ok>(raw);
    describe_outcome(o)
}

function json_reversed(raw: string) -> string {
    let r = baml.json.deserialize<Reversed>(raw);
    describe_outcome(r.outcome)
}

function json_report(raw: string) -> string {
    let r = baml.json.deserialize<Report>(raw);
    describe_outcome(r.outcome) + " " + label_text(r.label)
}

function label_text(l: Color | Size) -> string {
    match (l) {
        let c: Color => c.to_string(),
        let s: Size => s.to_string(),
    }
}

function json_carrier(raw: string) -> string {
    let c = baml.json.deserialize<Carrier>(raw);
    let p = c.payload;
    if (p == null) { "none" } else { match (p) { let ok: Ok => "ok", _ => "err" } }
}

function carry(n: int) -> string {
    let c = Carrier { payload: if (n == 0) { null } else { outcome(n) } };
    if (c.payload == null) { "none" } else { baml.json.to_string(c) }
}

function count_ok(n: int) -> int {
    let items: (Ok | Err)[] = [];
    let i = 0 - n;
    while (i <= n) {
        items.push(outcome(i));
        i = i + 1;
    }
    let oks = 0;
    for (let item in items) {
        if (item is Ok) { oks = oks + 1; }
    }
    oks
}

function thrown_payload(n: int) -> int {
    throw Report { outcome: outcome(n), label: label(n) }
}
