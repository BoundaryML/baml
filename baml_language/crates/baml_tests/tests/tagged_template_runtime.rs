//! BEP-049 §10 tagged templates — runtime execution tests (M4e.1).
//!
//! These execute the program (parse → … → MIR → bytecode → VM) via `baml_test!`,
//! which does NOT run the source formatter (so the missing formatter support,
//! task #42, is irrelevant here). They exercise the MIR lowering of a tagged
//! template to a hand-rolled body closure + `tag(body = closure)` call.
//!
//! M4e.1a covers text + interpolation + arity-N body params + captures (static
//! parts/values arrays). M4e.1b covers `${for}`/`${if}` flattening — the body
//! closure builds `parts`/`values` at runtime via empty lists + `push` in real
//! loops/branches, so their lengths are data-dependent (the
//! `parts.len() == values.len() + 1` invariant is preserved).

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn ok_string(s: &str) -> Result<BexExternalValue, bex_engine::EngineError> {
    Ok(BexExternalValue::String(s.into()))
}

#[tokio::test]
async fn tagged_template_parts_built_in_order() {
    // `probe` reads back the literal `parts` of the TaggedString the body
    // closure builds. `A${1}B${2}C` → parts == ["A","B","C"], values == [1,2].
    let output = baml_test!(
        r#"
//baml:tagged_string
function probe(body: () -> baml.TaggedString) -> string {
  let t = body()
  t.parts[0] + "|" + t.parts[1] + "|" + t.parts[2]
}
function main() -> string {
  probe`A${1}B${2}C`
}
"#
    );
    assert_eq!(output.result, ok_string("A|B|C"));
}

#[tokio::test]
async fn tagged_template_captures_enclosing_local() {
    // `${p}` references an enclosing local — the body closure must capture it.
    // values[0] is that captured value; parts == ["a","b"].
    let output = baml_test!(
        r#"
//baml:tagged_string
function cap(body: () -> baml.TaggedString) -> string {
  let t = body()
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}
function main() -> string {
  let p = "X-"
  cap`a${p}b`
}
"#
    );
    assert_eq!(output.result, ok_string("aX-b"));
}

#[tokio::test]
async fn tagged_template_outer_local_captured_through_user_lambda() {
    // `outer` is defined in `main`, referenced ONLY inside a tagged template
    // that is itself inside a user lambda `f`. The template's interp must
    // capture `outer` *through* `f` — i.e. `f` must learn it captures `outer`
    // for the compilation to succeed.
    let output = baml_test!(
        r#"
//baml:tagged_string
function cap(body: () -> baml.TaggedString) -> string {
  let t = body()
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}
function main() -> string {
  let outer = "OUT"
  let f = () -> { cap`a${outer}b` }
  f()
}
"#
    );
    assert_eq!(output.result, ok_string("aOUTb"));
}

#[tokio::test]
async fn tagged_template_body_param_injection() {
    // The tag supplies the body-lambda param `name`; `${name}` resolves to it
    // (arity-N body). values[0] is the injected value.
    let output = baml_test!(
        r#"
//baml:tagged_string
function fmt(body: (name: string) -> baml.TaggedString) -> string {
  let t = body("World")
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}
function main() -> string {
  fmt`Hi ${name}!`
}
"#
    );
    assert_eq!(output.result, ok_string("Hi World!"));
}

#[tokio::test]
async fn tagged_template_body_param_captured_by_nested_lambda() {
    // The body param `name` is referenced from a *nested* lambda inside the
    // interpolation (`(unused) -> { name }`, immediately invoked). The param is
    // a MIR-only local with no HIR binding, testing that capture analysis
    // correctly resolves it through nested lambda scopes.
    let output = baml_test!(
        r#"
//baml:tagged_string
function fmt(body: (name: string) -> baml.TaggedString) -> string {
  let t = body("World")
  match (t.values[0]) {
    let s: string => t.parts[0] + s + t.parts[1],
    _ => "?"
  }
}
function main() -> string {
  fmt`Hi ${((unused: string) -> { name })("z")}!`
}
"#
    );
    assert_eq!(output.result, ok_string("Hi World!"));
}

// ─── M4e.1b: ${for}/${if} runtime flattening ──────────────────────────────

