// `<expr> is <pattern>` — Rust `matches!`-style pattern test.
// Returns true if the scrutinee matches the pattern, false otherwise.

// Simple type check on a union.
function is_int(v: int | string) -> bool {
    v is int
}

// Negation by composing with `!`.
function is_not_string(v: int | string) -> bool {
    !(v is string)
}

// Or-pattern on the RHS.
function is_int_or_bool(v: int | string | bool) -> bool {
    v is int | bool
}

// Use inside an `if` condition.
function classify(v: int | string) -> string {
    if (v is int) { "number" }
    else { "text" }
}

// Chain into a boolean expression with `&&`.
function both_ints(a: int | string, b: int | string) -> bool {
    a is int && b is int
}

// Literal pattern.
function is_zero(n: int) -> bool {
    n is 0
}
