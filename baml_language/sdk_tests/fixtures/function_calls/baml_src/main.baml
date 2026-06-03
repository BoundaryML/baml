// Minimal expression-function fixture: a nullary function whose return
// type is the string literal "hello world" and whose body returns that
// same literal. Exercises the host→engine call path for a plain
// (non-LLM) BAML function with no arguments.
function hello_world() -> "hello world" {
  "hello world"
}

// Optional (default-valued) parameter with a non-null constant default.
// A host caller may omit `factor` — the engine substitutes the BAML-side
// default `2` — or pass it to override. Exercises the host→engine call
// path when a defaulted argument is dropped from the encoded call args.
function scale(base: int, factor: int = 2) -> int {
  base * factor
}

// Optional parameter whose default (`7`) is distinct from `null`, so three
// host calls are observably different: omitted → engine default `7`;
// explicit `null` → `-1`; a supplied value → that value. Pins the contract
// that an omitted default and an explicit null are NOT the same call.
function classify(value: int? = 7) -> int {
  if (value == null) { -1 } else { value }
}

// Single required argument: the host passes one positional value and it
// round-trips through the engine. Baseline for the one-arg call path
// (hello_world is nullary; this is the next step up).
function shout(text: string) -> string {
  text + "!"
}

// One required positional arg plus one optional arg whose default is
// `null`. Omitting `prefix` (engine fills `null`) and passing an explicit
// `null` both take the null branch; a supplied string is prepended.
function tag(name: string, prefix: string? = null) -> string {
  if (prefix == null) { name } else { prefix + ":" + name }
}

// One required positional arg plus one optional arg defaulted to `5`.
// Omitting `addend` lets the engine fill `5`; passing it overrides.
function add_five(base: int, addend: int = 5) -> int {
  base + addend
}
