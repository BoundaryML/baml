# OpenAI Responses API + image generation: sys_llm vs. native BAML

Research for the `sys_llm` → `baml_std` migration. Scope: the `openai-responses`
provider, the `ai-gateway-images` provider, and image output in general.

All paths are relative to `/Users/aaron/projects/baml/baml_language` unless
prefixed with `engine/`, which means `/Users/aaron/projects/baml/engine`.

---

## 0. Two corrections to the brief (read first)

**(a) There is no `/v1/images/generations` implementation anywhere in this repo,
and none in the engine.** The brief asks to port "sys_llm's images.rs" as an
OpenAI `/v1/images/generations` client for `gpt-image-1` / `dall-e-3`. That is not
what `build_request/openai/images.rs` is. Its module doc says so on line 1–3:

> `crates/sys_llm/src/build_request/openai/images.rs:1` — "Vercel AI Gateway
> image-model body builder. Mirrors the Vercel AI SDK gateway image model
> endpoint at `/v4/ai/image-model`."

It lives under `build_request/openai/` only because it shares the OpenAI bearer
auth path (`crates/sys_llm/src/auth_request/mod.rs:32-38`). It POSTs to
`{base_url}/image-model` where the base URL defaults to
`https://ai-gateway.vercel.sh/v4/ai` (`crates/sys_llm/src/baml_std.rs:223`), and
its response envelope is `{ images: string[], providerMetadata }`
(`crates/sys_llm/src/parse_response/openai/images.rs:7-13`) — not
`{ data: [{ b64_json, revised_prompt }], usage }`.

Grep confirms: no hit for `images/generations`, `gpt-image`, `dall-e`,
`b64_json` in `crates/` or in `engine/` (only `revised_prompt` appears, and only
in `parse_response/openai/responses.rs:40`).

So there are exactly **two** image-generation paths to port, neither of which is
the OpenAI Images endpoint:

1. `openai-responses` + the built-in `image_generation` **tool**, auto-enabled
   from the return type (`crates/sys_llm/src/lib.rs:421-462`).
2. `ai-gateway-images` — the Vercel AI Gateway `image-model` endpoint.

If a true `/v1/images/generations` client is wanted, it is **new work with no
ground truth to port**; §4 sketches it separately and flags it as such.

**(b) The engine has no image generation at all.** `grep -rn
"ai-gateway\|image-model\|image_generation\|modalities" --include="*.rs"` over
`engine/` returns nothing. The engine *does* implement `openai-responses`
(§3), so the Responses API has a parity reference; image generation does not.

---

## 1. Native `openai/responses.baml` — gap list vs. sys_llm

Files under comparison:

- native: `crates/baml_builtins2/baml_std/openai/responses.baml` (73 lines),
  `crates/baml_builtins2/baml_std/openai/ns_internal/responses.baml` (315 lines)
- sys_llm build: `crates/sys_llm/src/build_request/openai/responses.rs`
- sys_llm parse: `crates/sys_llm/src/parse_response/openai/responses.rs`
- sys_llm feature injection: `crates/sys_llm/src/lib.rs:360-462`

### 1.1 Where the native client is AHEAD of sys_llm

Worth stating up front so the migration does not regress these.

