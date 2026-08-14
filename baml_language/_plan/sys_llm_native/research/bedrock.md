# `aws-bedrock` → native BAML: research + migration seam

Scope: what it would take to move the `aws-bedrock` provider out of the Rust crate
`crates/sys_llm` and into native BAML under `crates/baml_builtins2/baml_std/`, the way
`openai/responses`, `anthropic/messages`, and `google/gemini` were done.

Every claim below cites `file:line`. Paths are relative to
`/Users/aaron/projects/baml/baml_language` (new compiler) or
`/Users/aaron/projects/baml/engine` (old compiler, "engine"), as marked.

---

## 0. TL;DR verdict

**Bedrock is the one provider that cannot be 100% BAML.** Everything except *four*
things is plain JSON-over-HTTP and moves cleanly.

| Piece | Verdict |
|---|---|
| Converse request body (messages/system/inferenceConfig/additionalModelRequestFields) | **pure BAML** — it is hand-built JSON already, no Smithy involved |
| Converse URL (`/model/{pct-encoded-id}/converse`) | **pure BAML** (needs a percent-encode helper, or hand-rolled in BAML) |
| Converse response parse (camelCase `output.message.content[]`) | **pure BAML** — same shape as the anthropic native parser |
| SigV4 signing (HMAC-SHA256 chain, canonical request) | **MUST stay Rust** — BAML has no SHA-256/HMAC (`baml describe uint8array` exposes only hex/base64: `crates/baml_builtins2/baml_std/baml/uint8array.baml:75-107`) |
| Credential chain (env/profile/`credential_process`/SSO/ECS/IMDS) | **can be BAML** in principle (env/fs/http/exec all exist) **except SSO**, whose cache key is a SHA-1 hex digest (`forks/aws-config/src/providers.rs:172`). Recommend keeping it Rust: it is already written, tested, and IO-abstracted. |
| Region resolution chain | can be BAML; ships free if the credential seam is one Rust call |
| ConverseStream (binary `vnd.amazon.eventstream` framing) | **MUST stay Rust if implemented at all** — `baml.http.fetch_sse` only speaks SSE (`crates/baml_builtins2/baml_std/baml/ns_http/http.baml:98-113`). **But sys_llm does not implement Bedrock streaming today** (`crates/sys_llm/src/stream_accumulator.rs:74-90`), so this is a *new-feature* decision, not a migration blocker. |

**Recommended minimal Rust seam: exactly one `$rust_io_function`** that takes a
`baml.http.Request` plus the client's AWS options and returns a *signed*
`baml.http.Request`. Everything else (body, URL, parse) becomes BAML. See §7.

---

## 1. What exists in `sys_llm` today

Three files, ~2.1k lines total (incl. tests):

- `crates/sys_llm/src/build_request/bedrock.rs` (972 lines)
- `crates/sys_llm/src/parse_response/bedrock.rs` (323 lines)
- `crates/sys_llm/src/auth_request/bedrock.rs` (842 lines, ~630 of which are test mocks)

Plus three **hand-written slim forks** (not the real AWS SDK — this is important):

- `forks/aws-bedrock/src/lib.rs` (481 lines) — serde model of the Converse wire format
- `forks/aws-sigv4/src/lib.rs` (610 lines) — self-contained SigV4 signer
- `forks/aws-config/src/{lib,profile,providers,ini}.rs` (921 lines) — credential/region chain
- `forks/aws-config-systest/src/main.rs` (355 lines) — standalone live harness, excluded from the workspace

Wired at `Cargo.toml:148-150`:

```toml
aws-config  = { path = "forks/aws-config",  package = "forked_aws_config" }
aws-sigv4   = { path = "forks/aws-sigv4",   package = "forked_aws_sigv4" }
aws-bedrock = { path = "forks/aws-bedrock", package = "forked_aws_bedrock" }
```

and consumed at `crates/sys_llm/Cargo.toml:29-30` (`aws-bedrock`, `aws-sigv4`) and
`:45,:50` (`aws-config`).

### 1.1 Is it aws-sdk or hand-rolled? → **hand-rolled**

This is the single most important fact for the migration. The *engine* uses the real
AWS SDK (`aws-sdk-bedrockruntime = "=1.106.0"`, `engine/baml-runtime/Cargo.toml:138` and
`:173`, with `aws-smithy-eventstream` in its dep tree, `engine/Cargo.lock:442-451`).
`sys_llm` deliberately does **not**:

- `forks/aws-bedrock/Cargo.toml:1-6` — *"Replaces `aws-sdk-bedrockruntime` (and its
  `aws-smithy-*` dependency tree) … No Smithy, no client, no async runtime — just the
  JSON shapes the Bedrock Converse endpoint expects."* Deps are only
  `base64`, `percent-encoding`, `serde`, `serde_json` (`:19-23`).
