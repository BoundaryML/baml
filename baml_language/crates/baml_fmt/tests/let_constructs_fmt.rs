//! Formatter regressions for `if let`, `while let`, and `let else`.
//!
//! Two things these tests assert that the project-level snapshot tier
//! doesn't:
//!
//!   1. **Comment trivia survives** the formatter for every position
//!      around the construct's header (before `let`, between pattern and
//!      `=`, between `=` and the initializer, between the initializer and
//!      the body block, in the else branch, etc.).
//!   2. **Formatter is idempotent** — `format(format(source)) == format(source)`.
//!
//! These regressions caught a bug where `IfLetExpr::print` /
//! `WhileLetStmt::print` re-implemented the let-header printing manually
//! and silently dropped comments around `=`. Both now delegate to
//! `LetStmt::print_header`.

use baml_fmt::{FormatOptions, format};

fn fmt(source: &str) -> String {
    let opts = FormatOptions::default();
    format(source, &opts).expect("formatter should succeed")
}

fn assert_idempotent(source: &str) {
    let first = fmt(source);
    let second = fmt(&first);
    assert_eq!(first, second, "formatter must be idempotent");
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected formatted output to contain {needle:?}, got:\n{haystack}"
    );
}

// ── if let ────────────────────────────────────────────────────────────────

#[test]
fn if_let_preserves_inline_comment_before_initializer() {
    let source = "\
function example(v: int | string) -> int {
    if let n: int = /* the int branch */ v {
        return n;
    }
    return -1;
}
";
    let out = fmt(source);
    assert_contains(&out, "/* the int branch */");
    assert_idempotent(source);
}

#[test]
fn if_let_preserves_line_comment_above_header() {
    let source = "\
function example(v: int | string) -> int {
    // try the int branch first
    if let n: int = v {
        return n;
    }
    return -1;
}
";
    let out = fmt(source);
    assert_contains(&out, "// try the int branch first");
    assert_idempotent(source);
}

#[test]
fn if_let_preserves_comment_inside_body() {
    let source = "\
function example(v: int | string) -> int {
    if let n: int = v {
        // double it
        return n * 2;
    }
    return -1;
}
";
    let out = fmt(source);
    assert_contains(&out, "// double it");
    assert_idempotent(source);
}

#[test]
fn if_let_preserves_comment_in_else_branch() {
    let source = "\
function example(v: int | string) -> int {
    if let n: int = v {
        return n;
    } else {
        // fallback path
        return -1;
    }
}
";
    let out = fmt(source);
    assert_contains(&out, "// fallback path");
    assert_idempotent(source);
}

#[test]
fn if_let_chain_preserves_comments_between_arms() {
    let source = "\
function example(v: int | string | bool) -> int {
    if let i: int = v {
        return 1;
    } else if let s: string = v {
        // we got a string
        return 2;
    } else {
        return -1;
    }
}
";
    let out = fmt(source);
    assert_contains(&out, "// we got a string");
    assert_idempotent(source);
}

#[test]
fn if_let_idempotent_when_used_as_expression() {
    let source = "\
function example(v: int | string) -> int {
    let r = if let n: int = v {
        n * 2
    } else {
        0
    };
    return r;
}
";
    assert_idempotent(source);
}

// ── while let ─────────────────────────────────────────────────────────────

#[test]
fn while_let_preserves_inline_comment_before_initializer() {
    let source = "\
function example() -> int {
    let v: int | string = 1;
    let count = 0;
    while let n: int = /* read v */ v {
        count = count + 1;
        v = \"done\";
    }
    return count;
}
";
    let out = fmt(source);
    assert_contains(&out, "/* read v */");
    assert_idempotent(source);
}

#[test]
fn while_let_preserves_comments_in_body() {
    let source = "\
function example() -> int {
    let v: int | string = 1;
    let count = 0;
    while let n: int = v {
        // increment per-iteration
        count = count + 1;
        v = \"done\";
    }
    return count;
}
";
    let out = fmt(source);
    assert_contains(&out, "// increment per-iteration");
    assert_idempotent(source);
}

// ── let else ──────────────────────────────────────────────────────────────

#[test]
fn let_else_preserves_inline_comment_in_header() {
    let source = "\
function example(raw: int | string) -> int {
    let n: int = /* the unwrap */ raw else {
        return -1;
    };
    return n;
}
";
    let out = fmt(source);
    assert_contains(&out, "/* the unwrap */");
    assert_idempotent(source);
}

#[test]
fn let_else_preserves_comment_inside_else_block() {
    let source = "\
function example(raw: int | string) -> int {
    let n: int = raw else {
        // bail when raw isn't an int
        return -1;
    };
    return n;
}
";
    let out = fmt(source);
    assert_contains(&out, "// bail when raw isn't an int");
    assert_idempotent(source);
}

#[test]
fn let_else_preserves_blank_line_between_header_and_else() {
    let source = "\
function example(raw: int | string) -> int {
    let n: int = raw else {
        return -1;
    };

    // use n
    return n;
}
";
    let out = fmt(source);
    assert_contains(&out, "// use n");
    assert_idempotent(source);
}

#[test]
fn let_else_chain_idempotent() {
    let source = "\
function example(a: int | string, b: int | string) -> int {
    let x: int = a else {
        return -1;
    };
    let y: int = b else {
        return -2;
    };
    return x + y;
}
";
    assert_idempotent(source);
}

// ── idempotency of the full grammar surface ───────────────────────────────

#[test]
fn all_three_constructs_idempotent_together() {
    let source = "\
function example(v: int | string) -> int {
    // let-else first
    let n: int = v else {
        // fallback
        return -1;
    };
    // then while-let
    let i: int | string = n;
    while let cur: int = i {
        if cur >= 5 {
            i = \"done\";
        } else {
            i = cur + 1;
        }
    }
    // then if-let as a tail value
    if let s: string = i {
        return 100;
    } else {
        return 0;
    }
}
";
    assert_idempotent(source);
}
