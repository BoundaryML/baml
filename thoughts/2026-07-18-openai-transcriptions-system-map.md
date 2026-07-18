---
date: 2026-07-18
agent: FrostyAnchor
stage: 1
topic: "OpenAI /v1/audio/transcriptions endpoint touchpoint system_map"
status: gate-candidate
branch: openai-transcriptions
base_commit: 621771315
---

# OpenAI Transcriptions Touchpoint System Map

## Inputs

- TDD plan: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/plans/2026-07-17-18-06-openai-transcriptions-endpoint-tdd.md`
- Research 1: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/research/2026-07-17-15-58-openai-transcriptions-endpoint-via-base-url.md`
- Research 2: `/home/maceo/ntm_Dev/baml/thoughts/searchable/shared/research/2026-07-17-16-24-existing-endpoint-layer-patterns-for-new-request-shape.md`
- Closure rules: `/home/maceo/Dev/silmari-agent-memory/SAI/commands/references/closure-test-framework.md`
- Stage 1 verifier support: `thoughts/searchable/shared/system_maps/2026-07-18-openai-transcriptions-touchpoint-system-map-pearlcat.md`

Note: the mission's `references/closure-test-framework.md` path is not present in this worktree. The exact framework file was found at the external path above; the TDD plan also embeds the relevant closure-test requirements.

## Operation Chain

```mermaid
flowchart TD
  A["provider \"openai-transcriptions\""] --> B["ClientProvider::OpenAI(OpenAIClientProviderVariant::Transcriptions)"]
  B --> C["OpenAIClientProviderVariant::create_from"]
  C --> D["UnresolvedOpenAI::create_transcriptions"]
  D --> E["ResolvedOpenAI { base_url, api_key, properties, client_response_type, media_url_handler }"]
  E --> F["LLMPrimitiveProvider factory"]
  F --> G["OpenAIClient::new_transcriptions / dynamic_new_transcriptions"]
  G --> H["OpenAIClient { provider: \"openai-transcriptions\", properties, reqwest::Client }"]
  H --> I["OpenAIClient::chat"]
  I --> J["make_parsed_request(..., ResponseType::OpenAITranscription, ...)"]
  J --> K["build_request"]
  K --> L["ProviderStrategy::TranscriptionsApi"]
  L --> M["get_endpoint(base_url, _) -> {base_url}/audio/transcriptions"]
  K --> N["build_transcription_parts(properties, messages)"]
  N --> O["reqwest multipart/form-data: file + model/pass-through fields"]
  O --> P["execute_request"]
  P --> Q["parse_openai_transcription_response({ text })"]
  Q --> R["LLMResponse::Success(LLMCompleteResponse { content: text, baml_is_complete: true })"]
```

Linear form required by the mission:

```text
provider enum
  -> create_from
  -> strategy/get_endpoint
  -> build_request
  -> make_parsed_request
  -> parser
  -> LLMCompleteResponse
```

## Interface Grammar

```text
provider_string ::= "openai-transcriptions"

ClientProvider ::= OpenAI(OpenAIClientProviderVariant) | Anthropic | AwsBedrock | GoogleAi | Vertex | Strategy
OpenAIClientProviderVariant ::= Base | Ollama | Azure | Responses | Generic | OpenRouter | Transcriptions

UnresolvedResponseType ::= OpenAI | OpenAIResponses | Anthropic | Google | Vertex | OpenAITranscription
ResponseType ::= OpenAI | OpenAIResponses | Anthropic | Google | Vertex | OpenAITranscription

ProviderStrategy ::= ResponsesApi | StandardOpenAI { provider: String } | TranscriptionsApi

TranscriptionRequest ::= multipart/form-data {
  file: Part::bytes(decoded_audio).file_name(filename).mime(mime),
  model: string,
  prompt?: string,
  language?: string,
  response_format?: string,
  temperature?: string
}

TranscriptionResponse ::= { "text": string, ... }
NormalizedResponse ::= LLMResponse::Success(LLMCompleteResponse { content: TranscriptionResponse.text, ... })
```

## Seams And Contracts

### S1: Provider String -> Config Enum

Boundary: `.baml`/dynamic client provider string into `internal-llm-client`.

Current anchors:
- `engine/baml-lib/llm-client/src/clientspec.rs:57` `OpenAIClientProviderVariant`
- `engine/baml-lib/llm-client/src/clientspec.rs:94` `Display`
- `engine/baml-lib/llm-client/src/clientspec.rs:116` `FromStr for ClientProvider`
- `engine/baml-lib/llm-client/src/clientspec.rs:148` `FromStr for OpenAIClientProviderVariant`
- `engine/baml-lib/llm-client/src/clientspec.rs:182` `allowed_providers`

