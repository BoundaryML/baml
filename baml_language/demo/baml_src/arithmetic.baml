// ── Arithmetic & Binary Ops ─────────────────────────────────────

// int + int → int
function IntArith(a: int, b: int) -> int {
    return a + b;
}

// fresh literal arithmetic: 1 + 5 on a let-bound var → int
function LiteralArith() -> int {
    let x = 1 + 10 * 10 - 5 / 2;
    x = x + 5;
    return x;
}

// float contaminates: int + float → float
function FloatContamination(a: int, b: float) -> float {
    return a + b;
}

// comparison → bool
function Comparison(a: int, b: int) -> bool {
    return a < b;
}

// logical ops → bool
function LogicalOps(a: bool, b: bool) -> bool {
    return a && b;
}

// string concat
function StringConcat(a: string, b: string) -> string {
    return a + b;
}

// compound assignment: x += 5
function CompoundAssign() -> int {
    let x = 10;
    x += 5;
    return x;
}

// ── Negation ────────────────────────────────────────────────────

function NegateInt(x: int) -> int {
    return -x;
}

function NegateFreshLiteral() -> int {
    let x = -42;
    return x;
}
