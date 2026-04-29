// Intent: an array of two comparisons: `[a < b, c > d]`.
// With identifier operands, the tokens `a < b , c > d` collide with the
// generic-call heuristic in parser.rs:3914 (`looks_like_generic_args`):
// seeing `<` followed by Word then `,` commits to parsing `a<b, c>` as a
// path with generic args, leaving `d` dangling inside the array.
//
// Documenting the parser behavior — the user-facing workaround is to
// parenthesize the comparisons (`[(a < b), (c > d)]`), which we also
// include here as a sanity check that the unambiguous form still parses.

function ArrayOfComparisons(a: int, b: int, c: int, d: int) -> bool[] {
    [a < b, c > d]
}

function ArrayOfComparisonsParenthesized(a: int, b: int, c: int, d: int) -> bool[] {
    [(a < b), (c > d)]
}

// Mixed: comparison followed by a real function call inside an array.
// `[f < x, y()]` should parse as two elements: `f < x` (comparison) and
// `y()` (call). The `<` looks generic-y but the `(` after `y` is inside
// the angle scan, so the heuristic falls back to comparison.
function ArrayCompareThenCall(f: int, x: int) -> int[] {
    [f < x, y()]
}

function y() -> int {
    0
}
