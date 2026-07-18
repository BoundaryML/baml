---
date: 2026-07-18
planner: FrostyAnchor
branch: openai-transcriptions
base_commit: 621771315
stage: 3
status: enhanced-plan
topic: "OpenAI /v1/audio/transcriptions endpoint support (issue #1724)"
supersedes: thoughts/searchable/shared/plans/2026-07-17-18-06-openai-transcriptions-endpoint-tdd.md
review_inputs:
  - "2808 FrostyAnchor: contracts/interfaces"
  - "2809 TurquoiseGlen: abstraction gaps"
  - "2812 ScarletMountain: closure-test integrity"
  - "2813 SageGlacier/PearlCat: coverage/boundary/wasm"
tags: [plan, tdd, llm-client, openai, transcriptions, multipart, provider]
---

# OpenAI Audio Transcriptions Endpoint - Enhanced TDD Implementation Plan

## Overview

Add first-class support for OpenAI's non-streaming `POST {base_url}/audio/transcriptions` endpoint as a new BAML provider string, `openai-transcriptions`. The implementation must mirror the existing `openai-responses` seams exactly: provider enum -> config `create_from` -> runtime factory -> provider strategy -> endpoint/body builder -> parser dispatch -> `LLMCompleteResponse`.

The only novel runtime shape is `multipart/form-data`. The plan therefore treats multipart construction as an explicit contract with unit tests, wasm build gates, and a blocking closure test that proves the real HTTP request contains the expected `file` bytes and form fields.

Confirmed constraints:
- Provider = `OpenAIClientProviderVariant::Transcriptions` plus `ProviderStrategy::TranscriptionsApi`, exposed as `provider "openai-transcriptions"`.
- Response type string = exactly `"openai-transcription"` and enum variant `ResponseType::OpenAITranscription`; do not add the provider-string alias unless deliberately requested later.
- Scope = OpenAI transcriptions only, non-streaming v1 only. No AssemblyAI, generic STT host abstraction, translations endpoint, or SSE streaming in this change.
- Audio arg -> multipart `file` part, borrowing the existing OpenAI media base64/PDF pattern in `openai_client.rs:786-819`.
- Multipart uses `reqwest::multipart::{Form, Part}` with `Part::bytes(decoded_audio)` only. No file-backed or stream-backed part path.
- Add reqwest feature `"multipart"` in both `engine/Cargo.toml` and `engine/baml-runtime/Cargo.toml`.
- Use table-driven tests for multipart conversion. Do not introduce proptest/quickcheck unless the dev-dependency is explicitly added.
- Native tests, native build, wasm32 build, and clippy gates must pass before Stage 4 is complete.

## Current State Analysis

Key system seams from the Stage 1 map:
- Provider grammar is in `engine/baml-lib/llm-client/src/clientspec.rs`.
- OpenAI config routing is in `engine/baml-lib/llm-client/src/clients/mod.rs` and `clients/openai.rs`.
- Runtime factory routing is in `engine/baml-runtime/src/internal/llm_client/primitive/mod.rs`.
- OpenAI endpoint strategy and request building are in `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs`.
- Parser dispatch is in `engine/baml-runtime/src/internal/llm_client/primitive/request.rs`.
- OpenAI response normalization is in `engine/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs`.
- Streaming parser dispatch in `stream_request.rs` is exhaustive and must receive a fail-closed `OpenAITranscription` arm.
- Current engine reqwest features omit `"multipart"` in both required manifests.

Stage 2 review amendments folded into this enhanced plan:
- Streaming must be disabled for `openai-transcriptions` at config creation and fail closed before HTTP if reached through the stream path.
- `model` is required and must be a string. Missing or non-string `model` errors before HTTP.
- Multipart fields are whitelisted and converted from `serde_json::Value` by a fixed table.
- Exactly one audio media part is accepted. Zero or multiple audio inputs error. Completion prompts (`Either::Left`) error.
- Rendered prompt text maps to multipart `prompt` only under an explicit single-text/no-conflict rule.
- `response_format` is restricted to absent, `json`, or `verbose_json`; non-JSON formats are rejected before HTTP because the parser path is JSON-only.
- Parser output model is the request model, passed as `model_name` where available, falling back to `model_name.unwrap_or_default()`.
- Multipart observability comes from mock-server capture, not raw-curl/tracing body capture.
- The closure test must use a real mock HTTP server, must not mock or seed the span under test, must not skip to green, and must record a red-at-seam proof.

