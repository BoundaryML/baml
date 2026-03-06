// ── Fresh/Regular Literal Widening ──────────────────────────────

// return literal 1 against literal return type → should match (no error)
function LiteralReturn() -> 1 {
    return 1;
}

// let widens fresh → x should be int, not Literal(1)
function LetWidening() -> int {
    let x = 1;
    return x;
}

// annotated let preserves literal type
function AnnotatedLet() -> 1 {
    let x: 1 = 1;
    return x;
}

// let widening with string
function StringWidening() -> string {
    let s = "hello";
    return s;
}

// return wrong literal against literal return type → error
function WrongLiteral() -> 1 {
    return 2;
}

// return string literal matching
function StringLiteralReturn() -> "ok" {
    return "ok";
}

// return string literal mismatch → error
function StringLiteralMismatch() -> "ok" {
    return "fail";
}
