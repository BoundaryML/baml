# OpenAI Chat Completions family — sys_llm vs engine vs native BAML

Scope: providers `openai` (chat), `openai-generic`, `azure-openai`, `ollama`, `openrouter`.

Path conventions:
- `NEW/…` = `/Users/aaron/projects/baml/baml_language/…` (new compiler)
- `ENG/…` = `/Users/aaron/projects/baml/engine/…` (old compiler, parity reference)

Everything below cites `file:line` in one of those two trees.

---

## 0. TL;DR for the plan

1. **There is no native BAML chat-completions client at all.** `baml_std/openai/` contains only a Responses-API client (`NEW/crates/baml_builtins2/baml_std/openai/responses.baml:1-73`, `NEW/crates/baml_builtins2/baml_std/openai/ns_internal/responses.baml:1-315`). Ollama / OpenRouter / vLLM / Azure endpoints do **not** speak `/v1/responses`, so today those users have no working path.
2. **The sys_llm chat-completions pipeline is already orphaned.** No caller outside `crates/sys_llm/src` references `execute_build_request_from_owned`, `execute_build_request_stream_from_owned`, `execute_specialize_prompt_from_owned`, `execute_parse_response_from_owned`, `execute_validate_finish_reason`, or `stream_accumulator::*`. The only live sys_llm surface in `sys_ops` is SAP parsing + output-format rendering (`NEW/crates/sys_ops/src/lib.rs:160,191,715,727,759,765,798,809`). So the migration is *porting knowledge*, not *cutting over wiring*.
3. **Neither sys_llm nor the engine parses `tool_calls` on chat completions.** Engine has the field commented out (`ENG/baml-runtime/src/internal/llm_client/primitive/openai/types.rs:300-301`); sys_llm's response message type has only `content` + `images` (`NEW/crates/sys_llm/src/parse_response/openai/chat_completions.rs:51-57`). A native client that implements `ai.Client` **must** produce `ai.content.ToolUse`, so tool-calling is *new work*, not a port.
4. **Media cannot reach a native BAML client today.** `ai.Prompt.messages()` returns `ai.PromptMessage { role: string, content: string }` and flattens media to a lossy `Display` placeholder (`NEW/crates/baml_builtins2/baml_std/ai/spec.baml:7-13`, `NEW/crates/baml_builtins2/src/adt.rs:89-107,129-139`, `NEW/crates/baml_builtins2/src/media.rs:209-234`). This is the single hardest blocker to chat-completions parity.
5. Several sys_llm behaviors are **regressions vs the engine** and should not be replicated verbatim — see §8.

---

## 1. Code map

### sys_llm (new compiler)

| Concern | File |
|---|---|
| Chat-completions body + URL | `NEW/crates/sys_llm/src/build_request/openai/chat_completions.rs` (943 L) |
| Chat-completions response parse | `NEW/crates/sys_llm/src/parse_response/openai/chat_completions.rs` (603 L) |
| Provider enum + api-key env defaults | `NEW/crates/sys_llm/src/provider.rs` (69 L) |
| Client construction + per-provider defaults | `NEW/crates/sys_llm/src/baml_std.rs` (429 L) |
| Auth (headers) | `NEW/crates/sys_llm/src/auth_request/mod.rs` (529 L) |
| Dispatch, user headers, query params, proxy | `NEW/crates/sys_llm/src/build_request/mod.rs` (1965 L) |
| Role/system/metadata transforms | `NEW/crates/sys_llm/src/specialize_prompt/{mod,transformations}.rs` |
| Media URL/file → base64 resolution | `NEW/crates/sys_llm/src/resolve_media.rs` (833 L) |
| `max_one_system_prompt`, `allowed_metadata` | `NEW/crates/sys_llm/src/model_features.rs` (116 L) |
| SSE accumulation | `NEW/crates/sys_llm/src/stream_accumulator.rs` (762 L) |
| Option schema (BAML classes) | `NEW/crates/baml_builtins2/baml_std/baml/ns_prompt/sys_llm_types.baml` (73 L) |

### engine (parity reference)

| Concern | File |
|---|---|
| Option resolution + all 6 provider constructors | `ENG/baml-lib/llm-client/src/clients/openai.rs` (483 L) |
| Request building, media lowering, streaming opts | `ENG/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs` (1245 L) |
| Response + stream-delta parsing | `ENG/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs` (989 L) |
| Wire types | `ENG/baml-runtime/src/internal/llm_client/primitive/openai/types.rs` (384 L) |
| Message merge + per-part metadata | `ENG/baml-runtime/src/internal/llm_client/traits/mod.rs:88-137` |
| Option helper defaults | `ENG/baml-lib/llm-client/src/clients/helpers.rs:249-546,1010-1019` |
| `AllowedRoleMetadata` / `ResponseType` | `ENG/baml-lib/llm-client/src/clientspec.rs:433-500` |
| SSE loop, `[DONE]` | `ENG/baml-runtime/src/internal/llm_client/primitive/stream_request.rs:60-140,212-225` |