## Desired End State

Given a `.baml` client using `provider "openai-transcriptions"`, a function call with one audio argument produces a multipart request to `{base_url}/audio/transcriptions` containing:
- `file`: decoded audio bytes, filename derived from MIME, MIME preserved.
- `model`: required string.
- Optional whitelisted fields: `prompt`, `language`, `response_format`, `temperature`.

A successful JSON response `{ "text": "..." }` or `verbose_json` response with a top-level `text` field becomes `LLMResponse::Success(LLMCompleteResponse { content: text, model: request_model, ... })`.

Observable behaviors:
- Provider parsing and display round-trip for `openai-transcriptions`.
- ResponseType parsing and resolution for `openai-transcription`.
- Config creation defaults match OpenAI and disables streaming.
- Endpoint selection returns `/audio/transcriptions` for both completion and chat call shapes.
- Multipart parts builder validates all inputs before HTTP.
- Parser extracts `text` and sets the required output model.
- Runtime request path uses multipart, not JSON, and dispatches to the transcription parser.
- Closure test proves audio in -> transcript out through a real local HTTP server.

## What We Are Not Doing

- No streaming/SSE transcription support.
- No non-OpenAI STT provider.
- No `/audio/translations` endpoint.
- No non-JSON transcription response formats (`text`, `srt`, `vtt`) until the request lifecycle supports raw-body parser dispatch.
- No playground upload UI.
- No broad refactor outside the OpenAI provider seams required for this endpoint.

## Testing Strategy

- Unit tests in `internal-llm-client` for provider grammar, response type, and config defaults.
- Unit tests in `baml-runtime` for endpoint strategy, pure `build_transcription_parts`, parser, and stream fail-closed dispatch.
- Runtime wiring tests for URL/content-type/no-JSON branch behavior.
- Blocking native closure test using a real local mock HTTP server. Preferred mock crate: `wiremock` with a custom matcher over raw request headers/body. `httpmock` is acceptable only if its API can inspect raw multipart bytes and hit counts.
- Table-driven cases for field conversion and base64/media edge cases. Do not use property tests unless a proptest/quickcheck dev-dependency is added explicitly.
- Build gates after multipart wiring: `cargo build -p baml-runtime`, `cargo build -p baml-runtime --target wasm32-unknown-unknown`, repo clippy gates, and wasm clippy if this repo has a local equivalent.

Dev-dependency policy:
- Declare the chosen mock server in the dependency scope used by `engine/baml-runtime`.
- If native-only mock dependencies cause wasm target issues, put them under `target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies`. Production transcriptions code must still compile for wasm.
- Missing mock-server infrastructure is a blocking red condition, not a reason to skip the closure test.

## Workflow Closure

Production operation chain:

```text
BAML function call
  -> orchestrator single_call
  -> OpenAIClient::chat
  -> make_parsed_request
  -> build_request (TranscriptionsApi multipart POST /audio/transcriptions)
  -> execute_request
  -> parse_openai_transcription_response
  -> LLMCompleteResponse.content
  -> function string output
```

Blocking closure test: "an audio clip sent to an openai-transcriptions client returns its transcript".

