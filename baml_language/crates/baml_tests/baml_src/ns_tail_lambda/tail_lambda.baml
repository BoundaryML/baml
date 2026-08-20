// Regression: a lambda is a valid block TAIL expression (it was silently
// dropped from block elements, typing the block as void), and equally valid as
// a `return` operand.
//
// Converted from crates/bex_engine/tests/tail_lambda.rs. Each Rust test's
// `function main()` becomes a top-level helper named after the test — the VM
// boxes `test`-block locals (a local compares unequal to a literal), so every
// value under test is produced by a function call and asserted on directly. The
// two Rust sources each declared an `mk`; they are renamed apart here since both
// now live in one namespace.

// ─── tail_lambda_annotated ────────────────────────────────────────────────────

function tail_mk() -> (int) -> int throws never {
    (x: int) -> int { x + 1 }
}

function tail_lambda_annotated() -> int { let f = tail_mk(); f(41) }

test "tail_lambda_annotated" {
    assert.equal(tail_lambda_annotated(), 42)
}

// ─── return_lambda_annotated ──────────────────────────────────────────────────

function return_mk() -> (int) -> int throws never {
    return (x: int) -> int { x + 1 }
}

function return_lambda_annotated() -> int { let f = return_mk(); f(41) }

test "return_lambda_annotated" {
    assert.equal(return_lambda_annotated(), 42)
}
