# Anthropic provider — parity research (sys_llm / engine → native BAML)

Research for migrating `crates/sys_llm` to native BAML under
`crates/baml_builtins2/baml_std/`. This document covers **only the Anthropic
Messages provider**.

Three implementations compared. All paths are absolute-relative to their repo
root; `NEW` = `/Users/aaron/projects/baml/baml_language`, `ENGINE` =
`/Users/aaron/projects/baml/engine`.

| # | Impl | Files |
|---|------|-------|
| **A** | sys_llm (new compiler, Rust) | `NEW/crates/sys_llm/src/build_request/anthropic.rs` (844 L), `parse_response/anthropic.rs` (452 L), `stream_accumulator.rs:196-246`, `auth_request/mod.rs:79-87`, `specialize_prompt/*`, `model_features.rs:71-75`, `baml_std.rs` |
| **B** | engine (old compiler, Rust) | `ENGINE/baml-runtime/src/internal/llm_client/primitive/anthropic/{anthropic_client.rs,response_handler.rs,types.rs}`, `ENGINE/baml-lib/llm-client/src/clients/anthropic.rs`, `ENGINE/baml-runtime/src/internal/llm_client/traits/mod.rs` |
| **C** | native BAML (target) | `NEW/crates/baml_builtins2/baml_std/anthropic/messages.baml` (43 L), `anthropic/ns_internal/messages.baml` (426 L) |

Supporting native infrastructure read for this report: `ai/turn.baml`,
`ai/spec.baml`, `ai/ns_content/content.baml`, `ai/ns_events/events.baml`,
`ai/ns_wire/wire.baml`, `ai/ns_stream/stream.baml`, `ai/ns_errors/errors.baml`,
`ai/ns_tools/tools.baml`, `ai/runner.baml`, `baml/ns_http/http.baml`,
`crates/baml_builtins2/src/adt.rs`, `crates/bex_vm/src/package_baml/prompt.rs`.

---

## 0. Executive summary

The native client (**C**) is a *narrower but structurally more advanced* client
than either Rust implementation. It is the only one of the three that carries a
**conversation** (journal → messages, `tool_use`/`tool_result` blocks) and the
only one that emits **native tool definitions**. Both Rust implementations are
single-shot request builders: they take a rendered prompt and produce one body;
tools only reach the wire because arbitrary user options are splatted into the
body verbatim.

Conversely the native client is missing everything that lives in *client
options* and everything that requires **media** or **per-message metadata**,
because the `ai.Prompt` → `ai.PromptMessage` boundary is `(role: string,
content: string)` and throws away both media structure and metadata
(`ai/spec.baml:7-13`; `crates/baml_builtins2/src/adt.rs:89-108`).

Highest-severity gaps: **media (images/PDF/audio) cannot be sent at all**,
**`cache_control` / message metadata is unrepresentable**, **no arbitrary body
options** (temperature, top_p, stop_sequences, thinking budget, tool_choice
override, service_tier), **cached-token usage is dropped**, **streaming decodes
only `text_delta`** (no tool-call, thinking, or error events), and **streaming
HTTP errors are misclassified as `NetworkFailure`**.

---

## 1. Architecture at a glance

### A — sys_llm
Pure function pipeline, no conversation state:

- `build_request/mod.rs:63-110` dispatches `LlmProvider::Anthropic =>
  anthropic::build_request` (line 97), after `resolve_media::resolve_media`
  (line 86-87).
- `build_request/anthropic.rs:80-113` builds headers + body.
- `auth_request/mod.rs:79-87` injects `x-api-key` afterwards.
- `parse_response/anthropic.rs:57-95` parses one non-streaming body.
- `stream_accumulator.rs:196-246` folds SSE events into a flat text buffer.
- Prompt normalization is provider-agnostic and runs *before* the builder:
  `specialize_prompt/mod.rs:18-41`.

### B — engine
Trait-object client with option resolution:

- Option resolution/defaults: `ENGINE/baml-lib/llm-client/src/clients/anthropic.rs:143-219`.
- Request build: `anthropic_client.rs:259-317`.
- Prompt → body: `ToProviderMessage`/`ToProviderMessageExt` impls,
  `anthropic_client.rs:347-476`.
- Parse: `response_handler.rs:27-98`; stream scan: `response_handler.rs:100-186`.
- Prompt normalization: `traits/mod.rs:88-101` (merge adjacent roles),
  `traits/mod.rs:279-296` (max-one-system-prompt).

### C — native BAML
`ai.Client` + `ai.stream.StreamingClient` implementor:

- Public class: `anthropic/messages.baml:1-43` (fields `model`, `api_key`,
  `base_url`, `max_tokens`).
- Prompt lowering: `ns_internal/messages.baml:50-71`.
- Journal lowering (conversation): `ns_internal/messages.baml:73-159`.
- Request: `ns_internal/messages.baml:177-249`.
- Parse: `ns_internal/messages.baml:265-310`.
- Stream decode: `ns_internal/messages.baml:347-410`; entry
  `ns_internal/messages.baml:414-426`.
- Registered for the `"anthropic/<model>"` spec shorthand:
  `NEW/crates/baml_compiler2_ast/src/lower_cst.rs:744-753`.

---

## 2. Request building — parity matrix