Closure harness requirements:
- Use a real local mock HTTP server on an ephemeral localhost port.
- Trigger through `OpenAIClient::chat` from `engine/baml-runtime/tests/transcriptions_closure.rs` with `--features internal`, or move the test in-crate if that is cleaner. Exact command if kept as an integration test: `cargo test -p baml-runtime --features internal --test transcriptions_closure`.
- Seed only source inputs: base64 audio media, client options (`base_url`, `api_key = "test-key"`, `model`), and mock server response `{ "text": "the transcript" }`.
- Do not construct, import, call, or mock `TranscriptionParts`, `reqwest::multipart::Form`, `reqwest::RequestBuilder`, `build_transcription_parts`, `build_request`, `execute_request`, `make_parsed_request`, or `parse_openai_transcription_response` in the closure test.
- Do not use `MockClient` from `primitive/mod.rs` for the closure test.
- Assert returned `LLMResponse::Success.content == "the transcript"` and `LLMCompleteResponse.model == "gpt-4o-transcribe"` or the seeded request model.
- Assert mock hit count exactly 1.
- Assert request method `POST`, path `/audio/transcriptions`, and header `Content-Type: multipart/form-data; boundary=...`.
- Parse the multipart boundary in the mock matcher and assert exactly one `file` part with `Content-Disposition` name `file`, expected filename, expected audio MIME, and payload bytes exactly equal to the decoded seeded audio bytes.
- Assert `model` field equals the seeded model.
- Assert reserved JSON/chat fields such as `messages` and `stream` are absent.
- Do not satisfy multipart observability with raw-curl/tracing/request-body `as_bytes()` output; multipart bodies may be empty or best-effort there.
- The hermetic closure test must not use `#[ignore]`, env-gated early returns, or skip-integ-tests gating. Live E2E is the only test allowed to skip/ignore when `OPENAI_API_KEY` is unset.

RED-AT-SEAM proof:
- Primary required proof: temporarily route `ProviderStrategy::TranscriptionsApi` to `/chat/completions`, run the closure test, and record failure because the `/audio/transcriptions` expectation has zero hits or receives an unmatched request.
- Optional parser proof, if retained, must be compileable: temporarily dispatch `ResponseType::OpenAITranscription` to the chat parser or return an explicit failure, then observe the transcript/content assertion fail. Do not rely on simply removing a match arm, which may only prove a compile error.

## Behavior 1: Provider Grammar Parses `openai-transcriptions`

### Test Specification

Given the string `"openai-transcriptions"`, when parsed as a `ClientProvider`, then it equals `ClientProvider::OpenAI(OpenAIClientProviderVariant::Transcriptions)`, `Display` returns `"openai-transcriptions"`, and `allowed_providers()` includes the plural string.

Edge cases:
- `"openai-transcription"` remains invalid as a provider string.
- Existing OpenAI provider variants continue to round-trip.

Files touched:
- `engine/baml-lib/llm-client/src/clientspec.rs`

### TDD Cycle

Red:
- Add/extend provider parsing tests near the existing `clientspec.rs` provider round-trip tests.
- The red failure should be an unknown provider parse failure or missing enum variant compile failure.

Green:
- Add `OpenAIClientProviderVariant::Transcriptions`.
- Add `Display`, `FromStr for ClientProvider`, `FromStr for OpenAIClientProviderVariant`, and `allowed_providers()` arms mirroring `Responses`.

Refactor:
- Keep provider string ownership consistent with existing enum display/from-str patterns.
- Do not introduce a parallel string constant system unless the surrounding code already prefers it.

Success criteria:
- `cargo test -p internal-llm-client clientspec`
- Existing provider round-trip tests still pass.

## Behavior 2: `ResponseType::OpenAITranscription` Resolves and Streams Fail Closed

### Test Specification

Given `ensure_client_response_type("openai-transcription")`, when resolved, then it yields `ResponseType::OpenAITranscription`.

Given a stream request reaches `ResponseType::OpenAITranscription`, when `stream_request.rs` matches on response type, then it returns an unsupported-streaming `LLMFailure` before HTTP rather than falling through to an existing parser or becoming a non-exhaustive compile break.

Files touched:
- `engine/baml-lib/llm-client/src/clientspec.rs`
- `engine/baml-lib/llm-client/src/clients/helpers.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/stream_request.rs`

### TDD Cycle

