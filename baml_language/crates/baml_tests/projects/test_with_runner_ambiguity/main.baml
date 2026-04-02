// Parser ambiguity: `with <expr> { ... }` — does `{` start an object
// literal (part of the runner expr) or the test body?
//
// Resolution: object literals are suppressed inside `with` runner expressions.
// `{` after the runner always starts the test body. Constructors must be
// parenthesized: `with (MyRunner { threshold: 0.5 }) { body }`.

// Case 1: Simple identifier runner — `{ body }` is the test body.
test "simple runner" with my_runner {
  assert.is_true(true)
}

// Case 2: Call expression runner — `{ body }` is the test body.
test "call runner" with testing.Quorum(5, 3) {
  assert.is_true(true)
}

// Case 3: Constructor runner — must be parenthesized so `{` isn't ambiguous.
// `with (MyRunner { threshold: 0.5 }) { body }` works because the `(` scopes
// the object literal, and the outer `{` starts the test body.
class MyRunner {
  threshold float
}

test "constructor runner" with MyRunner { threshold: 0.5 } {
  assert.is_true(true)
}
