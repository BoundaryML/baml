// Missing closing brace
class Broken {
    field int

// Invalid syntax
function 123Invalid() {}

// ============================================================================
// MATCH PATTERN SYNTAX ERRORS
// Per BEP-002: "Multiple Patterns Per Arm" - `_: A | _: B` is NOT supported
// The grammar parses `:` tighter than `|`, so `a: int | b: bool` becomes
// `a: (int | b)` with `: bool` as garbage, producing a confusing error.
// ============================================================================