//! BEP-049 §10 tagged templates — runtime execution tests (M4e.1).
//!
//! These execute the program (parse → … → MIR → bytecode → VM) via `baml_test!`,
//! which does NOT run the source formatter (so the missing formatter support,
//! task #42, is irrelevant here). They exercise the MIR lowering of a tagged
//! template to a hand-rolled body closure + `tag(body = closure)` call.
//!
//! M4e.1a covers text + interpolation + arity-N body params + captures (static
//! parts/values arrays). `${for}`/`${if}` flattening is M4e.1b.

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
