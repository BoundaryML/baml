// ── Type Mismatch Errors ────────────────────────────────────────

// return string where int expected → error
function ReturnWrongType() -> int {
    return "hello";
}

// return int where string expected → error
function ReturnIntForString() -> string {
    return 42;
}

// annotated let mismatch: let x: string = 1 → error
function AnnotatedLetMismatch() -> string {
    let x: string = 1;
    return x;
}

// ── Union Types ─────────────────────────────────────────────────

function ReturnUnion(useFirst: bool) -> int | string {
    if (useFirst) { 1 } else { "hello" }
}

// ── Optional Types ──────────────────────────────────────────────

function ReturnOptional(hasValue: bool) -> int? {
    if (hasValue) {
        return 42;
    }
    return null;
}
