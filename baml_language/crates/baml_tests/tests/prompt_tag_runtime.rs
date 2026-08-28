//! BEP-049 §10 (M5) — the built-in `prompt` tag, runtime execution.
//!
//! `` prompt`...` `` evaluates to a `(Context) -> ai.Prompt` closure;
//! invoking it folds the template into a `PromptAst`, where `${role("...")}`
//! markers split the content into chat messages (M5d structural assembly —
//! no magic delimiters). `${ctx.output_format()}` injects the return type's
//! schema (M5b). Orchestrator wiring (auto-building `Context` per attempt) is
//! a later slice; here we build a `Context` by hand and inspect the result.

use std::sync::Arc;

use baml_builtins2::{PromptAst, PromptAstSimple};
use baml_tests::{baml_test, engine::TestOutput};
use bex_engine::BexExternalValue;
use bex_external_types::BexExternalAdt;

fn test_result(output: TestOutput) -> BexExternalValue {
    output.result.expect("BAML execution should succeed")
}

fn prompt_ast(output: TestOutput) -> Arc<PromptAst> {
    match test_result(output) {
        BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => ast,
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name == "ai.Prompt" => match fields.get("_data") {
            Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
            other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
        },
        other => panic!("expected a portable ai.Prompt value, got {other:?}"),
    }
}

fn prompt_messages(ast: &PromptAst) -> &[Arc<PromptAst>] {
    match ast {
        PromptAst::Vec(messages) => messages,
        other => panic!("expected prompt messages, got {other:?}"),
    }
}

fn role_and_metadata(message: &PromptAst) -> (&str, &serde_json::Value) {
    match message {
        PromptAst::Message { role, metadata, .. } => (role, metadata),
        other => panic!("expected a prompt message, got {other:?}"),
    }
}

fn collect_media_kinds(ast: &PromptAst, out: &mut Vec<baml_base::MediaKind>) {
    fn collect_content(content: &PromptAstSimple, out: &mut Vec<baml_base::MediaKind>) {
        match content {
            PromptAstSimple::String(_) => {}
            PromptAstSimple::Media(media) => out.push(media.kind),
            PromptAstSimple::Multiple(parts) => {
                for part in parts {
                    collect_content(part, out);
                }
            }
        }
    }

    match ast {
        PromptAst::Simple(content) => collect_content(content, out),
        PromptAst::Message { content, .. } => collect_content(content, out),
        PromptAst::Vec(items) => {
            for item in items {
                collect_media_kinds(item, out);
            }
        }
    }
}

#[tokio::test]
async fn llm_spec_prompt_preserves_messages() {
    let output = baml_test!(
        r#"
function StructuredGreeting(name: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `${role("system")}Be concise.${role("user")}Hello ${name}`
}

function main() -> ai.Prompt {
  StructuredGreeting@spec("Ada").prompt()
}
"#
    );
    let ast = prompt_ast(output);
    assert_eq!(
        ast.to_messages(),
        vec![
            ("system".to_string(), "Be concise.".to_string()),
            ("user".to_string(), "Hello Ada".to_string()),
        ],
    );
}

#[tokio::test]
async fn llm_spec_prompt_preserves_media() {
    let output = baml_test!(
        r#"
function InspectPhoto(photo: image) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `${role("user")}Inspect ${photo}`
}

function main() -> ai.Prompt {
  InspectPhoto@spec(image.from_url("https://example.com/photo.png", "image/png")).prompt()
}
"#
    );
    let ast = prompt_ast(output);
    let mut media_kinds = Vec::new();
    collect_media_kinds(&ast, &mut media_kinds);
    assert_eq!(media_kinds, vec![baml_base::MediaKind::Image]);
}

#[tokio::test]
async fn prompt_role_metadata_is_preserved() {
    let output = baml_test!(
        r#"
function main() -> ai.Prompt {
  let system = ai.Role {
    name: "system",
    metadata: {
      "cache_control": { "type": "ephemeral" },
    },
  }
  let user = ai.Role {
    name: "user",
    metadata: { "priority": 3 },
  }
  let cc = ai.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["system", "user"] }
  let ctx = ai.Context {
    client: cc,
    tags: {},
    _output_format: ai.internal.build_output_format(reflect.Type.of<string>()),
  }
  let render = prompt`${system}Rules${user}Hello`
  render(ctx)
}
"#
    );
    let ast = prompt_ast(output);
    let messages = prompt_messages(&ast);
    assert_eq!(messages.len(), 2, "expected two prompt messages: {ast:?}");
    let (role, metadata) = role_and_metadata(&messages[0]);
    assert_eq!(role, "system");
    assert_eq!(
        metadata,
        &serde_json::json!({
            "cache_control": { "type": "ephemeral" },
        })
    );
    let (role, metadata) = role_and_metadata(&messages[1]);
    assert_eq!(role, "user");
    assert_eq!(metadata, &serde_json::json!({ "priority": 3 }));
}
