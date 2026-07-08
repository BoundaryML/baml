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
- [x] Stdlib capabilities marked (15 interfaces; markers sit *above* `///` blocks or they detach docstrings) + the five drivers written next to their capabilities with degrade chains. Refinements (see deviations.md): E0151 generics rule is **name-based** (`T` required, `TPartial` optional, others passthrough — the arity cap would reject `drive_with<T,V,E2>`); `drive_stream` has **no call→stream buffer degrade** yet (needs a stream constructor, net-new host surface) — typed `Unsupported` instead. Strict-checked; std snapshots regenerated.
- [x] `ToolLoop` shipped (scenario-10 `Bounded` + on-board `tools`/`dispatch`, `implements HttpProvider`, `is_effectful()=true` → `Retry` refuses with typed `CannotRetry`) + `step_count_is`/`has_tool_call`/`any_of` + `run_to_budget<T> -> T | Budget<T>` + `LoopBudgetExceeded` (full Failure axis). `per_turn_tools` deferred (needs a `set_tools` seam on `Tools` — D7 gap). 6 offline BAML tests in `baml_src/ns_ai_scenarios/10_agentic_loop/usage.baml` (seeds the Phase E layout).
- [x] **Bonus compiler fix (was a blocker):** interface matches compiled in dependency packages were closed-world — `package_lowering_data` built `interface_implementors` from the package + its deps only, so a stdlib driver's `match (client) { let tp: Tools => … }` could never see a user-authored provider (fell to `Unsupported`; the same match in user code worked; VM `IsType` has no runtime interface path). Fixed: implementor relations union session-wide (`lower.rs::package_lowering_data`); user-package lowering unchanged. Cost: stdlib MIR re-lowers when user `implements` change.
- [x] Exit met: registry pinned by tests; drivers strict-compile; full lib suite + snapshots green. **Phase A complete.**

### Phase B — `client: baml.ai.Provider` + LegacyClient bridge (DCP §1.1)
- [x] `LegacyClient` bridge, first slice (`ns_ai/core/legacy.baml` + `messages_to_prompt` reverse host fn in sys_ops): `implements Provider/HttpProvider` over a **primitive** legacy `Client` — messages → PromptAst → `specialize_prompt` → legacy `build_request`/`parse`; `LegacyMeta` best-effort. 3 offline wire-shape tests (`ns_ai_bridge/`). Strategy configs throw typed `Unsupported` — they lower to new-model combinators over per-member bridges in the lowering step. **Still open: `Streaming` through the bridge** (lands with the lowering step).
- [x] **Injected param retyped to `baml.ai.Provider`** — and the bridge design simplified: `baml.llm.Client` itself implements `Provider`/`HttpProvider` (out-of-body blocks replaced the `LegacyClient` wrapper class), so declared defaults AND legacy call-site overrides (`client = OtherClient`) pass through unchanged, while native overrides (`client = OpenAi{…}` or a combinator chain) route through `call_llm_function`'s HttpProvider/Streaming arms (neutral render → `prompt_to_messages` → native call/stream). Two e2e wiremock tests. Fixed along the way: parser mis-sniffed `Foo(x, client = …)` as an LLM body (named-arg exclusion, 2 parser tests); `$parse_stream` now calls `make_stream_for` (no member calls on the existential); **MIR fix: out-of-body class implements now register as match implementors**.
- [ ] Shorthand→native-class map for ported providers (declared openai/anthropic/gemini configs construct native classes instead of legacy `Client`; unexpressible configs stay legacy) — *optional now that legacy `Client` is itself a Provider; fold into Phase C.*
- [x] `client Gpt()` call form: a **user function returning `Provider`** as the declared client (DCP §1.4; zero-arg, args are a lowering error). e2e: declared client fn routes natively; a call-site override still wins.
- [x] Formatter: consciously accepted — user LLM functions format fine; only stdlib files with literal `client`-named params are skipped (pre-existing, harmless).
- [x] Exit met: full corpus green (lib 1768 / baml_src 2040 / all ai_* suites); native + combinator + legacy overrides and declared client functions all work e2e; snapshots regenned. **Phase B complete** (shorthand→native-class map deferred into Phase C as an optimization).

### Phase C — the desugar (DCP §1.3; blast radius)
- [x] **C.1** — `Foo$with` / `Foo$run_tools` / `Foo$live` generated per stdlib driver (companions.rs): body = `drive_<suffix><T,…>(client, Foo$render_prompt(args by name…, client = client), extras…)`; `$with` appends `V`/`E2` companion generics; param layout = required users → extras → defaulted users → client (E0005/ordering rules); `render_prompt` renders native providers with a neutral primitive (prompt-hook v1). 5 offline scenario tests + `common/fakes.baml` (EchoProvider/NoCapProvider).
- [x] **C.2** — native routing generalized + orchestrator delegation **deleted**: `baml.ai.native_provider_for(primitive)` maps ported configs (openai/anthropic/google-ai) onto native classes; `call_llm_function`/`stream_llm_function` route ported primitives natively (stream falls through to legacy when the native class lacks `Streaming` — the capability match is the fallback), strategy configs keep legacy loops. New e2e: declared `provider anthropic` hits the native wire. `_openai_delegation_ok`/`_openai_from` gone.
- [x] (folded into C.2 above.)
- [ ] **C.3 (optional cleanup)** — literal AST desugar of the main body (`Foo` → `drive_call(client, Foo$render_prompt(…))`) + `Foo$stream` onto `drive_stream`; requires strategy-config→combinator lowering so `call_llm_function` can retire fully. The semantic goals (negotiation via capability surface, delegation deleted, native routing) are already met; this is the cosmetic completion.
- [ ] Back-compat gate: entire legacy corpus + `ai_*` suites + wire-fidelity request-capture tests pass byte-identically where asserted.
- [ ] Exit: `Foo`/`Foo$stream`/`Foo$with`/`Foo$run_tools` work with declared, swapped, combinator, and bridge clients; anthropic/gemini LLM functions route natively.

