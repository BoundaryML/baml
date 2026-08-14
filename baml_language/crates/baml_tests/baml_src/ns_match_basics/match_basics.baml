// Unified tests for match basics: catch-all, literals (int/string/bool/null/negative),
// unions, expressions, nested, edge cases, optional with null, complex scrutinee expressions,
// and exhaustive bool.
//
// Converted from crates/baml_tests/tests/match_basics.rs.
//
// SKIPPED:
//   - match_float_literal: #[ignore = "patterns phase2: float switch lowering"]
//   - match_negative_int_first_arm: #[ignore = "compiler2 todo"]
//   - match_negative_int_fallback: #[ignore = "compiler2 todo"]
//   - match_negative_float_pattern: #[ignore = "compiler2 todo"]
//   - match_multiple_negative_patterns: #[ignore = "compiler2 todo"]
//   - match_negative_in_union_pattern: #[ignore = "compiler2 todo"]

// ============================================================================
// Basic Catch-All Tests
// ============================================================================

test "match_catch_all_underscore" {
    assert.equal(match (42) { _ => 100 }, 100)
}

test "match_catch_all_named_binding" {
    assert.equal(match (42) { let x => x + 1 }, 43)
}

// match_catch_all_with_variable: scrutinee is a local, match arms bind it —
// move into a function to avoid the VM local-boxing bug.
function catch_all_with_variable() -> int {
    let x = 42;
    match (x) {
        let y => y + 1
    }
}

test "match_catch_all_with_variable" {
    assert.equal(catch_all_with_variable(), 43)
}

// ============================================================================
// Literal Pattern Tests
// ============================================================================

// Scrutinee is a local, match dispatches on it — use a helper.
function literal_int_first_arm() -> int {
    let x = 1;
    match (x) {
        1 => 100,
        2 => 200,
        _ => 0
    }
}

test "match_literal_int_first_arm" {
    assert.equal(literal_int_first_arm(), 100)
}

function literal_int_second_arm() -> int {
    let x = 2;
    match (x) {
        1 => 100,
        2 => 200,
        _ => 0
    }
}

test "match_literal_int_second_arm" {
    assert.equal(literal_int_second_arm(), 200)
}

function literal_int_fallback() -> int {
    let x = 999;
    match (x) {
        1 => 100,
        2 => 200,
        _ => 0
    }
}

test "match_literal_int_fallback" {
    assert.equal(literal_int_fallback(), 0)
}

test "match_literal_int_single_arm" {
    assert.equal(match (1) { 1 => 100}, 100)
}

// Scrutinee `null` stored in local — use a helper.
function literal_null() -> string {
    let x = null;
    match (x) {
        null => "was null",
    }
}

test "match_literal_null" {
    assert.equal(literal_null(), "was null")
}

// bool scrutinee stored in local — use a helper.
function literal_bool_true() -> string {
    let b = true;
    match (b) {
        true => "yes",
        false => "no"
    }
}

test "match_literal_bool_true" {
    assert.equal(literal_bool_true(), "yes")
}

function literal_bool_false() -> string {
    let b = false;
    match (b) {
        true => "yes",
        false => "no"
    }
}

test "match_literal_bool_false" {
    assert.equal(literal_bool_false(), "no")
}

test "match_literal_bool_exhaustive_constant" {
    assert.equal(match (true) { true => "yes"}, "yes")
}

// ============================================================================
// Union Literal Pattern Tests
// ============================================================================

// Scrutinee local `code` → helper.
function union_literal_first() -> string {
    let code = 200;
    match (code) {
        200 | 201 => "success",
        400 | 404 => "client error",
        _ => "other"
    }
}

test "match_union_literal_first" {
    assert.equal(union_literal_first(), "success")
}

function union_literal_second() -> string {
    let code = 201;
    match (code) {
        200 | 201 => "success",
        400 | 404 => "client error",
        _ => "other"
    }
}

test "match_union_literal_second" {
    assert.equal(union_literal_second(), "success")
}

function union_large() -> string {
    let code = 204;
    match (code) {
        200 | 201 | 202 | 204 => "success",
        400 | 401 | 403 | 404 => "client error",
        500 | 501 | 502 | 503 => "server error",
        _ => "other"
    }
}

test "match_union_large" {
    assert.equal(union_large(), "success")
}

function union_client_error() -> string {
    let code = 404;
    match (code) {
        200 | 201 | 202 | 204 => "success",
        400 | 401 | 403 | 404 => "client error",
        500 | 501 | 502 | 503 => "server error",
        _ => "other"
    }
}

test "match_union_client_error" {
    assert.equal(union_client_error(), "client error")
}

function union_with_duplicates() -> string {
    let x = 1;
    match (x) {
        1 | 1 | 2 => "one or two",
        3 | 3 => "three",
        _ => "other"
    }
}

test "match_union_with_duplicates" {
    assert.equal(union_with_duplicates(), "one or two")
}

// ============================================================================
// Expression Context Tests
// ============================================================================

