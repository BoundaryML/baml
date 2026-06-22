//! BEP-049 §10 (M5) — the built-in `prompt` tag, runtime execution.
//!
//! `` prompt`...` `` evaluates to a `(Context) -> baml.llm.PromptAst` closure;
//! invoking it folds the template into a `PromptAst`, where `${role("...")}`
//! markers split the content into chat messages (M5d structural assembly —
//! no magic delimiters). `${ctx.output_format}` injects the return type's
//! schema (M5b). Orchestrator wiring (auto-building `Context` per attempt) is
//! a later slice; here we build a `Context` by hand and inspect the result.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;
use bex_external_types::BexExternalAdt;

#[tokio::test]
async fn role_construction_isolation() {
    // Isolation: does constructing a `Role { name, metadata }` even type-check?
    let output = baml_test!(
        r#"
function main() -> baml.llm.Role {
  return baml.llm.Role { name: "system", metadata: {} };
}
"#
    );
    assert!(
        output.result.is_ok(),
        "Role construction should compile + run, got {:?}",
        output.result
    );
}

#[tokio::test]
async fn prompt_tag_builds_promptast_with_role_messages() {
    let output = baml_test!(
        r#"
function main() -> baml.llm.PromptAst {
  let name = "World"
  let cc = baml.llm.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.llm.Context { client: cc, tags: {} }
  let render = baml.llm.prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
  render(ctx)
}
"#
    );
    // `baml.llm.PromptAst` is a handle class wrapping the `PromptAst` rust ADT
    // in its `_data` field, so the external value is an `Instance`, not a bare
    // `Adt`. Unwrap the handle to inspect the assembled prompt.
    let ast = match &output.result {
        Ok(BexExternalValue::Instance { class_name, fields })
            if class_name == "baml.llm.PromptAst" =>
        {
            match fields.get("_data") {
                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
                other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
            }
        }
        other => panic!("expected a baml.llm.PromptAst instance, got {other:?}"),
    };
    // Expect two messages: system "You are helpful." then user "Hi World!".
    let dbg = format!("{ast:?}");
    assert!(
        dbg.contains("\"system\""),
        "expected a system message: {dbg}"
    );
    assert!(dbg.contains("You are helpful."), "{dbg}");
    assert!(dbg.contains("\"user\""), "expected a user message: {dbg}");
    assert!(dbg.contains("Hi World!"), "{dbg}");
}

#[tokio::test]
async fn unqualified_prompt_tag_resolves_to_baml_llm_prompt() {
    // Ergonomic fallback: bare `prompt`...`` resolves to `baml.llm.prompt`
    // (no `baml.llm.` qualifier needed). Same assembly as the qualified form.
    let output = baml_test!(
        r#"
function main() -> baml.llm.PromptAst {
  let name = "World"
  let cc = baml.llm.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.llm.Context { client: cc, tags: {} }
  let render = prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
  render(ctx)
}
"#
    );
    let ast = match &output.result {
        Ok(BexExternalValue::Instance { class_name, fields })
            if class_name == "baml.llm.PromptAst" =>
        {
            match fields.get("_data") {
                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
                other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
            }
        }
        other => panic!("expected a baml.llm.PromptAst instance, got {other:?}"),
    };
    let dbg = format!("{ast:?}");
    assert!(
        dbg.contains("\"system\"") && dbg.contains("You are helpful."),
        "{dbg}"
    );
    assert!(
        dbg.contains("\"user\"") && dbg.contains("Hi World!"),
        "{dbg}"
    );
}

