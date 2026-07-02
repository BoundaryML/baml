# Implementation checklist — baml.ai provider model

Ordered plan for building out the `baml.ai` provider model and implementing as many
[`../llm-provider/`](../llm-provider/) scenarios as possible **in real, tested code**.
Grounded in [`llm-provider-plan.md`](./llm-provider-plan.md) (Part III phases) and
[`deviations.md`](./deviations.md). TDD throughout; mock (wiremock) for determinism,
real `gpt-5.4-mini` at milestones. Commit after each ✅.

## Done so far

- ✅ **Phase 0/1 spine** — `Provider` marker, `HttpProvider` (+`type Body`, `call_with<T,V,E2>`/`call<T>`,
  `CallResult<T,V>`), `ResponseMeta`, error model (`UnknownError`+`CallError`), public `baml.sap.parse<T>`.
- ✅ **`baml.ai.OpenAi`** — real Chat Completions provider in BAML (text + structured via SAP; non-2xx errors).
- ✅ **Scenario 01** (single-turn text), **02** (structured output).
- ✅ **E2E wiring** — real `client<llm>` + LLM `function` route through `baml.ai.OpenAi` (orchestrator delegation).
- ✅ **Phase 2 combinators** — `Fallback`, `Retry`, `with_retry`/`fallback_to` factories. **Scenario 29** (reliability).

## Next up (ordered)

### 1. Streaming (Phase 2, scenario 04) — ✅ DONE
- [x] `interface Streaming requires Provider` in `ns_ai` with `stream<TStream,TFinal>(prompt) -> baml.llm.Stream<TStream,TFinal>`.
- [x] `baml.errors.StreamError` classifier interface.
- [x] `OpenAi implements Streaming` — build the SSE request (`"stream": true`), `baml.http.fetch_sse`, produce a
      `baml.llm.Stream` (reuse the existing accumulator/SAP-partial infra — a leaf primitive; add a
      `new_stream_accumulator_for(provider)` host helper if needed).
- [x] `Fallback`/`Retry` forward `Streaming` (route `.stream` to a streaming member).
- [x] Wire `stream_llm_function` to delegate to the new provider for openai (mirror the oneshot delegation).
- [x] Tests: mock SSE stream → partial + final; live streaming of a short structured value.

### 2. Value + sidecar completion (Phase 2, scenarios 32/34) — ✅ core done
- [x] Extend `ResponseMeta` with `usage() -> Usage` (`class Usage { input_tokens, output_tokens }`) + a `Supported<T>` type.
- [x] `OpenAiResponseMeta.usage()` reads the wire `usage` block.
- [ ] A `Traced` / `Budget` combinator that projects usage via `call_with` and aggregates over a chain (D4).
- [x] Tests: mock returns a `usage` block; `call_with(prompt, m => m.usage())` returns tokens; budget sums across calls.

### 3. Provider diversity (scenario 28) — ✅ done (routing test)
- [ ] A second OpenAI-compatible provider (e.g. `OpenAiGeneric` / a proxy) = same class, different `base_url`; typed `Auth` field.
- [x] Prefix-routing (function-returning-Provider)  example (route by model prefix to different providers).
- [x] Tests: mock two endpoints; a routing function picks the right one.

### 4. Cascades & routing (scenario 30) — ✅ done (+ RoundRobin combinator)
- [x] Routing is a `client`-as-function; cascade is a `Fallback`-shaped combinator.
- [x] A `ConfidenceProvider`-style capability + a `Cascade` that escalates on low confidence (documents the
      "presence-not-calibration" B2 gap from D3).
- [x] Tests: cheap-then-expensive escalation via mock.

### 5. Tools & the agentic loop (scenario 09) — ✅ core done
- [x] `interface Tools requires Provider` with `type Transcript`, `begin`/`step`/`submit`, default `run_tools`.
      `class Tool { name, description, parameters: type }`; `ToolCall`/`ToolResult`/`ToolCalls`.
