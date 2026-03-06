// ── Enum Usage ──────────────────────────────────────────────────

function UseEnum() -> Color {
    let c = Color.Red;
    return c;
}

// enum variant in a block
function EnumInBlock() -> Color {
    let c = {
        let x = Color.Green;
        x
    };
    return c;
}

// wrong enum variant → error
function BadEnumVariant() -> Color {
    return Color.Purple;
}

// ── Match Expression ────────────────────────────────────────────

function MatchColor(c: Color) -> string {
    match (c) {
        Color.Red => "red",
        Color.Green => "green",
        Color.Blue => "blue",
    }
}

function MatchInt(x: int) -> string {
    match (x) {
        1 => "one",
        2 => "two",
        _ => "other",
    }
}
