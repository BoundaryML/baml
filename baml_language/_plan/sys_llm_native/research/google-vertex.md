# `google-ai` / `vertex-ai` → native BAML: parity research

Scope: what `sys_llm` (Rust) does today, what the OLD engine did (the parity
reference), and what the EXISTING native `baml_std/google/` does — so the
migration plan knows exactly what must be built, what may be dropped, and what
cannot leave Rust.

Paths are relative to `/Users/aaron/projects/baml/baml_language` unless prefixed
`engine/`, which means `/Users/aaron/projects/baml/engine`.

Three implementations under comparison:

| # | Name | Entry point |
|---|------|-------------|
| A | **sys_llm** (current Rust runtime of the new compiler) | `crates/sys_llm/src/build_request/google.rs`, `parse_response/google.rs`, `auth_request/vertex.rs` |
| B | **engine** (original, full implementation) | `engine/baml-runtime/src/internal/llm_client/primitive/{google,vertex}/`, options in `engine/baml-lib/llm-client/src/clients/{google_ai,vertex}.rs` |
| C | **native BAML** (target shape) | `crates/baml_builtins2/baml_std/google/gemini.baml`, `google/ns_internal/gemini.baml` |

---

## 0. Executive summary

1. **`google-ai`/`vertex-ai` are NOT "deferred" in sys_llm.** The doc comments at
   `crates/sys_llm/src/provider.rs:41` and `:46` say "deferred", but
   `crates/sys_llm/src/build_request/mod.rs:99-106` dispatches both to
   `google::build_request`, `parse_response/mod.rs:158-159` parses both, and
   `auth_request/mod.rs:42-45` authenticates both. The comments are **stale**;
   non-streaming Gemini on both backends works today.
2. **Streaming does NOT work for either provider in sys_llm.** Two independent
   blocks: `stream_accumulator.rs:73-89` rejects every provider except
   openai-family + anthropic with `NotImplemented`, and the stream request path
   (`lib.rs:339-351` → `add_stream_flag_to_request_body`, `lib.rs:509-518`)
   injects `"stream": true` into the JSON body — which Gemini ignores; Gemini
   needs the `:streamGenerateContent?alt=sse` **URL suffix**
   (engine `googleai_client.rs:223-226`). sys_llm's `resolve_url`
   (`build_request/google.rs:110-134`) has no `stream` parameter at all.
   The native `gemini.baml:271-275` already does this correctly.
3. **Vertex auth is the single hard blocker for a pure-BAML port.** It needs
   RSASSA-PKCS1-v1_5/SHA-256 JWT signing over a PKCS#8 PEM private key
   (`forks/google-cloud-auth/src/lib.rs:420-447`). **BAML's stdlib has no crypto
   surface at all** — no hash, no HMAC, no RSA, no base64url; the only related
   primitive is `baml.Uint8Array.to_base64` / `from_base64`
   (`crates/baml_builtins2/baml_std/baml/uint8array.baml:97,107`), which is
   standard base64, not URL-safe. Verdict in §7.
4. **The three implementations disagree on the response shape**, and sys_llm's
   `vertex-ai` parser is the odd one out: it takes `parts[0].text` only with no
   thought filtering (`parse_response/google.rs:146-156`), so a Gemini 2.5
   thinking model on Vertex returns the *thought* text as the answer. Engine has
   the same bug (`engine/.../vertex/response_handler.rs:66-70`). `google-ai`
   filters thoughts and joins (`parse_response/google.rs:56-68`). Native
   `gemini.baml:297-322` handles thoughts properly for BOTH backends.
5. **Native BAML is already AHEAD on tools, streaming, blockReason, and
   reasoning; BEHIND on media, Vertex entirely, and on option plumbing**
   (extra body / headers / query params / safety settings / finish-reason
   filters). Details in §6.

---

## 1. Provider routing and the `enterprise` switch

### sys_llm

- `LlmProvider` enum: `crates/sys_llm/src/provider.rs:41-47` (`google-ai`,
  `vertex-ai`; the "deferred" comments are stale, see §0.1).
- `google_use_enterprise` (`build_request/mod.rs:41-57`): only meaningful for
  `provider == "google-ai"`; reads `GoogleAiOptions.enterprise`, and only if
  **unset** falls back to `GOOGLE_GENAI_USE_ENTERPRISE` (truthy = `"true"`/`"1"`,
  trimmed, case-insensitive — `build_request/mod.rs:24-29`). An explicit
  `enterprise false` beats the env var.
- Routing (`build_request/mod.rs:78-83`): `GoogleAi` is rewritten to `VertexAi`
  when `google_use_enterprise(...)` **or** `GOOGLE_GENAI_USE_VERTEXAI` is truthy.
  This happens *before* media resolution and request building, so URL, auth, and
  media handling all follow the Vertex path.