/// A generic `//baml:tagged_string` renderer used by the M4e.1b tests below:
/// it reconstructs the rendered string from the flattened `(parts, values)`
/// by interleaving them, downcasting each (string-typed) value. This exercises
/// the data-dependent array lengths the dynamic flattener produces.
const RENDER_TAG: &str = r#"
//baml:tagged_string
function render(body: () -> baml.TaggedString) -> string {
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
"#;

#[tokio::test]
async fn tagged_template_for_loop_flattens() {
    // `${for}` grows parts/values per iteration: 2 items → 3 parts, 2 values.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let items = ["a", "b"]
  render`${{for (let x in items)}}[${{x}}]${{endfor}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string("[a][b]"));
}

#[tokio::test]
async fn tagged_template_for_loop_empty_collection() {
    // Empty collection → loop body never runs → parts == [""], values == [].
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let items: string[] = []
  render`${{for (let x in items)}}[${{x}}]${{endfor}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string(""));
}

#[tokio::test]
async fn tagged_template_if_block_taken_and_skipped() {
    // `${if}` includes its body only when the condition holds.
    let taken = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let flag = true
  let x = "Y"
  render`IN${{if (flag)}}${{x}} mid${{endif}}OUT`
}}
"#
    ));
    assert_eq!(taken.result, ok_string("INY midOUT"));

    let skipped = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let flag = false
  let x = "Y"
  render`IN${{if (flag)}}${{x}} mid${{endif}}OUT`
}}
"#
    ));
    assert_eq!(skipped.result, ok_string("INOUT"));
}

#[tokio::test]
async fn tagged_template_mixed_static_and_for() {
    // Static interpolation + a `${for}` in one template → the static fast-path
    // is NOT used (a block is present); everything flows through the dynamic
    // flattener. 1 leading value + 2 loop values → 4 parts, 3 values.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let a = "A"
  let xs = ["1", "2"]
  render`pre ${{a}} ${{for (let x in xs)}}${{x}},${{endfor}}end`
}}
"#
    ));
    assert_eq!(output.result, ok_string("pre A 1,2,end"));
}

#[tokio::test]
async fn tagged_template_cstyle_for_flattens() {
    // BEP §4: C-style `${for (init; cond; step)}` also drives the tagged
    // flatten path (push into parts/values per iteration), like the iterator
    // form. `RENDER_TAG` only renders string values (§11 preserves raw types),
    // so stringify the counter explicitly via `string.from`.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  render`${{for (let i = 0; i < 3; i += 1)}}[${{string.from(i)}}]${{endfor}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string("[0][1][2]"));
}

#[tokio::test]
async fn tagged_template_for_body_interp_with_nested_lambda_capturing_loop_local() {
    // Adversarial: a `${for}` body whose interpolation contains a NESTED lambda
    // that captures BOTH the loop-local (`x`) and an enclosing local (`outer`).
    // Exercises capture handling inside the flatten block's loop.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let outer = "O"
  let xs = ["a", "b"]
  render`${{for (let x in xs)}}${{["1"].map((y) -> {{ x + outer + y }}).join("")}}${{endfor}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string("aO1bO1"));
}

#[tokio::test]
async fn tagged_template_for_body_interp_nested_lambda_uses_loop_local_in_let() {
    // Companion to the capturing_loop_local case: the nested lambda captures the
    // `${for}` loop-local `x` and binds it through a local `let z` before use.
    // Confirms the captured loop-local resolves to a concrete `string` (so the
    // `let`/concat type-check and lower) rather than degrading to `Ty::Unknown`.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let xs = ["a", "b"]
  render`${{for (let x in xs)}}${{["1"].map((y) -> {{ let z = x + "!"; z + y }}).join("")}}${{endfor}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string("a!1b!1"));
}

#[tokio::test]
async fn tagged_template_nested_for_in_if() {
    // Nested control flow: a `${for}` inside a taken `${if}` branch.
    let output = baml_test!(&format!(
        r#"{RENDER_TAG}
function main() -> string {{
  let show = true
  let xs = ["p", "q"]
  render`${{if (show)}}${{for (let x in xs)}}<${{x}}>${{endfor}}${{endif}}`
}}
"#
    ));
    assert_eq!(output.result, ok_string("<p><q>"));
}