Contract:
- `"openai-transcriptions"` parses to `ClientProvider::OpenAI(OpenAIClientProviderVariant::Transcriptions)`.
- `Display` returns the exact provider string and round-trips.
- `allowed_providers()` includes the exact plural string.
- `"openai-transcription"` remains invalid.

### S2: Config Enum -> Resolved OpenAI Properties

Boundary: provider enum dispatch into OpenAI option validation/defaulting.

Current anchors:
- `engine/baml-lib/llm-client/src/clients/mod.rs:169` `OpenAIClientProviderVariant::create_from`
- `engine/baml-lib/llm-client/src/clients/openai.rs:382` `create_responses`
- `engine/baml-lib/llm-client/src/clients/openai.rs:448` `create_common`

Contract:
- `Transcriptions` routes to `UnresolvedOpenAI::create_transcriptions`.
- Defaults mirror `create_responses`: `base_url = "https://api.openai.com/v1"`, `api_key = env.OPENAI_API_KEY`, `ensure_http_config("openai")`.
- `base_url` remains a base URL; `/audio/transcriptions` is appended only by request strategy.
- `model`, `prompt`, `language`, `response_format`, and `temperature` remain pass-through `properties` for the multipart field builder.

### S3: Response Type -> Parser Dispatch Key

Boundary: config response-type grammar into runtime parser enum.

Current anchors:
- `engine/baml-lib/llm-client/src/clientspec.rs:493` `UnresolvedResponseType`
- `engine/baml-lib/llm-client/src/clientspec.rs:503` `ResponseType`
- `engine/baml-lib/llm-client/src/clients/helpers.rs:394` `ensure_client_response_type`
- `engine/baml-runtime/src/internal/llm_client/primitive/request.rs:486` response parser match

Contract:
- Add `OpenAITranscription` to unresolved and resolved response enums.
- `ensure_client_response_type("openai-transcription")` resolves to the unresolved variant and updates the accepted-values error text.
- Runtime dispatch handles `ResponseType::OpenAITranscription`; no default/fallback arm may route it through the chat parser.

### S4: Runtime Factory -> OpenAIClient

Boundary: resolved client provider into concrete runtime client.

Current anchors:
- `engine/baml-runtime/src/internal/llm_client/primitive/mod.rs:116` dynamic factory
- `engine/baml-runtime/src/internal/llm_client/primitive/mod.rs:185` static factory
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:617` `new_responses`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:664` `dynamic_new_responses`

Contract:
- Static and dynamic factories both route `Transcriptions`.
- Constructors set `OpenAIClient.provider = "openai-transcriptions"`.
- Constructors force `properties.client_response_type = ResponseType::OpenAITranscription`, mirroring `openai-responses`.
- Existing `make_openai_client!` media features are retained; audio URL/file inputs resolve to base64 before provider request construction.

### S5: Provider String -> Strategy / Endpoint

Boundary: concrete `OpenAIClient.provider` string into request-path strategy.

Current anchors:
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:103` `ProviderStrategy`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:110` `get_endpoint`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:386` `get_provider_strategy`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:397` `get_response_type`

Contract:
- `provider == "openai-transcriptions"` selects `ProviderStrategy::TranscriptionsApi`.
- `get_endpoint(base_url, _)` returns exactly `{base_url}/audio/transcriptions`; `is_completion` is ignored.
- `get_response_type()` returns `ResponseType::OpenAITranscription`.
- Matches stay exhaustive; no wildcard may silently route the provider to `/chat/completions`.

### S6: RequestBuilder -> Multipart Request

Boundary: generic request lifecycle into provider-specific HTTP request body.

