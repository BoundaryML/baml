// A postfix `!` (TypeScript/Swift/Kotlin non-null assertion) is not a BAML
// operator. It must produce a single targeted diagnostic pointing at the `!`,
// NOT a misleading "if without else" / "expected expression" cascade blaming
// the `else` keyword or the closing braces.
function f(xs: int[]) -> int {
    if (xs.length() > 0) {
        xs.at(0)!
    } else {
        0
    }
}