- Provider **defaults** are also switched: `baml_std.rs:55-63` picks
  `defaults_provider = VertexAi` when a `google-ai` client has
  `enterprise == Some(true)`, so `apply_provider_defaults` gives it Vertex media
  handlers / remap rules instead of Google AI Studio ones. NOTE: this
  compile-time-ish switch only honors the **explicit option**, not
  `GOOGLE_GENAI_USE_ENTERPRISE`/`GOOGLE_GENAI_USE_VERTEXAI` (those are only read
  at request time in `build_request/mod.rs:78-83`) — so an env-var-only flip gets
  Vertex URL+auth but **Google-AI-Studio media handlers and defaults**. This is a
  latent inconsistency worth carrying (or fixing) in the port.
- `GoogleAiOptions` can carry Vertex fields; `ProviderOptions::vertex_ai()`
  (`baml_std.rs:165-176`) projects `GoogleAiOptions{credentials,
  credentials_content, location, project_id}` onto `VertexAiOptions`.
- Option schemas: `crates/baml_builtins2/baml_std/baml/ns_prompt/sys_llm_types.baml:47-60`
  (`GoogleAiOptions{enterprise, credentials, credentials_content, location,
  project_id}`, `VertexAiOptions{credentials, credentials_content, location,
  project_id}`).

### engine

No enterprise switch. `google-ai` and `vertex-ai` are entirely separate client
types (`GoogleAIClient`, `VertexClient`) with separate option structs. The
`enterprise` alias is a **new-compiler-only feature** (google-genai parity),
documented in `crates/baml_builtins2/keyword_docs/baml_keywords.yaml:221-241`.

### native BAML

**Nothing.** `google/gemini.baml:1-33` defines `GoogleClient{model, api_key,
base_url}` only. No enterprise, no Vertex, no credentials, no location/project.

---

## 2. URL construction

| Aspect | sys_llm | engine | native BAML |
|---|---|---|---|
| Google AI base URL default | `https://generativelanguage.googleapis.com/v1beta` (`build_request/google.rs:107-108`), applied at request time (`baml_std.rs:230-233` leaves it `None` so routing can still flip it) | same default, applied at option-resolution time (`engine/baml-lib/llm-client/src/clients/google_ai.rs:181-183`) | same literal, inline (`google/ns_internal/gemini.baml:278`) |
| Google AI path | `{base}/models/{model}:generateContent` (`google.rs:119`) | `{base}/models/{model}:{generateContent\|streamGenerateContent?alt=sse}` (`engine/.../googleai_client.rs:223-233`) | `{base}/models/{model}:{generateContent\|streamGenerateContent?alt=sse}` (`gemini.baml:271-278`) |
| Vertex base URL | explicit `base_url`, else built from location+project (`google.rs:125-131,182-187`) | `BaseUrlOrLocation` — `base_url` XOR `location`, enforced as a compile error if both/neither (`engine/baml-lib/llm-client/src/clients/vertex.rs:399-422`) | — |
| Vertex host | `{location}-aiplatform.googleapis.com`, or `aiplatform.googleapis.com` when `location == "global"` (`google.rs:172-176`) | identical (`engine/.../vertex_client.rs:256-260`) | — |
| Vertex publisher path | `/v1/projects/{project}/locations/{location}/publishers/google/models` (`google.rs:184-186`); `publishers/anthropic/models` for `claude*` models (`google.rs:193-206`, selected by `is_anthropic_model` at `build_request/mod.rs:18-20,101`) | same, but publisher is chosen by `anthropic_version.is_some()` rather than the model name (`engine/.../vertex_client.rs:285-289`) | — |
| Vertex RPC verb | `generateContent` always; `rawPredict` for claude (`build_request/mod.rs:215`) | `generateContent` / `streamGenerateContent?alt=sse` / `rawPredict` / `streamRawPredict` (`engine/.../vertex_client.rs:296-309`) | — |
| Unresolved location/project | placeholders `__baml_vertex_location__` (lowercase, because it lands in the URL **host** which `url::Url` lowercases) and `__BAML_VERTEX_PROJECT_ID__` (`google.rs:138-146`), substituted during auth (`auth_request/vertex.rs:83-134`) | resolved eagerly during `build_request` by awaiting `VertexAuth` (`engine/.../vertex_client.rs:261-284`) | — |
| Explicitly-empty location/project | honored as-is, produces a deliberately broken URL so the misconfig surfaces (`google.rs:162-170` + test `google.rs:451-480`) | `project_id` resolving to `""` or `"$VAR"` becomes `None` (`engine/baml-lib/llm-client/src/clients/vertex.rs:320-330`) | — |
| Query params | appended + percent-encoded via `url::Url` after building (`build_request/mod.rs:117-125`) | appended raw (values NOT encoded — `engine/.../vertex_client.rs:311-330`), Google AI has no query-param option at all | — |
| Proxy (`BOUNDARY_PROXY_URL`) | wasm-only rewrite, sets `baml-original-url` (`build_request/mod.rs:135-153,164-185`) | `proxy_url` option on both clients (`engine/.../googleai_client.rs:235-241`, `vertex_client.rs:332-344`) | — |

**Native gap:** no Vertex URL construction, no `location`/`project_id`, no
publisher selection, no `rawPredict`.

---

## 3. Authentication

### 3.1 `google-ai`

