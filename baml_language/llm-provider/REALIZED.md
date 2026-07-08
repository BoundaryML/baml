# Scenarios realized in real code

Every scenario in [`ideas/scenarios/`](./ideas/scenarios/) is now realized as **compiling BAML**
against the real `baml.ai` provider model (built in `crates/baml_builtins2/baml_std/baml/ns_ai/`).
The HTTP request/response path is **live-tested** against `gpt-5.4-mini`; capabilities whose
transport/persistence is genuinely native (realtime duplex, durable stores) have their **shape +
negotiation** compiled and their host body stubbed pending that surface (plan P6/P8).

The complete verified test surface is indexed in [`E2E_TESTS.md`](./E2E_TESTS.md).
See [`../_plan/implementation-checklist.md`](../_plan/implementation-checklist.md) and
[`../_plan/deviations.md`](../_plan/deviations.md) for status and every divergence.

## The model (`baml.ai.*`, all BAML on leaf primitives)
- **Spine:** `Provider` marker, `HttpProvider` (`type Body`, `call_with<T,V,E2>`/`call`, `CallResult<T,V>`), `ResponseMeta` (usage/finish_reason/reasoning/logprobs/citations).
- **Capabilities:** `Streaming`, `Tools`, `Constrained`, `Realtime`/`LiveControl`, `Conversational`, `Compaction`, `Branching`, `Chain`, `MemoryStore`, `Background`, `ManagedCache`, `Suspendable`, `Capabilities` (Support lattice).
- **Combinators:** `Fallback`, `Retry`, `RoundRobin`.
- **Providers:** `OpenAi` (text/structured/streaming/tools/negotiation), `OpenAiRealtime`, `ClaudeCode`/`PiAgent` (harness).
- **Errors:** `UnknownError` + `CallError`/`StreamError`/`ToolError`/`RealtimeError`; `Unsupported` on all channels.

## Scenario → code