> Note: `ENG/baml-runtime/src/internal/llm_client/primitive/openai/properties/{azure,generic,ollama}.rs` are **dead files** — `properties/mod.rs:1-22` is the only module wired in, and it delegates to `baml-lib/llm-client`. They still document historical intent (e.g. `azure.rs:37-43` shows `AZURE_OPENAI_API_KEY` → `API-KEY` header) but are not the live path.

### native BAML client pattern (what to imitate)

`NEW/crates/baml_builtins2/baml_std/openai/ns_internal/responses.baml`:
- envelope classes for decoding — `:7-30`
- `openai_lower_prompt(ai.Prompt) -> map<string,unknown>[]` — `:36-52`
- `openai_lower_journal(ai.Journal) -> …` (assistant text, tool_use, tool results) — `:56-114`
- `_openai_request(client, input, stream) -> baml.http.Request` — `:123-166` (tools at `:137-153`, `stream` flag `:154-159`, URL/headers `:160-165`)
- `openai_parse(OaResponse) -> ai.ModelTurn` — `:169-227`
- `invoke` = render + `ai.wire.send_as<T>` + parse — `:230-234`
- SSE: envelope `:246-250`, `_openai_decode_batch` `:253-299`, `invoke_stream` `:303-315`

Public wrapper: `NEW/crates/baml_builtins2/baml_std/openai/responses.baml:1-73` (fields, `new()` with defaults `:15-29`, `resolved_api_key` `:33-43`, `resolved_base_url` `:46-54`, `implements ai.Client` `:56-66`, `implements ai.stream.StreamingClient` `:68-72`).

Supporting stdlib:
- `ai.wire.send_as<T>` / `render_output_format` — `NEW/crates/baml_builtins2/baml_std/ai/ns_wire/wire.baml:7-38`
- `ai.errors.classify_http` — `NEW/crates/baml_builtins2/baml_std/ai/ns_errors/errors.baml:124-137`
- `ai.content.{Text,Reasoning,ToolUse,StopReason}` — `NEW/crates/baml_builtins2/baml_std/ai/ns_content/content.baml:9-32`
- `ai.ModelTurnInput` / `ai.ModelTurn` / `interface ai.Client` — `NEW/crates/baml_builtins2/baml_std/ai/turn.baml:6-46`
- `ai.stream.{SseEvent,decode_sse_batch,TextDelta,TurnMeta,TurnDone,TurnStream}` — `NEW/crates/baml_builtins2/baml_std/ai/ns_stream/stream.baml:12-52,68-239`
- `ai.tools.{Tool,Toolbox}` — `NEW/crates/baml_builtins2/baml_std/ai/ns_tools/tools.baml:10-40,92-120`
- `baml.http.{Request,Response,SseStream,send,fetch_sse}` — `NEW/crates/baml_builtins2/baml_std/baml/ns_http/http.baml:8-13,16-96,99-113,150-162,165-167`
- Shorthand client registry (`"openai/gpt-4o"` → class) — `NEW/crates/baml_compiler2_ast/src/lower_cst.rs:744-753`

---

## 2. Request building — feature matrix

| Feature | sys_llm | engine | Native BAML today |
|---|---|---|---|
| Endpoint `{base}/chat/completions` | ✅ `chat_completions.rs:129-132` | ✅ `openai_client.rs:223-234` | ❌ none |
| Azure endpoint `{base}/chat/completions?api-version=…` | ✅ `chat_completions.rs:112-127` | ✅ base URL built at resolve time (`clients/openai.rs:167-177`), `api-version` as query param (`clients/openai.rs:323-333`), applied at `openai_client.rs:430-432` | ❌ |
| Azure `resource_name` + `deployment_id` → `https://{rn}.openai.azure.com/openai/deployments/{did}` | ✅ `chat_completions.rs:120-122` | ✅ `clients/openai.rs:172-174` | ❌ |
| Azure XOR validation (base_url **or** both of rn/did) | ✅ runtime, via `baml_base::validate_client_options` (`baml_std.rs:65-81`; error text asserted at `baml_std.rs:390-393`) | ✅ **compile-time diagnostics** with per-key spans (`clients/openai.rs:282-314`) | ❌ |
| Legacy `/completions` (non-chat) endpoint | ❌ removed | ✅ `openai_client.rs:223-234,857-871` (`WithNoCompletion` for OpenAI though: `openai_client.rs:82`) | ❌ |
| `model` in body | ✅ `chat_completions.rs:98` | ✅ passes through `properties` map (`openai_client.rs:282-296`) | n/a (`c.model` field) |
| Arbitrary passthrough options (`temperature`, `max_tokens`, …) | ✅ `extra_body` from `request_body` map, flattened (`baml_std.rs:104-112`, `chat_completions.rs:74-76,86`) | ✅ every unconsumed `options` key (`clients/openai.rs:197-222`) | ❌ no equivalent field on the native client |
| `content-type: application/json` | ✅ `chat_completions.rs:82-83` | via `req.json()` (`openai_client.rs:451`) | manual |
| User `headers` (lowercased, override provider defaults) | ✅ `build_request/mod.rs:112-115` | ✅ `openai_client.rs:434-436` (case preserved) | ❌ |
| User `query_params` (percent-encoded via `url` crate) | ✅ `build_request/mod.rs:117-125` | ✅ `openai_client.rs:430-432` | ❌ (no URL-encode helper in stdlib) |
| Playground proxy rewrite (`BOUNDARY_PROXY_URL` + `baml-original-url`) | ✅ wasm-only `build_request/mod.rs:135-153,164-185` | ✅ `clients/openai.rs:239` + `helpers.rs:1010-1019` + `openai_client.rs:406-413,441-444` | ❌ |
| Per-request timeout | ❌ none | ✅ `HttpConfig.request_timeout_ms` (`openai_client.rs:423-428`), plus connect / TTFT / idle (`helpers.rs:508-546`) | partial: `baml.http.send(req, timeout)` exists (`http.baml:150-155`); `fetch_sse` has **no** timeout param (`http.baml:165-167`) |
| Azure default `max_tokens` | ✅ `chat_completions.rs:89-95` — `entry().or_insert(azure.max_tokens)` | ✅ `clients/openai.rs:204-219` — insert `4096` **only if neither `max_tokens` nor `max_completion_tokens` present**; explicit `max_tokens: null` removes the key | ❌ |

