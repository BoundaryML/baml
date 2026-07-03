# E2E tests that work completely

The verified end-to-end surface of the `baml.ai` provider model. **Live** = hits the real
OpenAI API (`gpt-5.4-mini` / `gpt-realtime`), gated on `OPENAI_API_KEY` (skips without it).
**Mock** = deterministic wiremock at the HTTP/SSE level, often with request-capture
assertions on the exact wire bytes. Run everything:

```bash
export OPENAI_API_KEY=sk-...   # enables the live tier
cargo test -p baml_tests --test ai_provider --test ai_responses --test ai_strict --test ai_realtime
```

Suites: `crates/baml_tests/tests/{ai_provider,ai_responses,ai_strict,ai_realtime}.rs` —
49 tests, all green as of this writing.

## Live (17) — real API, end to end

| Test | Proves | Scenario |
|---|---|---|
| `openai_live_call` | single-turn text through `HttpProvider.call<string>` | 01 |
| `openai_structured_live_call` | typed extraction (`call<Person>`) via schema-inject + SAP | 02 |
| `strict_live_extraction` | **strict mode**: `OpenAiStrict` + `response_format: json_schema, strict:true` from a lowered BAML `type` | 02 |
| `openai_stream_live` | SSE streaming, partials + final | 04 |
| `structured_streaming_live` | partial **structured** streaming (`Extract$stream` → typed `Person` final) | 04 |
| `e2e_multimodal_live` | image input: `${img}` → native `image_url` part → model reads it | 05 |
| `tools_loop_live` | the agentic loop (`begin`/`step`/`submit` via `run_tools`) | 09 |
| `multi_tool_agent_live` | **multi-tool agent**: 3 registered tools, model calls ≥2, results composed | 09+11 |
| `typed_tool_agent_live` | **typed tools**: `Tool.from_type` schema out, dispatcher SAP-parses args back into the class | 09 (D6/P7) |
| `multi_agent_handoff_live` | **multi-agent**: specialist provider invoked inside the tool dispatch | 14 |
| `conversation_history_live` | multi-turn history threaded as native `ChatMessage[]` | 17 |
| `responses_live_chain` | server-stored chains: `Chain.extend` via `previous_response_id` | 20 |
| `realtime_text_exchange_live` | **realtime over WebSocket** (`baml.ws`, GA protocol), events through a user `Channel` | 22 |
| `responses_background_live` | background jobs: submit `background:true`, poll to typed value | 27 |
| `eval_judge_live` | eval: task call scored by an LLM judge returning a typed `Verdict` | 33 |
| `usage_metering_live` | real token counts via `call_with(…, m => m.usage())` | 34 |
| `e2e_client_function_via_new_provider_live` | a user-declared `client<llm>` + LLM `function` executing through `baml.ai.OpenAi` | wiring |

## Mock / deterministic (32) — wiremock, request-capture, VM

| Area | Tests |
|---|---|
| Call pipeline | `openai_call_via_mock`, `openai_structured_via_mock`, `responses_call_via_mock`, `strict_request_shape_via_mock` (asserts `strict:true` + `additionalProperties:false` on the wire; none for `string`) |
| Streaming | `openai_stream_via_mock` (SSE deltas), `e2e_function_stream_via_new_provider_mock` (`Foo$stream` companion) |
| Wire fidelity (request capture) | `e2e_options_and_headers_forwarded` (temperature/headers), `e2e_structured_schema_injected_once` (no double schema), `e2e_media_prompt_reaches_wire` (image on the wire), `e2e_roles_preserved_on_wire` (system+user preserved), `conversation_history_via_mock` (full history resent) |
| Tools | `tools_loop_via_mock` (2-turn tool exchange) |
| Combinators | `fallback_routes_to_first_working_member`, `retry_recovers_after_transient_failures`, `retry_exhausts_and_throws`, `retry_skips_non_retryable_errors` (400 → exactly 1 request), `retry_refuses_effectful_provider` (typed `CannotRetry`), `round_robin_alternates_members` |
| Chains / jobs | `responses_chain_via_mock` (asserts `previous_response_id`), `responses_background_via_mock` (queued → pending → value) |
| Routing / cascade | `provider_diversity_routing`, `cascade_escalates_on_low_confidence` |
| Capability model | `openai_implements_capabilities`, `realtime_capability_negotiation`, `stateful_capabilities_negotiation`, `constrained_capability_absent_is_runtime_promise`, `capability_error_normalization` |
| Meta / schema | `call_with_projects_usage`, `response_meta_reasoning_and_logprobs`, `schema_lowering_unit` |
| Transport | `ws_connect_unreachable_throws_io` |

Plus **2004 BAML-level tests** in the `baml_src` suite (incl. 17 `baml.json.path` unit tests
and the compiled scenario examples in `ns_ai_examples/`), bytecode-snapshotted.

## Providers exercised
`OpenAi` (Chat Completions: text/structured/streaming/tools/vision), `OpenAiStrict`
(response_format json_schema strict), `OpenAiResponses` (/v1/responses: chains +
background jobs), `OpenAiRealtime` (wss:// GA realtime).
