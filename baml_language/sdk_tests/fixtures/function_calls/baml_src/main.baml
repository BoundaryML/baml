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

// ── required + optional, one per default flavor ──────────────────────────────
// Three functions with the same `(int base, <optional> delta)` shape that vary
// ONLY in the optional parameter's type + default, so the host-call contracts
// sit side by side:
//
//   delta type/default │ omitted        │ explicit null │ value 3
//   ───────────────────┼────────────────┼───────────────┼─────────
//   int  = 5           │ default 5       │ (not nullable)│ 3
//   int? = 5           │ default 5       │ null          │ 3
//   int? = null        │ default null    │ null          │ 3
//
// The `int? = 5` row is the only one where omitted (5) and explicit null
// differ; the `int? = null` row collapses them.

// `T = value`: non-nullable optional. `delta` is always an int, so the
// engine fills `5` on omission and the host can never pass null.
function with_opt_value(base: int, delta: int = 5) -> int {
  base + delta
}

// `T? = value`: nullable optional with a non-null default. Omitting `delta`
// yields the default `5`; an explicit `null` takes the null branch — the two
// are observably distinct.
function with_opt_nullable_value(base: int, delta: int? = 5) -> int {
  if (delta == null) { base } else { base + delta }
}

// `T? = null`: nullable optional defaulted to null. Omitting `delta` and
// passing an explicit `null` both take the null branch.
function with_opt_nullable_null(base: int, delta: int? = null) -> int {
  if (delta == null) { base } else { base + delta }
}
