# Media in BAML: inputs, outputs, and what a native migration must build

Research for the `sys_llm` → `crates/baml_builtins2/baml_std/` migration.
Scope: **media** — how images/audio/video/PDF are represented, how they get
*into* a request, and (the hard part) how a model-returned image gets *out* to
user code.

Paths are relative to `/Users/aaron/projects/baml/baml_language` unless prefixed
with `engine/` (= `/Users/aaron/projects/baml/engine`, the old compiler).

Sibling doc: `_plan/sys_llm_native/research/openai-responses-images.md` covers
the two image-*generation* wire protocols in depth (`openai-responses` +
`image_generation` tool, and `ai-gateway-images`). This doc covers the
**type-system and stdlib** side: representation, plumbing, and the gap list.

---

## TL;DR

1. Media is a **first-class VM value** today (`image` / `audio` / `video` / `pdf`
   primitive types, backed by `Arc<MediaValue>`), fully usable from BAML source.
2. `sys_llm` (the Rust path) has **complete media input support** for 5 providers
   and **real media output support** for 4 providers, with return types `image`,
   `image[]`, `image?`, `(string|image)`, `(string|image)[]`.
3. The **native `ai.*` path supports neither**. `ai.Prompt` flattens media inputs
   to a debug placeholder string before a client can see them, and
   `ai.content.Block` has no media variant, so `ai.ModelTurn` cannot carry an
   image at all. The runner's only way to produce `Out` is
   `baml.sap.parse<Out>(text)`, and SAP **hard-rejects** media types.
4. Nothing about this is blocked by the language: classes hold `image` fields,
   union match narrowing works, `baml.json` round-trips media through a tagged
   `{kind,source,value,mime}` envelope, and `reflect.type_of<image>()` gives the
   runner a type test. The work is stdlib surface + runner branch, not compiler.

---

## 1. The current media representation map

### 1.1 Rust core: `MediaValue`

`crates/baml_builtins2/src/media.rs:20-29` defines the single runtime carrier:

```rust
pub struct MediaValue {
    pub random_id: usize,
    pub kind: MediaKind,
    mime_type: RwLock<Option<String>>,
    content: UnsafeCell<MediaContent>,
    content_rw_lock: RwLock<()>,
}
```

`MediaContent` (`crates/baml_builtins2/src/media.rs:164-177`) is a 3-variant enum
where `Url` and `File` carry an *optional pre-fetched* `base64_data`:

```rust
pub enum MediaContent {
    Url  { url: String,  base64_data: Option<String> },
    Base64 { base64_data: String },
    File { file: String, base64_data: Option<String> },
}
```

That `Option<String>` slot is the whole point of media resolution (§3): resolve
mutates the value in place via `write_content` (`media.rs:156-161`), and
`base64_data()` (`media.rs:184-190`) reads it back regardless of variant.

`MediaKind` is `baml_base` (`crates/baml_base/src/core_types.rs:222-231`):
`Image | Audio | Video | Pdf | Generic`. It carries the canonical JSON tag
(`tag_str`, `core_types.rs:235-243`) and the kind↔wrapper-class map
(`wrapper_class_name` / `from_wrapper_class_name`, `core_types.rs:251-273`).
`Generic` maps to no wrapper class and **has no BAML type keyword** — `media` is
not a type (`baml-cli run -e 'reflect.type_of<media>()'` → `E0002 unresolved type:
media`).

### 1.2 VM / value plumbing

- `bex_vm_types::MediaValue = Arc<baml_builtins2::MediaValue>`
  (`crates/bex_vm_types/src/types.rs:404-408`). Note the doc comment: *"Within
  `bex_vm`, media is now stored as `Object::Instance` with a `$rust_type` `_data`
  field"* — i.e. a media value at runtime is an ordinary class instance whose
  field 0 is `RustData(Arc<MediaValue>)`.
- Recognition is by class FQN: `media_kind_from_fqn`
  (`crates/bex_vm/src/package_baml/json.rs:1015-1022`) matches exactly
  `baml.media.{Image,Audio,Video,Pdf}`; `read_media_value`
  (`json.rs:1064-1084`) does instance → class-name → downcast.
- Crossing the FFI boundary it is `BexExternalValue::Adt(BexExternalAdt::Media(arc))`
  (`crates/bex_external_types/src/bex_external_value.rs:92`), typed as
  `RuntimeTy::Media(kind, attr)` (`crates/bex_engine/src/conversion.rs:505-510`).
  Inbound/outbound round-trip is regression-tested in
  `crates/bex_engine/tests/media_roundtrip.rs:1-40`.

### 1.3 BAML surface: `baml.media.*` and the `image`/`audio`/`video`/`pdf` types