- `forks/aws-sigv4/Cargo.toml:1-5` — *"Replaces the upstream `aws-sigv4` crate (and its
  `aws-smithy-*` / `aws-credential-types` dependency tree) … No Smithy, no async runtime,
  no event streams."* Deps: `hex`, `hmac`, `percent-encoding`, `sha2`, `web-time` (`:19-24`).
- `forks/aws-config/Cargo.toml:1-9` — *"Replaces `aws-config` (and the
  `aws-sdk-sso`/`aws-sdk-sts`/`aws-smithy-*` dependency tree) … All IO is abstracted
  behind the `CredentialIo` trait."*

**Consequence:** the "keep some awssdk Rust utilities" expectation is already satisfied
by these three forks. The migration does not need to introduce anything new on the Rust
side — it needs to *shrink the surface* the forks expose to BAML down to signing (and
optionally credential resolution).

---

## 2. Request building (sys_llm) — all of this is portable

### 2.1 Entry point and body

`crates/sys_llm/src/build_request/bedrock.rs:28-63`:

```rust
let (system_blocks, messages) = prompt_to_sdk_types(prompt, &client.default_role)?;
let inference_config = build_inference_config(client)?;
let additional_fields = collect_additional_fields(client);
let request = ConverseRequest { messages, system, inference_config, additional_model_request_fields };
let body = request.to_json()?;      // :47-49
let url  = resolve_url(client, io, &client.model).await?;   // :51
headers: content-type: application/json, accept: application/json   // :53-55
method: POST                                                          // :58
```

The serialized shape (`forks/aws-bedrock/src/lib.rs:265-284`) is exactly:

```json
{ "messages": [...], "system": [...]?, "inferenceConfig": {...}?, "additionalModelRequestFields": {...}? }
```

with `skip_serializing_if = Option::is_none` on the last three (`:269-276`). Model id and
credentials are **not** in the body — asserted by test `bedrock_no_model_or_creds_in_body`
(`crates/sys_llm/src/build_request/bedrock.rs:671-677`) and
`forks/aws-bedrock/src/lib.rs:468-480`.

### 2.2 URL / model-id encoding

`crates/sys_llm/src/build_request/bedrock.rs:68-89`:

- `endpoint_url` option, if set, is used verbatim (trailing `/` stripped) + path (`:80-83`)
- otherwise `https://bedrock-runtime.{region}.amazonaws.com{path}` (`:85-88`), where region
  comes from `auth_request::bedrock::resolve_region` (`:85`)

Path is `converse_model_path(model)` = `/model/{encoded}/converse`
(`forks/aws-bedrock/src/lib.rs:59-62`), where `encoded` uses `LABEL_SET`
(`forks/aws-bedrock/src/lib.rs:24-53`) — a copy of `aws_smithy_http::urlencode::BASE_SET`
that **encodes `/` as `%2F` and `:` as `%3A`** so an ARN stays one path segment. Verified
by tests `:301-317` and `crates/sys_llm/src/build_request/bedrock.rs:649-669`.

**Inference profiles / ARNs are not special-cased anywhere.** There is no
`inference_profile` handling in either tree (grep across `engine/baml-runtime/.../aws/`
and `engine/baml-lib/llm-client/src/clients/aws_bedrock.rs` returns nothing). Cross-region
inference profile ids (`us.anthropic.claude-…`) and full ARNs just flow through as the
model id and are percent-encoded into the path. That behavior is trivially reproducible
in BAML.

### 2.3 Prompt → Converse messages

`crates/sys_llm/src/build_request/bedrock.rs:95-141`:

- role `"system"` → `SystemContentBlock` list (top-level `system` array)
- other roles → `Message { role, content }`
- `PromptAst::Simple` uses `client.default_role`

`parse_conversation_role` accepts **only** `user`/`assistant` (`:143-151`) — anything else
(after system extraction) is a hard error.

System blocks are **text-only**: media in a system message errors
(`:169-185`, specifically `:174-176`).

`model_features.rs:76-80` marks Bedrock (with Google/Vertex) as `max_one_system_prompt: true`.

### 2.4 Media

`crates/sys_llm/src/build_request/bedrock.rs:191-346`:

| Kind | base64 → | s3:// → | formats |
|---|---|---|---|
| Image | `{"image":{"format","source":{"bytes"}}}` | `{"s3Location":{"uri"}}` | png/jpeg/jpg/gif/webp (`:240-250`) |
| Video | same | same | mp4/mpeg/mkv/mov/flv/webm/3gpp→`three_gp` (`:252-265`, serde rename at `forks/aws-bedrock/src/lib.rs:107-108`) |
| Pdf | `{"document":{"format":"pdf","name":"document","source":{"bytes"}}}` | s3Location | `application/pdf` only (`:311-326`) |
| Audio | `{"audio":{"format","source":{"bytes"}}}` | **rejected** (`:330-334`) | mp3/wav/flac/ogg/webm (`:267-278`) |
| Generic | error (`:342-345`) | | |

