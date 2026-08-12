// Array rest-binding torture cases (B-531), semantic half: every function
// here PARSES CLEAN (parser-recovery cases live in
// patterns_array_rest_parser_recovery) so the formatter stage pins output
// for the valid syntax, and the TIR stage pins the rejected shapes.

// ============================================================================
// Valid rest bindings: pins the formatter output for the good syntax.
// ============================================================================

function valid_rest_binding(xs: int[]) -> int {
    match (xs) {
        [let first, ..let rest, let last] => first + rest.length() + last,
        [..let all] => all.length()
    }
}

function valid_wildcard_rest(xs: int[]) -> int {
    match (xs) {
        [let first, .._] => first,
        _ => 0
    }
}

function valid_ascribed_rest() -> int {
    let [..let rest: int[]] = [1, 2]
    return rest[0]
}

// ============================================================================
// Degenerate nesting: rejected shapes with exactly one diagnostic each and
// no unresolved-name cascades from bindings inside the rejected rest.
// ============================================================================

// Bare rest nested inside a rest.
function nested_bare_rest(xs: int[]) -> int {
    match (xs) {
        [..[..]] => 1,
        _ => 0
    }
}

// Rest binding nested inside a rest; `r` must still resolve in the arm body.
function nested_rest_binding(xs: int[]) -> int {
    match (xs) {
        [..[..let r]] => r.length(),
        _ => 0
    }
}

// Rest destructured as the empty array (rest-must-be-empty).
function rest_empty_array(xs: int[]) -> int {
    match (xs) {
        [let a, ..[]] => a,
        _ => 0
    }
}

// The would-be flattening showcase.
function rest_flattening_showcase(xs: int[]) -> int {
    match (xs) {
        [let a, ..[let b, ..let r, let c], let d] => a + b + c + d,
        _ => 0
    }
}

// ============================================================================
// Rejected sub-pattern shapes (see compiler2_tir/array_rest.rs for the
// per-shape unit tests; this pins the rendered diagnostics).
// ============================================================================

class Wrapper {
    value int
}

function rest_type_pattern(xs: int[]) -> int {
    match (xs) {
        [let a, ..int] => a,
        _ => 0
    }
}

function rest_class_destructure(ws: Wrapper[]) -> int {
    match (ws) {
        [..Wrapper { value }] => value,
        _ => 0
    }
}
