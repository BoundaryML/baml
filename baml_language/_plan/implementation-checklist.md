# Implementation checklist — baml.ai provider model

The single live execution tracker for the LLM-provider work. How to work an item: see
[`README.md`](./README.md) (reading order, TDD + BAML-native-test rules, snapshot regen, commit-per-✅).
Design references: [`llm-desugar-capabilities-plan.md`](./llm-desugar-capabilities-plan.md) (current
macro-phase, "DCP §…") and [`llm-provider-plan.md`](./llm-provider-plan.md) (master decisions, "plan D…/P…").

## Done (compact — details in [`../llm-provider/REALIZED.md`](../llm-provider/REALIZED.md) / [`E2E_TESTS.md`](../llm-provider/E2E_TESTS.md))

- ✅ **Spine** — `Provider` marker, `HttpProvider` (`type Body`, messages-primitive `call_messages_with`, `call`/`call_with` sugar, `CallResult`), `ResponseMeta` (usage/finish_reason/reasoning/logprobs/citations), errors (`UnknownError`, `CallError`/`StreamError`/`ToolError`/`RealtimeError`), public `baml.sap.parse<T>`. Scenarios 01/02 live.
- ✅ **Native wire representation** — `ChatMessage[]`/`MessagePart` exchange type, `prompt_to_messages` host bridge, roles + media on the wire natively (multimodal live).
- ✅ **Providers in BAML** — `OpenAi` (text/structured/streaming/tools/vision), `OpenAiStrict` (json_schema strict), `OpenAiResponses` (chains + background jobs live), `OpenAiRealtime` (GA realtime over `baml.ws`, live), `Anthropic` (/v1/messages, live tier), `Gemini` (generateContent, live tier; **no streaming** — see backlog).
- ✅ **Streaming** — `Streaming` capability over `baml.llm.Stream`; partial structured streaming live. (Gotcha: `next()` partials are best-effort; assert on `final()`.)
- ✅ **Combinators** — `Fallback`, `Retry` (effect-aware: refuses effectful providers, skips non-retryable errors), `RoundRobin`; factories `with_retry`/`fallback_to`.
- ✅ **Tools** — `Tools` capability (`begin`/`step`/`submit`, default `run_tools` with dispatch closure), `Tool.from_type` (typed params via P7 `baml.schema.json_schema`), live multi-tool + typed-tool + handoff agents.
- ✅ **D2/D8 Failure axis** — mandatory on every error channel; OpenAi/Anthropic/Gemini HTTP errors typed.
- ✅ **Capability shapes compiled** (host surface stubbed pending P8): `Realtime`/`LiveControl`, `Conversational`, `Compaction`, `Branching`, `Chain` (live via Responses), `MemoryStore`, `Background` (live via Responses), `ManagedCache`, `Suspendable`, `Capabilities` Support lattice.
- ✅ **E0125 cross-package `requires` fixed** — user-authored providers e2e (`ai_user_provider.rs`).
- ✅ **E2E wiring (interim)** — `client<llm>` + LLM functions route through `baml.ai.OpenAi` via orchestrator delegation (`_openai_delegation_ok`). *Superseded by DCP Phase C below, which replaces it with true desugaring.*
- ✅ **`baml.json.path<T>`**, `@alias`-honoring SAP wire classes, `type`-named fields (codegen raw-ident fix), stdlib reorg (`ns_ai/{core,capabilities,providers}`), exact-throws (`throws never` legal).

## Next up — the desugar & capabilities plan ([DCP](./llm-desugar-capabilities-plan.md)), in order

### Phase A — capability registry (DCP §1.2)
- [x] Parse `//baml:llm_capability` (interfaces) + `//baml:llm_companion(<suffix>)` (driver fns): `baml_marker_arg` scanner in `docstring.rs`, `InterfaceDef.is_llm_capability` + `FunctionDef.llm_companion_suffix` populated in `lower_cst`, threaded through PPIR; unit-tested. *(Note: a driver's `prompt` param name trips the known LLM-body misparse unless the body leads with `let` — see gotchas.)*
- [x] Registry collection: marker flags flow AST → HIR item tree; `hir::capability_registry::capability_registry(db)` unions capabilities + drivers across builtin/user files in deterministic path order (3 integration tests). *Deviation from the plan's "pre-pass + build-time baking": Salsa per-file incrementality rules out feeding AST lowering, so the registry lives at HIR and companion generation will hook PPIR (the plan's flagged fallback); no baking needed — builtins lower in-session from embedded source.*
- [x] Semantic validation diagnostics (`check.rs` §8, 14 tests): **E0150** capability must transitively `requires baml.ai.Provider` (closure walk resolves each parent's requires in the parent's own package context); **E0151** driver convention (top-level fn, arity 1/2, `client: baml.ai.Provider` identity-resolved, `prompt: baml.llm.PromptAst` last-segment check, marker-on-method flagged); **E0152** duplicate suffix session-wide, first-in-path-order wins (stdlib sorts before user).
- [ ] Mark stdlib capabilities; write drivers `drive_call` / `drive_with` / `drive_stream` / `drive_run_tools` / `drive_live` with the §1.3 degrade chains (strict-check via `baml-cli run --file`).
- [ ] `ToolLoop` combinator (scenario-10 `Bounded` + on-board `tools`/`dispatch`, `implements HttpProvider` by driving the loop; `is_effectful() -> true` so `Retry` refuses it) + stop-policy vocabulary (`step_count_is`, `has_tool_call`, `any_of`, `per_turn_tools`) + `run_to_budget<T> -> T | Budget<T>` (absorbs plan-D5 for tools; see backlog).
- [ ] Exit: registry snapshot test; drivers strict-compile; zero behavior change elsewhere.