Red:
- Extend response type resolution tests to assert the singular string `"openai-transcription"`.
- Add/extend a streaming dispatch test that fails because `OpenAITranscription` is not handled or because it attempts the wrong path.

Green:
- Add `OpenAITranscription` to unresolved and resolved response enums.
- Add the `"openai-transcription"` arm to `ensure_client_response_type`.
- Update accepted-values error text with the exact singular response type string.
- Add an explicit `ResponseType::OpenAITranscription` arm in `stream_request.rs` returning a fail-closed unsupported-streaming failure.

Refactor:
- Keep the response-type match exhaustive. No wildcard arm that hides future response shapes.

Success criteria:
- `cargo test -p internal-llm-client response_type`
- `cargo test -p baml-runtime transcription_streaming_unsupported` or the local equivalent.

## Behavior 3: `create_transcriptions` Supplies OpenAI Defaults and Disables Streaming

### Test Specification

Given an empty options block for `provider "openai-transcriptions"`, when parsed and resolved, then:
- `base_url == "https://api.openai.com/v1"`.
- `api_key` defaults to env `OPENAI_API_KEY`.
- `supported_request_modes.stream == Some(false)` or `supports_streaming() == false` by the resolved-client convention.

Edge cases:
- Explicit `base_url` override is respected as a base URL only.
- Attempts to configure streaming for this provider are rejected or ignored into a false effective value, per existing config conventions.

Files touched:
- `engine/baml-lib/llm-client/src/clients/openai.rs`
- `engine/baml-lib/llm-client/src/clients/mod.rs`

### TDD Cycle

Red:
- Add a config resolution test mirroring `create_responses`, plus a streaming-support assertion.

Green:
- Implement `UnresolvedOpenAI::create_transcriptions` with OpenAI defaults.
- Wire `OpenAIClientProviderVariant::Transcriptions` in `create_from`.
- Force effective streaming support false for transcriptions.

Refactor:
- If `create_standard`, `create_responses`, and `create_transcriptions` duplicate substantial defaulting logic, extract only a local private helper that preserves existing behavior.

Success criteria:
- `cargo test -p internal-llm-client transcriptions_defaults`
- Test proves transcriptions support no streaming by default.

## Behavior 4: Transcriptions Strategy Builds `/audio/transcriptions`

### Test Specification

Given `ProviderStrategy::TranscriptionsApi` and `base_url = "https://api.openai.com/v1"`, when `get_endpoint` is called with either `is_completion = false` or `is_completion = true`, then the result is exactly `https://api.openai.com/v1/audio/transcriptions`.

Files touched:
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs`

### TDD Cycle

Red:
- Extend endpoint tests to assert both `is_completion` values route to `/audio/transcriptions`.

Green:
- Add `ProviderStrategy::TranscriptionsApi`.
- Select it when `self.provider == "openai-transcriptions"`.
- Add `get_endpoint` arm ignoring `is_completion`.
- Add `get_response_type` arm returning `ResponseType::OpenAITranscription`.

Refactor:
- Keep the strategy dispatch explicit and parallel to `ResponsesApi`; no fallback to `StandardOpenAI`.

Success criteria:
- `cargo test -p baml-runtime get_endpoint`

## Behavior 5: Pure Multipart `TranscriptionParts` Builder

### Test Specification

Given rendered chat messages containing exactly one audio media part with `BamlMediaContent::Base64` and properties containing `model = "gpt-4o-transcribe"`, when `build_transcription_parts(properties, prompt)` runs, then it returns plain data:
- `file_bytes == BASE64_STANDARD.decode(base64)`.
- `filename` is derived from MIME, including `audio/mpeg -> audio.mp3`.
- `mime == media.mime_type_as_ok()`.
- fields contain `model` and allowed optional fields.
- fields do not contain `messages`, `stream`, or unknown reserved request-body keys.

Files touched:
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/types.rs` if a dedicated `TranscriptionParts` struct belongs there.

### TranscriptionParts Contract