| Feature | Native | sys_llm |
|---|---|---|
| Tool definitions built from the toolbox | `ns_internal/responses.baml:137-153` builds `tools[]` from `input.toolbox.list()` with `{type:"function", name, description, parameters, strict:false}` and sets `tool_choice:"auto"` | **absent** — `responses.rs:62-67` only has `model`/`input` + flattened `extra_body`; tools must be hand-written into `request_body` |
| Tool *results* fed back | `openai_lower_journal` (`ns_internal/responses.baml:56-114`) emits `function_call` and `function_call_output` items from the journal | **absent** — sys_llm has no journal concept; one-shot only |
| `ToolUse` as structured content | `openai_parse` pushes `ai.content.ToolUse{id,name,args}` (`ns_internal/responses.baml:176-182`), args JSON-decoded | `parse_response/openai/responses.rs:74-86` stuffs the call into a **text** part as a JSON string `{"type":"function_call",...}` |
| `reasoning` output items | handled — `item.type == "reasoning"` → `ai.content.Reasoning{summary}` from `item.summary[].text` (`ns_internal/responses.baml:194-203`) | **dropped** — `ResponseOutputType` has no `Reasoning` variant, so it deserializes to `Unknown` and is skipped (`parse_response/openai/responses.rs:18-26`, `:106`) |
| Streaming | real SSE decoding of `response.output_text.delta` + terminal events (`ns_internal/responses.baml:246-315`) | **rejected** — `stream_accumulator.rs:69-88` explicitly excludes `OpenAiResponses` with the comment "its streaming format … extract_delta() does not yet handle that shape" |
| `store: false` | set (`ns_internal/responses.baml:136`) | never set → OpenAI defaults to `store: true`, i.e. sys_llm silently persists every response server-side |

### 1.2 Gaps — native BEHIND sys_llm

**G1. No `request_body` / `extra_body` passthrough at all.**
sys_llm flattens the whole client `request_body` map into the top level of the
Responses body: `RequestBody { model, input, #[serde(flatten)] extra }`
(`build_request/openai/responses.rs:61-67`, `:80`), populated from
`options.request_body` in `baml_std.rs:104-112`. That is the *only* way a user
sets `temperature`, `max_output_tokens`, `top_p`, `reasoning: {effort}`,
`text: {format}`, `parallel_tool_calls`, `truncation`, `metadata`, `service_tier`,
`previous_response_id`, `include`, custom `tools`, or a custom `tool_choice`.

The native `OpenAiClient` has exactly five fields — `model`, `api_key`,
`base_url`, `api_key_env`, `base_url_env` (`openai/responses.baml:1-13`) — and
`_openai_request` writes a fixed body (`ns_internal/responses.baml:133-159`).
**There is no user-controllable knob whatsoever.** No temperature, no token cap,
no reasoning effort. Compare `AnthropicClient`, which at least carries
`max_tokens: int` (`anthropic/messages.baml:7`, used at
`anthropic/ns_internal/messages.baml:192`). This is the single biggest gap.

**G2. No `headers` / `query_params` passthrough.** sys_llm applies
`client.options.headers` lowercased over the provider defaults and appends
`query_params` percent-encoded (`build_request/mod.rs:113-127`). Native hardcodes
exactly two headers (`ns_internal/responses.baml:163`).

**G3. Media INPUT is structurally impossible on the native path.** sys_llm lowers
each prompt part into a typed Responses content part
(`build_request/openai/responses.rs:26-54`, `:175-208`):

- `MediaKind::Image` → `{"type":"input_image","image_url": <url or data: URL>}`
- `MediaKind::Audio` → `{"type":"input_audio","data":<b64>,"format":<from mime>}`
  with mime→format mapping at `:245-254`
- `MediaKind::Pdf` → `{"type":"input_file","file_data": <url or data: URL>}`
- `MediaKind::Video` → error `:201`; `Generic` → error `:204`
- media in a non-`user` role → error `:154-156`, `:169-173`

The native client cannot do any of this, because `ai.Prompt.messages()` returns
`ai.PromptMessage { role: string, content: string }`
(`ai/spec.baml:7-13`) — and its own docstring admits "Media parts use a readable
placeholder." The flattening happens in Rust:
`PromptAst::collect_messages` calls `PromptAstSimple::to_text()`
(`crates/baml_builtins2/src/adt.rs:96-107`, `:129-137`), where a media node
becomes `media.to_string()`, i.e. a Display placeholder like `image::url(...)`.
`openai_lower_prompt` then sets that string as `content`
(`ns_internal/responses.baml:36-52`).

