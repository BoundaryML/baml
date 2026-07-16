# Scenario set — overview

This folder stress-tests one proposed redesign of BAML's LLM interface model against the full landscape of what people actually do with LLMs. The proposal (see [`../01-providers-clients-capabilities.md`](../01-providers-clients-capabilities.md)) has one spine: a base **`Provider`** with a single method `call<T>(prompt) -> T`; **capabilities** are interfaces that `requires Provider` (`HttpProvider`, `Streaming`, `Realtime`, `Tools`, …); a **`client` is sugar** for a function returning a `Provider`; **combinators** (retry/fallback/cache) are default methods that forward by runtime `match`; and capability negotiation is a runtime `match` over the existential `Provider`. Failures follow the **per-capability error model** ([`../error-model.md`](../error-model.md)): every fallible method declares `baml.ExtendUnknownError<CapErr>` (one `baml.errors.*Error` interface per capability, e.g. `CallError`/`StreamError`/`ToolError`/`RealtimeError`), deliberate domain failures are concrete classes that `implement` it, and foreign errors normalize into the universal `baml.UnknownError` — replacing the old single `baml.errors.LlmClient` channel. Each of the 47 scenarios below takes one real-world feature, writes `implement.baml` + `usage.baml` against this model, and records in `evaluation.md` whether the model holds, what net-new surface it forces, and where it leaks. The proposal ends with **7 Open Questions**; the scenarios are graded against them, and the consolidated answer lives in [`_gap-analysis.md`](_gap-analysis.md).

## Verdict legend

- **Clean** — the spine absorbs the feature with only library/host additions; no patches to the core model.
- **Workable-with-additions** — expressible, but requires net-new capability interfaces, types, or host primitives, and usually leaves at least one gap the type system cannot close.
- **Awkward** / **Unsupported** — (none in this set; see the verdict distribution in the gap analysis).

## How to read a scenario folder

Each `scenarios/<slug>/` contains four files, meant to be read in this order:

1. **`README.md`** — the real-world feature: what it is, which SDKs/providers do it, and why it is hard.
2. **`implement.baml`** — how a *library/provider author* would build it on the proposed model (the `class`/`interface`/`implements` side, including any net-new surface invented for the scenario).
3. **`usage.baml`** — how an *app author* would call it (the `client`/`function` side — the ergonomics test).
4. **`evaluation.md`** — the verdict, the net-new surface enumerated, and the gaps, each tagged to the relevant Open Question.

`_conventions.md` documents the shared vocabulary (`baml.*` host seams, naming) the scenarios assume.

## All 47 scenarios

### Single-turn & output shape

| # | Title | Verdict | One-line |
|---|---|---|---|
| [01](01-single-turn-text/) | Single-turn text | Workable-with-additions | One codec absorbs the 3 system-prompt placements + role split; usage rides `call_with` + a `ResponseMeta` projection (not a bespoke `Metered`), refusal stays a `Refused` error on `throws`, and a readable prompt view is still net-new. |
| [02](02-structured-output/) | Structured output (4 wire encodings) | Workable-with-additions | One `T` threaded through `build_request`+`parse` gives one function body across 4 encodings; needs schema-lowering + repair-parse host seams and leaks on per-attempt strict retry. |
| [03](03-constrained-decoding/) | Constrained decoding (regex/CFG/choice) | Workable-with-additions | A `Constrained` capability with no default lets self-hosted backends guarantee token-level decoding; the guarantee is a runtime promise the type system can't lift. |
| [04](04-streaming/) | Streaming tokens + partial structured | Workable-with-additions | One `Streaming` interface spans 3 SSE dialects via `stream_unfold` folds; partial structured falls out as client-side `parse_partial`, but the `Body` thesis breaks and TStream/TFinal disagree. |
| [05](05-multimodal-input/) | Multimodal input | Workable-with-additions | A `MediaIngest` capability negotiates forward/pre-fetch/pre-upload inside `build_request`; per-media gaps (audio≠Anthropic) are runtime-only throws. |
| [06](06-non-text-output/) | Non-text output (image/speech/STT) | Workable-with-additions | The `Body` codec absorbs bytes/audio/text and usage/revised_prompt ride `call_with` + a `ResponseMeta` projection; dedicated-vs-in-conversation is still exposed as two T types (`Media` vs `MixedReply`) that no single function spans. |
| [07](07-reasoning/) | Reasoning models | Workable-with-additions | Reasoning *text* rides `call_with` + a `ResponseMeta.reasoning()` projection (not a bespoke `WithReasoning<T>`); cross-turn continuity (threading reasoning back) stays a separate stateful capability with an opaque `ContinuationState` that breaks under `Fallback`. |
| [08](08-enriched-outputs/) | Logprobs / citations / grounding | Workable-with-additions | Each enrichment is a `ResponseMeta` dimension projected via `call_with` (logprobs/citations/grounding, the last an external-impl `Grounded`) rather than a bespoke `*$Result<T>`; still no shared citation type, no honest fallback, runtime-only. |