Current anchors:
- `engine/baml-runtime/src/internal/llm_client/primitive/request.rs:58` `RequestBuilder`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:410` `OpenAIClient::build_request`
- `engine/Cargo.toml:133` workspace `reqwest` features
- `engine/baml-runtime/Cargo.toml:145` wasm32 `reqwest` features

Contract:
- Transcriptions keeps existing timeout, query-param, custom-header, bearer-auth, and proxy-original-url behavior.
- Transcriptions does not call `ProviderStrategy::build_body`, `.json(&body)`, or `add_streaming_options`.
- Transcriptions attaches `reqwest::multipart::Form` with `Part::bytes`; no stream parts, to preserve wasm viability.
- `reqwest` feature `"multipart"` is enabled in both Cargo.toml declarations.

### S7: Rendered Audio Media -> TranscriptionParts

Boundary: rendered chat media into multipart input data.

Current anchors:
- `engine/baml-runtime/src/internal/llm_client/traits/mod.rs:163` `WithSingleCallable::single_call`
- `engine/baml-runtime/src/internal/llm_client/traits/mod.rs:446` `process_media_urls`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:727` OpenAI audio media arm
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/openai_client.rs:786` OpenAI PDF base64 pattern
- `engine/baml-lib/baml-types/src/media.rs:32` `BamlMedia`
- `engine/baml-lib/baml-types/src/media.rs:49` `mime_type_as_ok`
- `engine/baml-lib/baml-types/src/media.rs:144` `MediaBase64`

Contract:
- `build_transcription_parts(properties, messages) -> TranscriptionParts` is pure and unit-testable.
- It finds an audio `BamlMediaContent::Base64`; reaching it with `Url` or `File` is an internal error because upstream media processing should resolve those to base64.
- `file_bytes == BASE64_STANDARD.decode(base64)`.
- `mime == media.mime_type_as_ok()`.
- Filename extension follows the existing OpenAI audio mapping, including `audio/mpeg -> mp3`.
- Fields include `model` and supported optional pass-through values, excluding reserved `messages` and `stream`.

### S8: HTTP Response -> LLMCompleteResponse

Boundary: provider response JSON into BAML's normalized LLM response.

Current anchors:
- `engine/baml-runtime/src/internal/llm_client/primitive/request.rs:419` `make_parsed_request`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs:24` `parse_openai_response`
- `engine/baml-runtime/src/internal/llm_client/primitive/openai/response_handler.rs:258` `parse_openai_responses_response`
- `engine/baml-runtime/src/internal/llm_client/mod.rs:301` `LLMCompleteResponse`
- `engine/baml-runtime/src/internal/llm_client/mod.rs:314` `LLMCompleteResponseMetadata`

Contract:
- `parse_openai_transcription_response` deserializes `{ "text": string, ... }`.
- Success returns `LLMResponse::Success` with `content = text`, `baml_is_complete = true`, `finish_reason = Some("stop")`, and token fields `None`.
- Malformed JSON or missing `text` returns `LLMFailure` with an unsupported-response style error; extra keys are ignored.

### S9: Closure Test Boundary

Boundary: intention-anchored runtime path, with a real third-party boundary fake only at HTTP.

Closure derivation:
- SOURCE: base64 audio `BamlMedia`, client options (`model`, `base_url`, `api_key`), and local mock server response `{ "text": expected }`.
- TRIGGER: `OpenAIClient::chat`, which crosses the new strategy, multipart body, HTTP execution, response dispatch, and parser seams.
- DRIVER: actual `reqwest` HTTP request to an ephemeral local mock server.
- OBSERVE: `LLMResponse::Success(LLMCompleteResponse).content == expected`.
- FORBIDDEN SPAN: `build_request`, `build_transcription_parts`, multipart adapter, `execute_request`, `make_parsed_request`, and `parse_openai_transcription_response` are not called, seeded, or mocked by the test.
- RED-AT-SEAM: disabling the `TranscriptionsApi` endpoint arm must make the mock `/audio/transcriptions` expectation fail; removing the `ResponseType::OpenAITranscription` dispatch arm must make parsing fail.
- EXECUTION: hermetic closure test must run in CI and fail-closed if mock-server infrastructure is unavailable; no skip-to-green.

## Classification

The full runtime promise is BLOCKING: `OpenAIClient::chat -> make_parsed_request -> build_request -> real HTTP -> parser -> LLMCompleteResponse` crosses cross-module boundaries and an async HTTP edge. Unit tests for provider parsing, endpoint selection, pure form-parts construction, and parser shape are support tests only; they do not close the workflow promise without S9.

## Implementation Gate Checklist

- [ ] Provider grammar, `Display`, parsing, and `allowed_providers` stay in lockstep.
- [ ] `create_from`, static factory, and dynamic factory all route `Transcriptions`.
- [ ] Constructor response-type override and `get_response_type` agree on `OpenAITranscription`.
- [ ] Endpoint branch returns `/audio/transcriptions` independent of prompt shape.
- [ ] Multipart branch preserves shared request headers/auth/query/timeout behavior and avoids JSON/streaming fields.
- [ ] `build_transcription_parts` is pure and validates base64 audio bytes/mime/fields.
- [ ] Parser dispatch maps `{ "text": ... }` to `LLMCompleteResponse.content`.
- [ ] Native tests, wasm32 build, and clippy are run after implementation.
- [ ] Closure test executes against a real mock HTTP server, records red-at-seam then green, and is never skipped for missing infra.

## Unresolved Or Watch Items

- The local worktree did not contain the mission's relative source docs; canonical source docs were read from sibling clone `/home/maceo/ntm_Dev/baml`.
- The closure framework file is outside both BAML worktrees; this map cites the exact external copy read for Stage 1.
- Multipart has no existing `engine/` precedent. Stage 2 must scrutinize wasm compatibility and the two `reqwest` feature declarations.
