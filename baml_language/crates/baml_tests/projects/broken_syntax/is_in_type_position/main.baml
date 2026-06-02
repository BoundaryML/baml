// `is` is an expression operator (moved from tests/is_operator.rs Group I).
// Using it where a type is expected must be rejected by the parser — these are
// syntax errors, so they live in broken_syntax/.

// `is` in a parameter type position.
function bad_param(v: v is int) -> bool { true }

// `is` in a return type position.
function bad_return() -> v is int { true }