**This is not a client-level gap — it is a missing stdlib capability.** Fixing it
requires a structural accessor on `ai.Prompt` (e.g. a `parts()` returning a
`string | image | audio | pdf` union) plus a Rust-side `messages()` variant that
does not stringify. Every native client (anthropic, google too) has the same
hole; nothing in `baml_std/anthropic/` or `baml_std/google/` mentions media.

**G4. `input_text` vs `output_text` role-dependent content typing is lost.**
sys_llm emits `output_text` for assistant content and `input_text` otherwise
(`build_request/openai/responses.rs:145-152`, tested at `:490-510`). Native
passes a bare string as `content`, which the API accepts but which loses the
assistant/user distinction for multi-part content.

**G5. Message metadata (`@@meta`) is dropped.** sys_llm flattens
`PromptAst::Message.metadata` onto the message object
(`build_request/openai/responses.rs:19-21`, `:126-135`), gated by
`ModelFeatures::allowed_metadata` (`model_features.rs:20-39`; Responses defaults
to `AllowedMetadata::All`, `model_features.rs:60-70`). Native has no metadata
path.

**G6. Prompt specialization is not applied.** sys_llm runs the whole
`specialize_prompt` pipeline before building
(`specialize_prompt/mod.rs:19-41`): wrap-simple-as-message with the client's
`default_role` (`"system"` for openai-responses, `baml_std.rs:293-300`), promote
media to a user message (`specialize_prompt/mod.rs:44-54` includes
`OpenAiResponses`), merge adjacent same-role messages, consolidate system
prompts, validate roles against `allowed_roles`, remap roles, filter metadata.
Native does a single ad-hoc substitution: empty role → `"user"`, `"tool"` →
`"user"` (`ns_internal/responses.baml:38-44`). Note the *defaults differ*: sys_llm
role-less content becomes `system`, native becomes `user`.

**G7. Image OUTPUT (`image_generation_call`) is not parsed.**
sys_llm handles it (`parse_response/openai/responses.rs:87-105`): reads
`item.result` (base64), maps `item.output_format` (`png|jpeg/jpg|webp`) to a mime
type via `image_output_format_to_mime_type` (`:144-151`, default `image/png`),
and pushes a media part carrying `revised_prompt` + `output_format` metadata.
`openai_parse` in native only looks at `function_call`, `message`, `reasoning`
(`ns_internal/responses.baml:174-207`) — an `image_generation_call` item is
silently dropped, and `ai.content.Block` has nowhere to put it anyway (§4.1).

**G8. The `image_generation` tool is never requested.**
sys_llm derives an `ImageGenerationMode` from the *return type*
(`lib.rs:465-508`):

- `image` → `Required`
- `T[]` → recurse into `T`
- a `string | image` (± `null`) union → `Available` (`types/output_format.rs:771-788`)
- other unions: `Required` if every non-null member is image-ish and there were
  no nullable members, else `Available`, else `Disabled`

and for `Disabled != mode` it injects into the body
(`lib.rs:421-462`): push `{"type":"image_generation"}` into `tools[]` if not
already present, and set `tool_choice: {"type":"image_generation"}`. There is no
equivalent in the native client — and no natural place for it, since native
builds `tools` from the toolbox only.
(Sibling behavior for `openai-generic`: append `"image"` to `modalities`,
`lib.rs:376-419`. Out of scope here but the same missing hook.)

**G9. Usage fields are partly lost.** sys_llm maps
`prompt_tokens`/`input_tokens` → input, `completion_tokens`/`output_tokens` →
output, plus `total_tokens` and `input_tokens_details.cached_tokens`
(`parse_response/openai/responses.rs:119-132`; the serde aliases are in
`parse_response/openai/mod.rs`, `CompletionUsage`). Native's `OaUsage` has only
`input_tokens: int, output_tokens: int` (`ns_internal/responses.baml:21-24`) and
hardcodes `cached_input_tokens: null, reasoning_tokens: null`
(`ns_internal/responses.baml:216-225`), even though `ai.events.Usage` has fields
for both (`ai/ns_events/events.baml:36-41`). `output_tokens_details.reasoning_tokens`
is read by neither.