### Phase D — user-defined capabilities (DCP §1.4)
- [x] Companion generation from user-package markers: PPIR (`make_user_drive_companion`) reads each non-builtin registry driver's item-tree signature — extras = params after (client, prompt); name-based generics (`T`→return, `TPartial`→stream-expanded, rest passthrough); return type via TypeExpr substitution; body reuses the shared `make_drive_companion` builder. Driver call paths are `root.`-absolute (a bare `[ns…, name]` from inside `ns` compiles clean but mis-resolves and HANGS at runtime — see gotchas). `capability_registry` is now a Salsa-tracked per-project query (per-file consumers made the plain walk O(files²)).
- [x] The Moderated e2e fixture as **BAML test blocks** (`ns_ai_custom_capability/usage.baml`): capability + driver + provider all in user code; generated `ComposeNote$moderated` routes through the user provider; declared client without the capability → typed `Unsupported`; the driver directly callable (the no-sugar path). *(Still open: diagnostic snapshots for E0150–E0152 in the LSP snapshot suite; user-fn-declared-client already covered in Phase B tests.)*
- [ ] Two-marker recipe documented in an `ns_ai` README-comment.

### Phase E — scenario build-out (DCP §1.5) — **the goal: the full original corpus as scenarios**
Every `../llm-provider/ideas/scenarios/NN-*` gets a `ns_ai_scenarios/NN_name/` with runnable tests
(~40 of 47 can run end-to-end today; the P8-blocked tail gets offline/negotiation tests + a
`// BLOCKED: P8 <what>` marker and graduates later). This is a **build-out**, not just a move of
the 7 existing example files.
- [ ] Verify nested-dir namespace behavior of `baml_src/`; scaffold `ns_ai_scenarios/NN_name/{usage,implementation}.baml` (numbering + scenario-URI header comments mirroring the original corpus).
- [x] All 7 `ns_ai_examples/` files migrated + culled; **`ns_ai_examples/` deleted**. Remaining: fill out the untouched scenarios from the original designs.
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

- Single-turn & output: [x] 01 [x] 02 [x] 03 [x] 04 (negotiation offline; SSE = Rust tier) [ ] 05 (live-only: e2e_multimodal_live) [ ] 06 (needs media-output provider) [x] 07 [x] 08
- Tools & agents: [ ] 09 [x] 10 (ToolLoop + $run_tools, offline) [x] 11 (parallel dispatch) [x] 12 (taxonomy) [x] 13 (catalog paging) [x] 14 (handoff) [x] 15 (tripwires) [x] 16 (allowlist gate + ToolLoop compose)
- State & memory: [x] 17 (session threading, offline) [ ] 18(P8-store) [ ] 19(P8-store) [ ] 20 [ ] 21(P8-store)
- Realtime & voice: [x] 22 (offline `$live` + fake; live tier = ai_realtime.rs) [x] 23 (negotiation) [ ] 24 [x] 25 (cascaded pipeline, offline) [ ] 26
- Cross-cutting: [x] 27 (submit/poll + effect marker, offline; live = ai_responses) [ ] 28 [ ] 29 [ ] 30 [x] 31 (defer lifecycle) [x] 32 ($with value+meta, offline) [x] 33 (judge scoring; live = eval_judge_live) [ ] 34 [x] 35 (config variance) [x] 36 (Support lattice)
- Harnesses (P8-subprocess): [x] 37 (config+negotiation; BLOCKED:P8 for live) [ ] 38 [ ] 39 [ ] 40 [ ] 41 [x] 42 (drive_any negotiation)
- Workflows: [x] 43 (spawn/await graph, offline; live = workflow_graph_live) [x] 44 (suspend as sum arm, offline; P8-store for durability) [x] 45 (durable-step shape; P8-store for the log) [x] 46 (step events) [ ] 47

## Working rules (summary — full version in [`README.md`](./README.md))

TDD; **BAML-native tests preferred** (Rust only for wiremock/compiler-phase/runners); strict-check
stdlib via `baml-cli run --file`; run affected suites + regen snapshots before commit; commit per ✅;
log divergences in `deviations.md`, findings in `baml_gotchas.md`; keep `E2E_TESTS.md`/`REALIZED.md`
current as the verified surface grows.