`crates/baml_builtins2/baml_std/baml/ns_media/media.baml` declares four wrapper
classes, each `_data: $rust_type` plus four `//baml:vm` accessors and three
static constructors:

| class | line | accessors | constructors |
|---|---|---|---|
| `baml.media.Pdf`   | `media.baml:2`   | `url()->string?`, `file()->string?`, `base64()->string`, `mime_type()->string?` | `from_url`, `from_file`, `from_base64` → `pdf` |
| `baml.media.Audio` | `media.baml:46`  | same | → `audio` |
| `baml.media.Video` | `media.baml:90`  | same | → `video` |
| `baml.media.Image` | `media.baml:134` | same | → `image` |

Note the constructors return the **lowercase primitive type** (`-> image`,
`media.baml:162`), not the wrapper class; the wrapper is the nominal shell.
VM impls: `crates/bex_vm/src/package_baml/media.rs:20-...` (one block per kind).

Verified live (`target/debug/baml-cli`):

```
$ baml-cli run -e 'let i = baml.media.Image.from_base64("aGk=", "image/png");
                   `${i.mime_type() ?? "?"} ${i.base64()} ${i}`'
"image/png aGk= Image { _data: <rust_data> }"
```

**Gotcha:** `MediaValue::base64()` returns the **empty string** when no base64 is
available (`crates/baml_builtins2/src/media.rs:134-147`), so BAML code cannot
distinguish "unresolved URL media" from "empty payload". Any native resolver
must branch on `url()`/`file()` being non-null instead.

### 1.4 JSON envelope (BEP-038)

`serialize_media` (`crates/bex_vm/src/package_baml/json.rs:1033-1062`) emits
`{ kind, source, value, mime }` where `source ∈ {url, file, base64}` and `kind`
is `MediaKind::tag_str()`. Deserialization is `deserialize_media_by_kind`
(`json.rs:1235`, `json.rs:1400-1408`). Verified both directions:

```
$ baml-cli run -e 'baml.json.stringify(baml.json.to_json(
      baml.media.Image.from_base64("aGk=", "image/png")))'
"{\"kind\":\"image\",\"source\":\"base64\",\"value\":\"aGk=\",\"mime\":\"image/png\"}"

$ baml-cli run -e 'baml.json.from_string<image>(
      "{\"kind\":\"image\",\"source\":\"base64\",\"value\":\"aGk=\",\"mime\":\"image/png\"}").base64()'
"aGk="
```

This matters: `ai.events.FinalProduced` stores `baml.json.to_string(value)`
(`baml_std/ai/runner.baml:257-264`), so a media-bearing final value is already
journal-serializable with **zero new work**.

### 1.5 Prompt AST

`PromptAstSimple` (`crates/baml_builtins2/src/adt.rs:22-27`) is
`String | Media(Arc<MediaValue>) | Multiple(...)`, wrapped by
`PromptAst::{Simple, Message{role,content,metadata}, Vec}` (`adt.rs:7-20`).
This is the structural form every `sys_llm` provider builder consumes.

### 1.6 Old compiler (engine) — parity reference

`engine/baml-lib/baml-types/src/media.rs:7-45`:

```rust
pub enum BamlMediaType { Image, Audio, Pdf, Video }   // note: NO Generic
pub struct BamlMedia { media_type, mime_type: Option<String>, content: BamlMediaContent }
pub enum BamlMediaContent { File(MediaFile), Url(MediaUrl), Base64(MediaBase64) }
```

Differences from the new compiler worth knowing:

- Engine `MediaFile` carries `span_path` + `relpath`
  (`engine/baml-lib/baml-types/src/media.rs:99-108`) — file paths are resolved
  relative to the declaring `.baml` file. The new `MediaContent::File` carries a
  single flat `file: String`.
- Engine has no `Generic` kind and no in-place base64 back-patching; it resolves
  per-call inside each client.
- `mime_type_as_ok` in the engine defaults **PDF** to `application/pdf`
  (`engine/baml-lib/baml-types/src/media.rs:48-63`); the new
  `build_request::mime_type_as_ok`
  (`crates/sys_llm/src/build_request/mod.rs:226-232`) has **no such default** and
  errors on any missing MIME. *(Behavioural divergence already present in the
  Rust port; a native port should decide deliberately.)*

---

## 2. Media INPUTS per provider in `sys_llm`

All builders take `PromptAstSimple::Media(Arc<MediaValue>)` and lower it. Every
one calls `mime_type_as_ok` first (`build_request/mod.rs:226-232`) and hard-fails
without a MIME type.

