# Design: LLM Function Image Outputs

Status: draft design

Last reviewed: 2026-05-03

## Problem

BAML already has media input types (`image`, `audio`, `pdf`, `video`) and generated SDK media classes, but LLM function outputs are still treated as text. The runtime asks each provider for a response, collapses the provider-specific payload into `LLMCompleteResponse.content: String`, and then parses that string with `jsonish`.

That shape works for structured text extraction, but it cannot represent provider-native image outputs. OpenAI Responses image generation returns an `image_generation_call` output item with base64 image data, not JSON text. Anthropic Messages does not currently expose a native assistant image-output block for arbitrary generation; image-like outputs can come from tools such as code execution generated files.

The design goal is to support return types such as:

```baml
function DrawIcon(prompt: string) -> image
function DrawVariants(prompt: string) -> image[]
function ExplainAndDraw(prompt: string) -> (image | string)[]

class IllustratedAnswer {
  summary string
  hero image
}

function MakeIllustratedAnswer(prompt: string) -> IllustratedAnswer
```

## Current Code Shape

Relevant existing seams:

- `engine/baml-runtime/src/internal/llm_client/mod.rs` defines `LLMCompleteResponse { content: String, ... }`.
- `engine/baml-runtime/src/internal/llm_client/orchestrator/call.rs` parses only `s.content` via `parse_fn(&s.content)`.
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs` extracts only text from OpenAI Chat Completions and only the first text content from OpenAI Responses.
- `engine/baml-runtime/src/internal/llm_client/primitive/anthropic/response_handler.rs` extracts only the first Anthropic `Text` content block and ignores non-text output blocks for final values.
- `engine/baml-lib/jsonish/src/deserializer/coercer/coerce_primitive.rs` explicitly returns `Image type is not supported here` for `TypeValue::Media(BamlMediaType::Image)`.
- `engine/baml-lib/jinja-runtime/src/output_format/types.rs` rejects media in output schemas with `type '<media>' is not supported in outputs`.
- `baml_language/crates/sys_llm/src/types/output_format.rs` also rejects `Ty::Media` as `UnsupportedType("media")`.

The type system and code generators already understand media:

- `engine/baml-lib/baml-types/src/ir_type/mod.rs` includes `TypeValue::Media(BamlMediaType)`.
- `engine/baml-lib/baml-types/src/media.rs` defines `BamlMedia` with `File`, `Url`, and `Base64` representations.
- Newer `baml_language` codegen maps primitive image/audio/video/pdf types to media classes.

## Provider Facts

OpenAI Responses API:

- OpenAI's Responses API supports text/image/file inputs and text outputs generally, and image generation through the hosted `image_generation` tool.
- The image generation tool emits `image_generation_call` output items whose `result` is base64 image data.
- The same output array can contain message/text items and image generation items, so the response order should be preserved.
- Docs: [OpenAI image generation tool](https://platform.openai.com/docs/guides/tools-image-generation/), [OpenAI Responses API](https://platform.openai.com/docs/api-reference/responses).

Anthropic Messages API:

- Messages are content-block arrays. User-side content can include images. Assistant-side native model output is text and tool-use oriented, not arbitrary generated image blocks.
- Anthropic code execution can create generated files, and those files can be retrieved with the Files API. This can support chart/image outputs when code execution produces an image file.
- Docs: [Anthropic tool use](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/implement-tool-use), [Anthropic code execution](https://docs.anthropic.com/en/docs/agents-and-tools/tool-use/code-execution-tool), [Anthropic Files API](https://docs.anthropic.com/en/docs/build-with-claude/files).

## Core Design

Introduce a provider-neutral structured LLM output layer before BAML parsing.

```rust
pub struct LlmOutput {
    pub parts: Vec<LlmOutputPart>,
    pub text_content: String,
}