// Scrutinee local `x` → helper.
function as_expression_in_arithmetic() -> int {
    let x = 2;
    match (x) {
        1 => 10,
        2 => 20,
        _ => 0
    } + 1
}

test "match_as_expression_in_arithmetic" {
    assert.equal(as_expression_in_arithmetic(), 21)
}

function nested_match() -> int {
    let outer = 1;
    let inner = 2;
    match (outer) {
        1 => match (inner) {
            1 => 11,
            2 => 12,
            _ => 10
        },
        2 => 20,
        _ => 0
    }
}

test "match_nested" {
    assert.equal(nested_match(), 12)
}

// ============================================================================
// String Literal Tests
// ============================================================================

function classify_string(s: string) -> int {
    match (s) {
        "hello" => 100,
        "world" => 200,
        _ => 0
    }
}

test "match_string_literal_first_arm" {
    assert.equal(classify_string("hello"), 100)
}

test "match_string_literal_second_arm" {
    assert.equal(classify_string("world"), 200)
}

test "match_string_literal_fallback" {
    assert.equal(classify_string("other"), 0)
}

function classify_string_four_arms(s: string) -> int {
    match (s) {
        "a" => 1,
        "b" => 2,
        "c" => 3,
        "d" => 4,
        _ => 0
    }
}

test "match_string_four_arms" {
    assert.equal(classify_string_four_arms("c"), 3)
}

function classify_string_typed_fallback(s: string) -> int {
    match (s) {
        "ok" => 200,
        "error" => 500,
        string => 0
    }
}

test "match_string_literal_with_typed_fallback" {
    assert.equal(classify_string_typed_fallback("unknown"), 0)
}

// ============================================================================
// Catch-All Binding with Integer Patterns
// ============================================================================

function catch_all_binding_with_int_patterns() -> int {
    let x = 99;
    match (x) {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        let other => other * 10
    }
}

test "match_catch_all_binding_with_int_patterns" {
    assert.equal(catch_all_binding_with_int_patterns(), 990)
}

// ============================================================================
// Negative Literal Pattern Tests
// ============================================================================

// match_negative_int_with_variable: scrutinee stored in local → helper.
function negative_int_with_variable() -> string {
    let x = -1;
    match (x) {
        -1 => "negative one",
        0 => "zero",
        _ => "other"
    }
}

test "match_negative_int_with_variable" {
    assert.equal(negative_int_with_variable(), "negative one")
}

// match_negative_int_first_arm: scrutinee is `let x: int = -1` (variable) → helper.
// (Previously ignored; the variable-scrutinee form passes now.)
function negative_int_first_arm() -> string {
    let x: int = -1;
    match (x) {
        -1 => "negative one",
        0 => "zero",
        1 => "one",
        _ => "other"
    }
}

test "match_negative_int_first_arm" {
    assert.equal(negative_int_first_arm(), "negative one")
}

// BUG: compile error — "type mismatch: expected 5, got -1" and "unreachable arm"
// When the scrutinee is an integer literal the typechecker constant-folds it to
// that exact value and rejects other literal arms as type mismatches.  The
// original RS test used `match (5) {-1 => ..., 0 => ..., 1 => ..., _ => ...}`.
//
// function match_negative_int_fallback_literal() -> string {
//     match (5) {
//         -1 => "negative one",
//         0 => "zero",
//         1 => "one",
//         _ => "other"
//     }
// }
//
// test "match_negative_int_fallback" {
//     assert.equal(match_negative_int_fallback_literal(), "other")
// }

// BUG: compile error — "Float literal values are not supported"
// Float literals are not yet allowed as match-arm patterns; the checker
// reports E0010 for each float pattern.
// The original RS test was `#[ignore = "patterns phase2: float switch lowering"]`.
//
// function match_float_literal_fn() -> string {
//     let x = 1.5;
//     match (x) {
//         1.0 => "one",
//         1.5 => "one point five",
//         2.0 => "two",
//         _ => "other"
//     }
// }
//
// test "match_float_literal" {
//     assert.equal(match_float_literal_fn(), "one point five")
// }

// BUG: compile error — "Float literal values are not supported" (E0010)
// Negative float patterns are not supported in match arms.
// Original RS test: `match (-1.5) { -1.5 => ..., 0.0 => ..., 1.5 => ..., _ => ... }`.
//
// function match_negative_float_pattern_fn() -> string {
//     match (-1.5) {
//         -1.5 => "negative one point five",
//         0.0 => "zero",
//         1.5 => "one point five",
//         _ => "other"
//     }
// }
//
// test "match_negative_float_pattern" {
//     assert.equal(match_negative_float_pattern_fn(), "negative one point five")
// }

// BUG: compile error — "type mismatch: expected -2, got -3" and "unreachable arm"
// Literal-scrutinee constant-folding makes all other arms type errors.
// Original RS test: `match (-2) { -3 => ..., -2 => ..., -1 => ..., _ => ... }`.
//
// function match_multiple_negative_patterns_fn() -> string {
//     match (-2) {
//         -3 => "negative three",
//         -2 => "negative two",
//         -1 => "negative one",
//         _ => "other"
//     }
// }
//
// test "match_multiple_negative_patterns" {
//     assert.equal(match_multiple_negative_patterns_fn(), "negative two")
// }