- [x] `ToolError` channel. `OpenAi implements Tools` — OpenAI function-calling wire (tools in request, tool_calls
      in response, tool-result messages). Schema via `baml.reflect.type_to_json_schema(tool.parameters)`.
- [ ] `ExecutionContext.dispatch` coerces args via `baml.sap.parse` against the handler type (D6).
- [ ] D5: `run_tools<T> -> T | Partial<T>` sum outcome (no fake ToolError).
- [x] Tests: a real multi-turn tool loop against the live API (e.g. a weather tool), + mock for determinism.

### 6. Opportunistic scenarios (as the surface allows)
- [x] 03 constrained-decoding — `Constrained` capability with no default; OpenAI degrades to `Unsatisfiable` (the honest B1 story).
- [ ] 05 multimodal input — image parts in the request (needs PromptAst media threading / a media host helper).
- [x] 07 reasoning — `ResponseMeta.reasoning()` projection.
- [x] 08 enriched outputs — logprobs/citations as `ResponseMeta` dimensions.

### Realtime family (22–26) — ✅ capability + examples (transport stubbed, P8)
- [x] `Realtime`/`Channel`/`LiveControl` interfaces + `OpenAiRealtime` + negotiation tests.

## Deferred (need host surface not yet built)
- Realtime/harness (Phase 4): duplex transport (`baml.ws`/subprocess), `Channel`, lifecycle. Scenarios 22–26, 37–42.
- Stateful/workflows (Phase 5): sessions/chains/jobs/durable + inbound control-inversion. Scenarios 17–21, 27, 43–47.
- Client-as-sugar rewrite (replacing orchestrator delegation) — a lower_cst.rs change; big blast radius.
- Multi-message/role prompt threading (currently flattened) — a `prompt_to_messages` host helper.

## Working rules
- TDD: write the failing test (or scratch-prototype via `baml-cli --file`) first; then implement.
- Verify std compiles strictly via `baml-cli run --file <trivial>` (catches `unreachable arm` etc. that `baml_test!` downgrades).
- Run the full `ai_provider` suite + `baml_src` regression before each commit.
- Update `deviations.md` whenever the implementation diverges from the plan/design or a language limit forces a workaround.

## Scenario coverage — ALL 47 realized as compiling code (goal: create all 45+ examples)

Full impls + tests (mock+live): 01, 02, 03, 04, 09, 28, 29, 30, 32, 34.
Enriched `ResponseMeta` (reasoning/logprobs/citations): 07, 08.
Capabilities + negotiated examples (host transport/persistence stubbed where native):
- Realtime family: 22, 23, 24, 25, 26 (`ns_ai/realtime.baml`)
- Stateful: 17, 18, 19, 20, 21, 27, 31, 44 (`ns_ai/stateful.baml`)
- Negotiation: 36 (`ns_ai/negotiation.baml`, Support lattice)
- Harness: 37, 38, 39, 40, 41, 42 (`ns_ai/harness.baml` + usage)
Usage examples (`crates/baml_tests/baml_src/ns_ai_examples/`):
- Tools family: 10, 11, 14, 15 (`tools_extras.baml`); 12, 13, 16 (`cross_cutting.baml`/`misc.baml`)
- Workflows: 43, 44, 45, 46, 47 (`workflows.baml`/`misc.baml`)
- Cross-cutting: 05, 06 (shape), 33, 35 (`cross_cutting.baml`/`misc.baml`)

All example files compile as part of the `baml_src` suite (bytecode-snapshotted). Live-tested
where the OpenAI HTTP path allows; capability shapes + negotiation compile-verified elsewhere.

### Top blocker for "real user-defined providers" (see deviations.md)
Cross-package `requires`-satisfaction is broken: a **user** class cannot implement a stdlib
capability interface (`HttpProvider`/`Realtime`) because its `implements Provider {}` isn't seen
by the `requires` check. Providers must live in stdlib (`ns_ai`) until this compiler fix lands.