| # | Scenario | Realized in | Level |
|---|---|---|---|
| 01 | single-turn text | `ns_ai/providers/openai.baml`; test `openai_*` | live |
| 02 | structured output | SAP (`openai.baml`) + strict-mode (`OpenAiStrict`, `response_format` json_schema); `openai_structured_*`, `strict_live_extraction` | live |
| 03 | constrained decoding | `provider.baml` `Constrained`; `constrained_*` test | tested |
| 04 | streaming | `Streaming` capability; `openai_stream_*`, `structured_streaming_live` (partials + typed final) | live |
| 05 | multimodal input | native `ChatMessage`/`MessagePart` pipeline; `e2e_multimodal_live` | live |
| 06 | non-text output | `ns_ai_examples/misc.baml` (shape) | compiled |
| 07 | reasoning | `ResponseMeta.reasoning()`; `response_meta_*` test | tested |
| 08 | enriched (logprobs/citations) | `ResponseMeta.logprobs/citations`; test | tested |
| 09 | tool calling | `ns_ai/capabilities/tools.baml` (+`Tool.from_type` typed params); `tools_loop_*`, `typed_tool_agent_live` | live |
| 10 | agentic loop + stop | `ns_ai_examples/tools_extras.baml` (`bounded_agent`) | compiled |
| 11 | parallel tools | `tools_extras.baml` + `multi_tool_agent_live` (3 tools, multi-call turns) | live |
| 12 | tool taxonomy | `cross_cutting.baml` (`ToolKind`) | compiled |
| 13 | searchable tools | `misc.baml` (`search_tools`) | compiled |
| 14 | multi-agent / handoff | specialist-provider-in-dispatch; `multi_agent_handoff_live` | live |
| 15 | guardrails | `tools_extras.baml` (`guarded_call`) | compiled |
| 16 | agent security | `misc.baml` (`security_gated_dispatch`) | compiled |
| 17 | history + sessions | native `ChatMessage[]` threading; `conversation_history_*` | live |
| 18 | compaction | `stateful.baml` `Compaction` | compiled |
| 19 | fork/branch | `stateful.baml` `Branching` | compiled |
| 20 | server-stored chains | `ns_ai/providers/openai_responses.baml` (`Chain` via previous_response_id); `responses_live_chain` | live |
| 21 | memory | `stateful.baml` `MemoryStore` | compiled |
| 22 | realtime voice | `OpenAiRealtime` over `baml.ws` (GA protocol); `realtime_text_exchange_live` | live |
| 23 | barge-in | `realtime.baml` `LiveControl` | compiled |
| 24 | realtime tools | `realtime.baml` (+ Tools) | compiled |
| 25 | voice pipelines | `misc.baml` (`cascaded_voice`) | compiled |
| 26 | transports | HTTP + SSE + WebSocket (`baml.ws`) all real; same capability model over each | live |
| 27 | background jobs | `OpenAiResponses implements Background` (`background:true` + poll); `responses_background_live` | live |
| 28 | provider diversity | `provider_diversity_routing` test | tested |
| 29 | reliability | `ns_ai/combinators.baml`; `fallback_*`/`retry_*` | tested |
| 30 | cascades/routing | `cascade_escalates_*` test | tested |
| 31 | caching | `stateful.baml` `ManagedCache` | compiled |
| 32 | observability | `ResponseMeta.usage()`; `call_with_projects_usage` | tested |
| 33 | evaluation | task + LLM-judge with typed Verdict; `eval_judge_live` | live |
| 34 | cost/tokens | `call_with_projects_usage` (mock), `usage_metering_live` | live |
| 35 | deployment shapes | `cross_cutting.baml` (server/edge/browser clients) | compiled |
| 36 | capability negotiation | `ns_ai/capabilities/introspection.baml` (Support lattice) | tested |
| 37 | harness basics | `ns_ai/providers/harness.baml`; `ns_ai_examples/harness.baml` | compiled |
| 38 | permissions/sandbox | `harness.baml` (config fields) | compiled |
| 39 | harness extensibility | `harness.baml` (`allowed_tools`) | compiled |
| 40 | harness sessions | `harness.baml` + `stateful` Session | compiled |
| 41 | harness deployment | `harness.baml` (config variance) | compiled |
| 42 | harness abstraction | `ns_ai_examples/harness.baml` (`drive_any`) | compiled |
| 43 | workflow graph | parallel spawn/await fan-in over real calls; `workflow_graph_live` | live |
| 44 | suspend/resume | `stateful.baml` `Suspendable`; `workflows.baml` | compiled |
| 45 | durable execution | `workflows.baml` (`durable_step`) | compiled |
| 46 | workflow observability | `misc.baml` (`observe_pipeline`) | compiled |
| 47 | agents-in-workflows | `workflows.baml` (`workflow_as_tool_dispatch`) | compiled |

**"live"** = exercised against the real OpenAI API (and also mock-tested). **"tested"** = covered by a
deterministic test (wiremock request-capture or VM runtime test) in `crates/baml_tests/tests/ai_provider.rs`.
**"compiled"** = realized as compiling BAML, verified by the `baml_src` suite; the native transport/persistence
host surface is stubbed (throws) pending P6/P8.

## Blockers: none for user-authored providers or capabilities
E0125 is fixed, and the desugar/registry work went further: users declare their own
**capabilities** (`//baml:llm_capability` + `//baml:llm_companion(<suffix>)` drivers) and every
LLM function grows a generated `Foo$<suffix>` companion negotiating at runtime
(`ns_ai_custom_capability/usage.baml` proves it e2e). Every scenario above with a runnable
offline shape now lives as tests under `baml_src/ns_ai_scenarios/NN_*` (44 of 47; the rest are
live/media-gated or P8-transport-shaped) — see `_plan/implementation-checklist.md` for the
tracker and `_plan/llm-desugar-capabilities-plan.md` for the design.