Non-`s3://` URLs are rejected outright (`:206-213`, test `:851-870`).
Media URL policy for Bedrock (`crates/sys_llm/src/baml_std.rs:340-346`):
image/audio/pdf = `send_base64`, video = `send_url`.

**Migration note:** the native `ai` architecture has *no media in its content model* —
`ai.content.Block = Text | Reasoning | ToolUse`
(`crates/baml_builtins2/baml_std/ai/ns_content/content.baml:9-23`) and
`ai.PromptMessage.content` is a plain `string` with *"Media parts use a readable
placeholder"* (`crates/baml_builtins2/baml_std/ai/spec.baml:7-13`). So a native Bedrock
client inherits the same media gap the native openai/anthropic/google clients already
have. All the media code above has **no destination** until `ai.content` grows media.
Plan accordingly: port it as dead-but-ready code, or defer it.

### 2.5 Inference config

`crates/sys_llm/src/build_request/bedrock.rs:352-393` reads `BedrockOptions`:
`max_tokens` (narrowed to `i32`, overflow is a hard `InvalidOption` error `:359-369`,
test `:937-971`), `temperature` (f64→f32 `:371-374`), `top_p` (`:376-379`),
`stop_sequences` (only when non-empty `:381-385`). Emitted as `inferenceConfig` with
camelCase keys `maxTokens`/`temperature`/`topP`/`stopSequences`
(`forks/aws-bedrock/src/lib.rs:236-247`), and the whole object is dropped when empty
(`:249-256` + `crates/sys_llm/src/build_request/bedrock.rs:388-392`).

### 2.6 additionalModelRequestFields

`crates/sys_llm/src/build_request/bedrock.rs:397-409` — sourced from
`client.extra_body`, which is `PrimitiveClientOptions.request_body`
(`crates/sys_llm/src/baml_std.rs:31-32,104-119`). **Note the option-name drift**: the
engine calls this option `additional_model_request_fields`
(`engine/baml-lib/llm-client/src/clients/aws_bedrock.rs:482-485`); sys_llm folds it into
the generic `request_body`.

### 2.7 Client options schema

`crates/baml_builtins2/baml_std/baml/ns_prompt/sys_llm_types.baml:62-73`:

```baml
class BedrockOptions {
    region, endpoint_url, access_key_id, secret_access_key, session_token, profile,
    stop_sequences: string[]?, max_tokens: int?, temperature: float?, top_p: float?,
}
```

Note this is **flat** — the engine nests the last four under an
`inference_configuration` map (`engine/baml-lib/llm-client/src/clients/aws_bedrock.rs:487-543`).

### 2.8 Order of operations that the seam must preserve

`crates/sys_llm/src/build_request/mod.rs`:
- user headers applied **before** auth (`:112-116`)
- query params appended **before** auth (`:118-125`)
- `auth_request::auth_request(...)` runs **last** (`:145`)
- WASM playground proxy rewrite is **skipped for Bedrock** because *"its SigV4 signature is
  bound to the host and would not survive the rewrite"* (`:133-144`)

Any BAML port must sign the *final* request bytes and headers. That constrains the seam:
signing has to be the last step before `baml.http.send`.

---

## 3. Response parsing (sys_llm) — fully portable

`crates/sys_llm/src/parse_response/bedrock.rs`:

- serde envelope `{ output: { message: { content: [...] } }, stopReason, usage }`
  (`:13-41`, camelCase rename at `:14`)
- content blocks: only `{"text": …}` is read; everything else is `Other` and skipped
  (`:43-51`, `:81-87`)
- errors: no `output.message` (`:66-72`), empty content (`:74-79`), zero text blocks
  (`:89-104`)
- text blocks are joined with `""` (`:106`)
- stop reason mapping (`:110-116`): `end_turn|stop_sequence`→Stop, `max_tokens`→Length,
  `tool_use`→ToolUse, else `Other(raw)`, `None`→Unknown
- usage (`:118-126`): `inputTokens`/`outputTokens`/`totalTokens`;
  **`cached_input_tokens: None` with the comment "Bedrock Converse doesn't report cached
  tokens"** (`:124`) — this is **wrong**, see §5 gap G3
- `model: None` — *"Bedrock Converse responses don't include the model name"* (`:131`)

This maps 1:1 onto the native pattern: a `class BedrockResponse { … }` decoded via
`ai.wire.send_as<T>(req, "aws-bedrock")`
(`crates/baml_builtins2/baml_std/ai/ns_wire/wire.baml:7-32`), exactly like
`AnthropicResponse` (`crates/baml_builtins2/baml_std/anthropic/ns_internal/messages.baml:10-28,265-310`).

---

## 4. Auth (sys_llm) — the part that must stay Rust