- sys_llm: `auth_request/mod.rs:57-73` sets header `x-goog-api-key`. **Hard error
  if `options.api_key` is absent** — message at `auth_request/mod.rs:55`. There
  is **no env fallback**: `LlmProvider::default_api_key_env_var`
  (`provider.rs:62-68`) returns `Some` only for OpenAI/Anthropic, and no Rust
  code in the new compiler reads `GOOGLE_API_KEY` (verified by repo-wide grep).
  **This contradicts the shipped docs**, `keyword_docs/baml_keywords.yaml:235`
  ("either `options.api_key` or `GOOGLE_API_KEY`"), and contradicts the engine
  (`engine/baml-lib/llm-client/src/clients/google_ai.rs:175` defaults
  `api_key` to `StringOr::EnvVar("GOOGLE_API_KEY")`). **Real divergence to fix in
  the port.**
- engine: header set at `engine/.../googleai_client.rs:256-259`.
- native: `gemini.baml:280` — `c.api_key ?? baml.env.get_or_panic("GOOGLE_API_KEY")`.
  Already matches the ENGINE behavior (and the docs), i.e. the native client is
  *more* correct than sys_llm here. Note it uses `get_or_panic`, not the
  `api_key_env` indirection that `OpenAiClient` grew
  (`openai/responses.baml:6-12,31-42`) — worth adopting for consistency.

### 3.2 `vertex-ai` — what sys_llm does

`crates/sys_llm/src/auth_request/vertex.rs:54-158`:

1. If `query_params` contains `key` (Vertex "express mode" API key) **and** no
   URL placeholders remain → return immediately, no credentials touched
   (`vertex.rs:66-77,136-139`).
2. Resolve the `location` placeholder from `GOOGLE_CLOUD_LOCATION`; if unset and
   the client is in enterprise mode, default to `"global"` (google-genai parity)
   and rewrite the host to the region-less form; otherwise hard error
   (`vertex.rs:83-113`).
3. Resolve credentials **once** (`vertex.rs:116`, `resolve_credentials` at
   `vertex.rs:247-255`): `options.credentials` (a **file path**) → then
   `options.credentials_content` (inline JSON) → then ADC. An explicitly-set
   option is used as-is: a broken value is an error, never a cascade
   (`vertex.rs:16-17,302-304`).
4. Resolve the `project_id` placeholder from that credential, else the
   google-auth chain (`vertex.rs:119-134,305-324`).
5. Mint an OAuth2 access token and set `authorization: Bearer <token>`
   (`vertex.rs:141-145`).
6. Set `x-goog-user-project` from the quota project when the credential carries
   one and the header isn't already present (`vertex.rs:149-155,328-352`).

All token IO is routed through the sandbox: `BamlTokenIo` bridges
`google_cloud_auth::TokenIo` to BAML's `RuntimeIo`
(`vertex.rs:166-218`) — `env_get`, `fs_open`+`fs_file_text`, and `http__send`.

### 3.3 `vertex-ai` — the token minter (`forks/google-cloud-auth`)

A slim pure-Rust fork, `forks/google-cloud-auth/src/lib.rs` (1048 lines), whose
whole job is minting Google Cloud access tokens. Surface:

- `TokenIo` trait — env / read_file / http (`lib.rs:83-98`).
- Scope + endpoints: `CLOUD_PLATFORM_SCOPE`, `oauth2.googleapis.com/token`,
  `sts.googleapis.com/v1/token`, GCE metadata token + project URLs
  (`lib.rs:55-63`).
- Process-wide token cache keyed by SHA-256 over credential material + scope,
  re-minting inside a 225s refresh threshold (`lib.rs:65-67,146-188`).
- Credential-type dispatch on the JSON `type` field (`lib.rs:362-389`):
  `service_account`, `authorized_user`, `external_account` (workload identity
  federation), `external_account_authorized_user`, `impersonated_service_account`.
  Explicitly unsupported: `gdch_service_account`, AWS-sourced WIF,
  executable-sourced WIF (`lib.rs:377-383,497-508`).
- **Service account flow** (`lib.rs:402-447`): build `{alg:RS256,typ:JWT}` header
  + `{iss,scope,aud,iat,exp}` claims, base64**url**-no-pad each, parse the PEM via
  `RsaPrivateKey::from_pkcs8_pem`, sign with
  `rsa::pkcs1v15::SigningKey::<sha2::Sha256>`, POST the JWT-bearer grant.
- ADC chain (`lib.rs:247-274`): `GOOGLE_APPLICATION_CREDENTIALS` file → well-known
  ADC config file → GCE metadata server. A set-but-unreadable GAC is an error,
  not a fallthrough.
- `project_id` chain (`lib.rs:281-315`): `GOOGLE_CLOUD_PROJECT` / `GCLOUD_PROJECT`
  → GAC file → gcloud `core.project` (read from disk, no shell-out,
  `lib.rs:853-891`) → ADC file → metadata server.
- Quota project (`lib.rs:326-357`) backing `x-goog-user-project`.

### 3.4 `vertex-ai` — what the engine does (parity reference)

Two implementations behind a cfg:

- **native** (`engine/.../vertex/std_auth.rs`): delegates entirely to the
  `gcp_auth` crate — `CustomServiceAccount::from_file`/`from_json`,
  `ConfigDefaultCredentials`, `MetadataServiceAccount`, `GCloudAuthorizedUser`
  (`std_auth.rs:17-22,75-164`), with a process-wide `AUTH_CACHE`
  (`std_auth.rs:14-15,54-73`). SystemDefault tries ADC → metadata → gcloud in
  order, collecting errors (`std_auth.rs:112-162`).
- **wasm** (`engine/.../vertex/wasm_auth.rs`): hand-rolls the same JWT flow —
  `Claims{iss,scope,aud,exp,iat}` (`wasm_auth.rs:109-133`), `encode_jwt` via
  WebCrypto (`crate::internal::wasm_jwt`), then a `urn:ietf:params:oauth:grant-type:jwt-bearer`
  form POST to `token_uri` (`wasm_auth.rs:143-167`). SystemDefault delegates to a
  JS callback provider (`wasm_auth.rs:68-75,81-89`).
- Option resolution (`engine/baml-lib/llm-client/src/clients/vertex.rs:28-142`):
  `credentials` (string → file path OR inline JSON by "does it parse as JSON",
  `:87-93`), `credentials` as a JSON **object** (`:94-100`),
  `credentials_content` (`:101-104`), else env fallback to
  `GOOGLE_APPLICATION_CREDENTIALS` then **`GOOGLE_APPLICATION_CREDENTIALS_CONTENT`**
  (`:110-133`), else SystemDefault. Plus `try_unwrap_quoted_json`
  (`vertex.rs:151-165`) for `vercel env pull`-style double-quoted JSON.
- Bearer + scope applied at `engine/.../vertex_client.rs:355-370`; API-key
  express mode via `query_params["key"]` short-circuits it (`:250,358`), and
  requires an explicit `project_id` when using location-style URLs (`:264-268`).

### 3.5 Auth parity gaps (sys_llm vs engine)