### Phase B — `client: baml.ai.Provider` + LegacyClient bridge (DCP §1.1)
- [ ] `LegacyClient` class (`implements Provider/HttpProvider/Streaming` over legacy `Client`, PromptAst-threaded) — the only consumer of the legacy Rust pipeline.
- [ ] Retype the injected param (`append_default_client_param`); shorthand→native-class map (openai/anthropic/gemini); named `client<llm>` lowering → native fields or bridge (bridge for unported providers + `query_params`/finish-reason configs).
- [ ] `client <name>` accepts a **user function returning `Provider`** (declared-client for custom providers, DCP §1.4).
- [ ] Formatter: fix (or consciously accept) the `client`-named-param limitation.
- [ ] Exit: full existing LLM corpus green (still orchestrator-routed); `Extract(doc, client = baml.ai.OpenAi{…})` override works e2e; user provider as declared client works; snapshots regenned.

### Phase C — the desugar (DCP §1.3; blast radius)
- [ ] LLM body → `drive_call<T>(client, Foo$render_prompt(…))`; generate `Foo$<suffix>` per **stdlib** driver; unify `Foo$stream` onto the mechanism; prompt-specialization hook for the bridge.
- [ ] Delete `_openai_delegation_ok`/`_openai_from` + orchestrator special-cases; `call_llm_function`/`stream_llm_function` shrink to bridge internals.
- [ ] Back-compat gate: entire legacy corpus + `ai_*` suites + wire-fidelity request-capture tests pass byte-identically where asserted.
- [ ] Exit: `Foo`/`Foo$stream`/`Foo$with`/`Foo$run_tools` work with declared, swapped, combinator, and bridge clients; anthropic/gemini LLM functions route natively.

### Phase D — user-defined capabilities (DCP §1.4)
- [ ] Companion generation from user-package markers (pre-pass over the compiling package).
- [ ] The Moderated e2e fixture as **BAML test blocks** (capability + driver + provider + companion + typed `Unsupported` + user-fn declared client); diagnostic snapshots (Rust) for the new error codes.
- [ ] Two-marker recipe documented in an `ns_ai` README-comment.

### Phase E — scenario build-out (DCP §1.5) — **the goal: the full original corpus as scenarios**
Every `../llm-provider/ideas/scenarios/NN-*` gets a `ns_ai_scenarios/NN_name/` with runnable tests
(~40 of 47 can run end-to-end today; the P8-blocked tail gets offline/negotiation tests + a
`// BLOCKED: P8 <what>` marker and graduates later). This is a **build-out**, not just a move of
the 7 existing example files.
- [ ] Verify nested-dir namespace behavior of `baml_src/`; scaffold `ns_ai_scenarios/NN_name/{usage,implementation}.baml` (numbering + scenario-URI header comments mirroring the original corpus).
- [ ] Migrate + cull the 7 `ns_ai_examples/` files into their scenario homes (runnable test or real implementation — no third kind); then **fill out the remaining scenarios** from the original `usage.baml`/`implement.baml` designs, adapted to the desugared surface.
- [ ] Multiple tests per scenario for distinct behaviors/settings (happy path, negotiation failure, client swap, per-provider variants, stream vs oneshot).
- [ ] Triage example-squatting capabilities → `ns_ai/capabilities/` (with scenario URIs) vs per-scenario `implementation.baml`; add scenario URIs to existing capability files (map from REALIZED.md).
- [ ] `common/fakes.baml` (EchoProvider-style offline providers); all scenario code on the desugared surface (plain `Foo(…)` first — tools via `ToolLoop` client).
- [ ] Exit: `ns_ai_examples/` gone; **a directory per original scenario** with green tests (or an explicit P8-blocked marker); baml_src suite green; snapshots regenned.

### Phase F — integ testset + Rust→BAML migration (DCP §1.6)
- [ ] Offline-tier tests per scenario (fakes); `testset "integ-test"` live tests (OpenAI + Anthropic) calling the main functions.
- [ ] `baml_test()` runner gains `-x "integ-test::"`; new env-gated `baml_integ_test()` with `-i "integ-test::"`.
- [ ] Migrate `ai_*.rs`: network-free `baml_test!` tests → BAML test blocks; `*_live_*` → integ testset; delete migrated Rust tests. Keep only wiremock/request-capture + compiler-phase tests in Rust.
- [ ] Update `E2E_TESTS.md` + `REALIZED.md`.
- [ ] Exit: default `cargo test -p baml_tests` touches no network; keyed run green against both APIs.