### 4.1 Signing

`crates/sys_llm/src/auth_request/bedrock.rs:117-153`:

```rust
let credentials = resolve_credentials(&bedrock_opts, io.clone()).await?;   // :127
let region      = resolve_region(&bedrock_opts, io).await?;                 // :128
let signed = aws_sigv4::sign_request(
    &request.method, &request.url, &header_pairs, request.body.as_bytes(),
    &credentials, &region, "bedrock", now(),                                // :136-145
)?;
for (name, value) in signed { request.headers.insert(name, value); }        // :148-150
```

`forks/aws-sigv4/src/lib.rs:150-237` is a faithful reimplementation of the AWS default
HTTP signing settings, documented as byte-identical (`:5-18`):

- excluded headers `authorization`, `user-agent`, `x-amzn-trace-id`, `transfer-encoding` (`:34-39`)
- injects `host` (with default-port stripping, `:411-428`), `x-amz-date`, and
  `x-amz-security-token` when a session token is present (`:176-188`)
- canonical path = RFC-3986 dot-segment normalization then **double** percent-encoding
  (`:268-312`)
- canonical query = decode-then-re-encode, sorted (`:314-346`)
- header value canonicalization = `trim_all` (`:348-366`)
- signing key = `HMAC(HMAC(HMAC(HMAC("AWS4"+secret, date), region), service), "aws4_request")` (`:255-262`)
- returns `x-amz-date`, `authorization`, and optionally `x-amz-security-token` (`:229-236`)

**Why this cannot be BAML:** `sha256_hex` / `hmac` (`:243-253`) need SHA-256 + HMAC.
The BAML stdlib exposes no crypto at all — `baml.Uint8Array` has only
`to_hex`/`from_hex`/`to_base64`/`from_base64`
(`crates/baml_builtins2/baml_std/baml/uint8array.baml:75-107`) and grepping the whole
`baml_std` tree for `sha256|hmac|crypto` returns only prose. Implementing SHA-256 in BAML
byte loops is technically possible and practically indefensible (per-request cost, on a
hot path).

### 4.2 Credential chain

`forks/aws-config/src/lib.rs:120-160` — order:

1. **env** (`providers.rs:17-46`): `AWS_ACCESS_KEY_ID` + `AWS_SECRET_ACCESS_KEY`
   (legacy `SECRET_ACCESS_KEY` fallback `:27-32`) + optional `AWS_SESSION_TOKEN` (`:33-40`)
2. **shared profile** (`providers.rs:54-99`): static keys → `credential_process` → SSO
3. **ECS/container** (`providers.rs:253-302`): `AWS_CONTAINER_CREDENTIALS_RELATIVE_URI`
   (against `http://169.254.170.2`, `:249,:256-261`) or `..._FULL_URI` (`:262-267`), with
   `AWS_CONTAINER_AUTHORIZATION_TOKEN_FILE` taking precedence over `..._TOKEN` (`:274-292`)
4. **EC2 IMDS** (`providers.rs:312-368`): IMDSv2 `PUT /latest/api/token` with a 21600s TTL
   (`:322-341`), falling back to v1 on non-200; then role discovery and role credentials

Explicit client options short-circuit the chain
(`crates/sys_llm/src/auth_request/bedrock.rs:179-208`) — both `access_key_id` and
`secret_access_key` must be present or the chain runs (`:200-208`, tests `:823-841`).

Profile files: `AWS_SHARED_CREDENTIALS_FILE` else `~/.aws/credentials`
(`forks/aws-config/src/profile.rs:24-33`), `AWS_CONFIG_FILE` else `~/.aws/config`
(`:36-41`), merged with credentials-file precedence per key (`:57-86`), `[profile NAME]`
vs bare `[NAME]` handled at `:135-143`. INI parser at `forks/aws-config/src/ini.rs:19-49`
(lenient; skips malformed lines; no sub-properties, `:1-6`).

SSO (`forks/aws-config/src/providers.rs:131-211`): resolves `sso_session` →
`[sso-session NAME]` (`:142-155`), reads `~/.aws/sso/cache/{sha1_hex(cache_key)}.json`
(`:169-178` — **SHA-1**, the second crypto dependency), then calls
`https://portal.sso.{region}.amazonaws.com/federation/credentials` (`:188-210`).

`credential_process` executes via `CredentialIo::run_command`, implemented natively as
`sh -c` and hard-erroring on wasm
(`crates/sys_llm/src/auth_request/bedrock.rs:89-109`). Output must be
`{"Version":1,…}` (`forks/aws-config/src/providers.rs:101-115`).

All IO is routed through BAML's sandboxed `RuntimeIo` by the `BamlCredentialIo` adapter
(`crates/sys_llm/src/auth_request/bedrock.rs:37-110`): `env_get`, `fs_open`+`fs_file_text`,
`http__send`+`http_response_text`.