| provider | entry | image | audio | video | pdf | role rule |
|---|---|---|---|---|---|---|
| OpenAI Chat Completions | `build_request/openai/chat_completions.rs:231-266` | `image_url` (URL passthrough, else `data:` URL) `:237-242` | `input_audio{data,format}`, wav/mp3 only `:243-249` + `:308-317` | **rejected** `:260-262` | `file{file_data: data-url}` `:250-259` | user-only, else `UnsupportedMedia` `:209-213`, `:225-229` |
| OpenAI Responses | `build_request/openai/responses.rs:175-208` | `input_image{image_url}` `:181-187` | `input_audio{data,format}` (wav/mp3/flac/ogg) `:188-192`, `:244-250` | **rejected** `:201-203` | `input_file{file_data}` `:193-200` | user-only `:153-157`, `:169-173` |
| Anthropic | `build_request/anthropic.rs:240-264` | `{"type":"image","source":…}` `:244-247` | `{"type":"audio",…}` `:248-251` | **rejected** `:255-257` | `{"type":"document",…}` `:252-254` | user-only `:219-224`, `:234-238` |
| Google (Gemini/Vertex) | `build_request/google.rs:273-303` | `inlineData` if base64 present, else `fileData{fileUri}` `:276-301` | same path | same path | same path | **no role restriction** — `gemini_parts` `:255-271` handles media in any message |
| AWS Bedrock | `build_request/bedrock.rs:280-345` | `ImageBlock` (S3 URI or raw bytes) `:289-299` | `AudioBlock`, bytes only, **S3 rejected** `:327-340` | `VideoBlock` `:300-310` | `DocumentBlock`, mime must be exactly `application/pdf` `:311-326` | system messages reject media outright `:174-177` |
| `ai-gateway-images` | `build_request/openai/images.rs:95-118` | **rejects all media input** — text prompts only `:104`, `:114-118` | — | — | — | — |

Bedrock additionally requires `s3://` for URL media (`bedrock.rs:196-212`) and
base64-decodes to raw `Blob` bytes (`bedrock.rs:213-232`).

`MediaKind::Generic` is rejected by every provider
(`chat_completions.rs:263-265`, `responses.rs:204-206`, `anthropic.rs:258-260`,
`bedrock.rs:341-343`).

### 2.1 Prompt specialization

`specialize_prompt` promotes media-bearing non-user messages to `user` when the
prompt has media and no user message at all, for providers that need it
(`crates/sys_llm/src/specialize_prompt/mod.rs:26-27, 43-...`;
`specialize_prompt/transformations.rs:87-101, 112-137`). This is a real
behavioural feature a native port must reproduce or consciously drop.

### 2.2 Engine parity for inputs

The engine implements the same media lowering per provider:
`engine/.../primitive/openai/openai_client.rs:134-200` (chat) and `:694-820`
(responses), `engine/.../primitive/anthropic/anthropic_client.rs:364-420`,
`engine/.../primitive/aws/aws_client.rs:108-115, 1141-1290`. So inputs have a
two-implementation ground truth.

---

## 3. `resolve_media.rs` — what it does and who calls it

`crates/sys_llm/src/resolve_media.rs` is a **pre-pass over the whole PromptAst**
that mutates every media node in place so the provider builder can assume base64
(or a passthrough URL) is present.

- Entry: `resolve_media(prompt, handler, io)` (`resolve_media.rs:73-79`), walking
  `PromptAst` → `PromptAstSimple` recursively (`:81-123`).
- **Only caller:** `build_request::build_request`
  (`crates/sys_llm/src/build_request/mod.rs:86-87`) —
  `MediaUrlHandler::from_client(client)` then `resolve_media(...)`. It is a
  build-request-time pass, not a value-construction-time pass.
- Per-node logic (`resolve_media.rs:125-148`): skip if base64 already present
  (unless strategy is `SendUrlAddMimeType`), else dispatch on content variant.
- `data:` URLs are parsed inline with no fetch (`:162-173`, parser `:316-327`).
- Strategies (`ResolveMediaUrls`, `:21-28`):
  - `SendUrl` → no-op (`:176`)
  - `SendBase64UnlessGoogleUrl` + `gs://` → no-op (`:177`)
  - `SendBase64` → HTTP GET, base64-encode, MIME from `Content-Type` else
    byte-sniff via the `infer` crate (`:178-193`, `:331-361`)
  - `SendUrlAddMimeType` → HEAD-ish GET; header MIME wins, else download and
    sniff (`:194-212`)
- Files: `io.fs_open` + `fs_file_bytes` → base64; MIME from **extension first**
  (`mime_from_extension`, `:292-312`, 15 extensions), then byte-sniff (`:259-290`).