### Tools & agents

| # | Title | Verdict | One-line |
|---|---|---|---|
| [09](09-tool-calling/) | Tool calling basics | Workable-with-additions | The `Tools` seam (opaque `Transcript` + `begin`/`step`/`submit`) hides id-vs-name and ordering divergence below one interface; combinators erase `Transcript` to `unknown`. |
| [10](10-agentic-loop/) | Agentic loop + stop conditions | Workable-with-additions | `run_tools` is a free default; a `Bounded` combinator adds stop policy + partial-at-budget, but per-turn tool filtering pokes the opaque `Transcript` and "what is a step" is unchecked. |
| [11](11-parallel-tools/) | Parallel tool calls | Workable-with-additions | Parallelism slots below `ctx.dispatch` as a pluggable `Dispatcher`; needs a `Tool.effect` annotation, and the fan-out is shipped structured concurrency (`spawn` each + `baml.future.all`, `baml.spawn.TaskGroup` to cap) — not a missing host surface. |
| [12](12-tool-taxonomy/) | Hosted / computer-use / MCP | Workable-with-additions | A `ToolKind` union + `MixedTools` capability absorbs execution-location variants; catalog-gating leaks and server-driven runs are spectator-only. |
| [13](13-searchable-tools/) | Deferred & searchable tools | Workable-with-additions | A `SearchableTools` capability + `Catalog`/`SearchResolver` reuse the loop seam; prompt-cache cost divergence (native vs emulated defer) is invisible. |
| [14](14-multi-agent/) | Handoffs / sub-agents / orchestration | Workable-with-additions | An agent is a `Provider`, so deterministic orchestration is plain combinators; handoff smuggles a provider-swap through `submit` and history-threading fights PromptAst opacity. |
| [15](15-guardrails/) | Input/output tripwires | Workable-with-additions | A `Guarded` combinator wraps any provider; input guards race the call via `baml.future.race` + a `CancelToken` (shipped BEP-034), but output-trip-mid-loop can only throw-and-restart. |
| [16](16-agent-security/) | Lethal-trifecta threat model | Workable-with-additions | Quarantine falls out of capability subtraction and the allowlist/taint gate slots into `dispatch`, but there is no compile-time information-flow guarantee. |

### State, history & memory

| # | Title | Verdict | One-line |
|---|---|---|---|
| [17](17-history-and-sessions/) | Within-run history + sessions | Workable-with-additions | A `Conversational` capability + `Session` + pluggable store hold the transcript; the stateless-client rule means a conversation is forever two paired values. |
| [18](18-compaction/) | Shrinking the live context | Workable-with-additions | A `Compaction` capability (opaque `Window`) + an `AutoCompact` stateful combinator; the two-mode split (replace vs prune) can't share a signature and cache-bust is advisory-only. |
| [19](19-fork-branch/) | Fork / branch a conversation | Workable-with-additions | A `Session` base + `Branching` capability fork portably across copy/pointer/emergent backends; "forks anywhere" and "the branch is mine" stay un-typeable runtime promises. |
| [20](20-server-stored-chains/) | Server-stored chains + 3 ownership models | Workable-with-additions | A `Chain` capability + owner-tagged `ChainHandle` make 3 ownership models legible; reasoning-continuity is a runtime boolean, non-migratability is a string compare. |
| [21](21-memory/) | Cross-conversation long-term memory | Workable-with-additions | A `MemoryStore` over a `VectorStore` consumes providers rather than being one; middleware leaks through opaque PromptAst and Letta's server-authoritative agent is opaque-proxy-only. |

### Realtime & voice