pub enum LlmOutputPart {
    Text {
        text: String,
        provider_id: Option<String>,
        metadata: serde_json::Value,
    },
    Image {
        media: BamlMedia,
        provider_id: Option<String>,
        metadata: serde_json::Value,
    },
    File {
        media: BamlMedia,
        provider_id: Option<String>,
        filename: Option<String>,
        metadata: serde_json::Value,
    },
    ToolCall {
        name: String,
        arguments: serde_json::Value,
        provider_id: Option<String>,
        metadata: serde_json::Value,
    },
}
```

Then change `LLMCompleteResponse` to preserve structured parts:

```rust
pub struct LLMCompleteResponse {
    pub content: String, // backward-compatible concatenation of Text parts
    pub output: LlmOutput,
    ...
}
```

`content` stays for existing APIs, logging, raw response access, and text-only parsing. New parsing paths use `output`.

## Parsing Rules

Add a media-aware parse entrypoint:

```rust
fn parse_llm_output(
    output_format: &OutputFormatContent,
    target: &TypeIR,
    output: &LlmOutput,
    done: bool,
) -> Result<BamlValueWithFlags>
```

Rules:

- `-> string`: concatenate all text parts exactly as `content` does today. If image parts are present, keep current behavior by default and expose a warning/parse flag later.
- `-> image`: require exactly one image part, or one file part whose MIME type is an image. Return `BamlValue::Media`.
- `-> image[]`: return all image parts in response order.
- `-> (image | string)[]`: return every text and image part in provider order. Text parts become `BamlValue::String`; image parts become `BamlValue::Media`.
- `-> image | string`: if exactly one native image and no text, return image; otherwise return concatenated text.
- Structured classes with media fields use a manifest reference strategy described below.

This means `-> (image | string)[]` is a good user-facing shape for ordered multimodal output. It is better than forcing the model to serialize base64 images into JSON because provider image outputs are already structured and can be large.

## Structured Objects With Media Fields

For a return type like:

```baml
class IllustratedAnswer {
  caption string
  image image
}
```

the model cannot place the image bytes directly in the JSON text reliably. Instead, BAML should assign each extracted image a stable local ID and allow JSON text to reference it:

```json
{
  "caption": "A small fox in a library",
  "image": { "$baml_media": "image_0" }
}
```

The extractor creates an in-memory registry:

```text
image_0 -> BamlMedia::base64(Image, ...)
image_1 -> BamlMedia::base64(Image, ...)
```

`jsonish` media coercion then accepts:

- `{"$baml_media": "image_0"}` for provider-native parts.
- `{"url": "...", "mime_type": "image/png"}` for URL-backed media.
- `{"base64": "...", "mime_type": "image/png"}` for base64 media.

For top-level `(image | string)[]`, this JSON manifest is not needed. BAML maps provider parts directly.

## `ctx.output_format`

`ctx.output_format` cannot stay purely "tell the model to output JSON/schema" for media returns. It should become media-aware:

- For text-only return types, keep current rendering.
- For pure media return types (`image`, `image[]`), render provider-agnostic guidance such as: "Return image outputs using the model provider's native image output mechanism. Do not describe the image in text unless requested."
- For ordered multimodal return types like `(image | string)[]`, render: "Return text as normal assistant text and images as native image outputs. BAML will preserve their order."
- For structured media-containing types, render hybrid instructions: "Return JSON for scalar fields. For each media field, generate a native image output and reference it in JSON as `{ \"$baml_media\": \"image_N\" }`."

This keeps existing prompts from breaking when they include `{{ ctx.output_format }}`, but it stops pretending that images are JSON text. For OpenAI Responses, the baml_language implementation now enforces the API path by adding the image generation tool when the return type requires images.

## OpenAI Responses Mapping

Request:

- If the return type contains `image`, require or auto-enable an `image_generation` tool in the `openai-responses` request.
- For `-> image` / `image[]`, set `tool_choice` to force `image_generation` unless the user configured otherwise.
- Allow BAML client options to pass image-generation tool options: `size`, `quality`, `format`, `compression`, `background`, `partial_images`, and `action`.

Response extraction:

- Walk `response.output` in order.
- For each `message` item, walk `content` in order and append `Text` for `output_text`.
- For each `image_generation_call`, append `Image` when `status == "completed"` and `result` is present.
- Convert `result` to `BamlMedia::base64(BamlMediaType::Image, result, mime_type)`.
- Infer MIME type from the configured output format or by sniffing decoded bytes. Default to `image/png` only when no better source exists.
- Preserve provider metadata: provider output ID, revised prompt, status, model, and any raw output item fields needed for tracing/debugging.

Streaming:

- Keep text streaming behavior for `response.output_text.delta`.
- Treat `response.image_generation_call.partial_image` events as progress events, not final values.
- Final image values should be emitted when the completed response item is available. MVP can make image-output functions non-streaming, then add pending image placeholders later.

Current baml_language MVP:

- `openai-responses` requests add `tools: [{ "type": "image_generation" }]` for supported image-output return types.
- `-> image` and `-> image[]` also add `tool_choice: { "type": "image_generation" }` unless the user already set `tool_choice` in `request_body`.
- `-> (image | string)[]` adds the tool without forcing `tool_choice`, so the model can return both normal assistant text and generated image calls.
- The parser extracts `image_generation_call.result` into base64 image media and preserves mixed text/image ordering for top-level arrays.
- OpenAI Responses streaming still needs a separate event-to-`LlmOutput.parts` path before image-output streaming can work.

## Anthropic Mapping

Anthropic has two practical modes:

1. Text-only Claude Messages, current behavior.
2. Tool/file-mediated image outputs.

For `-> image` with a plain Anthropic client, return a capability error unless a configured output mechanism exists. The cleanest supported mechanism is code execution:

- Enable the Anthropic code execution tool.
- Prompt or `ctx.output_format` tells Claude to create an image file, for example `output.png`.
- Extract generated file IDs from code execution result content blocks.
- Use the Anthropic Files API to download the file bytes, or retain a provider file handle if BAML adds file-backed remote media.
- Convert image files to `BamlMedia::base64(Image, ...)` or `BamlMedia::url/file_id` if a safe URL/handle representation is added.

For `-> (image | string)[]`:

- Append assistant text blocks as `Text`.
- Append generated image files from tool/code-execution result blocks as `Image` in content-block order.
- If Anthropic returns only a textual description, parsing fails for required `image` fields but succeeds for `string` targets.

Current baml_language MVP:

- `parse_anthropic_response` collects text blocks and image-like files from code execution result blocks.
- Plain Anthropic Messages still has no native arbitrary image-generation output path.
- Streaming needs a tagged content block enum before mixed output streaming can work.

## Provider Capability Model

Add a provider feature description alongside `ModelFeatures`:

```rust
pub struct OutputCapabilities {
    pub text: bool,
    pub native_images: bool,
    pub tool_generated_files: bool,
    pub mixed_ordered_parts: bool,
    pub streaming_images: bool,
}
```

Initial matrix:

| Provider/client | `string` | `image` | `(image | string)[]` | Notes |
| --- | --- | --- | --- | --- |
| `openai-responses` | yes | yes | yes | Use `image_generation` tool. |
| `openai` chat completions | yes | no | no | No native image output path. |
| `openai-generic` | yes | provider-specific | provider-specific | Only if compatible provider returns known image parts. |
| `anthropic` messages | yes | no by default | no by default | Needs code execution/files or a configured custom image tool. |
| `anthropic` + code execution | yes | yes for generated image files | partial | Good for charts/plots; not general image model generation. |

Compile-time validation can catch obvious static cases. Runtime validation is still required because clients can be dynamic and model/tool support is option-dependent.

## Implementation Plan

1. Add `LlmOutput` and `LlmOutputPart`; keep `LLMCompleteResponse.content` as derived text.
2. Update provider response handlers to emit all text parts, not just the first text block.
3. Add OpenAI Responses image-generation extraction.
4. Add `jsonish::from_llm_output` and media coercion for direct media, arrays, unions, and media references.
5. Make `PromptRenderer::parse` choose string parsing or structured-output parsing based on whether the target type contains media.
6. Make `ctx.output_format` media-aware in `sys_llm`.
7. Add provider capability checks before dispatch.
8. Add tests:
   - OpenAI Responses fixture with text + `image_generation_call`.
   - `-> image`, `-> image[]`, `-> (image | string)[]`.
   - Class with image field using `$baml_media` reference.
   - Anthropic text-only capability error for `-> image`.
   - Anthropic code-execution fixture mapping generated image file to `image`.

## Open Questions

- Should BAML persist generated images to disk or return only in-memory base64 media? MVP should return base64 to match existing `BamlMedia`.
- Should `image` output allow remote provider file IDs as first-class media content? That would avoid downloading large files, but requires SDK/API additions.
- How strict should `-> string` be when the provider also returns images? Backward compatibility suggests concatenating text and ignoring image parts unless the return type asks for them.
- Should `ctx.output_format` automatically inject provider tool options, or should tool configuration remain solely in client options? Client options are cleaner because prompt text should not configure transport behavior.