// BUG: compile error — "unreachable arm" on the wildcard
// When the scrutinee is a literal (`-1`) the typechecker treats the union arm
// `-1 | 0 | 1` as exhaustive and rejects `_ => "large"` as unreachable.
// Original RS test: `match (-1) { -1 | 0 | 1 => "small", _ => "large" }`.
//
// function match_negative_in_union_pattern_fn() -> string {
//     match (-1) {
//         -1 | 0 | 1 => "small",
//         _ => "large"
//     }
// }
//
// test "match_negative_in_union_pattern" {
//     assert.equal(match_negative_in_union_pattern_fn(), "small")
// }

// ============================================================================
// Three-Level Nested Match
// ============================================================================

function classify_three(x: int, y: int, z: int) -> string {
    match (x) {
        0 => match (y) {
            0 => match (z) {
                0 => "all zero",
                _ => "z nonzero"
            },
            _ => "y nonzero"
        },
        _ => "x nonzero"
    }
}

test "match_three_levels_nested" {
    assert.equal(classify_three(0, 0, 0), "all zero")
}

test "match_three_levels_nested_middle" {
    assert.equal(classify_three(0, 1, 0), "y nonzero")
}

// ============================================================================
// Optional with Null Pattern
// ============================================================================

function process_optional(x: int?) -> string {
    match (x) {
        null => "none",
        let n: int => "some"
    }
}

test "match_optional_null_pattern" {
    assert.equal(process_optional(null), "none")
}

test "match_optional_value_pattern" {
    assert.equal(process_optional(42), "some")
}

function process_optional_with_zero(x: int?) -> string {
    match (x) {
        null => "none",
        0 => "zero",
        let n: int => "other"
    }
}

test "match_optional_with_literal_and_typed" {
    assert.equal(process_optional_with_zero(0), "zero")
}

// ============================================================================
// Complex Scrutinee Expressions
// ============================================================================

function classify_sum(a: int, b: int) -> string {
    match (a + b) {
        0 => "zero",
        1 => "one",
        _ => "other"
    }
}

test "match_arithmetic_scrutinee" {
    assert.equal(classify_sum(2, -1), "one")
}

function helper_42() -> int {
    42
}

function classify_by_call() -> string {
    match (helper_42()) {
        42 => "answer",
        _ => "other"
    }
}

test "match_function_call_scrutinee" {
    assert.equal(classify_by_call(), "answer")
}

function get_value() -> int {
    2
}

test "match_computed_discriminant" {
    assert.equal(match (get_value()) { 0 => 100, 1 => 101, 2 => 102, 3 => 103, _ => 999 }, 102)
}

// ============================================================================
// Edge Cases
// ============================================================================

// match_in_loop: uses locals + mutation + match, all in test block → helper.
function match_in_loop_helper() -> int {
    let sum = 0;
    let i = 0;
    while (i < 5) {
        sum = sum + match (i) {
            0 => 10,
            1 => 20,
            2 => 30,
            3 => 40,
            _ => 50
        };
        i = i + 1;
    }
    sum
}

test "match_in_loop" {
    // 10 + 20 + 30 + 40 + 50 = 150
    assert.equal(match_in_loop_helper(), 150)
}

function classify_dense(n: int) -> int {
    match (n) {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        _ => -1
    }
}

test "match_dense_with_catch_all_via_call" {
    assert.equal(classify_dense(2), 2)
}

class MatchResult {
    code int
}

// match_mixed_instanceof_and_literal: match on a local instance → helper.
function mixed_instanceof_and_literal() -> int {
    let x = MatchResult { code: 200 };
    match (x) {
        let r: MatchResult => r.code,
    }
}

test "match_mixed_instanceof_and_literal" {
    assert.equal(mixed_instanceof_and_literal(), 200)
}

// ============================================================================
// Exhaustive Bool with Variable Scrutinee Tests
// ============================================================================

function check_bool(flag: bool) -> string {
    match (flag) {
        true => "yes",
        false => "no"
    }
}

test "match_bool_variable_exhaustive" {
    assert.equal(check_bool(true), "yes")
}

function check_or_wildcard(flag: bool) -> int {
    match (flag) {
        _ | true => 1
    }
}

test "match_or_wildcard_bool_is_exhaustive" {
    assert.equal(check_or_wildcard(false), 1)
}

// ============================================================================
// String Patterns with 4+ Arms Tests
// ============================================================================

function classify_string_many(s: string) -> int {
    match (s) {
        "alpha" => 1,
        "beta" => 2,
        "gamma" => 3,
        "delta" => 4,
        _ => 0
    }
}

test "match_string_many_arms" {
    assert.equal(classify_string_many("gamma"), 3)
}
