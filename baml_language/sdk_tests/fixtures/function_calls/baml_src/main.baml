// Minimal expression-function fixture: a nullary function whose return
// type is the string literal "hello world" and whose body returns that
// same literal. Exercises the host→engine call path for a plain
// (non-LLM) BAML function with no arguments.
function hello_world() -> "hello world" {
  "hello world"
}

// One required positional argument: the host passes a single value and it
// round-trips through the engine unchanged. The one-arg step up from the
// nullary `hello_world`.
function single_required_arg(value: string) -> string {
  value
}

// Helper used as an *expression* default below — `opt2`'s default is a call to
// this function, so the engine evaluates it whenever `opt2` is omitted.
function make_opt2() -> int {
  99
}

// One required arg followed by two optionals that exercise the two default
// kinds the host call path must handle:
//   - `opt1: int? = 5`            — a literal default
//   - `opt2: int? = make_opt2()`  — an expression (function-call) default
// Return is `void` (→ None/null on the host): this fixture is about modelling
// the *call forms*, not an observable result. Every call form the language
// allows is enumerated in the host tests (test_main.py / main.test.ts), with
// TODOs where the generated SDK can't yet express a given form.
function required_with_optional_args(arg0: int, opt1: int? = 5, opt2: int? = make_opt2()) -> void {
}
