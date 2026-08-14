//! BEP-049 §10 (M5) — the built-in `prompt` tag, runtime execution.
//!
//! `` prompt`...` `` evaluates to a `(Context) -> ai.Prompt` closure;
//! invoking it folds the template into a `PromptAst`, where `${role("...")}`
//! markers split the content into chat messages (M5d structural assembly —
//! no magic delimiters). `${ctx.output_format}` injects the return type's
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
        BexExternalValue::Instance {
            class_name, fields, ..
        } if class_name == "ai.Prompt" => match fields.get("_data") {
            Some(BexExternalValue::Adt(BexExternalAdt::PromptAst(ast))) => ast.clone(),
            other => panic!("expected `_data` to hold a PromptAst ADT, got {other:?}"),
        },
        other => panic!("expected a ai.Prompt instance, got {other:?}"),
    }
}

fn string_result(output: TestOutput) -> String {
    match test_result(output) {
        BexExternalValue::String(value) => value.to_string(),
        other => panic!("expected a string result, got {other:?}"),
    }
}

fn prompt_text(output: TestOutput) -> String {
    prompt_ast(output).render_text()
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
async fn llm_render_prompt_companion_preserves_messages() {
    let output = baml_test!(
        r#"
function StructuredGreeting(name: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `${role("system")}Be concise.${role("user")}Hello ${name}`
}

function main() -> ai.Prompt {
  StructuredGreeting$render_prompt("Ada")
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
async fn llm_render_prompt_companion_preserves_media() {
    let output = baml_test!(
        r#"
function InspectPhoto(photo: image) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `${role("user")}Inspect ${photo}`
}

function main() -> ai.Prompt {
  InspectPhoto$render_prompt(image.from_url("https://example.com/photo.png", "image/png"))
}
"#
    );
    let ast = prompt_ast(output);
    let mut media_kinds = Vec::new();
    collect_media_kinds(&ast, &mut media_kinds);
    assert_eq!(media_kinds, vec![baml_base::MediaKind::Image]);
}

#[tokio::test]
async fn role_construction_isolation() {
    // Isolation: does constructing a `Role { name, metadata }` even type-check?
    let output = baml_test!(
        r#"
function main() -> baml.prompt.Role {
  return baml.prompt.Role { name: "system", metadata: {} };
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
async fn prompt_role_metadata_is_preserved() {
    let output = baml_test!(
        r#"
function main() -> ai.Prompt {
  let system = baml.prompt.Role {
    name: "system",
    metadata: {
      "cache_control": { "type": "ephemeral" },
    },
  }
  let user = baml.prompt.Role {
    name: "user",
    metadata: { "priority": 3 },
  }
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["system", "user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
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

#[tokio::test]
async fn prompt_tag_builds_promptast_with_role_messages() {
    let output = baml_test!(
        r#"
function main() -> ai.Prompt {
  let name = "World"
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = ai.prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
  render(ctx)
}
"#
    );
    assert_eq!(
        prompt_text(output),
        "[system]\nYou are helpful.\n\n[user]\nHi World!"
    );
}

#[tokio::test]
async fn unqualified_prompt_tag_resolves_to_baml_llm_prompt() {
    // Ergonomic fallback: bare `prompt`...`` resolves to `ai.prompt`
    // (no `baml.prompt.` qualifier needed). Same assembly as the qualified form.
    let output = baml_test!(
        r#"
function main() -> ai.Prompt {
  let name = "World"
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = prompt`${role("system")}You are helpful.${role("user")}Hi ${name}!`
  render(ctx)
}
"#
    );
    assert_eq!(
        prompt_text(output),
        "[system]\nYou are helpful.\n\n[user]\nHi World!"
    );
}

#[tokio::test]
async fn prompt_interpolates_class_and_array_like_ordinary_string() {
    // `${class_value}` / `${array_value}` in a `prompt` backtick string must
    // render via the same implicit `to_string` (`string.from`) as an ordinary
    // backtick string — the `prompt` form must be byte-identical. `main` renders
    // both and returns them joined by a sentinel for direct comparison.
    let output = baml_test!(
        r#"
class Point {
  x int
  y int
}

function main() -> string {
  let p = Point { x: 1, y: 2 }
  let xs = [10, 20, 30]
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = prompt`Point ${p} list ${xs}`
  let from_prompt = render(ctx).text()
  let from_string = `Point ${p} list ${xs}`
  from_prompt + " <=> " + from_string
}
"#
    );
    let rendered = string_result(output);
    let (from_prompt, from_string) = rendered
        .split_once(" <=> ")
        .expect("main should return the two renderings joined by ` <=> `");
    // The composite must render and match exactly what the same values yield in
    // an ordinary backtick string.
    assert_eq!(
        from_prompt, from_string,
        "prompt interpolation of a class/array must match ordinary string interpolation"
    );
    assert_eq!(
        from_prompt, "Point Point { x: 1, y: 2 } list [10, 20, 30]",
        "class and array must render via their `to_string` form, not empty"
    );
}

#[tokio::test]
async fn prompt_interpolation_honors_to_string_override() {
    // Composites route through `string.from` (the real implicit `to_string`), so
    // a user `baml.ToString` override is honored in `prompt` exactly as in an
    // ordinary backtick string. `${role(...)}` still splits messages.
    let output = baml_test!(
        r#"
class Labeled {
  tag string
  implements baml.ToString {
    function to_string(self) -> string throws never {
      "LBL<" + self.tag + ">"
    }
  }
}

function main() -> string {
  let v = Labeled { tag: "hi" }
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "system", allowed_roles: ["system", "user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = prompt`${role("system")}head ${v}${role("user")}tail ${v}`
  render(ctx).text()
}
"#
    );
    let rendered = string_result(output);
    // Both messages render the override, and the roles still split the content.
    assert_eq!(
        rendered, "[system]\nhead LBL<hi>\n\n[user]\ntail LBL<hi>",
        "override must apply in every message and roles must still split"
    );
}

#[tokio::test]
async fn prompt_preserves_media_nested_in_classes_arrays_and_maps() {
    let output = baml_test!(
        r#"
class MediaLeaf {
  picture image
  sound audio
  clip video
  document pdf
}

class MediaEnvelope {
  primary MediaLeaf
  gallery MediaLeaf[]
  lookup map<string, MediaLeaf>
}

function main() -> ai.Prompt {
  let leaf = MediaLeaf {
    picture: image.from_url("https://example.com/picture.png", "image/png"),
    sound: audio.from_url("https://example.com/sound.wav", "audio/wav"),
    clip: video.from_url("https://example.com/clip.mp4", "video/mp4"),
    document: pdf.from_url("https://example.com/document.pdf", "application/pdf"),
  }
  let envelope = MediaEnvelope {
    primary: leaf,
    gallery: [leaf],
    lookup: { "copy": leaf },
  }
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let ctx = baml.prompt.Context { client: cc, tags: {} }
  let render = prompt`${role("user")}Inspect ${envelope}`
  render(ctx)
}
"#
    );

    let ast = prompt_ast(output);
    let mut media_kinds = Vec::new();
    collect_media_kinds(&ast, &mut media_kinds);
    assert_eq!(
        media_kinds,
        vec![
            baml_base::MediaKind::Image,
            baml_base::MediaKind::Audio,
            baml_base::MediaKind::Video,
            baml_base::MediaKind::Pdf,
            baml_base::MediaKind::Image,
            baml_base::MediaKind::Audio,
            baml_base::MediaKind::Video,
            baml_base::MediaKind::Pdf,
            baml_base::MediaKind::Image,
            baml_base::MediaKind::Audio,
            baml_base::MediaKind::Video,
            baml_base::MediaKind::Pdf,
        ],
        "each nested occurrence must remain a structural media node: {ast:?}"
    );
    let rendered = ast.render_text();
    assert!(rendered.starts_with("[user]\nInspect MediaEnvelope"));
    assert!(rendered.contains("image::url(https://example.com/picture.png, loaded=false)"));
    assert!(rendered.contains("audio::url(https://example.com/sound.wav, loaded=false)"));
    assert!(rendered.contains("video::url(https://example.com/clip.mp4, loaded=false)"));
    assert!(rendered.contains("pdf::url(https://example.com/document.pdf, loaded=false)"));
    assert!(!rendered.contains("rust_data"));
}

#[tokio::test]
async fn prompt_interpolates_ctx_output_format() {
    // BEP-049 M5b: `${ctx.output_format}` renders the return type's schema.
    // `render_output_format(type.of<Person>())` produces the schema
    // string the orchestrator will later populate `Context.output_format` with;
    // here we wire it by hand and assert the assembled prompt embeds the schema.
    let output = baml_test!(
        r#"
class Person {
  name string
  age int
}

function main() -> ai.Prompt {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let of = baml.prompt.render_output_format(type.of<Person>())
  let ctx = baml.prompt.Context { client: cc, tags: {}, output_format: of }
  let render = ai.prompt`Answer using this schema:
${ctx.output_format}`
  render(ctx)
}
"#
    );
    let rendered = prompt_text(output);
    assert!(
        rendered.contains("Answer using this schema:"),
        "prompt text should be present: {rendered}"
    );
    assert!(
        rendered.contains("name") && rendered.contains("age"),
        "rendered output_format should list the Person fields: {rendered}"
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

function main() -> ai.Prompt {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let rt = type.of<Person>()
  let ctx = baml.prompt.Context { client: cc, tags: {}, output_format: baml.prompt.render_output_format(rt), _output_format: baml.prompt.build_output_format(rt) }
  let render = ai.prompt`${ctx.output_format_with(prefix = "Use this exact schema:")}`
  render(ctx)
}
"#
    );
    let rendered = prompt_text(output);
    assert!(
        rendered.contains("Use this exact schema:"),
        "the custom `prefix` option should be applied: {rendered}"
    );
    assert!(
        rendered.contains("name") && rendered.contains("age"),
        "rendered schema should list the Person fields: {rendered}"
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

function main() -> ai.Prompt {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let rt = type.of<Person>()
  let ctx = baml.prompt.Context { client: cc, tags: {}, output_format: baml.prompt.render_output_format(rt), _output_format: baml.prompt.build_output_format(rt) }
  let render = ai.prompt`${ctx.output_format_with(quote_class_fields = true)}`
  render(ctx)
}
"#
    );
    let rendered = prompt_text(output);
    assert!(
        rendered.contains("name") && rendered.contains("age"),
        "rendered schema should list the Person fields: {rendered}"
    );
}

#[tokio::test]
async fn prompt_output_format_with_can_render_null_as_omit() {
    let output = baml_test!(
        r#"
class Person {
  name string
  nickname string?
}

function main() -> ai.Prompt {
  let cc = baml.prompt.ContextClient { name: "c", provider: "openai", default_role: "user", allowed_roles: ["user"] }
  let rt = type.of<Person>()
  let ctx = baml.prompt.Context { client: cc, tags: {}, output_format: baml.prompt.render_output_format(rt), _output_format: baml.prompt.build_output_format(rt) }
  let render = ai.prompt`${ctx.output_format_with(render_null_as = "omit")}`
  render(ctx)
}
"#
    );
    let rendered = prompt_text(output);
    assert!(
        rendered.contains("nickname: string or omit,"),
        "custom null rendering should apply to output_format_with: {rendered}"
    );
    assert!(
        !rendered.contains("string or null"),
        "default null rendering should be replaced: {rendered}"
    );
}