- Non-2xx is a hard error (`:234-239`).

Per-provider defaults are applied in
`crates/sys_llm/src/baml_std.rs:289-355` (only when the user did not set
`media_url_handler`):

| provider | image | audio | video | pdf | line |
|---|---|---|---|---|---|
| openai / openai-generic / azure / ollama / openrouter / openai-responses / ai-gateway-images | `send_url` | `send_base64` | `send_url` | `send_url` | `baml_std.rs:298-303` |
| anthropic | `send_url` | `send_url` | `send_url` | `send_url` | `:305-310` |
| google-ai | `send_base64_unless_google_url` | `send_base64` | `send_base64` | `send_base64` | `:311-315` |
| vertex-ai (claude models) | `send_url` ×4 | | | | `:325-330` |
| vertex-ai (other) | `send_url_add_mime_type` | `send_url_add_mime_type` | `send_url` | `send_url` | `:332-337` |
| aws-bedrock | `send_base64` | `send_base64` | `send_url` | `send_base64` | `:340-345` |

The engine's equivalent lives in
`engine/baml-lib/llm-client/src/clientspec.rs:527-600` (`ResolveMediaUrls`,
`UnresolvedMediaUrlHandler`) and is wired per-client in
`engine/baml-lib/llm-client/src/clients/{google_ai,vertex}.rs`.

### 3.1 Can this be written in native BAML?

Mostly yes. Available primitives:

- `baml.http.fetch(url, timeout) -> baml.http.Response` (`baml/ns_http/http.baml:130`)
  and `Response.bytes() -> uint8array` / `Response.headers: map<string,string>`
  (`http.baml:16`).
- `baml.Uint8Array.to_base64()` / `from_base64()` (`baml/uint8array.baml`), plus
  `to_hex` for magic-byte checks.
- `baml.fs.read` / `baml.fs.write_bytes` (`baml/ns_fs/fs.baml:99, 109`).

Two real gaps:

1. **No MIME byte-sniffing.** `resolve_media.rs` uses the `infer` crate
   (`:208-210`, `:279-281`, `:358-360`). Native BAML would need a hand-written
   magic-byte table over `uint8array` (PNG/JPEG/GIF/WebP/PDF/MP3/WAV/MP4 covers
   the `mime_from_extension` set) or a new `//baml:vm` sniff builtin.
2. **`MediaValue` is immutable from BAML.** There is no `set_mime_type` /
   `set_base64` on `baml.media.Image`; a native resolver must *rebuild* a value
   with `Image.from_base64(b64, mime)` rather than back-patch, losing the
   original URL (`url()` becomes `null`). Either accept that, or add a
   `with_base64(...)`/`resolved(...)` constructor.

---

## 4. Media OUTPUTS today (`sys_llm`) — this part actually works

### 4.1 The output part model

`crates/sys_llm/src/parse_response/mod.rs:8-25`:

```rust
pub(crate) struct LlmOutput { pub parts: Vec<LlmOutputPart> }
pub(crate) enum LlmOutputPart {
    Text  { text: String },
    Media { media: Arc<MediaValue>, provider_id: Option<String>, metadata: serde_json::Value },
}
```

Ordered, interleaved text and media. `LlmProviderResponse.output` carries it
alongside the flattened `content: String` (`parse_response/mod.rs:96-110`).

### 4.2 Which providers emit media parts

| provider | source | file:line |
|---|---|---|
| `ai-gateway-images` | `images[]` → `Image.from_base64(b64, "image/png")` | `parse_response/openai/images.rs:15-57` (push at `:28-39`) |
| `openai-responses` | `image_generation_call.result` → base64 image, MIME from `output_format` (png/jpeg/webp) | `parse_response/openai/responses.rs:86-105`, mapper `:144-151` |
| OpenAI Chat Completions | `message.content[]` parts of type `image`/`image_url`/`output_image`, **and** the non-standard `message.images[]` array; `data:` URLs → base64, else URL media | `parse_response/openai/chat_completions.rs:201-229` (content), `:231-257` (images[]), helpers `:265-297` |
| Anthropic | `code_execution_tool_result` / `text_editor_code_execution_tool_result` file outputs, image MIME only, as `anthropic://files/{file_id}` URL media | `parse_response/anthropic.rs:97-121`, extractor `:128-168` |
| Google AI / Vertex | **none** — the `Part` struct only deserializes `text` and `thought` (`inlineData` is dropped) | `parse_response/google.rs:27-31`, `:56-67` |
| Bedrock | **none** — "We only extract text; other block types (image, toolUse, etc.) are skipped" | `parse_response/bedrock.rs:44` |