Input prompt:
- Accept `Either::Right(chat_messages)` only.
- Reject `Either::Left(prompt)` with a clear pre-HTTP error because transcriptions require audio media and a single multipart file.

Audio:
- Require exactly one `Audio` media part.
- Require `BamlMediaContent::Base64` after `process_media_urls`.
- Reject zero audio parts.
- Reject more than one audio part.
- Reject `Url` or `File` media content at this point as an internal unresolved-media error.
- Decode plain base64 only. Upstream data URLs should already be stripped by `process_media`; table-driven tests may include data-URL-like strings only to assert rejection or upstream assumptions.

Rendered text:
- If there are no non-empty rendered `Text` parts, no text-derived `prompt` is produced.
- If there is exactly one non-empty rendered `Text` part and `properties.prompt` is absent, map that text to multipart field `prompt`.
- If `properties.prompt` is present and any non-empty rendered text part is present, reject as a prompt conflict.
- If more than one non-empty rendered `Text` part is present, reject as ambiguous prompt text.
- Do not silently drop rendered instruction text.

Fields and JSON Value conversion:

| Field | Accepted JSON value | Multipart value | Notes |
| --- | --- | --- | --- |
| `model` | string only | same string | Required. Missing or non-string errors before HTTP. |
| `prompt` | string only | same string | May also come from one rendered text part only when absent in properties. |
| `language` | string only | same string | Optional. |
| `response_format` | absent, `json`, `verbose_json` strings only | same string, default absent or `json` by implementation choice | Reject `text`, `srt`, `vtt`, and any other value before HTTP. |
| `temperature` | number or string | stable `to_string()` for number, same string for string | Reject bool/object/array/null. |

Rejected values:
- object, array, null for any multipart field.
- bool for every field, including `temperature`, unless a future API contract explicitly needs it.
- unknown reserved request-body keys such as `messages` and `stream`.

Parser output model:
- The parser should receive or otherwise use the request model. `LLMCompleteResponse.model = request model`, falling back to `model_name.unwrap_or_default()` if only that optional parser argument is available.

Streaming:
- `ResponseType::OpenAITranscription` stream path fails closed before HTTP; this builder is non-streaming only.

### TDD Cycle

Red:
- Add table-driven tests covering:
  - happy path with one base64 audio and `model`.
  - missing `model`.
  - non-string `model`.
  - zero audio.
  - multiple audio inputs.
  - `Either::Left` prompt.
  - invalid base64.
  - `audio/mpeg` filename extension.
  - text-to-`prompt` mapping.
  - text plus `properties.prompt` conflict.
  - optional string fields.
  - `temperature` number and string.
  - object/array/null/bool rejection.
  - `response_format` allowed `json`/`verbose_json` and rejected `text`/`srt`/`vtt`.

Green:
- Implement pure `build_transcription_parts` returning a plain struct such as `TranscriptionParts { file_bytes, filename, mime, fields }`.
- Reuse existing base64 and MIME access patterns; preserve the `mpeg -> mp3` extension mapping used by OpenAI audio handling.

Refactor:
- Keep reqwest `Form` out of the pure builder.
- Extract a small media accessor only if it removes real duplication with the PDF/audio base64 paths.

Success criteria:
- `cargo test -p baml-runtime transcription_parts`
- No proptest/quickcheck dependency is needed because coverage is table-driven.

## Behavior 6: Parse OpenAI Transcription JSON into `LLMCompleteResponse`

### Test Specification

Given response JSON `{"text":"hello world"}` and request model `"gpt-4o-transcribe"`, when parsed as `ResponseType::OpenAITranscription`, then return `LLMResponse::Success` with:
- `content == "hello world"`.
- `model == "gpt-4o-transcribe"` or `model_name.unwrap_or_default()` by the chosen parser signature.
- `metadata.baml_is_complete == true`.
- `finish_reason == Some("stop")`.
- token counts `None`.

Edge cases:
- `verbose_json` payload with top-level `text` plus extra keys succeeds and ignores extras.
- missing `text` fails with unsupported-response style `LLMFailure`.
- malformed JSON fails through the existing JSON parse path.