| Feature | engine | sys_llm | Note |
|---|---|---|---|
| `credentials` as inline JSON string | yes (`vertex.rs:87-93`) | **no** — always a file path (`auth_request/vertex.rs:9-12,20-22,248-250`) | deliberate |
| `credentials` as a JSON object | yes (`vertex.rs:94-100`) | **no** | deliberate |
| `GOOGLE_APPLICATION_CREDENTIALS` as inline JSON | yes (`vertex.rs:120-124`) | **no** — file path only (`forks/.../lib.rs:820-826`) | deliberate |
| `GOOGLE_APPLICATION_CREDENTIALS_CONTENT` env | yes (`vertex.rs:126-133`) | **no** | **gap** |
| double-quoted JSON unwrapping | yes (`vertex.rs:151-165`) | **no** | gap (only matters for inline JSON, which is already gone) |
| gcloud CLI shell-out | yes via `GCloudAuthorizedUser` (`std_auth.rs:141`) | **no** — reads the gcloud config file directly (`forks/.../lib.rs:853-891`) | deliberate |
| workload identity federation | no (gcp_auth 0.x doesn't) | **yes** (`forks/.../lib.rs:477-616`) | sys_llm ahead |
| impersonated service account | no | **yes** (`forks/.../lib.rs:654-737`) | sys_llm ahead |
| `x-goog-user-project` quota header | no | **yes** (`auth_request/vertex.rs:149-155`) | sys_llm ahead |
| token caching | yes (`std_auth.rs:14`) | yes, hashed cache key (`forks/.../lib.rs:155-188`) | parity |

---

## 4. Request body

### 4.1 Message lowering

| Aspect | sys_llm | engine | native BAML |
|---|---|---|---|
| system → `systemInstruction` | ALL system messages collected into one `systemInstruction.parts` (`build_request/google.rs:216-253`); casing is **camelCase** (`google.rs:63,67`) | only if the **first** message is system; other system messages stay in `contents` (`engine/.../googleai_client.rs:318-347`, `vertex_client.rs:517-555`); casing is **snake_case** `system_instruction` | ALL system messages collected (`gemini.baml:78-104,241-248`); camelCase; PLUS a fallback: if the first non-system role isn't `user`, the system parts are prepended as a leading `user` content instead (`gemini.baml:233-240`) |
| Multiple system messages in practice | the full pipeline runs `consolidate_system_prompts` first (`specialize_prompt/mod.rs:33`, `transformations.rs:158-200`) which downgrades the 2nd+ system message to `user` when `max_one_system_prompt` (true for both Google providers, `model_features.rs:73-77`), and rewrites a lone system message to `user` (`transformations.rs:179-185`) | `max_one_system_prompt: true` on both clients (`googleai_client.rs:136`, `vertex_client.rs:160`) | no equivalent normalization — native trusts `ai.Prompt` |
| `assistant` → `model` remap | via `remap_roles` defaults (`baml_std.rs:278-287`): always for `google-ai`; for `vertex-ai` only when the model is NOT `claude*` | via `remap_role()` (`engine/baml-lib/llm-client/src/clients/google_ai.rs:98-109`: assistant→model unless `model` is an allowed role); Vertex has NO default remap (`clients/vertex.rs:238-240`) — **engine Vertex+Gemini does not remap assistant→model by default**, sys_llm does | done inline in `google_lower_prompt` (`gemini.baml:88-92`) — and note anything not `system`/`assistant` collapses to `"user"`, so a stray role is silently coerced |
| Journal / multi-turn tool history | n/a — sys_llm gets a flat `PromptAst` | n/a | full: `google_lower_journal` (`gemini.baml:138-212`) lowers user/assistant/tool events, merges consecutive tool results into ONE user content of `functionResponse` parts, and correlates results to tool NAMES via `_gm_tool_name_for` (`gemini.baml:108-132`) since Gemini has no call ids |

### 4.2 Media parts

| Aspect | sys_llm | engine | native BAML |
|---|---|---|---|
| inline base64 | `inlineData{mimeType,data}` (camelCase) whenever base64 is available — covers Base64, prefetched URL, resolved File (`build_request/google.rs:273-303`) | google-ai: `inline_data{mime_type,data}` snake_case (`googleai_client.rs:366-375`); vertex: `inlineData{data,mimeType}` camelCase (`vertex_client.rs:490-499`) | **none** |
| remote URL | `fileData{mimeType,fileUri}` (`google.rs:290-297`) | google-ai `file_data{file_uri,mime_type}` with mime only if known (`googleai_client.rs:376-384`); vertex `fileData{fileUri,mimeType}` with a `video/mp4` default (`vertex_client.rs:470-489`) | **none** |
| missing mime | hard error (`build_request/mod.rs:226-232`) | google-ai omits it; vertex defaults video to `video/mp4` | n/a |
| unresolved File | `FileNotResolved` error (`google.rs:298-300`) | `anyhow::bail!` (`googleai_client.rs:385-388`) | n/a |
| URL resolution policy | `google-ai`: image `send_base64_unless_google_url`, audio/video/pdf `send_base64` (`baml_std.rs:311-316`); `vertex-ai` non-claude: image/audio `send_url_add_mime_type`, video/pdf `send_url` (`baml_std.rs:331-338`); claude-on-vertex uses the Anthropic set (`baml_std.rs:322-330`). `gs://` URLs skip the base64 fetch (`resolve_media.rs:177`) | same defaults (`googleai_client.rs:137-156`, `vertex_client.rs:161-181`) | none — `ai.PromptMessage.content` is a plain `string` and media is a "readable placeholder" (`baml describe ai.PromptMessage`, `ai/spec.baml:7`) |

**This is the single biggest native gap: the native client has no media path at
all, and the `ai.Prompt` surface does not currently expose structured media.**

### 4.3 Generation config / safety settings / tools

| Aspect | sys_llm | engine | native BAML |
|---|---|---|---|
| `generationConfig`, `safetySettings`, `tools`, anything else | pass-through only: `client.request_body` is flattened at the TOP level of the body via `#[serde(flatten)] extra` (`build_request/google.rs:64-70,90`; conversion at `baml_std.rs:104-112`). Users must write `request_body { generationConfig { ... } }` themselves (see test `google.rs:745-781`). No typed surface, no key normalization, no validation | same pass-through: `json!(self.properties)` is the base object (`engine/.../googleai_client.rs:261-262`, `vertex_client.rs:376`), but the engine ALSO carries typed structs `GenerationConfig`, `SafetySetting`/`HarmCategory`/`HarmBlockThreshold`, `Tool`/`FunctionDeclaration`/`Schema`, `Retrieval`/`VertexAiSearch` (`engine/.../google/types.rs:5-136`) — declared but **not** wired into request building | **no pass-through at all** — the body is fully constructed (`gemini.baml:232-270`); no temperature, no maxOutputTokens, no safetySettings, no extra headers, no query params |
| tools / function calling | **none** — the request has no `tools`, and the response parser has no `functionCall` (`parse_response/google.rs:27-31`) | types exist but unused | **yes**: `tools[0].functionDeclarations[]` with `{name, description, parametersJsonSchema}` plus `toolConfig.functionCallingConfig.mode = "AUTO"` (`gemini.baml:250-270`). Note it uses `parametersJsonSchema`, not the older `parameters` |
| claude-on-vertex body | Anthropic Messages body + `anthropic_version: "vertex-2023-10-16"` (defaulted) + `max_tokens` defaulted to `DEFAULT_MAX_TOKENS` (`build_request/mod.rs:196-223`) | same, via a synthetic `AnthropicClient` (`engine/.../vertex_client.rs:378-396`), version default from the model prefix (`vertex_client.rs:64-78`), and `stream: true` added to the BODY when streaming raw-predict (`vertex_client.rs:398-403`) | none |
| user headers | lower-cased and applied over provider defaults (`build_request/mod.rs:112-115`) | applied as given (`googleai_client.rs:252-255`, `vertex_client.rs:372-374`) | fixed two headers only (`gemini.baml:279-282`) |

---

## 5. Response parsing

### 5.1 Non-streaming

| Aspect | sys_llm google-ai | sys_llm vertex-ai | engine google-ai | engine vertex-ai | native BAML |
|---|---|---|---|---|---|
| candidate count | must be exactly 1, else `NoContent` (`parse_response/google.rs:80-88`) | same (`:134-142`) | same (`google/response_handler.rs:50-65`) | same (`vertex/response_handler.rs:49-64`) | takes `candidates?.at(0)`, tolerates 0 or many (`gemini.baml:294`) |
| text extraction | filter `thought == true`, join the rest (`:56-68,99`) | **`parts[0].text` only, no thought filter** (`:146-156`) | filter thoughts, join (`response_handler.rs:108-120`) | **`parts[0].text` only** (`vertex/response_handler.rs:66-70`) | walks all parts; `thought` → `ai.content.Reasoning`, else `ai.content.Text` (`gemini.baml:297-322`) |
| empty content | google-ai: empty string is OK (`unwrap_or_default()`, `:99`); missing `content` object → error (`:92-97`) | missing part → error (`:153-156`) | empty OK, missing `content` → error | missing → error | no error; empty block list |
| tool calls | not parsed | not parsed | `FunctionCall` type exists (`google/types.rs:267-271`) but is never read by the handler | same | parsed into `ai.content.ToolUse` with a synthesized id `call_{seq}_{i}` since Gemini sends none (`gemini.baml:288-289,298-308`) |
| finish reason | `STOP`→Stop, `MAX_TOKENS`→Length, else `Other(raw)`; raw preserved (`:45-52,101,119`) | same | raw string kept + `baml_is_complete = (reason == "STOP")` (`response_handler.rs:93-99`) | same (`vertex/response_handler.rs:97-101`) | mapped to `ai.content.StopReason`: ToolUse if any call, MaxTokens for `MAX_TOKENS`, Refused for `SAFETY`/`RECITATION`/`PROHIBITED_CONTENT`/blocked, else Complete (`gemini.baml:324-332`) |
| `promptFeedback.blockReason` | **not parsed** | **not parsed** | typed but not read by the handler (`google/types.rs:142,146-166`) | not read | **parsed and honored** (`gemini.baml:32-34,39,323`) |
| usage | prompt/candidates/total/cachedContent (`:103-112`) | same (`:160-169`) | same four (`response_handler.rs:100-103`) | same, but `usage_metadata.clone().unwrap()` — **panics if `usageMetadata` is absent** (`vertex/response_handler.rs:86`) | prompt + candidates only; `cached_input_tokens`/`reasoning_tokens` hardcoded `null` (`gemini.baml:333-342`) — misses `cachedContentTokenCount` and `thoughtsTokenCount` |
| `modelVersion` | not read (`model: None`, `:117`) | not read | not read (uses the requested model name) | not read | not read |
| finish-reason allow/deny filter | `PrimitiveClient::is_finish_reason_allowed` (`baml_std.rs:125-142`) + `execute_validate_finish_reason` (`lib.rs:521-540`) | same | `finish_reason_filter` on both clients | same | **none** |

### 5.2 Streaming

- **sys_llm: not supported for either provider.** `new_accumulator` rejects
  anything outside openai-family+anthropic (`stream_accumulator.rs:73-89`), and
  the request path would produce a body-level `"stream": true` against a
  `:generateContent` URL (`lib.rs:339-351,509-518`), which Gemini ignores.
- **engine: fully supported.** `stream_chat` on both clients
  (`googleai_client.rs:101-118`, `vertex_client.rs:123-142`) selects
  `streamGenerateContent?alt=sse` (or `streamRawPredict` + body `stream: true`
  for claude-on-vertex, `vertex_client.rs:398-403`), and the scanners
  (`google/response_handler.rs:122-183`, `vertex/response_handler.rs:118+`)
  accumulate text with thought filtering and overwrite usage/finish reason from
  each chunk.
- **native: supported and closest to the engine.** `_google_request(..., stream:
  true)` swaps the URL suffix (`gemini.baml:221-285`), `fetch_sse`
  (`gemini.baml:433`), `_google_decode_batch` decodes each `data:` payload as a
  `GmResponse` fragment, skips thought parts, emits `ai.stream.TextDelta` and a
  `TurnMeta` carrying stop reason + cumulative token counts
  (`gemini.baml:366-427`). Gaps: **no streamed tool-call deltas** (only text) and
  malformed chunks are silently dropped (`gemini.baml:370-372`).
- Gemini's non-SSE **JSON-array** streaming format (`streamGenerateContent`
  without `alt=sse`) is not implemented anywhere in the three; all three
  implementations that stream use `alt=sse`. Nothing to port.

