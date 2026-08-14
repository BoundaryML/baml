// BEP-049 §10 tagged templates — runtime execution tests (M4e.1).
//
// M4e.1a covers text + interpolation + arity-N body params + captures (static
// parts/values arrays). M4e.1b covers `${for}`/`${if}` flattening — the body
// closure builds `parts`/`values` at runtime via empty lists + `push` in real
// loops/branches, so their lengths are data-dependent (the
// `parts.len() == values.len() + 1` invariant is preserved).
//
// Transcription deviation: in the two nested-lambda `.map(...)` tests the
// mapper is written `.map<string, never>((y: string) -> …)` — inside a
// `${for}` flatten block the compiler cannot infer `map`'s error type
// parameter `E` from the lambda (E0002), so both type arguments are explicit.

// ─── M4e.1a: text + interpolation + params + captures ─────────────────────────

// `tt_probe` reads back the literal `parts` of the TaggedString the body
// closure builds. `A${1}B${2}C` → parts == ["A","B","C"], values == [1,2].
//baml:tagged_string
function tt_probe(body: () -> baml.TaggedString) -> string {
  let t = body()
  t.parts[0] + "|" + t.parts[1] + "|" + t.parts[2]
}

function tt_parts_built_in_order_main() -> string {
  tt_probe`A${1}B${2}C`
}

test "tagged_template_parts_built_in_order" {
    assert.equal(tt_parts_built_in_order_main(), "A|B|C")
}

// `${p}` references an enclosing local — the body closure must capture it.
// values[0] is that captured value; parts == ["a","b"].
//baml:tagged_string
function tt_cap(body: () -> baml.TaggedString) -> string {
  let t = body()
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}

function tt_captures_enclosing_local_main() -> string {
  let p = "X-"
  tt_cap`a${p}b`
}

test "tagged_template_captures_enclosing_local" {
    assert.equal(tt_captures_enclosing_local_main(), "aX-b")
}

// `outer` is defined in the helper, referenced ONLY inside a tagged template
// that is itself inside a user lambda `f`. The template's interp must capture
// `outer` *through* `f` — i.e. `f` must learn it captures `outer`. Before
// late-lowering, the synthesized template body lambda was a separate HIR
// capture boundary that stole the reference, so `f` never recorded the capture
// and this failed to compile with `[E0003] unresolved name: outer`.
function tt_outer_local_through_user_lambda_main() -> string {
  let outer = "OUT"
  let f = () -> { tt_cap`a${outer}b` }
  f()
}

test "tagged_template_outer_local_captured_through_user_lambda" {
    assert.equal(tt_outer_local_through_user_lambda_main(), "aOUTb")
}

// The tag supplies the body-lambda param `name`; `${name}` resolves to it
// (arity-N body). values[0] is the injected value.
//baml:tagged_string
function tt_fmt(body: (name: string) -> baml.TaggedString) -> string {
  let t = body("World")
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}

function tt_body_param_injection_main() -> string {
  tt_fmt`Hi ${name}!`
}

test "tagged_template_body_param_injection" {
    assert.equal(tt_body_param_injection_main(), "Hi World!")
}

// The body param `name` is referenced from a *nested* lambda inside the
// interpolation (`(unused) -> { name }`, immediately invoked). The param is
// a MIR-only local with no HIR binding, so neither the nested lambda's
// standalone TIR scope inference nor its MIR capture analysis can see it
// through the normal paths.
function tt_body_param_captured_by_nested_lambda_main() -> string {
  tt_fmt`Hi ${((unused: string) -> { name })("z")}!`
}

test "tagged_template_body_param_captured_by_nested_lambda" {
    assert.equal(tt_body_param_captured_by_nested_lambda_main(), "Hi World!")
}

// ─── M4e.1b: ${for}/${if} runtime flattening ──────────────────────────────

// A generic `//baml:tagged_string` renderer used by the M4e.1b tests below:
// it reconstructs the rendered string from the flattened `(parts, values)`
// by interleaving them, downcasting each (string-typed) value. This exercises
// the data-dependent array lengths the dynamic flattener produces.
//baml:tagged_string
function tt_render(body: () -> baml.TaggedString) -> string {
  let t = body()
  let out = ""
  let i = 0
  while (i < t.values.length()) {
    let v = match (t.values[i]) {
      let s: string => s,
      _ => "?"
    }
    out = out + t.parts[i] + v
    i = i + 1
  }
  out + t.parts[t.values.length()]
}

