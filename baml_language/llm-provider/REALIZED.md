# Scenarios realized in real code

Every scenario in [`ideas/scenarios/`](./ideas/scenarios/) is now realized as **compiling BAML**
against the real `baml.ai` provider model (built in `crates/baml_builtins2/baml_std/baml/ns_ai/`).
The HTTP request/response path is **live-tested** against `gpt-5.4-mini`; capabilities whose
transport/persistence is genuinely native (realtime duplex, durable stores) have their **shape +
negotiation** compiled and their host body stubbed pending that surface (plan P6/P8).

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
| 01 | single-turn text | `ns_ai/openai.baml`; test `openai_*` | live |
| 02 | structured output | `openai.baml` (SAP); `openai_structured_*` | live |
| 03 | constrained decoding | `provider.baml` `Constrained`; `constrained_*` test | tested |
| 04 | streaming | `openai.baml` `Streaming`; `openai_stream_*` | live |
| 05 | multimodal input | `ns_ai_examples/cross_cutting.baml` (`image_prompt`) | compiled |
| 06 | non-text output | `ns_ai_examples/misc.baml` (shape) | compiled |
| 07 | reasoning | `ResponseMeta.reasoning()`; `response_meta_*` test | tested |
| 08 | enriched (logprobs/citations) | `ResponseMeta.logprobs/citations`; test | tested |
| 09 | tool calling | `ns_ai/tools.baml`; `tools_loop_*` | live |
| 10 | agentic loop + stop | `ns_ai_examples/tools_extras.baml` (`bounded_agent`) | compiled |
| 11 | parallel tools | `tools_extras.baml` (`parallel_dispatch`, spawn/all) | compiled |
| 12 | tool taxonomy | `cross_cutting.baml` (`ToolKind`) | compiled |
| 13 | searchable tools | `misc.baml` (`search_tools`) | compiled |
| 14 | multi-agent / handoff | `tools_extras.baml` (`handoff_dispatch`) | compiled |
| 15 | guardrails | `tools_extras.baml` (`guarded_call`) | compiled |
| 16 | agent security | `misc.baml` (`security_gated_dispatch`) | compiled |
| 17 | history + sessions | `ns_ai/stateful.baml` `Conversational` | compiled |
| 18 | compaction | `stateful.baml` `Compaction` | compiled |
| 19 | fork/branch | `stateful.baml` `Branching` | compiled |
| 20 | server-stored chains | `stateful.baml` `Chain` | compiled |
| 21 | memory | `stateful.baml` `MemoryStore` | compiled |
| 22 | realtime voice | `ns_ai/realtime.baml` `Realtime`; `realtime_*` test | tested |
| 23 | barge-in | `realtime.baml` `LiveControl` | compiled |
| 24 | realtime tools | `realtime.baml` (+ Tools) | compiled |
| 25 | voice pipelines | `misc.baml` (`cascaded_voice`) | compiled |
| 26 | transports | `realtime.baml` (transport orthogonal to capability) | compiled |
| 27 | background jobs | `stateful.baml` `Background`/`Job` | compiled |
| 28 | provider diversity | `provider_diversity_routing` test | tested |
| 29 | reliability | `ns_ai/combinators.baml`; `fallback_*`/`retry_*` | tested |
| 30 | cascades/routing | `cascade_escalates_*` test | tested |
| 31 | caching | `stateful.baml` `ManagedCache` | compiled |
| 32 | observability | `ResponseMeta.usage()`; `call_with_projects_usage` | tested |
| 33 | evaluation | `cross_cutting.baml` (`run_eval`, `Scorer`) | compiled |
| 34 | cost/tokens | `call_with_projects_usage` test | tested |
| 35 | deployment shapes | `cross_cutting.baml` (server/edge/browser clients) | compiled |
| 36 | capability negotiation | `ns_ai/negotiation.baml` (Support lattice) | tested |
| 37 | harness basics | `ns_ai/harness.baml`; `ns_ai_examples/harness.baml` | compiled |
| 38 | permissions/sandbox | `harness.baml` (config fields) | compiled |
| 39 | harness extensibility | `harness.baml` (`allowed_tools`) | compiled |
| 40 | harness sessions | `harness.baml` + `stateful` Session | compiled |
| 41 | harness deployment | `harness.baml` (config variance) | compiled |
| 42 | harness abstraction | `ns_ai_examples/harness.baml` (`drive_any`) | compiled |
| 43 | workflow graph | `ns_ai_examples/workflows.baml` (`doc_pipeline`) | compiled |
| 44 | suspend/resume | `stateful.baml` `Suspendable`; `workflows.baml` | compiled |
| 45 | durable execution | `workflows.baml` (`durable_step`) | compiled |
| 46 | workflow observability | `misc.baml` (`observe_pipeline`) | compiled |
| 47 | agents-in-workflows | `workflows.baml` (`workflow_as_tool_dispatch`) | compiled |

**"live"** = exercised against the real OpenAI API (and also mock-tested). **"tested"** = covered by a
deterministic test (wiremock request-capture or VM runtime test) in `crates/baml_tests/tests/ai_provider.rs`.
**"compiled"** = realized as compiling BAML, verified by the `baml_src` suite; the native transport/persistence
host surface is stubbed (throws) pending P6/P8.

## The one blocker for user-authored providers
Cross-package `requires`-satisfaction is broken (E0125): a user-package class can't implement a
stdlib capability interface (its `implements Provider {}` isn't seen by the `requires` check), so
provider classes currently live in stdlib. Fixing this is the top priority to make user-defined
providers real — see [`../_plan/deviations.md`](../_plan/deviations.md).