### 5.3 Error shapes

- sys_llm: `ParseResponseError::{Deserialize, NoContent, UnsupportedProvider}`
  (`parse_response/mod.rs`), auth/build failures as
  `BuildRequestError::{AuthorizationFailed, Other, …}`
  (`build_request/mod.rs:234-250`). Google API error bodies (`{"error":{"code",
  "message","status"}}`) are **not** decoded anywhere.
- engine: `LLMErrorResponse` with `ErrorCode::{UnsupportedResponse(2),
  Other(200)}` (`google/response_handler.rs:43,62`).
- native: HTTP status is classified by `ai.errors.classify_http`
  (`ai/ns_errors/errors.baml:124-137`): 429→`RateLimited`, 408/5xx→
  `NetworkFailure`, else `InvalidRequest`; body decode failure →`ParseFailed`
  (`ai/ns_wire/wire.baml:7-32`). Also no Google-specific error-body decoding.

---

## 6. Native `gemini.baml` gap list

Everything below is missing from `crates/baml_builtins2/baml_std/google/` today.

**Blocking for `google-ai` parity**

1. **Media parts** — no `inlineData` / `fileData`, and `ai.PromptMessage.content`
   is a flat string (`ai/spec.baml:7`). Needs an `ai.Prompt`-level media surface
   before the client can lower anything (`build_request/google.rs:273-303` is the
   target shape). Also needs the URL-resolution policies
   (`send_base64_unless_google_url`, `gs://` passthrough) —
   `baml_std.rs:311-316`, `resolve_media.rs:177`.
