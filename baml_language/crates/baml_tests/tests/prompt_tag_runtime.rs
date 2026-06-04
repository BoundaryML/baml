//! BEP-049 §10 (M5) — the built-in `prompt` tag, runtime execution.
//!
//! `` prompt`...` `` evaluates to a `(Context) -> baml.llm.PromptAst` closure;
//! invoking it folds the template into a `PromptAst`, where `${role("...")}`
//! markers split the content into chat messages (M5d structural assembly —
//! no magic delimiters). Output-format injection / orchestrator wiring are
//! later slices; here we build a `Context` by hand and inspect the result.

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