| Feature | A sys_llm | B engine | C native | Verdict |
|---|---|---|---|---|
| Endpoint `POST {base}/v1/messages` | `build_request/anthropic.rs:104-112` | `anthropic_client.rs:275-279` | `ns_internal/messages.baml:239-248` | **parity** |
| Legacy `POST {base}/v1/complete` (text completion) | ✗ (chat only) | ✓ `anthropic_client.rs:275-279`, body `anthropic_client.rs:479-485` | ✗ | *deliberate drop* — engine sets `completion: false` for Anthropic anyway (`anthropic_client.rs:147`) |
| Default base URL `https://api.anthropic.com` | `baml_std.rs:219` | `clients/anthropic.rs:187-188` | `ns_internal/messages.baml:238` | **parity** |
| `base_url` override | client option | client option | `AnthropicClient.base_url` (`messages.baml:6`) | **parity** |
| `base_url` from a *named env var* | ✗ | ✗ | ✗ — but `OpenAiClient` has `base_url_env` (`openai/responses.baml:12-13,45-54`) | **inconsistency inside native**: Anthropic lacks `api_key_env`/`base_url_env` |
| System content hoisted to top-level `system` | `build_request/anthropic.rs:143-186` (all `system` messages, as an array of parts) | `anthropic_client.rs:446-464` (**only if the FIRST message is system**) | `ns_internal/messages.baml:50-71` (all `system` messages → array of text blocks) | **C matches A**, both stricter than B |
| `system` as an array of content blocks (not a bare string) | ✓ `RequestBody.system: Vec<ContentPart>` (`build_request/anthropic.rs:60`) | ✓ (`parts_to_message`) | ✓ `map<string,unknown>[]` (`ns_internal/messages.baml:46`) | **parity** |
| `system` omitted when empty | `#[serde(skip_serializing_if = "Vec::is_empty")]` (`build_request/anthropic.rs:59`) | key only inserted when present | `if (system.length() > 0)` (`ns_internal/messages.baml:204-209`) | **parity** |
| Multiple system messages collapsed to one before the builder | `specialize_prompt/transformations.rs:158-219` (keeps the FIRST system anywhere, rest → user) | `traits/mod.rs:283-292` (`skip(1)`: any system **after index 0** → user) | ✗ — all system messages are concatenated into the `system` array with no consolidation | **gap (low)**; native behavior is arguably more correct, but A/B/C all differ. Note A and B also differ from each other. |
| Single system-only prompt → becomes a user message | `transformations.rs:179-185, 208-216` | `traits/mod.rs:285-287` | `ns_internal/messages.baml:198-202` (moves `system` blocks into a leading user message when the first message is not `user`) | **parity in effect** |
| Adjacent same-role messages merged | `transformations.rs:32-85` | `traits/mod.rs:88-101` (`merge_messages`) | ✗ | **gap (low)** — Anthropic itself combines consecutive same-role turns server-side, so this is a fidelity, not a correctness, gap |
| First message must be `user` | guaranteed by consolidation + default_role `user` (`baml_std.rs:106-115`) | same | partially — only rescued when `system` is non-empty (`ns_internal/messages.baml:198-212`) | **bug (medium)** — see §7.1 |
| Role-less content → `user` | `transformations.rs:12-24` with `default_role = "user"` (`baml_std.rs:106-115`) | `default_role()` `clients/anthropic.rs:85-97` | `ns_internal/messages.baml:61-65` (`else → "user"`) | **parity** |
| `allowed_roles` validation | `transformations.rs:225-266`, defaults `["system","user","assistant"]` (`baml_std.rs:99-104`) | `clients/anthropic.rs:75-83` | ✗ | **gap (low)** |
| `remap_roles` | `transformations.rs:250` | `clients/anthropic.rs:99-101` | ✗ | **gap (low)** — Anthropic never needs it (Gemini does) |
| Message `metadata` → merged into the **last** content block (`cache_control`) | `build_request/anthropic.rs:192-209`; tested at `:638-663` (user) and `:695-718` (system) | `traits/mod.rs:117-127` (`ChatMessagePart::WithMeta`, filtered by `allowed_metadata`) | ✗ **impossible** — `ai.PromptMessage` has no metadata field (`ai/spec.baml:7-13`) and `PromptAst::collect_messages` drops the metadata field (`adt.rs:95-108`) | **GAP (high)** — see §7.6 |
| `allowed_role_metadata` filtering | `model_features.rs:20-39, 90-115`; `transformations.rs:273-334` | `traits/mod.rs:117-127` | n/a (no metadata at all) | gap follows from the above |
| Image content block (`{"type":"image","source":{...}}`) | `build_request/anthropic.rs:246-250`, tests `:367-424` | `anthropic_client.rs:363-388` | ✗ | **GAP (high)** |
| Image `source.type = "url"` | `build_request/anthropic.rs:272-276` | `anthropic_client.rs:380-386` | ✗ | **GAP (high)** |
| Image `source.type = "base64"` + `media_type` | `build_request/anthropic.rs:277-282` | `anthropic_client.rs:367-374` | ✗ | **GAP (high)** |
| PDF → `{"type":"document"}` | `build_request/anthropic.rs:255-258`, tests `:502-538` | `anthropic_client.rs:389-413` | ✗ | **GAP (high)** |
| Audio → `{"type":"input_audio"}` | `build_request/anthropic.rs:30-35, 251-254`, tests `:442-499` | `anthropic_client.rs:364-388` (uses `media_type.to_string()`, i.e. `"audio"`) | ✗ | **GAP (high)**; note A and B already **disagree on the wire tag** (`input_audio` vs `audio`) |
| Video rejected with a helpful error | `build_request/anthropic.rs:259-261` | `anthropic_client.rs:414-421` | n/a | gap follows |
| Media rejected in non-`user` roles | `build_request/anthropic.rs:221-223, 236-240`, test `:762-787` | ✗ | n/a | A-only |
| Media promoted to `user` when the prompt has no user message | `specialize_prompt/mod.rs:26-30, 43-54` (Anthropic is in the list) | ✗ | n/a | A-only |
| Media URL → base64 pre-resolution policy | `resolve_media.rs` driven by `MediaUrlHandler`; Anthropic defaults to `send_url` for **all four** kinds (`baml_std.rs:305-311`) | `anthropic_client.rs:149-168` (`SendUrl` for image/audio/video, `SendBase64` for pdf) | n/a | A and B **disagree on the PDF default** |
| `max_tokens` always sent | `build_request/anthropic.rs:96-101` | `clients/anthropic.rs:164-167` (defaulted into `properties`) | `ns_internal/messages.baml:192` | **parity** |
| `max_tokens` **default value** | **8192** (`build_request/anthropic.rs:74`) | **4096** (`clients/anthropic.rs:17`) | **4096** (`messages.baml:16`) | **C matches B, not A** — pick one deliberately; A's comment (`:68-73`) explains 8192 |
| Per-model `max_tokens` defaults | ✗ — none of the three vary by model | ✗ | ✗ | **no gap** (the task brief's "max_tokens defaults per model" does not exist in any impl) |
| Arbitrary extra body keys (temperature, top_p, top_k, stop_sequences, `thinking`, `service_tier`, `metadata`, `tool_choice` override, betas) | ✓ `extra_body` flattened via `#[serde(flatten)] extra` (`build_request/anthropic.rs:64-65`, built at `baml_std.rs:104-112`, passed at `:102`) | ✓ `let mut body = json!(self.properties.properties)` (`anthropic_client.rs:299-300`) | ✗ **no mechanism at all** | **GAP (high)** — see §7.7 |
| Native `tools: [...]` from a toolbox | ✗ (only via extra body) | ✗ (only via options) | ✓ `ns_internal/messages.baml:213-229` | **native-only (ahead)** |
| `tool_choice` | only via extra body | only via options | hardcoded `{"type":"auto"}` (`ns_internal/messages.baml:222-223`) | **gap (medium)** — no `any`/`tool`/`none`, no `disable_parallel_tool_use` |
| Assistant `tool_use` blocks replayed | ✗ (no conversation) | ✗ | ✓ `ns_internal/messages.baml:87-95` | **native-only (ahead)** |
| `tool_result` blocks, grouped into one user message | ✗ | ✗ | ✓ `ns_internal/messages.baml:101-126` (open-group coalescing via `AnthropicMessage.tool_results`) | **native-only (ahead)** |
| `tool_result.is_error` on tool failure | ✗ | ✗ | ✓ `ns_internal/messages.baml:112, 154` | **native-only (ahead)** |
| `thinking` blocks replayed with `signature` | ✗ | ✗ | ✗ — explicitly dropped, with a comment (`ns_internal/messages.baml:84-86`) | **gap (medium, native-specific)**: extended thinking + tool use requires replaying the signed thinking block |
| `stream: true` flag | `lib.rs:511-517` (generic body patch) | `anthropic_client.rs:312-314` | `ns_internal/messages.baml:231-236` | **parity** |
| User-supplied `headers` passthrough | `build_request/mod.rs:112-115` | `anthropic_client.rs:290-292` | ✗ | **gap (medium)** |
| `query_params` passthrough | `build_request/mod.rs:117-125` | ✗ | ✗ | gap (low) |
| Proxy (`baml-original-url`) | `build_request/mod.rs:127-152` (wasm) | `anthropic_client.rs:266-273, 296-298` | ✗ | **gap (medium for the playground)** |
| Vertex-hosted Claude (`rawPredict`, `anthropic_version` in body) | ✓ reuses the same body builder: `build_request/mod.rs:187-223`, entry `build_request/anthropic.rs:118-133` | ✓ synthetic client `anthropic_client.rs:228-250`, `clients/anthropic.rs:105-122` | ✗ | **migration constraint** — keep the body builder factored so a future `vertex` package can call it |
| Bedrock-hosted Claude | separate Converse-API path (`build_request/bedrock.rs`) | separate | ✗ | out of scope for this file |

---

## 3. Auth & headers — parity matrix

| Header / concern | A sys_llm | B engine | C native | Verdict |
|---|---|---|---|---|
| `x-api-key` | `auth_request/mod.rs:79-87` | `anthropic_client.rs:293-294` | `ns_internal/messages.baml:243` | **parity** |
| API key resolution order | explicit `options.api_key` → `ANTHROPIC_API_KEY` env (`auth_request/mod.rs:119-129`, `provider.rs:62-68`) | explicit `api_key` option, defaulting to `env.ANTHROPIC_API_KEY` (`clients/anthropic.rs:189-192`) | `self.api_key ?? baml.env.get_or_panic("ANTHROPIC_API_KEY")` (`ns_internal/messages.baml:243`) | **parity**; covered by test `NEW/crates/baml_tests/tests/env.rs:145-163` |
| Missing key behavior | header simply omitted (`auth_request/mod.rs:84-86`) → provider 401 | key resolves to `""` | **panics** via `get_or_panic` | **divergence (low)** — a panic is not an `ai.errors.Failure`; consider `InvalidRequest` instead |
| Named-env-var indirection (`api_key_env`) | ✗ | ✗ | ✗ for Anthropic; ✓ for OpenAI (`openai/responses.baml:12,33-43`) | **gap (low)** — inconsistent with the sibling native client |
| `anthropic-version: 2023-06-01` | `build_request/anthropic.rs:87` (hardcoded) | `clients/anthropic.rs:16, 152-155` (**user-overridable** via `headers`) | `ns_internal/messages.baml:244` (hardcoded) | **C matches A**; B is more flexible |
| `content-type: application/json` | `build_request/anthropic.rs:86` | via `req.json()` | `ns_internal/messages.baml:245` | **parity** |
| `anthropic-beta` header | ✗ (no impl sets it; only reachable via user `headers`) | ✗ (same) | ✗ (**not even reachable** — no headers passthrough) | **gap (medium)** — beta features (extended cache TTL, 1M context, code execution) require it; today only the native client makes it *impossible* |
| `anthropic-dangerous-direct-browser-access: true` (wasm) | ✓ `build_request/anthropic.rs:90-94` (`#[cfg(target_arch = "wasm32")]`) | ✗ | ✗ | **gap (medium for the browser playground)** |
| Per-request timeouts | ✗ (no timeout plumbed for Anthropic) | ✓ `anthropic_client.rs:283-288` from `http_config.request_timeout_ms`; option surface `ENGINE/baml-lib/llm-client/src/clients/helpers.rs:508-531` (connect / request / time-to-first-token / idle) | ✗ — `ai.wire.send_as` calls `baml.http.send(req)` with the default `timeout = null` = unbounded (`ai/ns_wire/wire.baml:8`; `baml/ns_http/http.baml:150-155`) | **gap (medium)** |

---

## 4. Response parsing — parity matrix

| Feature | A sys_llm | B engine | C native | Verdict |
|---|---|---|---|---|
| Envelope decode | strict serde struct, all of `id/role/type/content/model/stop_reason/stop_sequence/usage` **required** (`parse_response/anthropic.rs:42-52`) | same, required (`types.rs:41-51`) | lenient: only `content` required; `stop_reason`/`usage` optional (`ns_internal/messages.baml:24-28`) | **C is more robust** |
| `text` block | `parse_response/anthropic.rs:101` | `response_handler.rs:59-63` | `ns_internal/messages.baml:271-273` | **parity** |
| Text concatenation vs first-text-only | A **concatenates all** text parts (`parse_response/mod.rs:53-62`) | B takes the **first** text block only (`response_handler.rs:56-63`) | C pushes **every** text block, and `ModelTurn.terminal_text()` returns the **last** one (`ai/turn.baml:21-29`) | **three-way divergence**; C's "last text block wins" is the SAP-relevant choice but differs from both |
| No text block at all | not an error (empty string) | **hard error** `"Anthropic response contains no text"` (`response_handler.rs:68-78`) | not an error; the runner decides (`ai/runner.baml:148-158`) | C matches A |
| `tool_use` block → structured tool call | decoded but **discarded** (`parse_response/anthropic.rs:13-17` then `_ => {}` at `:117`) | decoded but discarded (`types.rs:14-18`, `response_handler.rs:59-63`) | ✓ → `ai.content.ToolUse` (`ns_internal/messages.baml:277-286`) | **native-only (ahead)** |
| `thinking` block | **not even a variant** (falls into `#[serde(other)] Other`, `parse_response/anthropic.rs:27-28`) | variant is **commented out** (`types.rs:19-23`) | ✓ → `ai.content.Reasoning` (`ns_internal/messages.baml:274-276`) | **native-only (ahead)**, but `signature` is dropped |
| `redacted_thinking` block | variant exists, then dropped (`parse_response/anthropic.rs:24-26`, `:117`) | variant exists, then dropped (`types.rs:24-27`) | falls through the `else` (`ns_internal/messages.baml:287-289`) | **parity in effect** (all three drop it) |
| `code_execution_tool_result` → image file media output | ✓ `parse_response/anthropic.rs:18-23, 102-116, 130-183`, test `:347-390` | ✗ | ✗ — `ai.content.Block` has no media variant (`ai/ns_content/content.baml:23`) | **gap (medium)**, blocked on an `ai.content` extension |
| `stop_reason: end_turn` | `Stop` (`parse_response/anthropic.rs:71`) | complete (`response_handler.rs:21-25`) | `Complete` (else-branch, `ns_internal/messages.baml:259-261`) | **parity** |
| `stop_reason: stop_sequence` | `Stop` (`:71`) | complete (`:23`) | `Complete` (else-branch) | **parity** |
| `stop_reason: max_tokens` | `Length` (`:72`) | **not complete** | `MaxTokens` (`ns_internal/messages.baml:255-256`) | **parity** |
| `stop_reason: tool_use` | `ToolUse` (`:73`) | not complete | `ToolUse` (`ns_internal/messages.baml:253-254`) | **parity** |
| `stop_reason: refusal` | `Other("refusal")` (`:74`) | not complete | `Refused` (`ns_internal/messages.baml:257-258`) | **native-only (ahead)** |
| `stop_reason: pause_turn` / `model_context_window_exceeded` / any unknown | `Other(str)`, preserved (`:74`) | **not complete** (`:21-25`) | **`Complete`** (else-branch, `:259-261`) | **BUG (medium)** — C silently reports success for unknown/paused stops; A and B both treat them as non-complete |
| `stop_reason: null` | `Unknown` (`:75`) | not complete | `Complete` (`ns_internal/messages.baml:292-295`) | **divergence (low)** |
| Raw finish-reason string preserved | ✓ `finish_reason_raw` (`parse_response/anthropic.rs:92`) | ✓ `metadata.finish_reason` (`response_handler.rs:91`) | ✗ — `ai.ModelTurn` has only the enum (`ai/turn.baml:15-19`) | **gap (low)** |
| `finish_reason_allow_list` / `deny_list` filtering | ✓ `baml_std.rs:125-142` | ✓ `clients/anthropic.rs:69` + `WithClientProperties::finish_reason_filter` (`anthropic_client.rs:85-87`) | ✗ | **gap (low)** |
| `usage.input_tokens` / `output_tokens` | ✓ (`parse_response/anthropic.rs:78-85`) | ✓ (`response_handler.rs:92-93`) | ✓ (`ns_internal/messages.baml:297-307`) | **parity** |
| `usage.total_tokens` | ✓ computed (`:83`) | ✓ computed (`:94`) | ✗ (no field on `ai.events.Usage`, `ai/ns_events/events.baml:36-41`) | trivially derivable — **no real gap** |
| `usage.cache_read_input_tokens` → cached tokens | ✓ (`parse_response/anthropic.rs:37-39, 84`), tests `:416-451` | ✓ (`types.rs:38`, `response_handler.rs:95`) | ✗ — **hardcoded `null`** (`ns_internal/messages.baml:302`); the field is not even in the envelope class (`:19-22`) | **GAP (high)** — the runner aggregates it (`ai/runner.baml:133-135`), so cost reporting is silently wrong |
| `usage.cache_creation_input_tokens` | decoded, unused (`parse_response/anthropic.rs:36-37`) | decoded, unused (`types.rs:37`) | ✗ | gap (low) — no canonical field exists |
| `model` echoed from the response | ✓ (`parse_response/anthropic.rs:90`) | ✓ (`response_handler.rs:88`) | ✗ — nowhere to put it on `ai.ModelTurn` | **gap (low)** |
| Error body `{"type":"error","error":{...}}` parsed | ✗ — status → generic error | type exists (`types.rs:84-95`, `ENGINE/baml-lib/llm-response-parser/src/anthropic.rs:68-79`) but is **not used** on the primitive-client path (`primitive/request.rs` uses `ErrorCode::from_status` only) | ✗ — `ai.wire.send_as` classifies by status only, body attached raw (`ai/ns_wire/wire.baml:26-28`; `ai/ns_errors/errors.baml:124-137`) | **parity in effect**; all three leave `error.type` / `error.message` unparsed. Native has the best raw-body retention. |
| `retry-after` header on 429 | ✗ | ✗ | ✗ — `RateLimited.retry_after_ms` is always `null` (`ai/ns_errors/errors.baml:126`); `send_as` never sees headers | **gap (low, shared)** — but native has a *typed slot* for it, so it is worth filling |

---

## 5. Streaming — parity matrix

| SSE event / concern | A sys_llm | B engine | C native | Verdict |
|---|---|---|---|---|
| `message_start` → input tokens | ✓ `stream_accumulator.rs:199-213` | ✓ `response_handler.rs:135-145` | ✓ `ns_internal/messages.baml:372-380` | **parity** |
| `message_start` → model name | ✓ (`:201-203`) | ✓ (`:137`) | ✗ | gap (low) |
| `message_start` → cached tokens | ✗ | ✓ (`response_handler.rs:144`) | ✗ | **gap (medium)** — compounded by `TurnStream` always emitting `cached_input_tokens: null` (`ai/ns_stream/stream.baml:223-232`) |
| `content_block_start` (tool_use id/name, initial text) | ✗ ignored | ✓ typed but ignored (`types.rs:131-137`, `response_handler.rs:151`) | ✗ ignored | **gap (high for tool streaming)** |
| `content_block_delta` / `text_delta` | ✓ but **without checking `delta.type`** (`stream_accumulator.rs:214-222`) | ✓ typed match (`response_handler.rs:146-150`) | ✓ checks `delta.type == "text_delta"` (`ns_internal/messages.baml:356-370`) | **C is the most correct** |
| `content_block_delta` / `input_json_delta` (streamed tool args) | ✗ | ✗ (variant not modeled; `types.rs:150-190` lists `ToolUse` but not `input_json_delta`) | ✗ | **gap (high)** — no impl can stream tool calls |
| `content_block_delta` / `thinking_delta` | ✗ | typed (`types.rs:157-159`) but ignored | ✗ | **gap (medium)** |
| `content_block_delta` / `signature_delta` | ✗ | typed (`types.rs:154-156`) but ignored | ✗ | gap (medium) — needed for thinking replay |
| `content_block_stop` | ignored | ignored (`response_handler.rs:152`) | ignored | parity |
| `ping` | ignored | ignored (`response_handler.rs:153`) | ignored (else-branch) | parity |
| `message_delta` → `stop_reason` | ✓ (`stream_accumulator.rs:223-230`) | ✓ (`response_handler.rs:154-158`) | ✓ (`ns_internal/messages.baml:381-393`) | **parity** |
| `message_delta` → `usage.output_tokens` | ✓ (`:231-238`) | ✓ (`:159-160`) | ✓ (`:389-390`) | **parity** |
| `message_delta` → cached tokens (only when non-null) | ✗ | ✓ **with the null-guard** (`response_handler.rs:161-165`) | ✗ | gap (medium) |
| `message_stop` → done | ✓ (`stream_accumulator.rs:240-242`) | ignored (`response_handler.rs:167`) | ✓ `TurnDone` (`ns_internal/messages.baml:394-396`) | **C matches A** |
| SSE `error` event → surfaced as a failure | ✗ silently ignored | ✓ `types.rs:115-117` → `LLMErrorResponse` (`response_handler.rs:168-180`) | ✗ **silently ignored** (falls into the final `else`, `:397-399`) | **GAP (high)** — a mid-stream overloaded/rate-limit error ends the turn as a *successful* truncated response |
| Malformed `data:` payload | skipped (`stream_accumulator.rs:138-140`) | hard error | skipped (`ns_internal/messages.baml:351-353` `catch_all → null`) | C matches A |
| Non-2xx on the streaming connection | n/a (status handled outside) | ✓ status-coded `LLMErrorResponse` | **misclassified**: `fetch_sse` throws `Io` with the status+body (`NEW/crates/sys_native/src/io_impls.rs:2696-2705`), and the client maps **every** such error to `ai.errors.NetworkFailure` (`ns_internal/messages.baml:416-424`), whose `retry_safety` is `Safe` (`ai/ns_errors/errors.baml:36-45`) | **BUG (high)** — a 400/401 during streaming becomes retryable "network" failure |
| Streaming + tools | n/a | n/a | **rejected upstream** (`ai/ns_stream/stream.baml:306-309`) | known phase limit |
| Streamed turn yields structured blocks | n/a (flat text) | n/a (flat text) | `TurnStream.final_turn` always emits exactly one `Text` block (`ai/ns_stream/stream.baml:233-237`) | shared limitation |

---

## 6. Native-only capabilities (C is ahead of A and B)

These have **no** counterpart in either Rust implementation and must not be
regressed by the migration:

1. **Journal → messages** conversation lowering: `ns_internal/messages.baml:130-159`.
2. **Native tool definitions** (`tools` + `tool_choice`): `ns_internal/messages.baml:213-229`.
3. **`tool_use` replay** in assistant messages: `ns_internal/messages.baml:87-95`.
4. **`tool_result` blocks with correct grouping** (consecutive results coalesce
   into a single user message, which the API requires):
   `ns_internal/messages.baml:101-126`.
5. **`is_error` on failed tool results**: `ns_internal/messages.baml:112,154`.
6. **`thinking` block parsing** → `ai.content.Reasoning`: `ns_internal/messages.baml:274-276`.
7. **`refusal` stop reason** → `ai.content.StopReason.Refused` → the runner
   throws `ai.errors.Refused`: `ns_internal/messages.baml:257-258`, `ai/runner.baml:160-162`.
8. **Typed failure taxonomy** (`RateLimited` / `NetworkFailure` /
   `InvalidRequest` / `Refused` / `ParseFailed` with `retry_safety`):
   `ai/ns_errors/errors.baml:25-137`, applied via `ai/ns_wire/wire.baml:7-32`.
9. **Composable reliability** (`ai.clients.Retry` / `Fallback` / `RoundRobin`)
   replacing engine's per-client `retry_policy` and the `baml-fallback` /
   `baml-round-robin` pseudo-providers: `ai/ns_clients/clients.baml:49-190`.

---

## 7. Gap list for the native client (prioritized)

### 7.1 BUG — a prompt whose first message is `assistant` and has no system content produces an invalid request
`ns_internal/messages.baml:198-212`: the leading-user rescue only fires when
`system.length() > 0`. With an empty system array and an assistant-first
prompt, `messages[0].role == "assistant"` and the API rejects it (the first
message must be `user`). A/B avoid this structurally because `default_role` is
`user` and consolidation runs first.
**Fix:** unconditional — if the first lowered message is not `user`, prepend a
user message (from the system blocks if any, else a minimal placeholder), or
reject with `ai.errors.InvalidRequest` before sending.

### 7.2 BUG — an assistant turn containing only `Reasoning` lowers to an empty `content` array
`_anthropic_assistant_blocks` drops `ai.content.Reasoning` entirely
(`ns_internal/messages.baml:84-86`). A turn whose content is only reasoning (or
empty) yields `{"role":"assistant","content":[]}`, which the API rejects
("all messages must have non-empty content"). The runner *does* produce such
messages — every repair attempt commits an `AssistantMessage` unconditionally
(`ai/runner.baml:141-142, 166-172`).
**Fix:** skip assistant messages that lower to zero blocks, or substitute a
single non-empty text block.

### 7.3 BUG — mid-stream `error` events are dropped
`ns_internal/messages.baml:397-399`. Anthropic emits
`event: error\ndata: {"type":"error","error":{"type":"overloaded_error",...}}`
mid-stream; the decoder ignores it and the turn completes as a truncated
success. Engine handles it (`response_handler.rs:168-180`).
**Fix:** decode `type == "error"` and `throw ai.errors.classify_*` /
`RateLimited` / `NetworkFailure` from the decoder closure (it is typed
`throws unknown`, `ai/ns_stream/stream.baml:72`).

### 7.4 BUG — streaming HTTP errors are all `NetworkFailure`
`ns_internal/messages.baml:416-424` catches everything from `fetch_sse` and
throws `NetworkFailure` (retry-safe). But `fetch_sse` already failed the
non-2xx status itself (`NEW/crates/sys_native/src/io_impls.rs:2696-2705`), so a
401/400 is reported as a transient network problem and `ai.clients.Retry` will
happily replay it (`ai/ns_clients/clients.baml:69-91`).
**Fix:** either have `fetch_sse` surface the status (preferred), or parse the
`Io` message's status prefix and route through `ai.errors.classify_http`.

### 7.5 BUG — unknown / `pause_turn` stop reasons map to `Complete`
`ns_internal/messages.baml:259-261`. A `pause_turn` (server tool pause) or
`model_context_window_exceeded` is reported as a normal completion, so the
runner will parse a truncated candidate as final. A returns `Other(raw)`
(`parse_response/anthropic.rs:74`); B treats anything but `end_turn`/
`stop_sequence` as incomplete (`response_handler.rs:21-25`).
**Fix:** add a `StopReason` catch-all (or preserve the raw string on
`ai.ModelTurn`) and treat unknown reasons as non-complete.

### 7.6 GAP (high) — media cannot be sent
`ai.Prompt.messages()` returns `(role, content: string)` and flattens media to a
display placeholder: `crates/baml_builtins2/src/adt.rs:89-100` (`to_text` →
`media.to_string()`), surfaced through
`crates/bex_vm/src/package_baml/prompt.rs:217-236`, typed at
`ai/spec.baml:7-13`. So the native client cannot produce `image` / `document` /
`input_audio` blocks that A (`build_request/anthropic.rs:242-286`) and B
(`anthropic_client.rs:358-424`) both produce, and there is no equivalent of
`resolve_media.rs` (URL→base64 pre-fetch) or the `MediaUrlHandler` policy
(`baml_std.rs:300-311`).
**Fix (architectural, blocks the migration):** extend `ai.PromptMessage` to
carry structured parts (text | media) — mirroring `PromptAstSimple` — plus a
native media-resolution helper. This is a prerequisite for *every* provider,
not just Anthropic.

### 7.7 GAP (high) — no arbitrary request-body options
A splats `extra_body` (`build_request/anthropic.rs:64-65,102`; built at
`baml_std.rs:104-112`); B splats `properties` (`anthropic_client.rs:299-300`).
C has four fields and no escape hatch (`messages.baml:1-24`). Unreachable
today: `temperature`, `top_p`, `top_k`, `stop_sequences`, `thinking`
(extended-thinking budget), `service_tier`, `metadata.user_id`, `betas`,
`container`, `mcp_servers`, custom `tool_choice`.
**Fix:** add `extra_body: map<string, unknown>?` (merged last, before
`stream`), plus first-class fields for the common ones.

### 7.8 GAP (high) — `cache_control` / per-message metadata is unrepresentable
A merges message metadata into the last content block
(`build_request/anthropic.rs:192-209`, tested at `:638-663` and `:695-718`); B
does it per-part with allow-list filtering (`traits/mod.rs:117-127`). The
native prompt pipeline *has* the concept — `baml.prompt.Role` carries
`metadata: map<string, unknown>` "provider-specific per-message metadata such
as cache control" (`baml/ns_prompt/prompt.baml:7-12`) — but it is dropped at
`PromptAst::collect_messages` (`adt.rs:95-108`) and absent from
`ai.PromptMessage`.
**Fix:** same boundary change as §7.6 — carry `metadata` on `ai.PromptMessage`,
then merge into the last block exactly as `merge_metadata_into_last` does.

### 7.9 GAP (high) — cached-token usage always `null`
`ns_internal/messages.baml:19-22` (envelope) and `:302` (hardcoded `null`).
Both Rust impls read `cache_read_input_tokens`
(`parse_response/anthropic.rs:37-39,84`; `types.rs:38`, `response_handler.rs:95`).
The runner sums it into the run total (`ai/runner.baml:133-135`), so prompt
caching looks free and reporting is wrong.
**Fix:** add `cache_read_input_tokens` / `cache_creation_input_tokens` to
`AnthropicUsage` and map the first to `cached_input_tokens`. Also fix the
streaming path (`ai/ns_stream/stream.baml:223-232` discards it structurally —
`TurnMeta` has no cached-token field).

### 7.10 GAP (high) — streaming decodes only text
No `content_block_start` (so tool-call ids/names are lost), no
`input_json_delta` (tool arguments), no `thinking_delta`/`signature_delta`
(`ns_internal/messages.baml:356-370` handles `text_delta` only). Combined with
`ai.stream.StreamEvent` having only `TextDelta | TurnMeta | TurnDone`
(`ai/ns_stream/stream.baml:52`) and `TurnStream.final_turn` producing a single
`Text` block (`:233-237`), streaming with tools is structurally impossible —
consistent with the explicit rejection at `ai/ns_stream/stream.baml:306-309`.
**Fix:** phase 2 — extend `ai.stream.StreamEvent` with tool/reasoning deltas
before extending the Anthropic decoder.

### 7.11 GAP (medium) — no user headers, no `anthropic-beta`, no browser header, no proxy
- headers passthrough: A `build_request/mod.rs:112-115`, B `anthropic_client.rs:290-292`; C none.
- `anthropic-dangerous-direct-browser-access`: A `build_request/anthropic.rs:90-94` (wasm only); C none — the native client cannot work in the WASM playground against `api.anthropic.com`.
- proxy `baml-original-url`: A `build_request/mod.rs:127-152`, B `anthropic_client.rs:266-273,296-298`; C none.
**Fix:** add `headers: map<string,string>?` to `AnthropicClient`, applied over
the defaults; decide how the wasm/proxy concerns surface natively.

### 7.12 GAP (medium) — `tool_choice` is hardcoded to `auto`
`ns_internal/messages.baml:222-223`. No `{"type":"any"}`, `{"type":"tool",
"name":...}`, `{"type":"none"}`, or `disable_parallel_tool_use`.

### 7.13 GAP (medium) — thinking blocks are not replayable
`ns_internal/messages.baml:84-86` documents the drop. Anthropic requires the
original `thinking` block **with its `signature`** to be replayed when extended
thinking is combined with tool use; without it, multi-turn thinking + tools is
unsupported. Needs `signature` on `ai.content.Reasoning`
(`ai/ns_content/content.baml:13-15`) and read/write in both directions.

### 7.14 GAP (medium) — no request timeout
`ai/ns_wire/wire.baml:8` calls `baml.http.send(req)` with the default
`timeout = null` = unbounded (`baml/ns_http/http.baml:150-155`). B exposes
connect / request / time-to-first-token / idle timeouts
(`ENGINE/baml-lib/llm-client/src/clients/helpers.rs:508-531`) and applies
`request_timeout_ms` per request (`anthropic_client.rs:283-288`).

### 7.15 GAP (medium) — no `code_execution_tool_result` media output
A extracts image files into media outputs
(`parse_response/anthropic.rs:102-116, 130-183`, test `:347-390`). Blocked on
`ai.content.Block` having no media variant (`ai/ns_content/content.baml:23`).

### 7.16 GAP (low) — assorted option surface
`allowed_roles` / `default_role` / `remap_roles` (A `baml_std.rs:99-148`, B
`clients/anthropic.rs:75-101`), `finish_reason_allow_list` / `deny_list` (A
`baml_std.rs:125-142`, B `anthropic_client.rs:85-87`), `supports_streaming` /
`supported_request_modes` (B `anthropic_client.rs:79-84`), `query_params` (A
`build_request/mod.rs:117-125`), adjacent-role merging, multi-system
consolidation, `api_key_env` / `base_url_env` (present on the sibling
`OpenAiClient`, `openai/responses.baml:12-13,33-54`).

### 7.17 GAP (low) — response metadata dropped
`ai.ModelTurn` (`ai/turn.baml:15-19`) has no slot for the echoed `model`, the
message `id`, the raw `stop_reason` string, or `stop_sequence`. A and B keep
model + raw finish reason (`parse_response/anthropic.rs:90-92`;
`response_handler.rs:88,91`).

### 7.18 Decision needed — default `max_tokens`
A = 8192 with an explicit rationale (`build_request/anthropic.rs:68-74`); B and
C = 4096 (`clients/anthropic.rs:17`; `messages.baml:16`). Pick one for the
merged implementation and record the reason.

### 7.19 Migration constraint — Vertex/Bedrock Claude
A reuses the exact same body builder for Vertex `rawPredict`
(`build_request/mod.rs:187-223` calling `build_request/anthropic.rs:118-133`,
with `anthropic_version` moved into the body and `max_tokens` pulled out of the
extra map); B does it with a synthetic client (`anthropic_client.rs:228-250`).
The native port should keep `_anthropic_request`'s **body construction** split
from its URL/header construction so a future `vertex` package can reuse it.

---

## 8. Test coverage that exists today

- Native request shape: `NEW/crates/baml_tests/tests/structured_prompt_requests.rs:58-77`
  (system split + first user message).
- Native API-key resolution from env:
  `NEW/crates/baml_tests/tests/env.rs:145-163`.
- sys_llm: 20 unit tests in `build_request/anthropic.rs:292-844` (media kinds,
  system extraction, metadata/cache_control, max_tokens defaults, version
  header) and 10 in `parse_response/anthropic.rs:185-452` (stop reasons, cached
  tokens, code-execution media, multi-block).
- sys_llm streaming: `stream_accumulator.rs:612-668`.

There is **no** native test for: tool lowering, tool_result grouping, the
journal path, streaming decode, stop-reason mapping, or usage mapping. Any
migration work should land those first — they are the parts the native client
uniquely owns.