2. **Request-body pass-through** (`generationConfig`, `safetySettings`, anything
   else): sys_llm flattens `request_body` at the top level
   (`build_request/google.rs:64-70`). Native has no equivalent field on
   `GoogleClient` (`gemini.baml:1-14`).
3. **Custom headers + query params** — `build_request/mod.rs:112-125`. Native
   sets exactly two headers (`gemini.baml:279-282`).
4. **`api_key_env` / `base_url_env` indirection**, as `OpenAiClient` has
   (`openai/responses.baml:6-12,31-52`); `gemini.baml:280` hardcodes
   `GOOGLE_API_KEY` via `get_or_panic`.
5. **Usage fields**: `cachedContentTokenCount` and `thoughtsTokenCount` are
   dropped (`gemini.baml:27-30,333-342`); sys_llm carries cached tokens
   (`parse_response/google.rs:110`).
6. **Finish-reason allow/deny lists** (`baml_std.rs:125-142`).
7. **Prompt normalization**: no `max_one_system_prompt` consolidation
   (`transformations.rs:158-200`), no allowed-roles validation
   (`transformations.rs:233-282`), and any unknown role silently becomes `user`
   (`gemini.baml:88-92`).
8. **Streamed tool calls** — `_google_decode_batch` only emits text deltas
   (`gemini.baml:376-387`).
9. **`modelVersion`** from the response is never surfaced (all three drop it).

**Blocking for `vertex-ai` parity (everything is missing)**

10. A `VertexClient` class: `location` / `project_id` / `credentials` /
    `credentials_content` / `base_url` (schema reference:
    `sys_llm_types.baml:55-60`).
11. Vertex URL construction incl. the `global` host special-case and the
    publisher-path split (`build_request/google.rs:172-206`).
12. OAuth2 bearer auth + `x-goog-user-project` (`auth_request/vertex.rs:141-155`)
    — see §7.
13. Express-mode API-key auth via `query_params.key`
    (`auth_request/vertex.rs:66-77`).
14. `location`/`project_id` env resolution (`GOOGLE_CLOUD_LOCATION`,
    `GOOGLE_CLOUD_PROJECT`) — `auth_request/vertex.rs:83-134`,
    `forks/.../lib.rs:281-315`.
15. Claude-on-Vertex `rawPredict`/`streamRawPredict` with the Anthropic body
    (`build_request/mod.rs:196-223`, `engine/.../vertex_client.rs:296-309`).
16. The `enterprise` / `GOOGLE_GENAI_USE_VERTEXAI` backend switch
    (`build_request/mod.rs:41-57,78-83`) — including the Vertex-specific media
    handler / remap defaults it flips (`baml_std.rs:55-63`).

**Divergences the port must decide about (not strictly gaps)**

17. `systemInstruction` casing: sys_llm & native use camelCase; the engine used
    snake_case (`googleai_client.rs:324`). Both are accepted by the API.
18. sys_llm remaps `assistant`→`model` for Vertex+Gemini by default
    (`baml_std.rs:282-284`); the engine did NOT (`clients/vertex.rs:238-240`).
19. Native's "system-first fallback" (`gemini.baml:233-240`) has no counterpart
    in sys_llm or the engine — it exists because Gemini requires a leading user
    turn. Keep it.
20. sys_llm's `vertex-ai` parser reads only `parts[0]` with no thought filtering
    (`parse_response/google.rs:146-156`) — the native unified parse is strictly
    better; do NOT port the divergence.

---

## 7. Verdict: what Vertex auth requires (the pure-Rust surface)

**Vertex auth cannot be written in BAML today.** The mandatory piece is minting
an OAuth2 access token, and the service-account path — the most common
configuration and the only one that works without a GCE metadata server or a
prior `gcloud` login — requires **RSASSA-PKCS1-v1_5 over SHA-256 signing of a JWT
using a PKCS#8 PEM private key** (`forks/google-cloud-auth/src/lib.rs:419-447`).

Evidence for the missing capability:

- No crypto namespace exists. `baml describe baml` lists exactly: `baml.csv`,
  `env`, `errors`, `fs`, `future`, `glob`, `host`, `http`, `id`, `io`, `iter`,
  `json`, `media`, `net`, `ops`, `panics`, `prompt`, `random`, `sap`, `spawn`,
  `sys`, `time`, `toml`, `ws`, `yaml`. No `crypto`, `hash`, or `jwt`.
- Repo-wide grep of `crates/baml_builtins2/baml_std/` for
  `sha256|hmac|base64|jwt|crypto` matches only base64 helpers
  (`baml/uint8array.baml:97,107`) and unrelated docs. Those are **standard**
  base64; JWT needs base64url-no-pad (`forks/.../lib.rs:437-438,445`) — writable
  in BAML by post-processing (`+/` → `-_`, strip `=`), so base64url alone is not
  the blocker.
- `crates/sys_ops/src/lib.rs` (2466 lines) registers no hashing/signing op.

What *is* available in BAML and therefore does NOT need Rust:

- `baml.http.send` / `fetch_sse` (`baml/ns_http/http.baml:150-166`) — the OAuth2
  token POST, the STS exchange, the IAM `generateAccessToken` call, and the GCE
  metadata fetch are all plain HTTP.
- `baml.env.get` / `get_or_panic` (`baml/ns_env/env.baml:6,11`) — every env var
  in the ADC chain.
- `baml.fs.read` / `exists` (`baml/ns_fs/fs.baml:99,80`) — credential files, the
  well-known ADC file, the gcloud config file.
- `baml.time.Instant.now()` (`baml/ns_time/instant.baml:13`) — JWT `iat`/`exp`
  and token-cache expiry.
- `baml.json.*`, `baml.sys.shell` (`baml/ns_sys/sys.baml:141`) if a
  `gcloud auth print-access-token` fallback were ever wanted.
- Non-`baml` std packages **can** host Rust sysops (`$rust_function` appears in
  `ai/spec.baml:24`, `boundary/core.baml:3`, `reflect/reflect.baml:71`), so a
  `google.ns_internal` sysop is architecturally normal.

### Recommendation

Keep a **minimal Rust surface — one sysop — and move everything else to BAML.**

- **Must stay Rust (the irreducible core):** RS256 signing. Smallest useful
  primitive:
  `google.ns_internal.rs256_sign_pkcs8(private_key_pem: string, message: string) -> string`
  (base64url-no-pad signature), or the slightly larger
  `sign_service_account_jwt(sa_json, scope) -> string`. Dependencies already in
  the tree: `rsa` + `sha2` (`forks/google-cloud-auth/Cargo.toml:20-27`), pure
  Rust, wasm-clean.
- **Could stay Rust (pragmatic):** the whole token minter, exposed as
  `google.ns_internal.gcp_access_token(credentials_json: string?, scope: string) -> string`
  and `gcp_project_id() -> string?`. This keeps the ~1000-line fork and its
  process-wide token cache (`forks/.../lib.rs:146-188`) and the five credential
  flows (`lib.rs:362-389`) intact — a straight re-wiring of `BamlTokenIo`
  (`auth_request/vertex.rs:166-218`) onto BAML-visible IO rather than a rewrite.
  **This is the low-risk option and the one I recommend for the first cut.**
- **Should move to BAML** either way: credential-source selection
  (`auth_request/vertex.rs:247-255`), the `location`/`project_id` env chain
  (`vertex.rs:83-134`), URL construction, the express-mode API-key branch, header
  application, and `x-goog-user-project`. All of it is string/JSON/env/file/HTTP
  work with direct BAML equivalents.
- **Do not attempt** a pure-BAML service-account flow without new crypto sysops.
  Adding a general `baml.crypto` namespace (sha256/hmac/rsa-sign) is a defensible
  larger investment — AWS SigV4 for Bedrock needs HMAC-SHA256 for exactly the
  same reason — but it is a separate project from this migration.

**Token caching caveat:** the cache is a Rust `static`
(`forks/.../lib.rs:156-188`). If token minting moves to BAML, the cache moves
with it and needs a BAML-visible home (a module-level mutable, or keep the
caching inside the Rust sysop). Losing it means an RSA sign + token POST on
**every** Vertex request.

---

## 8. Suggested migration order

1. **Fix the stale `provider.rs:41,46` comments** — they will mislead the plan.
2. **`google-ai` first, no Vertex.** Port sys_llm's body/URL/auth behavior onto
   the existing `gemini.baml`: add `request_body` pass-through, headers, query
   params, `api_key_env`, cached-token usage fields, finish-reason filters.
   Reconcile the `GOOGLE_API_KEY` env fallback (native already has it; sys_llm
   does not — native/engine/docs win).
3. **Media**, which needs an `ai.Prompt` structural-media surface first. This is
   the largest single item and is shared with every other provider port.
4. **Vertex**, gated on the auth sysop decision in §7. Order inside: URL +
   express-mode API key (no crypto needed) → bearer token via the Rust sysop →
   `x-goog-user-project` → enterprise switch → claude-on-vertex `rawPredict`.
5. **Streaming** is already better in native than in sys_llm; only tool-call
   deltas are missing. Do not port sys_llm's streaming — it is broken for these
   providers (§0.2).