| # | Title | Verdict | One-line |
|---|---|---|---|
| [22](22-realtime-voice/) | Realtime / voice session | Workable-with-additions | A config-only client + a pass-in `Channel` to `Realtime.run`; needs a duplex socket opener + a `spawn` concurrency primitive, and server-authoritative non-retryable state is inexpressible. |
| [23](23-barge-in/) | Barge-in / interruption / mutation | Workable-with-additions | A `LiveControl requires Realtime` capability with `cancel`/`truncate`; truncate's correctness depends on app-side played-ms the model can't own, and degrades to a no-op on Gemini. |
| [24](24-realtime-tools/) | Tools in a realtime session | Workable-with-additions | A `RealtimeTools requires Realtime` capability runs an event-driven loop over the `Channel`; three pass-in params weaken the ergonomic and fallback can only pick at session open. |
| [25](25-voice-pipelines/) | Cascaded voice + unified STS | Workable-with-additions | STT/LLM/TTS become three single-capability providers wired by a `CascadedVoice` combinator; OQ1 bites hard (`call` degrades to just-the-LLM). |
| [26](26-transports/) | Transport taxonomy | Workable-with-additions | Transport (which host primitive) and capability (which interface) become orthogonal axes; needs a real duplex socket and a WebRTC media/data split, and forcing `call` onto Realtime is dishonest. |

### Cross-cutting concerns