Secondary risk: `OaUsage`'s two fields are non-optional `int`, so a response
whose `usage` object omits either one fails `baml.json.from_string` and surfaces
as `ai.errors.ParseFailed` (`ai/ns_wire/wire.baml:29-31`).

**G10. `finish_reason_allow_list` / `deny_list` is not enforced.** sys_llm checks
it after parsing (`lib.rs:560-570`, implementation
`baml_std.rs:124-142`). No native equivalent.

**G11. `incomplete_details.reason` is ignored on both sides, but the engine reads
it.** See §3.

**G12. Streaming event coverage is thin.** Native decodes only three event names:
`response.output_text.delta`, and the terminal trio
`response.completed` / `response.incomplete` / `response.failed`
(`ns_internal/responses.baml:262-288`). Missing relative to the engine's
`ResponsesApiStreamEvent` enum
(`engine/baml-runtime/src/internal/llm_client/primitive/openai/types.rs:95-152`):
`response.created`, `response.in_progress` (engine uses these to set the model),
`response.output_text.done`, `response.content_part.added/done`. Also unhandled
anywhere: `response.output_item.added/done` (needed to stream tool calls),
`response.function_call_arguments.delta/done`, and
`response.image_generation_call.partial_image`.

Two more streaming defects in the native client:

- `response.failed` is treated as a *successful* terminal event — it runs
  `openai_parse` and emits `TurnDone` (`ns_internal/responses.baml:270-285`)
  instead of throwing. The engine raises an `LLMErrorResponse` when
  `response.error` is present
  (`engine/…/openai/response_handler.rs:425-443`).
- `invoke_stream` never classifies a non-2xx status: `baml.http.fetch_sse` is
  wrapped in `catch_all` → `ai.errors.NetworkFailure`
  (`ns_internal/responses.baml:305-313`), so a 401/429 from the SSE endpoint is
  reported as a transport failure. The non-streaming path does it correctly via
  `ai.wire.send_as` → `ai.errors.classify_http` (`ai/ns_wire/wire.baml:26-28`).
- `ai.stream.TurnStream.final_turn` reconstructs the turn as a single
  `ai.content.Text` block (`ai/ns_stream/stream.baml:214-238`), so streamed tool
  calls and reasoning can never reach the caller regardless of decoding.