### 4.3 Region chain

`forks/aws-config/src/lib.rs:103-117`: `AWS_REGION` → `AWS_DEFAULT_REGION` → active
profile's `region` (following `source_profile` links,
`forks/aws-config/src/profile.rs:108-131`). Explicit `options.region` wins
(`crates/sys_llm/src/auth_request/bedrock.rs:164-166`). Missing region is a hard error
(`:170-175`) unless `endpoint_url` is set (test
`bedrock_endpoint_url_does_not_require_region`, `crates/sys_llm/src/build_request/bedrock.rs:920-935`).

---

## 5. Engine (old compiler) parity reference

Engine `AwsClient` uses the **real SDK**: `BedrockRuntimeClient::from_conf(...)`
(`engine/baml-runtime/src/internal/llm_client/primitive/aws/aws_client.rs:665-681`),
`aws_client.converse()` (`:1400-1406`) and `aws_client.converse_stream()` (`:955-961`).

Config loading (`:576-682`):
- `aws_config::defaults(BehaviorVersion::latest())` native (`:585`); a browser-specific
  loader on wasm (`:581-582`, `aws/wasm.rs:42-48`)
- no explicit creds → `DefaultCredentialsChain` with `profile_name` and — explicitly for
  **IRSA / `AssumeRoleWithWebIdentity`** — a region (`:602-615`)
- explicit creds → `ExplicitCredentialsProvider` (`:324-351`), with an odd `$`-prefix
  sentinel skip (`:619-650`)
- `region` option with a `$` prefix is a hard error (`:654-660`)
- custom reqwest-backed `HttpClient` so `HTTPS_PROXY` works (`:663-667`,
  `aws/custom_http_client.rs:22-49`), and `StalledStreamProtectionConfig::disabled()`
  because that custom client breaks the 5s grace period (`:668-669`)
- `endpoint_url` override (`:677-679`)

Request build (`:730-783`): system split off the first message only (`:738-748`),
`InferenceConfiguration` builder (`:755-762`), `additional_model_request_fields` converted
to a Smithy `Document` (`:764-773`, converter `:199-227`), `model_id` (`:778`).
**No `toolConfig`, no `guardrailConfig`, no `promptVariables`** — grepping
`guardrail|toolConfig|tool_config|inference_profile` across the whole engine aws directory
and `aws_bedrock.rs` returns **zero hits**. So: no tool calling and no guardrails on
Bedrock in either implementation.

Response (`:684-728`, `:1453-1486`): joins all `ContentBlock::Text` (`:699-710`), errors
listing block kinds otherwise (`:713-727`), `baml_is_complete` iff
`StopSequence|EndTurn` (`:1463-1467`), usage including
**`cache_read_input_tokens`** (`:1481-1484`).

Engine also has a *second*, hand-built HTTP path,
`build_modular_http_request` (`:419-471`): identical body via `build_converse_body_json`
(`:354-417`), region required (`:432-443`), URL
`https://bedrock-runtime.{region}.amazonaws.com/model/{encoded}/converse` (`:450-453`)
using `PATH_SEGMENT_ENCODE_SET = NON_ALPHANUMERIC - {-,_,.,~}` (`:166-170`) — a *stricter*
set than sys_llm's smithy-derived `LABEL_SET`, though both encode `:` and `/`. It
**bails on streaming**: *"AWS Bedrock modular streaming is not supported"* (`:426-430`).
This modular path is the closest existing analogue to what native BAML would do, and it
is unsigned (signing was left to the SDK-free caller).

Cache points: engine emits `{"cachePoint":{"type":"default"}}` when `cache_control`
metadata is allowed, for both system (`:123-138`) and chat parts (`:140-155`,
`:1286-1335`), with `allowed_role_metadata` defaulting to `All`
(`engine/baml-lib/llm-client/src/clients/aws_bedrock.rs:475-480`).

Engine media (`:1136-1267`) differs from sys_llm: **audio is rejected outright**
(`:1260-1265`), PDF-by-URL wrongly sends the *URL string bytes* as document bytes and sets
`CitationsConfig` enabled (`:1188-1203`), and the modular JSON path derives `format` by
stripping the MIME prefix rather than validating an enum (`:71-73`, `:75-117`).

Engine options (`engine/baml-lib/llm-client/src/clients/aws_bedrock.rs`):
`model`/`model_id` (mutually exclusive, `:430-452`), `region` with
`AWS_REGION`→`AWS_DEFAULT_REGION` fallback (`:256-271`), creds with env fallback only when
*all three* are unset (`:296-360`), `profile` with `AWS_PROFILE` fallback (`:362-380`),
`endpoint_url` (`:382-392`), `additional_model_request_fields` (`:482-485`),
nested `inference_configuration` map with per-key validation (`:487-543`),
`supported_request_modes` (`:481`), `finish_reason_filter` (`:544`),
`media_url_handler` (`:545`), `http_config` (`:546`).