Files touched:
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/types.rs`

### TDD Cycle

Red:
- Add parser tests for normal JSON, verbose JSON, missing text, malformed shape, and the required output model value.

Green:
- Add `TranscriptionResponse { text: String }`.
- Add `parse_openai_transcription_response`.
- Wire the parser to populate `LLMCompleteResponse.model` from the request model path.

Refactor:
- Share only obvious response-skeleton construction with existing parsers; keep errors aligned with OpenAI parser conventions.

Success criteria:
- `cargo test -p baml-runtime transcription_response`

## Behavior 7: Runtime Wiring Uses Multipart and Dispatches the Parser

### Test Specification

Given an `OpenAIClient` constructed for `provider "openai-transcriptions"`, when `build_request` runs, then:
- the URL ends with `/audio/transcriptions`.
- `Content-Type` starts with `multipart/form-data`.
- request construction uses `req.multipart(form)`.
- request construction does not use the JSON body branch.
- request construction does not add `stream` or call `add_streaming_options`.
- parser dispatch routes `ResponseType::OpenAITranscription` to `parse_openai_transcription_response`.

Files touched:
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/mod.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/request.rs`
- `engine/baml-runtime/src/internal/llm_client/primitive/stream_request.rs`
- `engine/Cargo.toml`
- `engine/baml-runtime/Cargo.toml`

### TDD Cycle

Red:
- Add a request-builder test that constructs a transcriptions client, builds a request, and asserts URL and multipart content type.
- Add a parser dispatch test or extend existing `make_parsed_request` coverage for `OpenAITranscription`.
- Add a stream path test that proves unsupported streaming returns before HTTP.
- Before enabling `"multipart"` features, referencing `reqwest::multipart::{Form, Part}` should compile-fail; this is expected red at the dependency seam.

Green:
- Add `"multipart"` to reqwest features in both `engine/Cargo.toml` and `engine/baml-runtime/Cargo.toml`.
- Add `OpenAIClient::new_transcriptions` and `dynamic_new_transcriptions`, mirroring `new_responses` and forcing `properties.client_response_type = ResponseType::OpenAITranscription`.
- Add static and dynamic factory arms in `primitive/mod.rs`.
- Add a small adapter from `TranscriptionParts` to `reqwest::multipart::Form` using `Part::bytes(file_bytes).file_name(filename).mime_str(&mime)` and `.text(k, v)` fields.
- In `build_request`, branch on `ProviderStrategy::TranscriptionsApi` after shared timeout/query/header/auth/proxy setup, attach multipart, and return the request builder without `.json(&body)` or streaming options.
- Add `ResponseType::OpenAITranscription` parser dispatch in `make_parsed_request`.
- Add `ResponseType::OpenAITranscription` fail-closed dispatch in `stream_request.rs`.

Refactor:
- Keep `build_request` flat by using a named helper such as `attach_transcription_body`.
- Compute strategy once and dispatch exhaustively.
- Do not hide transcriptions behind `StandardOpenAI`.

Success criteria:
- `cargo test -p baml-runtime transcriptions_build_request`
- `cargo test -p baml-runtime transcription_streaming_unsupported`
- `cargo build -p baml-runtime`
- `cargo build -p baml-runtime --target wasm32-unknown-unknown`
- Repo clippy gate and wasm clippy if locally defined.

## Behavior 8: Blocking Closure - Audio In, Transcript Out

### Test Specification

Given an `openai-transcriptions` client whose `base_url` points at a local mock server, one base64 audio input, and `model = "gpt-4o-transcribe"`, when `OpenAIClient::chat` runs, then the mock server receives exactly one multipart `POST /audio/transcriptions` with the expected file bytes and model field, and the returned `LLMResponse::Success.content == "the transcript"`.

Files touched:
- `engine/baml-runtime/tests/transcriptions_closure.rs` or an in-crate equivalent if the internal trigger is easier.
- `engine/baml-runtime/Cargo.toml` dev-dependencies for the chosen mock server.
- Live E2E files under `integ-tests/` are follow-on after the hermetic closure is green; they do not replace the hermetic closure.

