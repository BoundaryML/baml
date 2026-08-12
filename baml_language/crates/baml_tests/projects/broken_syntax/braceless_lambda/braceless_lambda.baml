// Regression test for B-235: a braceless lambda body must report only the
// actionable `Expected lambda body '{'` syntax error, not the cascade of
// misleading type errors that parser error recovery would otherwise produce
// (the parser consumes `x` as the lambda's return type and leaves `+ 1`
// dangling, which used to emit `unresolved type: x` and `operator + cannot be
// applied to a function` *before* the real syntax error).
function Braceless() -> int {
  let add1 = (x: int) => x + 1;
  add1(2)
}

// An incomplete binary expression after the arrow must still produce a focused
// required-block diagnostic without cascading type errors.
function Braceless2() -> int {
  let add1 = (x: int) => x + ;
  add1(2)
}

// The suppression is scoped to the broken body: a genuine type error in a
// separate, well-formed function must still be reported.
function GenuineTypeError() -> int {
  "hello" + 5
}