### Message lowering

| Feature | sys_llm | engine |
|---|---|---|
| `messages: [{role, content: [parts]}]` | ✅ `chat_completions.rs:16-22,97-101,139-154` | ✅ `openai_client.rs:821-855` |
| `openai-generic` collapses all-text content to a **plain string** | ❌ **missing** | ✅ `openai_client.rs:339-365` — critical for strict OpenAI-compatible servers that reject array content |
| Merge adjacent same-role messages | ✅ twice: in specialization (`transformations.rs:32-85`) and again in the builder (`chat_completions.rs:189-201`, guarded on equal metadata) | ✅ once, in `merge_messages` (`traits/mod.rs:88-101`, honoring `allow_duplicate_role`) |
| Message metadata (e.g. `cache_control`) placement | **message level** — flattened into the message object (`chat_completions.rs:20-22,167-174,181-186`); the code comment claiming "merge into the last content part" is wrong | **content-part level** (`traits/mod.rs:109-126`) |
| Metadata allow-list default | `AllowedMetadata::All` (`model_features.rs:69`) → metadata always forwarded | `UnresolvedAllowedRoleMetadata::None` (`helpers.rs:390`) → metadata **dropped** unless allow-listed |
| Metadata allow-list from options | ✅ `"all"` / `"none"` / `string[]` (`model_features.rs:90-115`), applied in `transformations.rs:273-…` | ✅ `helpers.rs:364-391`, `clientspec.rs:433-490` |
| `max_one_system_prompt` | false for all 5 providers (`model_features.rs:61-70`) | `max_one_system_prompt: false` (`openai_client.rs:503`) — agree |
| Role validation against `allowed_roles` (hard error) | ✅ `transformations.rs:225-266`; error `specialize_prompt/mod.rs:56-60` | roles are enforced at render time via `ChatOptions` (`traits/chat.rs:15-20`) |
| `remap_roles` | ✅ `transformations.rs:250` (no OpenAI-family defaults: `baml_std.rs:273-287`) | ✅ `clients/openai.rs:120-123` |
| Role-less prompt → `default_role` | ✅ `transformations.rs:12-24` | jinja renderer + `ChatOptions` |
| Media-in-non-user message promoted to `user` when no user message exists | ✅ `specialize_prompt/mod.rs:26-30,43-54`, `transformations.rs:94-148` | ❌ not present |

---

## 3. Per-provider option resolution / defaults

