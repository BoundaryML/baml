//! B-627 — `ai.Prompt` readable accessors.
//!
//! `PromptAst` (returned by `<Fn>$render_prompt` and the `prompt` tag) is a
//! `$rust_type`-backed handle. Before B-627 it had no way to read the rendered
//! prompt from BAML — `.text()`/`.messages()` didn't exist and `to_string`
//! leaked the Rust `Debug` of the opaque handle. These tests pin the new
//! accessors and the readable `to_string`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// A `prompt` body with `${role(...)}` markers, assembled against a hand-built
/// `Context`, then the given `expr` evaluated on the resulting `PromptAst`.
fn prompt_expr_src(expr: &str) -> String {
    format!(
        r#"
function main() -> string {{
  let cc = baml.prompt.ContextClient {{ name: "c", provider: "openai", default_role: "system", allowed_roles: ["system", "user"] }}
  let ctx = baml.prompt.Context {{ client: cc, tags: {{}} }}
  let render = prompt`${{role("system")}}You are helpful.${{role("user")}}Hi World!`
  let ast = render(ctx)
  {expr}
}}
"#
    )
}

async fn run_string(src: &str) -> String {
    let output = baml_test!(src);
    match output.result {
        Ok(BexExternalValue::String(s)) => s.to_string(),
        other => panic!("expected a string result, got {other:?}"),
    }
}

#[tokio::test]
async fn text_renders_role_headed_prompt() {
    let text = run_string(&prompt_expr_src("ast.text()")).await;
    assert_eq!(text, "[system]\nYou are helpful.\n\n[user]\nHi World!");
}

#[tokio::test]
async fn to_string_is_readable_not_rust_debug() {
    // `string.from` routes through the `baml.ToString` override.
    let rendered = run_string(&prompt_expr_src("string.from(ast)")).await;
    assert_eq!(rendered, "[system]\nYou are helpful.\n\n[user]\nHi World!");
    // The bug: `to_string` used to leak the opaque handle's Rust `Debug`
    // (`PromptAst { _data: <rust_data> }`, `Adt(...)`, `String(...)`). Must not.
    for leak in ["Adt(", "String(", "$rust_type", "_data", "rust_data"] {
        assert!(
            !rendered.contains(leak),
            "to_string leaked `{leak}`: {rendered}"
        );
    }
}

#[tokio::test]
async fn messages_expose_role_and_content() {
    // Fold the structured messages back into a string so the assertion needs no
    // nested-value inspection: `role=content` per message, `;`-separated.
    let src = prompt_expr_src(
        r#"let out = ""
  for (let m in ast.messages()) {
    out += m.role + "=" + m.content + ";"
  }
  out"#,
    );
    let folded = run_string(&src).await;
    assert_eq!(folded, "system=You are helpful.;user=Hi World!;");
}

#[tokio::test]
async fn messages_yields_promptmessage_instances() {
    // The element type is the new `ai.PromptMessage` class.
    let output = baml_test!(
        r#"
function main() -> ai.PromptMessage[] {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = prompt`Just a system prompt.`
  let ast = render(ctx)
  ast.messages()
}
"#
    );
    match &output.result {
        Ok(BexExternalValue::Array { items, .. }) => {
            assert_eq!(items.len(), 1, "expected one message: {items:?}");
            match &items[0] {
                BexExternalValue::Instance {
                    class_name, fields, ..
                } if class_name == "ai.PromptMessage" => {
                    assert_eq!(
                        fields.get("content"),
                        Some(&BexExternalValue::from("Just a system prompt."))
                    );
                }
                other => panic!("expected a ai.PromptMessage instance, got {other:?}"),
            }
        }
        other => panic!("expected a PromptMessage array, got {other:?}"),
    }
}

#[tokio::test]
async fn message_metadata_survives_prompt_assembly() {
    // `baml.prompt.Role.metadata` is the per-message channel providers read for
    // directives like Anthropic `cache_control`. `collect_structured_messages`
    // used to drop it; it must reach `ai.PromptMessage.metadata` intact, and a
    // message that carried none must read as an empty map (never null).
    let output = run_string(
        r#"
function main() -> string {
  let cc = baml.prompt.ContextClient { name: "c", provider: "anthropic", default_role: "user", allowed_roles: ["system", "user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let cached = baml.prompt.Role { name: "system", metadata: { "cache_control": { "type": "ephemeral" } } }
  let render = prompt`${cached}Long shared preamble.${role("user")}Hi World!`
  let out: string[] = []
  for (let m in render(ctx).messages()) {
    out.push(`${m.role}=${baml.json.stringify(baml.json.to_json(m.metadata))}`)
  }
  out.join(";")
}
"#,
    )
    .await;
    assert_eq!(
        output,
        r#"system={"cache_control":{"type":"ephemeral"}};user={}"#
    );
}

#[tokio::test]
async fn message_parts_preserve_all_media_structurally() {
    let output = run_string(
        r#"
function main() -> string {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let img = image.from_url("https://example.com/photo.png", "image/png")
  let sound = audio.from_url("https://example.com/sound.mp3", "audio/mpeg")
  let movie = video.from_url("https://example.com/movie.mp4", "video/mp4")
  let document = pdf.from_url("https://example.com/document.pdf", "application/pdf")
  let render = prompt`before:${img}:${sound}:${movie}:${document}:after`
  let kinds: string[] = []
  for (let part in render(ctx).messages()[0].parts) {
    match (part) {
      let text: string => kinds.push(`text=${text}`),
      let image: baml.media.Image => kinds.push(`image=${image.url() ?? ""}`),
      let audio: baml.media.Audio => kinds.push(`audio=${audio.url() ?? ""}`),
      let video: baml.media.Video => kinds.push(`video=${video.url() ?? ""}`),
      let pdf: baml.media.Pdf => kinds.push(`pdf=${pdf.url() ?? ""}`),
    };
  }
  kinds.join("|")
}
"#,
    )
    .await;
    assert_eq!(
        output,
        "text=before:|image=https://example.com/photo.png|text=:|audio=https://example.com/sound.mp3|text=:|video=https://example.com/movie.mp4|text=:|pdf=https://example.com/document.pdf|text=:after"
    );
}
