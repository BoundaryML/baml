// Quick survey of what the parser does on unbalanced brackets/parens/angles.
// Each function isolates one bad shape so the diagnostic snapshot stays
// readable. None of these are valid Baml — the test is purely about how the
// parser reports the failure.

function f<K, V>(k: K, v: V) -> V {
    v
}

// Unclosed paren in a call: `f(1, "two"` — missing `)`
function UnclosedCall() -> string {
    f(1, "two"
}

// Extra closing paren after a complete call: `f(1, "two"))`
function ExtraCloseParen() -> string {
    f(1, "two"))
}

// Unclosed angle bracket on a generic call: `f<int, string(1, "two")` — `>` missing
function UnclosedGeneric() -> string {
    f<int, string(1, "two")
}

// Extra `>` after a generic call.
function ExtraCloseAngle() -> string {
    f<int, string>>(1, "two")
}

// Mismatched: opens `(`, closes `]`.
function MismatchedParenBracket() -> int {
    (1 + 2]
}

// Unclosed bracket in array literal.
function UnclosedArray() -> int[] {
    [1, 2, 3
}

// Mixed: unclosed angle inside an unclosed call.
function NestedUnclosed() -> string {
    f<int, string(1, "two"
}