## Backlog — carried-over gaps (not blocking the DCP phases; schedule after C unless noted)

From the master plan and the review-pass "known gaps" (sources noted):
- [ ] **D4 aggregate provenance** — combinators forward an aggregate `ResponseMeta` (sum members across fallback/retry/loops); a `Traced`/`Budget` combinator projecting usage over a chain. *(plan D4; checklist-item was never built)*
- [ ] **D5 sum outcomes beyond tools** — widen frozen `-> T` capability returns to honest sums (`T | Partial<T>`, `T | Handoff`) in the stdlib interfaces; `ToolLoop.run_to_budget` (Phase A) covers the tools case first. *(plan D5)*
- [ ] **D6 inbound args integrity** — canonical dispatch-side coercion: SAP `ToolCall.args` against the handler's declared type as the *standard* path (today it lives in `Tool.from_type` dispatchers), plus optional dynamic-`type` validation (`baml.sap.parse_type(t, raw)`) against the *stored* `Tool.parameters`. *(plan D6)*
- [ ] **Retry backoff** — legacy had exponential backoff; `baml.ai.Retry` has none. *(deviations "known gaps")*
- [ ] **Retry/Fallback re-drive on projection throw** — `call_with` retries/fails-over when only the projection (`E2`) threw, re-issuing a billed call; separate the channels. *(deviations "known gaps")*
- [ ] **Gemini streaming** — host SSE accumulator (`sys_llm::stream_accumulator`) rejects `google-ai`; add the shape or a BAML-side accumulator, then `Gemini implements Streaming`. *(gemini.baml:10 comment)*
- [ ] **OpenAI-compatible generic provider** — same class, different `base_url`, typed `Auth` field (proxies/local runtimes; also the cheapest second data point for provider diversity). *(old checklist §3, never built)*
- [ ] **P8 host surface** — harness subprocess transport (`ClaudeCode`/`PiAgent` bodies currently `throw Unsupported`), durable/local stores for sessions/memory/compaction/durable-workflows (scenarios 18/19/21/43–47 are shape-only). *(plan P8/Phases 4–5)*
- [ ] **Legacy retirement completion** — port azure/bedrock/vertex/ollama natively (needs `baml.cloud.sigv4_sign`/OAuth host fns, plan P8) and delete the bridge + `PrimitiveClient` pipeline; until then the bridge is load-bearing. *(plan Part IV; explicitly out of DCP scope)*
- [ ] **D3 static capability syntax** — decide the intersection-type surface (`Provider & Streaming`) even if implementation stays deferred, so companions stay forward-compatible. *(plan D3 "decide syntax early")*
- [ ] **P1 generic type aliases** (`type ExtendUnknownError<E> = E | UnknownError`) — pure ergonomics; unions stay inline until a compiler owner picks it up. *(plan P1)*

## Scenario coverage tracker (Phase E/F unit of work — one line per original scenario)

Check a scenario when its `ns_ai_scenarios/NN_*/` exists with green tests on the desugared surface
(offline tier at minimum; integ-test tier where the scenario is API-facing). Mark P8-blocked ones
with the blocker instead of faking a run.

- Single-turn & output: [ ] 01 [ ] 02 [ ] 03 [ ] 04 [ ] 05 [ ] 06 [ ] 07 [ ] 08
- Tools & agents: [ ] 09 [ ] 10 [ ] 11 [ ] 12 [ ] 13 [ ] 14 [ ] 15 [ ] 16
- State & memory: [ ] 17 [ ] 18(P8-store) [ ] 19(P8-store) [ ] 20 [ ] 21(P8-store)
- Realtime & voice: [ ] 22 [ ] 23 [ ] 24 [ ] 25 [ ] 26
- Cross-cutting: [ ] 27 [ ] 28 [ ] 29 [ ] 30 [ ] 31 [ ] 32 [ ] 33 [ ] 34 [ ] 35 [ ] 36
- Harnesses (P8-subprocess): [ ] 37 [ ] 38 [ ] 39 [ ] 40 [ ] 41 [ ] 42
- Workflows: [ ] 43 [ ] 44(P8-store) [ ] 45(P8-store) [ ] 46 [ ] 47

## Working rules (summary — full version in [`README.md`](./README.md))

TDD; **BAML-native tests preferred** (Rust only for wiremock/compiler-phase/runners); strict-check
stdlib via `baml-cli run --file`; run affected suites + regen snapshots before commit; commit per ✅;
log divergences in `deviations.md`, findings in `baml_gotchas.md`; keep `E2E_TESTS.md`/`REALIZED.md`
current as the verified surface grows.