// `${for}` grows parts/values per iteration: 2 items → 3 parts, 2 values.
function tt_for_loop_flattens_main() -> string {
  let items = ["a", "b"]
  tt_render`${for (let x in items)}[${x}]${endfor}`
}

test "tagged_template_for_loop_flattens" {
    assert.equal(tt_for_loop_flattens_main(), "[a][b]")
}

// Empty collection → loop body never runs → parts == [""], values == [].
function tt_for_loop_empty_collection_main() -> string {
  let items: string[] = []
  tt_render`${for (let x in items)}[${x}]${endfor}`
}

test "tagged_template_for_loop_empty_collection" {
    assert.equal(tt_for_loop_empty_collection_main(), "")
}

// `${if}` includes its body only when the condition holds.
function tt_if_block_taken_main() -> string {
  let flag = true
  let x = "Y"
  tt_render`IN${if (flag)}${x} mid${endif}OUT`
}

function tt_if_block_skipped_main() -> string {
  let flag = false
  let x = "Y"
  tt_render`IN${if (flag)}${x} mid${endif}OUT`
}

test "tagged_template_if_block_taken_and_skipped" {
    assert.equal(tt_if_block_taken_main(), "INY midOUT")
    assert.equal(tt_if_block_skipped_main(), "INOUT")
}

// Static interpolation + a `${for}` in one template → the static fast-path
// is NOT used (a block is present); everything flows through the dynamic
// flattener. 1 leading value + 2 loop values → 4 parts, 3 values.
function tt_mixed_static_and_for_main() -> string {
  let a = "A"
  let xs = ["1", "2"]
  tt_render`pre ${a} ${for (let x in xs)}${x},${endfor}end`
}

test "tagged_template_mixed_static_and_for" {
    assert.equal(tt_mixed_static_and_for_main(), "pre A 1,2,end")
}

// BEP §4: C-style `${for (init; cond; step)}` also drives the tagged
// flatten path (push into parts/values per iteration), like the iterator
// form. `tt_render` only renders string values (§11 preserves raw types),
// so stringify the counter explicitly via `string.from`.
function tt_cstyle_for_flattens_main() -> string {
  tt_render`${for (let i = 0; i < 3; i += 1)}[${string.from(i)}]${endfor}`
}

test "tagged_template_cstyle_for_flattens" {
    assert.equal(tt_cstyle_for_flattens_main(), "[0][1][2]")
}

// Adversarial: a `${for}` body whose interpolation contains a NESTED lambda
// that captures BOTH the loop-local (`x`) and an enclosing local (`outer`).
// Exercises capture handling inside the flatten block's loop.
function tt_for_body_nested_lambda_capturing_loop_local_main() -> string {
  let outer = "O"
  let xs = ["a", "b"]
  tt_render`${for (let x in xs)}${["1"].map<string, never>((y: string) -> { x + outer + y }).join("")}${endfor}`
}

test "tagged_template_for_body_interp_with_nested_lambda_capturing_loop_local" {
    assert.equal(tt_for_body_nested_lambda_capturing_loop_local_main(), "aO1bO1")
}

// Companion to the capturing_loop_local case: the nested lambda captures the
// `${for}` loop-local `x` and binds it through a local `let z` before use.
// Confirms the captured loop-local resolves to a concrete `string` (so the
// `let`/concat type-check and lower) rather than degrading to `Ty::Unknown`.
function tt_for_body_nested_lambda_loop_local_in_let_main() -> string {
  let xs = ["a", "b"]
  tt_render`${for (let x in xs)}${["1"].map<string, never>((y: string) -> { let z = x + "!"; z + y }).join("")}${endfor}`
}

test "tagged_template_for_body_interp_nested_lambda_uses_loop_local_in_let" {
    assert.equal(tt_for_body_nested_lambda_loop_local_in_let_main(), "a!1b!1")
}

// Nested control flow: a `${for}` inside a taken `${if}` branch.
function tt_nested_for_in_if_main() -> string {
  let show = true
  let xs = ["p", "q"]
  tt_render`${if (show)}${for (let x in xs)}<${x}>${endfor}${endif}`
}

test "tagged_template_nested_for_in_if" {
    assert.equal(tt_nested_for_in_if_main(), "<p><q>")
}