#[tokio::test]
async fn prompt_interpolates_ctx_output_format() {
    // BEP-049 M5b: `${ctx.output_format}` renders the return type's schema.
    // `render_output_format(reflect.type_of<Person>())` produces the schema
    // string the orchestrator will later populate `Context.output_format` with;
    // here we wire it by hand and assert the assembled prompt embeds the schema.
    let output = baml_test!(
        r#"
class Person {
  name string
  age int
}

function main() -> baml.llm.PromptAst {
  let cc = baml.llm.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let of = baml.llm.render_output_format(reflect.type_of<Person>())
  let ctx = baml.llm.Context { client: cc, tags: {}, output_format: of }
  let render = baml.llm.prompt`Answer using this schema:
${ctx.output_format}`
  render(ctx)
}
"#
    );
    let ast = match &output.result {
        Ok(BexExternalValue::Instance { class_name, fields })
            if class_name == "baml.llm.PromptAst" =>
        {
            match fields.get("_data") {
                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
                other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
            }
        }
        other => panic!("expected a baml.llm.PromptAst instance, got {other:?}"),
    };
    let dbg = format!("{ast:?}");
    assert!(
        dbg.contains("Answer using this schema:"),
        "prompt text should be present: {dbg}"
    );
    assert!(
        dbg.contains("name") && dbg.contains("age"),
        "rendered output_format should list the Person fields: {dbg}"
    );
}

#[tokio::test]
async fn prompt_interpolates_ctx_output_format_with() {
    // BEP-049 M5b.2: `${ctx.output_format_with(prefix=..., ...)}` re-renders the
    // return type's schema with caller options. `Context._output_format` carries
    // the prebuilt schema handle; a non-default `prefix` must appear in the
    // assembled prompt, proving the option took effect. Exercises two infra
    // paths: a method call on a body-param inside a template, and an io-function
    // with optional params called with most omitted.
    let output = baml_test!(
        r#"
class Person {
  name string
  age int
}

function main() -> baml.llm.PromptAst {
  let cc = baml.llm.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let rt = reflect.type_of<Person>()
  let ctx = baml.llm.Context { client: cc, tags: {}, output_format: baml.llm.render_output_format(rt), _output_format: baml.llm.build_output_format(rt) }
  let render = baml.llm.prompt`${ctx.output_format_with(prefix = "Use this exact schema:")}`
  render(ctx)
}
"#
    );
    let ast = match &output.result {
        Ok(BexExternalValue::Instance { class_name, fields })
            if class_name == "baml.llm.PromptAst" =>
        {
            match fields.get("_data") {
                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
                other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
            }
        }
        other => panic!("expected a baml.llm.PromptAst instance, got {other:?}"),
    };
    let dbg = format!("{ast:?}");
    assert!(
        dbg.contains("Use this exact schema:"),
        "the custom `prefix` option should be applied: {dbg}"
    );
    assert!(
        dbg.contains("name") && dbg.contains("age"),
        "rendered schema should list the Person fields: {dbg}"
    );
}

#[tokio::test]
async fn prompt_output_format_with_omits_leading_optional_arg() {
    // Regression: a method io-sysop (`$rust_io_function` instance method) called
    // with a LATER optional arg provided and an EARLIER one omitted. The call
    // plan's `param_index` is receiver-relative (self stripped), but the sys-op
    // default arena is indexed self-inclusive — so the omitted `prefix` read
    // `self`'s (absent) default → `OmittedArg` → engine panic. Here
    // `quote_class_fields = true` is provided while `prefix` (and everything
    // before it) is omitted. Must render the schema, not panic.
    let output = baml_test!(
        r#"
class Person {
  name string
  age int
}

function main() -> baml.llm.PromptAst {
  let cc = baml.llm.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let rt = reflect.type_of<Person>()
  let ctx = baml.llm.Context { client: cc, tags: {}, output_format: baml.llm.render_output_format(rt), _output_format: baml.llm.build_output_format(rt) }
  let render = baml.llm.prompt`${ctx.output_format_with(quote_class_fields = true)}`
  render(ctx)
}
"#
    );
    let ast = match &output.result {
        Ok(BexExternalValue::Instance { class_name, fields })
            if class_name == "baml.llm.PromptAst" =>
        {
            match fields.get("_data") {
                Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
                other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
            }
        }
        other => panic!("expected a baml.llm.PromptAst instance, got {other:?}"),
    };
    let dbg = format!("{ast:?}");
    assert!(
        dbg.contains("name") && dbg.contains("age"),
        "rendered schema should list the Person fields: {dbg}"
    );
}
