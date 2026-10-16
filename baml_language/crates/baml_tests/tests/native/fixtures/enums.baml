// Native backend fixture: enums as values, in matches, as fields and in JSON.

enum Color { Red, Green, Blue }

class Pixel { color: Color, weight: int }

function pick(n: int) -> Color {
    if (n % 3 == 0) { Color.Red } else if (n % 3 == 1) { Color.Green } else { Color.Blue }
}

function name(c: Color) -> string {
    match (c) {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue => "blue",
    }
}

function is_warm(c: Color) -> bool {
    match (c) {
        Color.Red => true,
        _ => false,
    }
}

function same(a: int, b: int) -> bool {
    pick(a) == pick(b)
}

function differ(a: int, b: int) -> bool {
    pick(a) != pick(b)
}

function render(n: int) -> string {
    let p = Pixel { color: pick(n), weight: n };
    pick(n).to_string() + " " + p.to_string() + " " + [pick(n), Color.Blue].to_string()
}

function json_out(n: int) -> string {
    baml.json.to_string(Pixel { color: pick(n), weight: n }) + baml.json.to_string(pick(n + 1))
}

function json_in(raw: string) -> string {
    let p = baml.json.deserialize<Pixel>(raw);
    name(p.color) + p.weight.to_string()
}

function count_red(n: int) -> int {
    let colors: Color[] = [];
    let i = 0;
    while (i < n) {
        colors.push(pick(i));
        i = i + 1;
    }
    let reds = 0;
    for (let c in colors) {
        if (c == Color.Red) { reds = reds + 1; }
    }
    reds
}

function tint(n: int, base: Color = Color.Green) -> string {
    name(base) + n.to_string()
}

function tinted(n: int) -> string {
    tint(n) + tint(n, base = Color.Blue)
}

function nullable(n: int) -> string {
    let c: Color | null = if (n > 0) { pick(n) } else { null };
    if (c == null) { "none" } else { "some" }
}