| # | Title | Verdict | One-line |
|---|---|---|---|
| [27](27-background-jobs/) | Async background + poll | Workable-with-additions | A `Background` capability + persistable `Job<T>` handle absorbs the 4th lifecycle; needs a first-class `sleep`, and inherited combinators silently double-submit billed jobs. |
| [28](28-provider-diversity/) | Provider diversity & gateways | **Clean** | A proxy is the same class with a different `base_url`; auth is a typed `Auth` field; prefix-routing is an ordinary function returning a `Provider`. No spine patches. |
| [29](29-reliability/) | Retry / fallback / load-balance | Workable-with-additions | Reliability lands on the combinator layer + a `FailureKind`/`LlmError` taxonomy; load-balancing needs cross-call mutable state the stateless client can't hold. |
| [30](30-cascades-routing/) | Cascades & semantic routing | Workable-with-additions | Routing is `client`-as-a-function; cascades are `Fallback`-shaped combinators; a per-backend `ConfidenceProvider` capability lets a logprob-less cascade silently never escalate. |
| [31](31-caching/) | Caching (3 shapes) | Workable-with-additions | Response-cache + inline cache-control + prompt-cache-key are free; Gemini's `cachedContents` resource forces a `ManagedCache` capability and still can't model billed-while-idle state. |
| [32](32-observability/) | Spans / cost / wire-vs-UI | Workable-with-additions | A `Traced` combinator wraps any provider; usage rides `call_with` + `ResponseMeta` (forwarded through combinators instead of a dropped side-channel, though the Fallback aggregate still sees only the winner's usage), and the wire-vs-UI split has no home in the model. |
| [33](33-evaluation/) | Dataset / scorer / runner | Workable-with-additions | The task is already a function→Provider and judges are just providers; needs a `Scorer` interface + a `Deterministic` capability, but can't give compile-time reproducibility. |
| [34](34-cost-and-batch/) | Cost / tokens + batch | Workable-with-additions | Metering rides `call_with` + a `ResponseMeta.usage()` projection (not a bespoke `Metered<T>`); a `Budget` combinator sums usage via a host cell; batch is a submit→poll→download handle lifecycle. Base contract + sugar still leak under the weight. |
| [35](35-deployment-shapes/) | Server / browser / edge / durable | Workable-with-additions | Four homes collapse to four small functions over one class; a `Credential` capability + `host_can_hold` probe absorb auth/transport divergence, but no build-time secret hygiene. |
| [36](36-capability-negotiation/) | Capability negotiation | Workable-with-additions | The binary gate is native (`match`+`catch`); the non-binary gradient (shallow-schema, image-not-at-resolution) forces a whole declarative `Capabilities`+`Support` lattice. |

### Harnesses (coding agents / Claude Code)

| # | Title | Verdict | One-line |
|---|---|---|---|
| [37](37-harness-basics/) | What a harness is + driving it | Workable-with-additions | A harness implements base `Provider` over a subprocess + `Realtime`; the control plane is a closed `OutEvent`/`InEvent` union over the `Channel`, not a method per verb. |
| [38](38-permissions-sandbox/) | Permissions & sandboxing | Workable-with-additions | Permission config dissolves into bound fields; the approval gate slots onto `dispatch`; but `require_approval` can't live on the frozen `Tool` and the sandbox fence is unprovably wired. |
| [39](39-harness-extensibility/) | Tools / skills / sub-agents / hooks / MCP / A2A | Workable-with-additions | In-process + MCP tools collapse to one `Tool[]` field; sub-agents/A2A are named providers; `begin(tools)` fights field-configured tools and hook-guarantees are runtime-only. |
| [40](40-harness-sessions/) | Built-in tools + on-disk sessions | Workable-with-additions | A `SessionCatalog` capability + on-disk `SessionStore` proven across JSONL and pointer-tree; can't guarantee resume-ability, prevent the cwd footgun, or model out-of-process state. |
| [41](41-harness-deployment/) | Embedding & deployment | Workable-with-additions | A `Drivable` capability hides JSONL/RPC/generator transports; `Durable`/`Trigger`/`Registry` carry instance identity + a runtime registry that contradicts options-as-fields. |
| [42](42-harness-abstraction/) | Wrapping harnesses behind one abstraction | Workable-with-additions | A `HarnessAgent` combinator over per-runtime adapters; durable sessions need a `Harness` capability + opaque `SessionHandle`, and lossy features ride an untyped `raw()` escape hatch. |

### Workflows & durable execution

| # | Title | Verdict | One-line |
|---|---|---|---|
| [43](43-workflow-graph/) | Composing a durable step graph | Workable-with-additions | A step is a typed function and a model call is `provider.call<T>`; fan-in/join are shipped structured concurrency (`spawn` each + `baml.future.all`), so the only net-new layer is durable resume — an app `Checkpoint`/replay layer outside the Provider hierarchy. |
| [44](44-workflow-suspend-resume/) | Suspend / resume & HITL | Workable-with-additions | A `Suspendable` capability with `start`/`reenter -> T \| Suspend` mirrors `Iterator.next`; state crosses as an opaque `Snapshot`; schema-drift is detected but not migratable. |
| [45](45-workflow-durable/) | Durable execution | Workable-with-additions | `Durable` is a Cache-shaped combinator keyed by `StepCoord` that records-then-replays so the model is never re-sampled; record/replay sits at the wrong (post-parse) layer. |
| [46](46-workflow-observability/) | Streaming & observability of a workflow | Workable-with-additions | A `Steppable` capability returns `Stream<StepEvent, T>`; the inter-step carry is `unknown` (no type safety) and the token interleave is buffered, not live. |
| [47](47-workflow-agent-nesting/) | Agents-in-workflows, workflows-in-agents | Workable-with-additions | Workflow-as-tool is free (workflow→function→Provider→Tool); agent-as-step collapses a loop into one `Durable`-wrapped `call`, with the replayed-echo bit surfaced as a `ResponseMeta` dimension via `call_with`; determinism is host-convention, not typed. |

## Verdict distribution

- **Clean:** 1 ([28](28-provider-diversity/))
- **Workable-with-additions:** 46
- **Awkward / Unsupported:** 0

The single Clean result is the one scenario that is purely about *configuration variance* (endpoints, auth shapes, routing) — exactly the axis the "options are fields, a client is a function" thesis was designed for. Every other scenario needs something added; what those additions are, and which recur, is the subject of [`_gap-analysis.md`](_gap-analysis.md). The error model applies cleanly within each capability and adds **no** sixth structural pressure. Its one apparent gap — a typed error boxing into `baml.UnknownError` when it crosses into a *different* capability's channel — is closed by BAML's Rust-like `implements`: errors implement all the common interfaces (`CallError`/`StreamError`/`ToolError`/`RealtimeError`), so core-channel crossings stay typed, and because `implements` admits *external* impls in both directions, a channel author can write `implements TheirError for <upstream error>` so even a net-new channel (`StepError`, `HarnessError`) admits upstream errors typed. What remains is a coverage convention (and the explicit-`from<T>` recovery / `UnknownError` escape hatch for genuinely-foreign values), not a limit on the model's shape.
