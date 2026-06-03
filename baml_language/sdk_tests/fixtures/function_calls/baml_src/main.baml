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
