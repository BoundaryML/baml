// Native backend fixture: unions of literals erase to their primitive.

class Tagged { tag: "lo" | "hi", level: 1 | 2 | 3 }

function pick(n: int) -> "lo" | "hi" {
    if (n < 10) { "lo" } else { "hi" }
}

function level(n: int) -> 1 | 2 | 3 {
    if (n < 0) { 1 } else if (n == 0) { 2 } else { 3 }
}

function describe(n: int) -> string {
    match (pick(n)) {
        "lo" => "low",
        "hi" => "high",
    }
}

function level_name(n: int) -> string {
    match (level(n)) {
        1 => "one",
        2 => "two",
        3 => "three",
    }
}

function same_tag(a: int, b: int) -> bool {
    pick(a) == pick(b)
}

function tag_is_hi(n: int) -> bool {
    pick(n) == "hi"
}

function is_lo(n: int) -> bool {
    pick(n) is "lo"
}

function maybe(n: int) -> "x" | "y" | null {
    if (n == 0) { null } else if (n > 0) { "x" } else { "y" }
}

function maybe_text(n: int) -> string {
    let m = maybe(n);
    if (m == null) { "none" } else { "some" }
}

function flag(n: int) -> true | false {
    n > 0
}

function width(n: int) -> int {
    let w: 1 | 2 = if (n > 0) { 1 } else { 2 };
    w * 10
}

function render(n: int) -> string {
    let t = Tagged { tag: pick(n), level: level(n) };
    t.to_string() + " " + t.tag + " " + t.level.to_string()
}

function json_out(n: int) -> string {
    baml.json.to_string(Tagged { tag: pick(n), level: level(n) })
}

function count_lo(n: int) -> int {
    let tags: ("lo" | "hi")[] = [];
    let i = 0;
    while (i < n) {
        tags.push(pick(i * 4));
        i = i + 1;
    }
    let lows = 0;
    for (let t in tags) {
        if (t == "lo") { lows = lows + 1; }
    }
    lows
}