---

## 6. Streaming — the load-bearing question

**sys_llm does not support Bedrock streaming.**
`crates/sys_llm/src/stream_accumulator.rs:74-90` rejects any provider other than
openai/openai-generic/azure-openai/ollama/openrouter/anthropic with
`LlmOpError::NotImplemented`, and `:447-463` is an explicit test that `"aws-bedrock"` is
rejected. The doc comment at `:60-62` names Bedrock as unsupported.

**The engine supports it only through the SDK.** `stream_chat`
(`aws_client.rs:901-1128`) calls `converse_stream()` and pulls typed
`ConverseStreamOutput` variants off `response.stream.recv()` (`:1037`, `:1055-1100`) —
`ContentBlockDelta` text (`:1056-1067`), `MessageStop` → completeness (`:1077-1084`),
`Metadata` → usage incl. `cache_read_input_tokens` (`:1085-1096`). That decoding is
`aws-smithy-eventstream` (in the `aws-sdk-bedrockruntime` dep tree,
`engine/Cargo.lock:442-451`), i.e. AWS's **binary** `application/vnd.amazon.eventstream`
framing — length-prefixed frames with a header block and a CRC, not SSE.

BAML's only streaming transport is `baml.http.fetch_sse` →
`baml.http.SseStream` (`crates/baml_builtins2/baml_std/baml/ns_http/http.baml:98-113,164-167`),
and `ai.stream.TurnStream.from_sse` consumes a JSON array of `{event,data,id}` SSE records
(`crates/baml_builtins2/baml_std/ai/ns_stream/stream.baml:16-28,89-106`). There is **no
event-stream decoder anywhere in the new-compiler tree** (grep for
`eventstream|vnd.amazon` across `crates/` and `forks/` returns nothing).