**G13. Reasoning items are not echoed back.** `openai_lower_journal` explicitly
drops `ai.Reasoning` on the way out (`ns_internal/responses.baml:86` — "ai.Reasoning
lowers to nothing in this phase"). For reasoning models the Responses API expects
prior `reasoning` items (or their encrypted form) to be replayed in `input`;
without them multi-turn tool loops lose reasoning context. sys_llm never had this
either (no journal), so it is a forward gap, not a regression.

**G14. No `parallel_tool_calls`, no strict/structured tool schemas, no custom
`tool_choice`.** Native hardcodes `strict: false` and `tool_choice: "auto"`
(`ns_internal/responses.baml:145-149`). sys_llm can express any of these only
through `request_body`, which native lacks (G1).

**G15. Structured output is SAP-only on BOTH sides — this is parity, not a gap.**
Neither implementation ever sets `text.format` / `response_format` /
`json_schema`. sys_llm's Responses builder writes only `model` + `input` + extra
(`build_request/openai/responses.rs:61-67`), and `apply_output_request_features`
touches only image modalities (`lib.rs:360-378`). Native injects the schema as
prompt text via `ai.wire.render_output_format(input.output_type)`
(`ns_internal/responses.baml:128`, `ai/ns_wire/wire.baml:36-38`) and the runner
parses the terminal text with `baml.sap.parse<Out>`
(`ai/runner.baml:148`, `:251-252`). Keep it that way; do not "fix" it during the
port.

**G16. Provider-level plumbing that exists only in sys_llm.**
`api_key` fallback to `OPENAI_API_KEY` exists on both
(`provider.rs:62-68` vs. `openai/responses.baml:33-43` — native is arguably nicer
with `api_key_env`). But `base_url` defaulting
(`baml_std.rs:220-222` → `https://api.openai.com/v1`, native hardcodes the same
literal at `ns_internal/responses.baml:162`) and the WASM/playground proxy rewrite
via `BOUNDARY_PROXY_URL` (`build_request/mod.rs:128+`) have no native equivalent —
the playground will not work with native clients until that is ported.

---

## 2. `ai-gateway-images` — full spec to port

This is the one image provider with real ground truth. It is small; the whole
thing is ~120 lines of Rust.

### 2.1 Request (`crates/sys_llm/src/build_request/openai/images.rs:15-56`)

- **Method**: `POST`
- **URL**: `{base_url}/image-model`, with `base_url` trailing-slash-trimmed
  (`:44-52`). Default `base_url` = `https://ai-gateway.vercel.sh/v4/ai`
  (`crates/sys_llm/src/baml_std.rs:223`), so the effective default endpoint is
  `https://ai-gateway.vercel.sh/v4/ai/image-model` (asserted at
  `build_request/openai/images.rs:188-191`).
- **Headers** (`:19-29`):
  - `content-type: application/json`
  - `ai-gateway-protocol-version: 0.0.1`
  - `ai-image-model-specification-version: 4`
  - `ai-model-id: <client.model>` — the model is a **header**, not a body field
  - added by auth (`auth_request/mod.rs:92-110`):
    `authorization: Bearer <api_key>` and `ai-gateway-auth-method: api-key`
  - then user `headers` overlaid lowercased (`build_request/mod.rs:113-116`)
- **Body**: `{ "prompt": <string>, ...request_body }` — `prompt` plus the
  flattened `extra_body` (`:8-13`, `:36-39`).
  - `n` is defaulted to `1` when the user did not set it (`:31-34`, tested
    `:206-216`).
  - Everything else is user-supplied, e.g. `providerOptions: { blackForestLabs:
    { outputFormat: "jpeg" } }` (test at `:169-204`). These are Vercel AI SDK
    image-model options (`size`, `aspectRatio`, `seed`, `providerOptions`), *not*
    OpenAI Images fields.
- **Prompt flattening** (`:58-112`): walk the whole `PromptAst` (Vec / Message /
  Simple), collect every string, `trim()` each, drop empties, join with `"\n\n"`.
  Roles are discarded entirely. Empty result → error "Images API request requires
  a text prompt" (`:70-74`). Any media part → `UnsupportedMedia` error
  "Images API only supports text prompts; found {kind} input" (`:104`, `:114-118`).
  So image-to-image / editing is **not** supported today.
- **Auth key resolution**: `resolve_api_key` uses `options.api_key` and falls back
  to `provider.default_api_key_env_var()`, which returns `None` for
  `AiGatewayImages` (`provider.rs:62-68`). So there is **no env fallback** — users
  must write `api_key: env.AI_GATEWAY_API_KEY` explicitly (as in the LSP fixture
  `.claude/worktrees/…/baml_lsp2_actions_tests/test_files/semantic_tokens/ns_images_pipeline.baml:5`).
  A native client should keep the explicit form and may add
  `AI_GATEWAY_API_KEY` as the conventional default.

### 2.2 Response (`crates/sys_llm/src/parse_response/openai/images.rs:7-57`)

```jsonc
{ "images": ["<base64>", "..."], "providerMetadata": { /* opaque */ } }
```

- Each element of `images[]` is raw base64 (no data URL prefix) →
  `MediaValue::from_base64(Image, b64, Some("image/png"))` (`:27-39`). Mime is
  **hardcoded** to `image/png` even when `providerOptions.outputFormat` asked for
  jpeg — a real (small) bug worth fixing in the port.
- Per-image metadata attached: `{"provider":"ai-gateway-images","providerMetadata":…}`.
- Empty `images[]` → `ParseResponseError::NoContent` "response contained no
  images in images[]" (`:42-47`).
- `LlmProviderResponse`: `content: ""`, `model: None`,
  `finish_reason: Stop` / raw `"stop"`, `usage: TokenUsage::default()` (all None)
  (`:49-56`). The gateway's own usage/cost data is not read.

### 2.3 Provider-table entries to reproduce

For `AiGatewayImages`, from `crates/sys_llm/src/`:

| Behavior | Location | Value |
|---|---|---|
| default base_url | `baml_std.rs:223` | `https://ai-gateway.vercel.sh/v4/ai` |
| default_role | `baml_std.rs:246-252` | `system` |
| allowed_roles | `baml_std.rs:238-243` | `["system","user","assistant"]` |
| media_url_handler | `baml_std.rs:290-300` | image `send_url`, audio `send_base64`, video `send_url`, pdf `send_url` (moot — media is rejected) |
| model features | `model_features.rs:60-70` | `max_one_system_prompt: false`, `allowed_metadata: All` |
| media→user promotion | `specialize_prompt/mod.rs:44-54` | **NOT** in the list |
| streaming | `stream_accumulator.rs:73-88` | not supported (rejected) |
| api key env default | `provider.rs:62-68` | none |

---

## 3. What the engine did with the Responses API

The engine implements `openai-responses` as a *provider variant* of the OpenAI
client, not a separate client:

- provider string parsing: `engine/baml-lib/llm-client/src/clientspec.rs:126`,
  `:156`, `:188`; response-type selection
  `engine/baml-lib/llm-client/src/clients/helpers.rs:400-416`
  (`client_response_type` may be forced to `"openai-responses"`).
- constructor: `engine/…/primitive/openai/openai_client.rs:610-611`
  (`make_openai_client!(client, properties, "openai-responses")`);
  strategy dispatch `:375-390`.
- endpoint: `{base_url}/responses` (`openai_client.rs:397-404`).
- body: **all client properties passed through verbatim**, then
  `input` inserted (`openai_client.rs:243-278`). Same "everything is
  request_body" model sys_llm inherited. A bare-string prompt is allowed as
  `input` (`:248-252`) — sys_llm always sends an array.
- content parts: `responses_content_part` (`openai_client.rs:108-215`) —
  `input_text`/`output_text`, `input_image` **with `"detail":"auto"`** (`:143-150`,
  which sys_llm drops), `input_audio` **nested under an `input_audio` object**
  (`:160-170`) whereas sys_llm puts `data`/`format` flat on the part
  (`build_request/openai/responses.rs:41-43`) — one of these two is wrong on the
  wire and should be checked against the live API before porting; `input_file`
  with `file_url` + `filename:"document.pdf"` for URLs and `file_data` +
  `filename` for base64 (`:171-190`), where sys_llm sends `file_data` for both and
  no filename; local PDF and video are errors; per-part metadata merged when
  allowed (`:192-210`).
- streaming: `{"stream": true}` only, no `stream_options`
  (`openai_client.rs:299-311`); events typed as `ResponsesApiStreamEvent`
  (`types.rs:95-152`) and folded in `scan_openai_responses_stream`
  (`response_handler.rs:357-470`) — model from `created`/`in_progress`,
  content + usage from `completed`, `incomplete_details.reason` as the finish
  reason, error → failure.
- non-streaming parse: `parse_openai_responses_response`
  (`response_handler.rs:258-355`) — `find_map` over `output` taking the **first**
  message-with-text or function-call (so multiple messages are lost, unlike
  sys_llm which concatenates), `baml_is_complete = status == "completed"`,
  `finish_reason = status`, usage incl. `input_tokens_details.cached_tokens`.
  `Reasoning`, `WebSearchCall`, `FileSearchCall`, `ComputerCall`, `McpListTools`,
  `McpCall` are explicitly matched and skipped (`:317-325`) — the engine *knows*
  about MCP output items (`types.rs:56-68`), which neither sys_llm nor native do.

Engine `openai-responses` is the default provider in new projects
(`engine/baml-runtime/src/cli/initial_project/baml_src/clients.baml:5`,
`:13`; `resume.baml:13` uses `"openai-responses/gpt-5-mini"`), so parity here is
user-visible.

---

## 4. What a native image client needs — `ai.Client` fit

### 4.1 `invoke()` does NOT fit today. The blocker is `ai.content.Block`.

`ai.ModelTurn` is `{ content: ai.content.Block[], stop_reason, usage }`
(`ai/turn.baml:15-19`) and

```baml
type Block = Text | Reasoning | ToolUse;   // ai/ns_content/content.baml:23
```

There is **no media block**. Verified with the CLI:

```
$ target/debug/baml-cli describe ai.ModelTurn
class ai.ModelTurn   <builtin>/ai/turn.baml:15
fields:
  content ai.content.Block[]
  stop_reason ai.content.StopReason
  usage ai.events.Usage | null
```

And the runner is text-only end to end: it takes `turn.terminal_text()` (a
`string?`) and runs `baml.sap.parse<Out>(candidate)`
(`ai/runner.baml:148`, `:251-252`). So even if a client produced an image, an
`-> image` function could not receive it.

Contrast the sys_llm path, which has a full media→return-value coercion contract
in `parse_llm_output_for_target` (`crates/sys_llm/src/lib.rs:585-700+`):

- target `image` → exactly one image part, else error
  "Expected exactly one image output, got N. Use image[] for multiple outputs."
  (`lib.rs:596-608`)
- target `image[]` → all image parts (`lib.rs:609-625`)
- mixed output rejection: any non-image part with non-blank text →
  "Expected only image output parts, got N non-image part(s). Use a text/image
  union return type to preserve mixed outputs." (`lib.rs:741-762`)
- target `string | image` (± null) → ordered union items preserving interleaving
  (`lib.rs:764-826`), with the single/multi collapsing rules in
  `single_text_or_image_union_output` (`lib.rs:828-870`)
- if no media matched, fall through to the ordinary SAP parse of
  `response.content` (`lib.rs:571-583`)

**Recommended shape for the native port** (this is a stdlib change, not a client
change):

1. Add `class Media { image: image }` — or a broader
   `class MediaBlock { value: image | audio | pdf | video, metadata: map<string, unknown> }` —
   to `ai/ns_content/content.baml` and extend `type Block`.
   Every existing `match` over `Block` is already `_`-terminated
   (`openai/ns_internal/responses.baml:83-88`, `ai/runner.baml`), so adding a
   variant is low-risk, but each provider's `lower_journal` needs an arm.
2. Extend `ai.ModelTurn` with a `media_blocks(self) -> image[]` helper mirroring
   `tool_uses()` (`ai/turn.baml:32-40`).
3. Teach the runner: before `baml.sap.parse<Out>`, if `Out` is `image` /
   `image[]` / a `string|image` union, build the value from the turn's media
   blocks using the rules above. This is the direct port of
   `parse_llm_output_for_target`.
4. Only then does `invoke()` fit an image client unchanged: same
   `ai.ModelTurnInput` in, an `ai.ModelTurn` whose `content` is media blocks and
   `stop_reason: Complete` out.

**Do not invent a separate `ai.ImageClient` interface.** Two arguments: (a)
`openai-responses` returns text *and* images from the same call
(`parse_response/openai/responses.rs` test `test_parse_text_and_multiple_images_response`,
`:188-248` — one response with a caption plus two images), so the media path
must exist on the ordinary turn regardless; (b) `ai.Runner`/`ai.stream.from_spec`
are typed against `ai.Client`, and a parallel interface would need parallel
runners.

The one thing a gateway-images client legitimately ignores is the toolbox and
the output-format text — it should assert `input.toolbox.is_empty()` and pass
`""` to `input.prompt(...)` rather than `ai.wire.render_output_format(...)`,
since a schema blob in an image prompt is actively harmful.

### 4.2 Sketch of `ai_gateway/images.baml`

```
class AiGatewayImageClient {
    model: string,                 // -> ai-model-id header
    api_key: string?,              // no env fallback in sys_llm; add AI_GATEWAY_API_KEY
    api_key_env: string?,
    base_url: string?,             // default https://ai-gateway.vercel.sh/v4/ai
    base_url_env: string?,
    n: int = 1,
    extra: map<string, unknown> = {},   // the request_body escape hatch (G1)

    implements ai.Client { id, invoke }
    // no ai.stream.StreamingClient — the endpoint does not stream
}
```

`invoke` = flatten prompt to text (§2.1) → `baml.http.Request` → `ai.wire.send_as<GwImages>` →
for each base64, `baml.Image.from_base64(b64, "image/png")` → media blocks.
`baml.Image.from_base64(base64, mime_type) -> image` exists
(`crates/baml_builtins2/baml_std/baml/ns_media/media.baml:171-174`), which is the
native counterpart of `MediaValue::from_base64`. Errors go through
`ai.errors.classify_http` automatically via `send_as`
(`ai/ns_wire/wire.baml:7-32`); empty `images[]` should throw
`ai.errors.ParseFailed` to mirror `NoContent`.

### 4.3 If a real OpenAI `/v1/images/generations` client is wanted

**No ground truth exists to port** — nothing in `crates/` or `engine/`
implements it (§0). It would be new work against the public API:
`POST {base_url}/images/generations`, body
`{model, prompt, n, size, quality, style, background, output_format,
output_compression, moderation, response_format}` (note: `response_format`
`url|b64_json` is dall-e-only; `gpt-image-1` always returns b64), response
`{created, data: [{b64_json?, url?, revised_prompt?}], usage: {input_tokens,
output_tokens, total_tokens, input_tokens_details}}`. It shares §4.1's media-block
prerequisite and would otherwise be a near-copy of §4.2 with a different
envelope. Recommend treating it as a follow-on, not part of the sys_llm port.

---

## 5. Suggested migration order

1. **Stdlib prerequisite A** — media output: `ai.content.Media` block +
   `ai.ModelTurn` helper + runner media coercion (port of
   `lib.rs:585-870`). Unblocks G7, G8, and all of §4.
2. **Stdlib prerequisite B** — media input: a structural `ai.Prompt` accessor so
   clients can see `image`/`audio`/`pdf` parts instead of placeholders
   (`ai/spec.baml:7-13`, `crates/baml_builtins2/src/adt.rs:96-137`). Unblocks G3
   for *all* providers.
3. **`OpenAiClient` options** — G1/G2: `temperature`, `max_output_tokens`,
   `reasoning_effort`, plus a `request_body: map<string, unknown>` /
   `headers` / `query_params` escape hatch. Cheapest, highest-value fix.
4. **`openai/ns_internal/responses.baml` parse** — G7 (`image_generation_call`),
   G9 (usage), G12 (`response.failed` must throw; SSE status classification).
5. **`ai_gateway/images.baml`** — new client per §4.2 (needs step 1).
6. **Image-generation tool auto-enable** — G8: port `image_generation_mode`
   (`lib.rs:465-508`) as a BAML function over `input.output_type` (needs a
   `reflect`-style type predicate for `image` / `image[]` / `string|image`;
   confirm what `reflect` exposes before planning this).
7. **Specialization + finish-reason policy** — G5, G6, G10.