### 4.3 Request-side feature injection driven by the return type

`apply_output_request_features` (`crates/sys_llm/src/lib.rs:360-381`) inspects the
declared return type and mutates the request body:

- `openai-responses` → append `{"type":"image_generation"}` to `tools`, and set
  `tool_choice` when the image is required/available (`lib.rs:421-462`).
- `openai-generic` → add `"image"` to `modalities` (`lib.rs:383-419`).
- `image_generation_mode` (`lib.rs:465-507`) classifies the return type as
  `Required` (`image`, `image[]`, `image?`), `Available` (`string|image` unions,
  or unions where some members are non-media), or `Disabled`.

### 4.4 How a media part becomes a BAML value

`execute_parse_response_from_owned` (`lib.rs:542-584`) first tries
`parse_llm_output_for_target(return_type, &response.output)`
(`lib.rs:586-648`); only if that returns `None` does it fall back to SAP parsing
the text. Supported target shapes:

| return type | behaviour | file:line |
|---|---|---|
| `image` (any `RuntimeTy::Media(kind)`) | exactly one matching media part; mixed output rejected | `lib.rs:594-608` |
| `image[]` | all image parts; mixed output rejected | `lib.rs:609-621` |
| `(string \| image)[]` | ordered interleaved union items | `lib.rs:622-631`, builder `:769-820` |
| `image?` (`image \| null`) | 0 or 1 | `lib.rs:632-634`, `:650-685` |
| `image[]?` / `(string\|image)[]?` | nullable list | `lib.rs:635-637`, `:687-729` |
| `string \| image` | one item; multiple text parts collapse into one string | `lib.rs:638-645`, `:822-859` |

The terminal conversion is one line —
`BexExternalValue::Adt(BexExternalAdt::Media(media))` (`lib.rs:871-873`).

Error messages are deliberate and user-facing, e.g.
`"Expected only {kind} output parts, got {n} non-{kind} part(s). Use a text/image
union return type to preserve mixed outputs."` (`lib.rs:764-766`) and
`"Expected exactly one {kind} output, got {n}. Use {kind}[] for multiple
outputs."` (`lib.rs:602-605`).

### 4.5 Output-format prompt text for media returns

`media_output_instruction` (`crates/sys_llm/src/types/output_format.rs:728-748`)
replaces the schema block with a sentence. Verified live:

```
$ baml-cli run -e 'baml.prompt.render_output_format(reflect.type_of<image>())'
"Return an image output."
$ ... reflect.type_of<image[]>()      → "Return one or more image outputs."
$ ... reflect.type_of<string|image>() → "Return either text or an image output."
```

This is already reachable from BAML via `baml.prompt.render_output_format` /
`ai.wire.render_output_format` (`baml_std/ai/ns_wire/wire.baml:36-38`), so a
native client gets the right prompt text for free.

### 4.6 Streaming

No media in the streaming path. `stream_accumulator.rs` folds text only
(`get_content -> String`, `crates/sys_llm/src/stream_accumulator.rs:253-263`);
`extract_delta` (`:149-252`) has no media branch. Native
`ai.stream.StreamEvent = TextDelta | TurnMeta | TurnDone`
(`baml_std/ai/ns_stream/stream.baml:53`) likewise.

### 4.7 Engine parity: none