**Verdict:** if native Bedrock streaming is wanted, it needs a *new* Rust piece — either
(a) a `vnd.amazon.eventstream` frame decoder exposed as a BAML-visible stream that yields
the same `{event,data}` records `TurnStream` already understands, or (b) a whole
`fetch_eventstream` transport. Neither exists. **Recommendation: ship non-streaming
Bedrock first** (matching sys_llm's current capability exactly — zero regression) and
treat streaming as a separately-scoped follow-up.

---

## 7. The proposed Rust seam (precise)

### 7.1 Shape

Add **one** `$rust_io_function` (async — the credential chain does IO) in a new
`baml_std` namespace, e.g. `crates/baml_builtins2/baml_std/aws/ns_internal/sigv4.baml`:

```baml
// Options the signer needs. Mirrors baml.prompt.BedrockOptions minus the
// request-shaping fields.
class SigningOptions {
    region: string?,
    access_key_id: string?,
    secret_access_key: string?,
    session_token: string?,
    profile: string?,
}

/// Resolve AWS credentials + region (explicit options, else the default
/// provider chain: env → profile → credential_process → SSO → ECS → IMDS),
/// then return `request` with `x-amz-date`, `authorization`, and (when a
/// session token is in play) `x-amz-security-token` added.
///
/// Must be the LAST mutation before send: the signature covers the method,
/// URL, headers, and body bytes exactly as given.
function sign(
    request: baml.http.Request,
    opts: SigningOptions,
    service: string,
) -> baml.http.Request throws root.errors.Io {
    $rust_io_function
}

/// The resolved AWS region (explicit option, else AWS_REGION →
/// AWS_DEFAULT_REGION → active profile). Needed to build the endpoint host
/// before signing.
function resolve_region(opts: SigningOptions) -> string? throws root.errors.Io {
    $rust_io_function
}
```

Optionally a third, if the BAML side wants to avoid hand-rolling percent-encoding:

```baml
/// Percent-encode `s` as a single URI path label (`/` → %2F, `:` → %3A),
/// matching the AWS SDK's path-label encoding.
function encode_path_label(s: string) -> string { $rust_function }
```

### 7.2 Why this exact shape

- **Sign-the-whole-request, not sign-the-whole-call.** A "do the entire HTTP call in Rust"
  seam would drag body construction, response parsing, error classification, tracing, and
  `ai.errors` normalization back into Rust — the opposite of the migration's goal. Signing
  a `baml.http.Request` keeps *everything else* in BAML and still gives Rust the one thing
  it must own: the crypto over the final bytes.
- **The signature binds host + path + query + headers + body**
  (`forks/aws-sigv4/src/lib.rs:204-217`), so the seam must be last. Making it
  `Request → Request` makes that ordering explicit and un-skippable at the call site
  (`ai.wire.send_as(aws.sigv4.sign(req, opts, "bedrock"), "aws-bedrock")`).
- **`baml.http.Request` is already a codegen-supported argument and return type** — it is
  what `baml.http._send` takes (`crates/baml_builtins2/baml_std/baml/ns_http/http.baml:157-162`),
  and its `headers` is `map<string, string>` (`:8-13`), so nothing new is needed from the
  builtins codegen.
- **The implementation is a ~30-line adapter** over what already exists: it is literally
  `crates/sys_llm/src/auth_request/bedrock.rs:117-153` with `BedrockOptions` swapped for
  `SigningOptions` and `HttpRequest` swapped for `owned::http::Request`. The
  `BamlCredentialIo` adapter (`:37-110`), all three forks, and the systest harness carry
  over unchanged.
- **`resolve_region` must be separate** because the BAML side needs the region to build the
  URL *before* signing (`crates/sys_llm/src/build_request/bedrock.rs:85-88`), and signing
  needs it again for the credential scope. Two calls, or have `sign` return the region —
  the former is simpler.

### 7.3 Mechanics of adding it

`$rust_io_function` declarations in `baml_std` are extracted at build time
(`crates/baml_builtins2_codegen/src/extract.rs:50-55`) and codegen'd into:
- `crates/sys_types` (structs + `RuntimeIo` trait) via `crates/sys_types/build.rs:1-13`
- `crates/sys_ops` (the sysop trait + adapter) via `crates/sys_ops/build.rs:1-18`

Implementations then land in `crates/sys_native/src/io_impls.rs` (native) and
`crates/sys_wasm/src/web_sysops.rs` (wasm). The wasm path must keep the existing
`credential_process` behavior: hard error on wasm
(`crates/sys_llm/src/auth_request/bedrock.rs:102-108`).

### 7.4 What stays in Rust, exhaustively

| Rust (keep) | Why |
|---|---|
| `forks/aws-sigv4` | SHA-256 + HMAC; no BAML crypto |
| `forks/aws-config` | credential/region chain; SSO needs SHA-1 (`providers.rs:172`); already IO-sandboxed and tested |
| `BamlCredentialIo` adapter (`auth_request/bedrock.rs:37-110`) | bridges `CredentialIo` → `RuntimeIo` |
| `credential_process` subprocess (`:89-109`) | *could* be BAML via `baml.sys.exec` (`baml describe baml.sys` → `baml.sys.exec` at `baml/ns_sys/sys.baml:120`), but it lives inside the Rust chain |
| (new, only if streaming is scoped in) `vnd.amazon.eventstream` decoder | binary framing; `fetch_sse` cannot |

| BAML (port) | Replaces |
|---|---|
| Converse body build | `build_request/bedrock.rs:28-63,95-185,352-409` + `forks/aws-bedrock` serde model |
| URL + model-id encoding | `build_request/bedrock.rs:68-89` + `forks/aws-bedrock/src/lib.rs:24-62` |
| Response parse + stop-reason + usage | all of `parse_response/bedrock.rs` |
| Error classification | `ai.errors.classify_http` via `ai.wire.send_as` (`ai/ns_wire/wire.baml:7-32`) |

`forks/aws-bedrock` can then be **deleted entirely** — it is only a serde model and a
path-encoder, both of which BAML expresses directly (`baml.json.to_json` +
`baml.json.stringify`, as the anthropic client does at
`anthropic/ns_internal/messages.baml:247`). That is the single biggest simplification in
this migration: one of the three forks goes away.

---

## 8. Parity gaps and risks (carry these into the plan)

- **G1 — no tool calling.** Neither sys_llm nor the engine emits `toolConfig`
  (`aws_client.rs:775-782` sets only inference/additional/model/system/messages; zero
  `toolConfig` hits in either tree). But `ai.ModelTurnInput` carries a `toolbox`
  (`ai/turn.baml:6-13`) and the native anthropic/google clients *do* lower tools
  (`anthropic/ns_internal/messages.baml:213-229`). A native Bedrock client will either
  need new `toolConfig` support (a genuine feature, not a port) or must document that
  tools go through the SAP/prompt path only.
- **G2 — no guardrails.** No `guardrailConfig` anywhere in either tree. Nothing to port.
- **G3 — cached-token accounting is wrong in sys_llm.** `parse_response/bedrock.rs:124`
  hardcodes `cached_input_tokens: None` claiming Converse doesn't report it, while the
  engine reads `usage.cache_read_input_tokens()` (`aws_client.rs:1481-1484`). The native
  port should read `usage.cacheReadInputTokens` (and `cacheWriteInputTokens`) — a free fix.
- **G4 — cache points dropped.** sys_llm has no `cachePoint` emission at all; the engine
  emits it for system and chat parts (`aws_client.rs:119-121,123-138,140-155,1286-1335`).
  sys_llm is already a regression vs the engine here; the port inherits it unless fixed.
- **G5 — IRSA / `AssumeRoleWithWebIdentity` / `role_arn` + `source_profile` assume-role are
  missing from the slim `aws-config`.** Grepping `forks/aws-config/src` for
  `role_arn|AssumeRole|web_identity|sts` returns only `mod tests` lines. The engine
  explicitly wires region into `DefaultCredentialsChain` *for IRSA*
  (`aws_client.rs:608-613`). This is an existing sys_llm gap the migration inherits; worth
  calling out because EKS users hit it.
- **G6 — no proxy support in the signed path.** The engine plumbs `HTTPS_PROXY` through a
  custom Smithy `HttpClient` (`aws/custom_http_client.rs:22-49`); the native path would
  depend on whatever `baml.http.send` does. Also the WASM playground proxy is deliberately
  disabled for Bedrock (`build_request/mod.rs:133-144`) — a native client must keep that
  exclusion or signatures break.
- **G7 — media has nowhere to go.** See §2.4: `ai.content.Block` has no media variant
  (`ai/ns_content/content.baml:23`). All of `build_request/bedrock.rs:191-346` is
  unportable until that lands.
- **G8 — option-shape drift.** Flat `BedrockOptions`
  (`baml/ns_prompt/sys_llm_types.baml:62-73`) vs the engine's nested
  `inference_configuration` (`engine/.../aws_bedrock.rs:487-543`), and
  `request_body` vs `additional_model_request_fields` (§2.6). Pick one for the native
  `BedrockClient.new(...)` and document the mapping.
- **G9 — no `client<llm>`/DSL surface exists for a native Bedrock client yet.** The only
  reference to `aws-bedrock/...` shorthand in the new tree is a *negative* diagnostic test
  (`crates/baml_tests/projects/diagnostic_errors/client_option_types/client_option_types.baml:39`).

---

## 9. Sketch of the native surface

Mirroring `anthropic/messages.baml:1-43`:

```baml
// crates/baml_builtins2/baml_std/aws/bedrock.baml
class BedrockClient {
    model: string,               // model id, inference-profile id, or ARN
    region: string?,             // null → AWS_REGION → AWS_DEFAULT_REGION → profile
    endpoint_url: string?,       // overrides the bedrock-runtime host
    profile: string?,
    access_key_id: string?,
    secret_access_key: string?,
    session_token: string?,
    max_tokens: int?,
    temperature: float?,
    top_p: float?,
    stop_sequences: string[]?,

    function new(model: string, ...) -> BedrockClient { ... }

    implements ai.Client {
        function id(self) -> string { `aws-bedrock/${self.model}` }
        function invoke(self, input: ai.ModelTurnInput) -> ai.ModelTurn {
            root.internal.invoke(self, input) catch_all (e) { _ => throw ai.errors.normalize(e) }
        }
    }
    // NOTE: no `implements ai.stream.StreamingClient` — ConverseStream is binary
    // event-stream framing, which baml.http.fetch_sse cannot consume (see §6).
}
```

and `aws/ns_internal/bedrock.baml` holding `_bedrock_request` (body + URL),
`_bedrock_lower_prompt` / `_bedrock_lower_journal` (system split + user/assistant
alternation, per `build_request/bedrock.rs:95-151`), `BedrockResponse`/`BedrockUsage`
envelope classes, `_bedrock_stop_reason` (per `parse_response/bedrock.rs:110-116`), and:

```baml
function invoke(c: root.BedrockClient, input: ai.ModelTurnInput) -> ai.ModelTurn {
    let req = _bedrock_request(c, input);
    let signed = aws.sigv4.sign(req, _signing_options(c), "bedrock");
    let resp = ai.wire.send_as<BedrockResponse>(signed, "aws-bedrock");
    ...
}
```

---

## 10. Test assets that carry over

- Body/URL snapshots: `crates/sys_llm/src/build_request/bedrock.rs:519-935` (14 cases:
  system+user, multi-turn, inference config, URL/region, endpoint override incl. trailing
  slash, ARN encoding, no-model-in-body, image/video/pdf/audio base64, image/video s3,
  multipart, non-s3 rejection) — all become BAML `test` blocks against a
  `_bedrock_request` helper.
- Response parse: `crates/sys_llm/src/parse_response/bedrock.rs:142-322` (9 cases) →
  BAML tests over `baml.json.from_string<BedrockResponse>`.
- Wire-shape tests in the fork: `forks/aws-bedrock/src/lib.rs:300-480` — port then delete.
- Signing/credential tests stay Rust: `crates/sys_llm/src/auth_request/bedrock.rs:654-841`
  (SigV4 headers present, explicit-creds short-circuit, env/fs/http credential injection).
- Live end-to-end: `forks/aws-config-systest/src/main.rs` (excluded from the workspace,
  `forks/aws-config-systest/Cargo.toml:6-8`; defaults to profile `boundaryml-dev`,
  `us-east-1`, `amazon.nova-micro-v1:0` at `src/main.rs:12-15`) — keep as the signer's
  regression harness.