### TDD Cycle

Red:
- Add the closure test before runtime wiring is complete.
- It must fail because the mock `/audio/transcriptions` expectation is not hit or because the response cannot be parsed.
- It must compile and run on native. It must not be `#[ignore]` or env-gated.

Green:
- Behaviors 1-7 make the closure pass without mocking the forbidden span.
- The closure asserts the response content, model, hit count, multipart file part bytes, model field, and absence of `messages`/`stream`.

Refactor:
- Factor mock-server setup and multipart parsing into local test helpers.
- Run the RED-AT-SEAM endpoint mutation once and record the observed failure in the implementation notes or commit/PR description.

Success criteria:
- `cargo test -p baml-runtime --features internal --test transcriptions_closure` if the test remains in `engine/baml-runtime/tests/`.
- Or the equivalent in-crate cargo test command if the implementation chooses an in-crate test module.
- Test output shows the closure test executed and the mock hit assertion ran.
- Native-only mock test status is acceptable only for the test harness; production code still passes wasm build.

## Integration and E2E

Hermetic CI integration:
- Behavior 8 is the primary closure gate and must run without `OPENAI_API_KEY`.

Live E2E:
- Add a BAML fixture client and `Transcribe(clip: audio) -> string` function after the hermetic closure exists.
- Live test may be `#[ignore]` or skipped only when `OPENAI_API_KEY` is unset.
- Live test is not a substitute for the blocking closure test.

Documentation:
- Add provider docs for `openai-transcriptions` after behavior tests are green.
- Document required `model`, optional `prompt`/`language`/`response_format`/`temperature`, JSON-only `response_format` support for v1, and non-streaming status.

## Stage 4 Implementation Split

Recommended split after Gate 3 review:
- FrostyAnchor: Behavior 1 provider variant and Behavior 2 response type plus streaming fail-closed compile arm.
- SageGlacier: Behavior 3 `create_transcriptions`, Behavior 4 endpoint strategy, and reqwest `"multipart"` feature in both Cargo.toml files.
- TurquoiseGlen: Behavior 5 pure `build_transcription_parts` and Behavior 6 parser.
- ScarletMountain: Behavior 7 runtime wiring and Behavior 8 closure test.

Dependency notes:
- Behavior 7 depends on Behaviors 1-6.
- Behavior 8 depends on all runtime seams but should be written red before the green implementation is finished.
- Shared files such as `clientspec.rs`, `openai_client.rs`, `request.rs`, and `stream_request.rs` require Agent Mail file reservations before editing.

## Verification Gates for Stage 4

Required local commands after implementation:
- `cargo test -p internal-llm-client`
- `cargo test -p baml-runtime`
- `cargo test -p baml-runtime --features internal --test transcriptions_closure` or the exact in-crate closure command chosen in Behavior 8
- `cargo build -p baml-runtime`
- `cargo build -p baml-runtime --target wasm32-unknown-unknown`
- repo clippy gate
- wasm clippy if the repo has a local equivalent

No fabricated green:
- Every behavior test must be observed red for the right reason before the implementation that makes it pass.
- The closure test must be observed red at the endpoint seam before final green.
- A skipped closure test is unverified.

## References

- Draft plan superseded by this file: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/plans/2026-07-17-18-06-openai-transcriptions-endpoint-tdd.md`
- Stage 1 map: `thoughts/2026-07-18-openai-transcriptions-system-map.md`
- Research 1: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/research/2026-07-17-15-58-openai-transcriptions-endpoint-via-base-url.md`
- Research 2: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/research/2026-07-17-16-24-existing-endpoint-layer-patterns-for-new-request-shape.md`
- Closure framework copy read for this mission: `/home/maceo/Dev/silmari-agent-memory/SAI/commands/references/closure-test-framework.md`
- Stage 2 review messages: Agent Mail `2808`, `2809`, `2812`, `2813`