`engine/baml-runtime/src/internal/llm_client/mod.rs:302-312` —
`LLMCompleteResponse.content: String`. Grep over `engine/` for
`image_generation`, `LlmOutputPart`, `push_media`, `output_image`, `modalities`
returns **zero hits**. Media *outputs* are a new-compiler-only feature (added in
PR #3481, "re add baml image inputs and outputs"). **`sys_llm` is the sole parity
reference for outputs.**

---

## 5. The native `ai.*` path — the gap list

### 5.1 Media inputs are destroyed before a client sees them

The structural media survives *into* `ai.Prompt`: `PromptContentSink::try_push_special`
(`crates/bex_vm/src/package_baml/prompt.rs:51-57`) calls `read_media_value` and
pushes a `PromptAstSimple::Media` node, and `assemble_prompt`
(`baml_std/ai/ns_internal/helpers.baml:27`, impl `prompt.rs:240-252`) builds a
real `PromptAst`. So the data is there.

But the **only** BAML accessors are (`baml_std/ai/spec.baml:18-38`):

```baml
class Prompt {
    _data: $rust_type,
    function text(self) -> string           // media → placeholder
    function messages(self) -> PromptMessage[]
}
class PromptMessage { role: string, content: string }   // spec.baml:7-13
```

`messages()` (`prompt.rs:216-233`) calls `PromptAst::to_messages()`
(`crates/baml_builtins2/src/adt.rs:89-104`), which calls
`PromptAstSimple::to_text()` (`adt.rs:129-137`) — and that renders media via its
`Display` impl (`crates/baml_builtins2/src/media.rs:209-237`).

Confirmed empirically. Project `baml_src/main.baml` with
`function Describe(img: image) -> string { client: "openai/gpt-5" prompt: `Describe ${img}` }`,
then:

```baml
test "media in prompt" {
    let img = baml.media.Image.from_base64("aGk=", "image/png");
    let p = Describe$spec(img).prompt(output_format = "");
    assert.equal(`n=${p.messages().length()} text=<${p.text()}>`, "SHOWME")
}
```

→ `left = n=1 text=<Describe image::base64(aGk=, len=4)>`.

**The model would literally receive the string `image::base64(aGk=, len=4)`.**

All three built-in native clients consume `message.content` as a string:
`baml_std/openai/ns_internal/responses.baml:38-49`,
`baml_std/anthropic/ns_internal/messages.baml:53-56`,
`baml_std/google/ns_internal/gemini.baml:82-83`. Blast radius for changing the
prompt surface is exactly those 3 call sites.

### 5.2 `ai.ModelTurn` cannot carry media

`baml_std/ai/ns_content/content.baml:23`:

```baml
type Block = Text | Reasoning | ToolUse;
```

`ai.ModelTurn { content: Block[], stop_reason, usage }`
(`baml_std/ai/turn.baml:15-41`) — no media variant, and `terminal_text()`
(`turn.baml:20-29`) only scans `Text` blocks. `ai.events.AssistantMessage.content`
is the same `Block[]` (`baml_std/ai/ns_events/events.baml:15-18`), so the journal
inherits the gap.

### 5.3 The runner's only output path is SAP, and SAP rejects media

`ai.Agent.run` (`baml_std/ai/runner.baml:250-267`) does:

```baml
let candidate = turn.terminal_text() ?? "";
let value = baml.sap.parse<Out>(candidate) catch_all (e) { ... };
```

and `baml.sap.parse<image>` throws:

```
$ baml-cli run -e 'baml.sap.parse<image>("{\"kind\":\"image\",\"source\":\"base64\",\"value\":\"aGk=\",\"mime\":\"image/png\"}")'
uncaught throw: baml.errors.LlmClient {message: "<root>: Image type is not supported here"}
```

Source: `crates/bex_sap/src/deserializer/coercer/mod.rs:190-196` (and `:198-220`
for Audio/Pdf/Video). Note also `crates/bex_sap/src/to_external.rs:189-190`:
`BamlValue::Media(_) => unimplemented!("Media value conversion to BexExternalValue
is not yet implemented")`. The `_parses` pre-check in the repair loop
(`runner.baml:99-106`) uses the same `baml.sap.parse<Out>`, so an `image` return
type would make **every turn look unparseable**, burn the repair attempt, and
then throw `ai.errors.ParseFailed`.

### 5.4 The desugar already routes `-> image` here

`synthesize_spec_agent_run_body`
(`crates/baml_compiler2_ast/src/lower_expr_body.rs:714-800`) emits
`ai.Agent<Out>.new(client = client).run(Fn$spec(...)).value` for every LLM
function, `Out` unrestricted. `function DrawIt(what: string) -> image { ... }`
**compiles clean** (`baml-cli check` → `Finished checked 1 file(s)`) and then
fails at runtime in §5.3. That is the worst failure mode: statically accepted,
dynamically broken.

### 5.5 No native request-side image-generation injection

The `image_generation` tool / `modalities` injection of `lib.rs:360-462` has no
native counterpart. `baml_std/openai/ns_internal/responses.baml` builds its body
without ever consulting `input.output_type` for media (see the sibling doc §1).

### 5.6 Tool results are strings

`ai.tools.Tool.call(self, args) -> string` (`baml_std/ai/ns_tools/tools.baml:10`,
confirmed via `baml describe ai.tools.Tool`). A tool cannot return an image, and
`ai.events.ToolCompleted.output` is a `string`
(`baml_std/ai/ns_events/events.baml:26-29`). Out of scope for the first pass, but
worth recording.

---

## 6. What the language already supports (nothing here needs compiler work)

Verified in a scratch project (`baml-cli test`):

```baml
class Shot { caption: string, pic: image }
class MediaBlock { media: image }
type Blk = MediaBlock | string;

test "media in classes" {
    let img = baml.media.Image.from_base64("aGk=", "image/png");
    let j = baml.json.stringify(baml.json.to_json(Shot { caption: "c", pic: img }));
    let blks: Blk[] = [MediaBlock { media: img }, "hi"];
    let n = 0;
    for (let b in blks) { match (b) { let m: MediaBlock => { n = n + 1; }, _ => { n = n; }, }; }
    assert.equal(`${j}|n=${n}`, "SHOWME")
}
```

→ `left = {"caption":"c","pic":{"kind":"image","source":"base64","value":"aGk=","mime":"image/png"}}|n=1`

So: **class fields of type `image` work; union match narrowing over a
media-carrying class works; `baml.json.to_json` serializes it.**

Also available to a native runner:

- `reflect.type_of<T>()` returns a `type` with `.to_string()` and `==`
  (`baml_std/reflect/reflect.baml:15-17`). Verified:
  `reflect.type_of<image>() == reflect.type_of<image>()` → `true`,
  `... == reflect.type_of<string>()` → `false`,
  `reflect.type_of<image[]>().to_string()` → `"image[]"`.
- `ai.FunctionSpec.output_type() -> type` (`baml_std/ai/spec.baml:76-78`) already
  hands the runner the declared return type.
- `baml.media.Image.from_base64(...)` is callable from ordinary BAML — a native
  client can *construct* the returned image itself.

---

## 7. Recommendation

### 7.1 Shape of the fix (four pieces, in dependency order)

**(A) `ai.content` gets a media block.**

```baml
// baml_std/ai/ns_content/content.baml
class Media {
    image: image?,          // exactly one of these is non-null
    audio: audio?,
    video: video?,
    pdf: pdf?,
    provider_id: string?,   // mirrors LlmOutputPart::Media.provider_id
    metadata: map<string, unknown>,
}
type Block = Text | Reasoning | ToolUse | Media;
```

*Why a single class with four nullable slots rather than four classes*: BAML has
no union of primitive media types short of `image | audio | video | pdf`, and a
four-member union inside a class field is fine but forces every consumer to
re-match. A single `Media` block with a `kind`-style discriminator keeps
`ModelTurn.content` a flat `Block[]` and keeps `match` arms to one.

*Alternative worth considering*: `class Media { content: image | audio | video | pdf, ... }`
— a nested union is cleaner to consume (`match (m.content) { let i: image => ... }`)
and I confirmed union-of-media typing works in the value system. Pick one
early; it leaks into every provider decoder.

Add to `ai.ModelTurn` (`turn.baml`), mirroring `tool_uses()`:

```baml
function media(self) -> root.content.Media[]   // all Media blocks, in order
```

**(B) `ai.Prompt` exposes structural parts.** Keep `messages()` for
backward-compat, add a structural accessor so clients can lower media:

```baml
// ai/spec.baml
class PromptPart { text: string?, media: root.content.Media? }  // or a real union
class PromptMessage { role: string, content: string, parts: PromptPart[] }
```

VM side: extend `BamlClassPrompt::messages`
(`crates/bex_vm/src/package_baml/prompt.rs:216-233`) to walk `PromptAstSimple`
instead of calling `to_text()`, allocating a `baml.media.*` instance per
`PromptAstSimple::Media` node. `PromptAst::to_messages`
(`crates/baml_builtins2/src/adt.rs:89-104`) stays as the text-only path for
`render_text`. Then update the 3 client lowerers
(`openai/ns_internal/responses.baml:38-49`, `anthropic/ns_internal/messages.baml:53-56`,
`google/ns_internal/gemini.baml:82-83`) to emit provider media parts — porting
the tables in §2.

**(C) The runner branches on the output type instead of always SAP-parsing.**
In `ai/runner.baml`, before `baml.sap.parse<Out>(candidate)` (`runner.baml:252`),
add a media path guarded by `spec.output_type()`:

```baml
// sketch
let t = spec.output_type();
if (t == reflect.type_of<image>() || t == reflect.type_of<image[]>() || ...) {
    // build Out from turn.media() rather than from text
}
```

This is the only genuinely awkward piece: BAML's `type` values compare by
equality, so the runner needs an explicit list of accepted media shapes rather
than the structural walk `image_generation_mode`/`parse_llm_output_for_target`
does in Rust (`lib.rs:465-507`, `:586-648`). Two options:

1. **Enumerate** the six supported shapes (`image`, `image[]`, `image?`,
   `image[]?`, `string|image`, `(string|image)[]`) × 4 media kinds — verbose but
   pure BAML, no new builtins. `image` alone covers the real demand.
2. **Add a small reflection builtin** — e.g. `baml.type.media_kind(t) -> string?`
   or expose `t.is_media()` / `t.element_type()` on `type`. Cleaner, and it also
   unblocks `_parses` (`runner.baml:99`), which must *not* call SAP for a media
   `Out`. I recommend (2); it is ~30 lines of Rust against the existing
   `RuntimeTy` and removes a combinatorial table from the stdlib.

Also gate the repair loop: for a media `Out`, `_parses` should test
"did the turn produce ≥1 media block of the right kind", not "does the text
parse".

**(D) Request-side injection stays in the client, not the runner.** Each native
client reads `input.output_type` (already on `ai.ModelTurnInput`,
`baml_std/ai/turn.baml:6-13`) and adds `tools:[{type:"image_generation"}]` /
`modalities:["image","text"]` itself — porting `lib.rs:383-462`. That keeps the
runner provider-agnostic, which is the stated design of the `ai` namespace
(`ai/turn.baml:1-4`).

### 7.2 How an image-generation client should return images to user code

**Recommended:** the client parses the provider envelope, constructs the image
with `baml.media.Image.from_base64(b64, mime)` in BAML, and returns it as a
`ai.content.Media` block inside `ai.ModelTurn.content`. The runner, seeing
`Out == image`, pulls the single media block out and returns it. User code then
has a real `image` value:

```baml
function DrawLamp(subject: string) -> image {
    client: OpenAiImageClient
    prompt: `Draw ${subject}. ${ctx.output_format}`   // → "Return an image output."
}

test "save it" {
    let img = DrawLamp("a brass desk lamp");
    baml.fs.write_bytes("/tmp/lamp.png", baml.Uint8Array.from_base64(img.base64()))
}
```

This preserves exactly the semantics `sys_llm` already ships
(`lib.rs:594-608`), keeps `image[]` / `string|image` reachable later, keeps the
journal serializable for free (§1.4), and requires **no new VM value type** —
`Arc<MediaValue>` and `BexExternalAdt::Media` already exist end to end
(`crates/bex_engine/tests/media_roundtrip.rs`).

**Rejected alternatives:**

- *Return a base64 `string` and make the user call `Image.from_base64`.* Loses
  the type, loses `image[]`, breaks parity with the Rust path, and makes
  `${ctx.output_format}`'s "Return an image output." a lie.
- *Teach SAP to coerce `{kind,source,value,mime}` JSON into media.* Tempting
  because `baml.json.from_string<image>` already does it (§1.4), and it would
  make `class Shot { caption: string, pic: image }` an LLM return type. But the
  model does not emit that envelope for a generated image — the image arrives on
  a *separate wire channel* (`image_generation_call.result`,
  `images[]`), not in the text. SAP coercion solves a different problem
  (structured outputs *containing* media) and should be a later, separate BEP.
  Worth flagging: today `bex_sap` explicitly refuses all four media kinds
  (`coercer/mod.rs:190-220`) and `to_external.rs:189` is an `unimplemented!()`.
- *A new `baml.Media` VM value type.* Unnecessary — it exists
  (`bex_vm_types/src/types.rs:404-408`).

### 7.3 Migration ordering suggestion

1. **(A)** `ai.content.Media` + `ModelTurn.media()` — pure BAML, no VM change.
2. **(C)** runner branch + the `type` reflection helper — unblocks `-> image`.
3. **(D)** per-client request injection for `openai-responses` and the gateway
   images client (see sibling doc for the wire details).
4. **(B)** structural prompt parts + per-provider input lowering — the biggest
   chunk, and the one with a two-implementation parity reference (§2.2).
5. Media resolution (§3) — port `resolve_media` last, and decide whether it lives
   in BAML (needs a MIME-sniff helper and a media rebuild constructor) or stays a
   `//baml:vm` builtin the clients call.
6. Explicitly **out of scope / deferred**: Google + Bedrock media *outputs*
   (`sys_llm` has none — `parse_response/google.rs:27-31`,
   `parse_response/bedrock.rs:44`), streaming media (§4.6), media tool results
   (§5.6), media inside SAP-parsed structured outputs (§7.2).

### 7.4 Behaviours a native port must not silently drop

- Role restrictions and their error text (§2) — user-only media on OpenAI/Anthropic,
  Bedrock system-message rejection.
- Video rejection on OpenAI + Anthropic (`chat_completions.rs:260`,
  `responses.rs:201`, `anthropic.rs:255`).
- The missing-MIME hard error (`build_request/mod.rs:226-232`) — and the engine's
  PDF default that the Rust port already dropped (§1.6).
- The `promote_media_to_user_when_no_user_message` transform
  (`specialize_prompt/transformations.rs:87-101`).
- The mixed-output error messages (`lib.rs:602-605`, `:764-766`, `:855-857`) —
  they are the documented UX for "you asked for `image`, the model also talked".
- The `Generic` media kind rejection on every provider.