| Option | provider | sys_llm | engine |
|---|---|---|---|
| default `base_url` | `openai` | `https://api.openai.com/v1` (`baml_std.rs:220-222`) | same (`clients/openai.rs:253-254`) |
| | `openai-generic` | **`https://api.openai.com/v1`** (`baml_std.rs:220-222`) | **required, no default** — `ensure_base_url(true)` (`clients/openai.rs:346`), hard error `clients/openai.rs:179-181` |
| | `azure-openai` | none; built at request time (`baml_std.rs:230-233`) | built at resolve time (`clients/openai.rs:167-177`) |
| | `ollama` | **`http://localhost:11434`** (`baml_std.rs:224`) — no `/v1` | **`http://localhost:11434/v1`** (`clients/openai.rs:361-362`) |
| | `openrouter` | `https://openrouter.ai/api/v1` (`baml_std.rs:225`) | same (`clients/openai.rs:427-429`) |
| default `allowed_roles` | `ollama` | `["user","assistant"]` (`baml_std.rs:240`) | `["system","user","assistant"]` (`helpers.rs:252-256` + `clients/openai.rs:98-110`) |
| | others | `["system","user","assistant"]` (`baml_std.rs:241`) | same, **except o1 models → `["user","assistant"]`** (`clients/openai.rs:82-89,98-110`) |
| default `default_role` | `openai`/`generic`/`azure`/`openrouter` | `"system"` (`baml_std.rs:246-254`) | first allowed role = `"system"` (`clients/openai.rs:112-118`) |
| | `ollama` | `"user"` (falls to `_ => "user"`, `baml_std.rs:253`) | `"user"`, set explicitly (`clients/openai.rs:374-377`) |
| | clamping to allowed set | ✅ `baml_std.rs:255-270` | validated at parse time (`helpers.rs:264-…`) |
| default api-key env var | `openai` | `OPENAI_API_KEY` (`provider.rs:62-68`) | `OPENAI_API_KEY` (`clients/openai.rs:256-259`) |
| | `azure-openai` | **none** (`provider.rs:62-68` returns `None`) | **`AZURE_OPENAI_API_KEY`** (`clients/openai.rs:317-319`) |
| | `openrouter` | **none** | **`OPENROUTER_API_KEY`** (`clients/openai.rs:431-435`) |
| | `openai-generic`, `ollama` | none | none (`clients/openai.rs:348,364`) |
| `model` required | ✅ hard error if missing/blank (`baml_std.rs:83-90`) | not required by the resolver (it's just a passthrough property) |
| `media_url_handler` defaults (all 5) | image `send_url`, audio `send_base64`, video `send_url`, pdf `send_url` (`baml_std.rs:289-304`) | identical defaults (`openai_client.rs:504-523`); user override via `helpers.rs:424-446` |
| `supports_streaming` option | declared (`sys_llm_types.baml:19`) but **never read** — no hits in `crates/sys_llm/src` | ✅ `clients/openai.rs:91-96` (defaults false for o1) |
| `client_response_type` | ❌ dropped; parse dispatch is by provider only (`parse_response/mod.rs:146-163`) | ✅ lets an openai-generic client parse anthropic/google shapes (`clients/openai.rs:241-244`, `clientspec.rs:492-500`) |
| `finish_reason_allow_list` / `deny_list` | ✅ `baml_std.rs:125-142`, enforced `lib.rs:521-539,559-570` | ✅ `helpers.rs:312-…`, exposed via `WithClientProperties::finish_reason_filter` (`openai_client.rs:55-57`) |
| `http` timeouts | ❌ | ✅ `helpers.rs:508-546` |

---

## 4. Auth & headers

| Provider | sys_llm | engine |
|---|---|---|
| `openai`, `openai-generic`, `ollama`, `openrouter` | `authorization: Bearer {key}` (`auth_request/mod.rs:99-112`) | `req.bearer_auth(key)` (`openai_client.rs:437-439`) |
| `azure-openai` | `api-key: {key}` header, no `Authorization` (`auth_request/mod.rs:100-102`; test `auth_request/mod.rs:329-356`) | `api-key` inserted into `headers` at construction (`clients/openai.rs:335-341`), `api_key` left `None` so no bearer |
| key resolution order | explicit `api_key` → `provider.default_api_key_env_var()` env (`auth_request/mod.rs:119-129`) | explicit `api_key` StringOr → per-provider default `EnvVar` (`clients/openai.rs:256-259,317-319,431-435`) |
| missing key | header simply omitted (`auth_request/mod.rs:99`) | omitted / azure header absent |
| user headers applied after auth defaults | ✅ lowercased, overrides (`build_request/mod.rs:112-115`) — note this runs **before** `auth_request` (`build_request/mod.rs:145`), so a user-supplied `authorization` header is **overwritten** by auth | ✅ headers first, then `bearer_auth` (`openai_client.rs:434-439`) — same precedence bug shape |

---

## 5. Media (chat completions content parts)

| Kind / source | sys_llm | engine |
|---|---|---|
| image, URL | `{"type":"image_url","image_url":{"url": <url>}}` (`chat_completions.rs:236-242,274-287`) | same (`openai_client.rs:700-702`) |
| image, base64 | `data:{mime};base64,{b64}` (`chat_completions.rs:281-283`) | same (`openai_client.rs:703-710`) |
| audio, base64 | `{"type":"input_audio","input_audio":{data,format}}`, format **restricted to `wav`/`mp3`** — anything else is an error (`chat_completions.rs:243-249,309-317`) | same shape, but format = mime suffix with only `mpeg→mp3` mapping; **any** format is forwarded (`openai_client.rs:716-743`) |
| audio, URL | **error** "audio URL not pre-fetched" (`chat_completions.rs:290-304`) | sends the URL as `data` with a format guessed from mime/extension (`openai_client.rs:744-767`) |
| pdf, base64 | `{"type":"file","file":{"file_data":"data:…;base64,…"}}` (`chat_completions.rs:250-259`) — **no `filename`** | `{"type":"file","file":{"filename":"document.pdf","file_data":"data:…"}}` (`openai_client.rs:792-800`) |
| pdf, URL | `{"type":"file","file":{"file_data": <raw url>}}` (`chat_completions.rs:274-280`; test asserts it at `chat_completions.rs:568-587`) — **wrong shape**; OpenAI expects `file_url` | `{"type":"file","file":{"type":"input_file","file_url":…,"filename":"document.pdf"}}` (`openai_client.rs:781-791`) |
| video | error (`chat_completions.rs:260-262`) | error with a Realtime-API pointer (`openai_client.rs:809-816`) |
| media in a non-user role | **hard error** (`chat_completions.rs:210-212,225-229`) | allowed — no role check on the chat path |
| URL→base64 pre-resolution pass | ✅ `resolve_media.rs:73-200` driven by `media_url_handler` (`resolve_media.rs:37-67`), incl. `data:` URLs, mime sniffing from headers/bytes | ✅ equivalent, driven by `ModelFeatures::resolve_*_urls` (`openai_client.rs:504-523`) |

---

## 6. Response parsing

| Feature | sys_llm | engine |
|---|---|---|
| Decode envelope | `ChatCompletionResponse` (`parse_response/openai/chat_completions.rs:11-21`) | `ChatCompletionGeneric<ChatCompletionChoice>` (`types.rs:203-224,262-274`) |
| `created` float-or-int tolerance | ✅ `parse_response/openai/chat_completions.rs:23-41` | ✅ `types.rs:237-253` |
| 0 choices | error `NoContent` (`:143-148`) | error "Expected exactly one choices block" (`response_handler.rs:53-68`) |
| >1 choice | **error** `UnsupportedResponseFormat` (`:150-160`) | **error** (same check, `!= 1`) |
| `message.content` as **string** | ✅ untagged (`:59-64,207`) | ✅ only string (`types.rs:295-309`) |
| `message.content` as **array of parts** (text + image parts) | ✅ (`:59-73,208-228`) | ❌ |
| `message.images[]` (Ollama / OpenRouter image output) | ✅ (`:54-55,75-82,231-258`) | ❌ |
| image data-URL / extension → `MediaValue` | ✅ (`:267-299`) | ❌ |
| `tool_calls` | ❌ | ❌ (commented out, `types.rs:300-301`) |
| `refusal` | ❌ | ❌ |
| finish reason normalization | `stop→Stop`, `length→Length`, `tool_calls→ToolUse`, else `Other`, none → `Unknown` (`:169-175`); raw kept (`:197`) | raw string only; `baml_is_complete = (finish_reason == "stop")` (`response_handler.rs:86-93`) |
| usage | `prompt_tokens`/`completion_tokens`/`total_tokens` + `input_tokens_details.cached_tokens` (`:177-190`, aliases at `parse_response/openai/mod.rs:9-17`) | identical incl. `prompt_tokens_details` alias (`response_handler.rs:94-103`, `types.rs:277-292`) |
| finish-reason allow/deny enforcement | ✅ `lib.rs:559-570` and `lib.rs:521-539` | ✅ via `finish_reason_filter` (`openai_client.rs:55-57`) |
| HTTP error classification | in `sys_native` HTTP layer, not here | `ErrorCode::from_status` etc. (`primitive/request.rs:280-333,447-481`); typed `OpenAIErrorResponse` exists (`types.rs:374-384`) |

---

## 7. Streaming

| Feature | sys_llm | engine |
|---|---|---|
| `stream: true` in body | ✅ blanket JSON patch (`lib.rs:342-351,509-518`) | ✅ `openai_client.rs:300-324` |
| `stream_options: {include_usage: true}` | ❌ **never set** — yet the accumulator claims to read usage "when `stream_options.include_usage` is set" (`stream_accumulator.rs:179`), so streaming usage is always null | ✅ but only when `provider == "openai"` (`openai_client.rs:311-321`) |
| Providers allowed to stream | openai / generic / azure / ollama / openrouter / anthropic (`stream_accumulator.rs:73-89`) | anything not gated by `supports_streaming` (false by default for o1: `clients/openai.rs:91-96`) |
| Delta extraction | `choices[0].delta.content` (`stream_accumulator.rs:159-178`) | same (`response_handler.rs:142-150`) |
| `model` from chunk | ✅ (`stream_accumulator.rs:159-161`) | ✅ (`response_handler.rs:146`) |
| finish reason from chunk; `stop`/`length` mark done | ✅ (`stream_accumulator.rs:171-177`) | records finish reason; done comes from `[DONE]` (`stream_request.rs:101-103`) |
| `[DONE]` sentinel | ✅ (`stream_accumulator.rs:132-135`) | ✅ (`stream_request.rs:101-103`) |
| usage from final chunk | ✅ `prompt_tokens`/`completion_tokens` (`stream_accumulator.rs:180-193`) | ✅ + `total_tokens` + `cached_input_tokens` (`response_handler.rs:152-162`) |
| tool-call deltas | ❌ | ❌ (`types.rs:319-333`) |
| unparsable SSE payload | silently skipped (`stream_accumulator.rs:138-141`) | surfaced as a stream error (`stream_request.rs:104-116`) |
| stream timeout / TTFT | ❌ | ✅ `time_to_first_token_timeout_ms`, `idle_timeout_ms` (`helpers.rs:508-546`) |

---

## 8. sys_llm regressions vs engine (do **not** port these)

1. **Ollama base URL is missing `/v1`.** `NEW/crates/sys_llm/src/baml_std.rs:224` yields `http://localhost:11434`, and `chat_completions.rs:129-132` appends `/chat/completions` → `http://localhost:11434/chat/completions`, which Ollama does not serve. Engine: `http://localhost:11434/v1` (`ENG/…/clients/openai.rs:361-362`). Every other reference in the new repo also uses `/v1` (`NEW/crates/baml_lsp2_actions_tests/test_files/syntax/prompt_fiddle_example.baml:494`).
2. **Ollama loses the `system` role.** sys_llm defaults `allowed_roles` to `["user","assistant"]` (`baml_std.rs:240`) and `transformations.rs:244-249` turns a `system` message into a hard error. Engine allows system (`clients/openai.rs:98-110`).
3. **`openai-generic` silently defaults to `api.openai.com`.** `baml_std.rs:220-222`. Engine hard-errors when `base_url` is missing (`clients/openai.rs:179-181,346`).
4. **Azure `max_tokens` is injected even when `max_completion_tokens` is set.** `chat_completions.rs:89-95` uses `or_insert` unconditionally; the test at `chat_completions.rs:886-919` locks in a body carrying *both* `max_tokens: 4096` and `max_completion_tokens: 2048`. Engine skips the default in exactly that case and honors an explicit `max_tokens: null` as a removal (`clients/openai.rs:204-219`).
5. **No `AZURE_OPENAI_API_KEY` / `OPENROUTER_API_KEY` env defaults** (`provider.rs:62-68` vs `clients/openai.rs:317-319,431-435`).
6. **`openai-generic` never collapses text content to a string** (engine `openai_client.rs:339-365`); many OpenAI-compatible servers reject `content: [...]`.
7. **Message metadata lands on the message object instead of the content part** (`chat_completions.rs:20-22,167-174` vs `traits/mod.rs:109-126`) — `cache_control` / `prompt_cache_breakpoint` on the wrong node.
8. **Metadata allow-list default flipped** — sys_llm `All` (`model_features.rs:69`) vs engine `None` (`helpers.rs:390`).
9. **PDF-by-URL uses `file_data` instead of `file_url`, and omits `filename`** (`chat_completions.rs:250-259,274-287` vs `openai_client.rs:781-800`).
10. **Audio-by-URL errors instead of being forwarded**, and non-wav/mp3 audio is rejected outright (`chat_completions.rs:290-317` vs `openai_client.rs:716-767`).
11. **`stream_options.include_usage` never sent** — streaming token usage is silently lost (§7).
12. **Dropped options with no replacement:** `supports_streaming` (declared, unread), `client_response_type`, `http`/timeouts, o1 model special-casing, `/completions` completion mode.

### sys_llm features the engine lacks (worth keeping)

- Chat response `message.content` as an **array of parts** and `message.images[]` → `MediaValue` (`parse_response/openai/chat_completions.rs:59-82,202-299`).
- Normalized `FinishReason` enum incl. `ToolUse` (`parse_response/mod.rs:112-127`).
- Media-in-non-user-role promotion to `user` (`transformations.rs:94-148`).
- `openai-generic` image outputs via `modalities: ["image","text"]` derived from the return type (`lib.rs:373-378,383-419`, `image_generation_mode` `lib.rs:465-507`) — no engine equivalent (`grep modalities` in `ENG/baml-runtime/src` returns nothing).

---

## 9. What a native BAML ChatCompletions client must implement

Target layout, mirroring the Responses client:
- `baml_std/openai/chat.baml` — public `OpenAiChatClient` (or extend `openai/responses.baml`'s neighbor set).
- `baml_std/openai/ns_internal/chat_completions.baml` — envelopes, lower, render, parse, SSE decode.
- Per-provider wrappers: `baml_std/azure/`, `baml_std/ollama/`, `baml_std/openrouter/` (each its own package like `openai/baml.toml:1-2`), or thin `new()` factories on one class.

### 9.1 Request

1. **Body**: `{model, messages}` + passthrough options. Needs a `request_body: map<string, unknown>?` (or explicit `temperature`/`max_tokens`/…) field on the client class — the Responses client has no such escape hatch today (`openai/responses.baml:1-14`).
2. **`lower_prompt`**: `ai.Prompt.messages()` → `[{role, content}]`. Decide the content encoding:
   - array-of-parts (`[{type:"text",text}]`) like sys_llm `chat_completions.rs:208`, **or**
   - plain string for `openai-generic`/ollama/vLLM, like engine `openai_client.rs:339-365`.
   Recommendation: a client field (`content_as_string: bool`) defaulting to plain string for generic/ollama and array for openai/azure.
3. **Role mapping**: empty role → default role. The native model has no `allowed_roles`/`default_role` options anymore — see `NEW/crates/baml_tests/projects/compiles/o1_allowed_roles/o1_clients.baml:1-3` ("roles are the client's wire concern now"). So the client hardcodes: `"" → "system"` for openai/azure/openrouter, `"" → "user"` for ollama; `"tool"` handled via the journal path (compare `openai/ns_internal/responses.baml:36-52`).
4. **Merge adjacent same-role messages** — port `transformations.rs:32-85` / `chat_completions.rs:189-201`.
5. **`lower_journal`** → chat-completions shape (this differs from Responses):
   - assistant text → `{"role":"assistant","content": text}`
   - `ai.content.ToolUse` → `{"role":"assistant","tool_calls":[{"id","type":"function","function":{"name","arguments": <json string>}}]}`
   - `ai.events.ToolCompleted` → `{"role":"tool","tool_call_id": id,"content": output}`
   - `ai.events.ToolFailed` → `{"role":"tool","tool_call_id": id,"content": {"error": msg}}`
   (Compare the Responses versions at `openai/ns_internal/responses.baml:56-114`; both sys_llm and the engine are **no help** here — neither implements it.)
6. **Tools** → `tools: [{"type":"function","function":{"name","description","parameters": t.input_schema}}]` + `tool_choice: "auto"`. Note the nesting differs from Responses (`openai/ns_internal/responses.baml:137-153`). Source: `ai.tools.Tool` (`ai/ns_tools/tools.baml:10-40`).
7. **URL**: `${base_url ?? default}/chat/completions`; Azure `${base}/chat/completions?api-version=${api_version}`; Azure base from `resource_name`/`deployment_id` when `base_url` is null (`chat_completions.rs:112-127`).
8. **Auth headers**: bearer for openai/generic/ollama/openrouter; `api-key` for azure (`auth_request/mod.rs:99-112`). Env fallbacks: `OPENAI_API_KEY`, `AZURE_OPENAI_API_KEY`, `OPENROUTER_API_KEY`.
9. **Extra headers / query params**: needs new client fields; percent-encoding must be written by hand (no stdlib encoder — see §10).
10. **Azure `max_tokens` default 4096** with the engine's guard conditions (`clients/openai.rs:204-219`).
11. **Streaming body**: `stream: true` **and** `stream_options: {include_usage: true}` for `openai` (fix the sys_llm gap).

### 9.2 Response

Envelope classes (only what's read; `from_string` ignores unknowns — see `openai/ns_internal/responses.baml:5-6`):

```
class CcFunction  { name: string?, arguments: string? }
class CcToolCall  { id: string?, type: string?, function: CcFunction? }
class CcMessage   { content: string?, tool_calls: CcToolCall[]? }   // + parts/images variants
class CcChoice    { index: int?, message: CcMessage?, finish_reason: string? }
class CcUsageDetails { cached_tokens: int? }
class CcUsage     { prompt_tokens: int?, completion_tokens: int?, total_tokens: int?, prompt_tokens_details: CcUsageDetails? }
class CcResponse  { model: string?, choices: CcChoice[], usage: CcUsage? }
```

Parse → `ai.ModelTurn`:
- `content` string → `ai.content.Text`
- each `tool_calls[i]` → `ai.content.ToolUse { id, name, args: baml.json.from_string<map<string,unknown>>(arguments) }` (**new work**)
- stop reason: `tool_calls → StopReason.ToolUse`, `length → MaxTokens`, `content_filter → Refused`, `stop → Complete` (map from `parse_response/openai/chat_completions.rs:169-175` + `ai/ns_content/content.baml:27-32`)
- usage → `ai.events.Usage { input_tokens, output_tokens, cached_input_tokens: usage.prompt_tokens_details.cached_tokens, reasoning_tokens: null }` (`ai/ns_events/events.baml:36-41`)
- reject `choices.length() != 1` (both prior impls do)
- optional: port the parts-array and `message.images[]` handling (`parse_response/openai/chat_completions.rs:202-299`) once `ai.content` grows a media block — it has none today (`ai/ns_content/content.baml:23`).

Errors: reuse `ai.wire.send_as<CcResponse>(req, provider)` which already classifies non-2xx via `ai.errors.classify_http` (`ai/ns_wire/wire.baml:7-32`, `ai/ns_errors/errors.baml:124-137`).

### 9.3 Streaming

Decoder over `ai.stream.decode_sse_batch` (`ai/ns_stream/stream.baml:26-28`):
- skip `data: [DONE]` → emit `ai.stream.TurnDone`
- `choices[0].delta.content` → `ai.stream.TextDelta`
- `choices[0].finish_reason` → `ai.stream.TurnMeta { stop_reason }`
- top-level `usage` → `TurnMeta { input_tokens, output_tokens }`
- wrap with `ai.stream.TurnStream.from_sse(sse, decode)` and `baml.http.fetch_sse` (pattern: `openai/ns_internal/responses.baml:253-315`).
- `TurnStream.final_turn()` only ever emits a single `Text` block (`ai/ns_stream/stream.baml:214-238`) — streamed tool calls cannot be represented without a stdlib change.

### 9.4 Per-provider wrappers

| Wrapper | base_url default | auth | roles | notes |
|---|---|---|---|---|
| `OpenAiChatClient` | `https://api.openai.com/v1` | `Bearer` / `OPENAI_API_KEY` | default `system` | send `stream_options.include_usage`; array content parts |
| `OpenAiGenericClient` | **required** (error if unset) | `Bearer` if key present, else no header | default `system` | collapse all-text content to a string |
| `AzureOpenAiClient` | `base_url` **or** `resource_name`+`deployment_id` (XOR, validated in `new()`) | `api-key` header / `AZURE_OPENAI_API_KEY` | default `system` | `?api-version=` query; `max_tokens` 4096 guard |
| `OllamaClient` | `http://localhost:11434/v1` | `Bearer` only if key given | default **`user`**, keep `system` allowed | string content; no `stream_options` |
| `OpenRouterClient` | `https://openrouter.ai/api/v1` | `Bearer` / `OPENROUTER_API_KEY` | default `system` | `HTTP-Referer` / `X-Title` via a headers field (`ENG/…/clients/openai.rs:406-422`) |

Register any shorthand prefixes in `NEW/crates/baml_compiler2_ast/src/lower_cst.rs:744-753` (today only `openai`, `anthropic`, `google`, `claude-code`). Note the existing diagnostic at `lower_cst.rs:795-804` already tells users to reach for `openai.OpenAiClient.new(base_url = …)` for "OpenAI-compatible endpoints" — but that class is the **Responses** client, so the advice is currently wrong for ollama/vLLM/openrouter. Adding a chat client fixes that message.

---

## 10. What cannot be pure BAML (today)

1. **Media in prompts — hard blocker.** `ai.Prompt.messages()` returns `role: string, content: string` (`ai/spec.baml:7-13`), and the VM flattens media into `PromptAstSimple::to_text()` → `MediaValue`'s `Display` (`baml_builtins2/src/adt.rs:95-107,132-139`; `baml_builtins2/src/media.rs:209-234`), which is lossy (base64 is truncated to `base64(abcde...vwxyz, len=N)`). No image/audio/PDF can be lowered. Requires a stdlib change: a structural accessor (e.g. `Prompt.parts()` returning a `Text | Image | Audio | Pdf | Video` union), backed by `bex_vm/src/package_baml/prompt.rs:217-236`.
   *Once that exists*, the rest of media handling **is** expressible in BAML: `Image.url()/.base64()/.mime_type()` exist (`baml/ns_media/media.baml`), URL→base64 is `baml.http.fetch(u).bytes().to_base64()` (`baml/ns_http/http.baml:130-137`, `baml/uint8array.baml:107`), file→base64 is `baml.fs.open(path, "r").bytes().to_base64()` (`baml/ns_fs/fs.baml:17,72-77` — there is no module-level `read_bytes`, only `File.bytes`/`File.read_bytes`).
2. **Media in *responses*.** `ai.content.Block = Text | Reasoning | ToolUse` (`ai/ns_content/content.baml:23`) has no media block, so `message.images[]` / image content parts (`parse_response/openai/chat_completions.rs:231-299`) cannot round-trip. Needs an `ai.content.Media` variant.
3. **Streaming tool calls.** `ai.stream.StreamEvent = TextDelta | TurnMeta | TurnDone` (`ai/ns_stream/stream.baml:52`) and `TurnStream.final_turn()` hardcodes a single `Text` block (`:233-237`). Delta-accumulated `tool_calls` need new event variants.
4. **SSE timeouts.** `baml.http.fetch_sse(request)` takes no timeout (`baml/ns_http/http.baml:165-167`), unlike `send`/`fetch`. TTFT/idle timeouts (engine `helpers.rs:508-546`) are unreachable. Non-streaming `request_timeout_ms` *is* reachable via `baml.http.send(req, timeout)` (`:150-155`).
5. **Percent-encoding of query params.** sys_llm uses the `url` crate (`build_request/mod.rs:117-125`); no BAML stdlib encoder exists (`ns_net/net.baml` has none; `baml/string.baml` has no `encode`/`escape`). Either add one or restrict query params to already-safe values (`api-version=2024-02-15-preview` is safe).
6. **Playground/WASM proxy rewrite.** Reads `BOUNDARY_PROXY_URL` (`build_request/mod.rs:135-153`) and is `cfg(target_arch = "wasm32")`-gated. `baml.env.get` works in BAML, but the "wasm only" conditional and the `baml-original-url` convention are host policy, not client policy — decide whether native clients opt in explicitly.
7. **Compile-time validation of Azure's base_url/resource_name XOR.** The engine reports per-key spans (`clients/openai.rs:282-314`); a BAML `new()` can only `throw baml.errors.InvalidArgument` at runtime — same as sys_llm's runtime check (`baml_std.rs:65-81`).
8. **Retry/fallback/round-robin** are *already* native (`ai/ns_clients/clients.baml:49-190`), so nothing is lost there.

---

## 11. Open questions for the plan

1. Do we keep the `content: [{type:"text"}]` array shape for `openai`, or match the engine's per-provider split (string for `openai-generic`)? Servers differ; a client field is the safest.
2. One `ChatCompletionsClient` class with provider-flavor fields, or five classes? The Responses client is one class per API today, and the shorthand table (`lower_cst.rs:744-753`) maps prefix → class, favoring separate classes.
3. Do we port the sys_llm-only `modalities` image-output feature (`lib.rs:383-419`)? It depends on the *return type*, which a native client sees only as `input.output_type` (`ai/turn.baml:12`) — reachable via `reflect`, but nontrivial.
4. `allowed_role_metadata` and `cache_control`: which node does the metadata go on, and does the native prompt even carry per-message metadata? `ai.PromptMessage` has no metadata field (`ai/spec.baml:7-13`), so this whole feature is currently unrepresentable natively.
5. Should the native chat client reject `choices.length() != 1` (both prior impls do), or take `choices[0]`?
