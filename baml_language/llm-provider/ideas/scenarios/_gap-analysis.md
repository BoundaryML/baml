# Gap analysis — what the proposal must add to cover the landscape

This consolidates the net-new surface and the unresolved gaps across all 47 scenarios into a deduplicated, prioritized list. The model's spine (`Provider.call<T>`, capabilities that `requires Provider`, `client` = function, combinators by runtime `match`) survives everywhere — but it survives by *accreting* surface, and a handful of structural pressures recur in nearly every scenario. This document names them once, says which scenarios demand them, and tags each to the proposal's 7 Open Questions.

The Open Questions, for reference:

> **OQ1** every capability `requires Provider`? · **OQ2** how does live `io` surface at the call? · **OQ3** request side is HTTP-specific · **OQ4** `parse` input is the full `Body` · **OQ5** capability `match` over an interface-existential · **OQ6** SAP as a public primitive · **OQ7** host-backed `implements` blocks

---

## Part A — The five structural pressures (the headline findings)

These are not features to add; they are recurring places where the *shape* of the model is wrong or insufficient. Every one shows up in 8+ scenarios. Fixing these is worth more than any individual primitive below.

### P1. `call<T> -> T` is too narrow to be the irreducible truth — the value+sidecar problem

The base method returns exactly `T` and nothing else. But an enormous fraction of real responses are *value + out-of-band metadata*: token usage, a refusal reason, logprobs, citations, grounding, a reasoning block, a continuation handle, a revised prompt, a "this was a replayed echo" bit. None of these fit `-> T`, so each scenario invents a parallel companion (`call_metered`, `.think`, `.logprobs`) returning a `Wrapper<T>` (`Metered<T>`, `Scored<T>`, `WithReasoning<T>`, `ChainTurn<T>`, `Suspend`), duplicating send/parse plumbing and — critically — **silently dropping the sidecar when routed through the inherited `.call`** (a Fallback that forwards `Metered` degrades to zero-usage; a `Suspendable` under Fallback turns a legitimate pause into a thrown error).

- **Demands it:** 01 (usage, refusal), 06 (usage, revised_prompt), 07 (reasoning), 08 (logprobs/citations/grounding), 20 (chain handle), 27/34 (job/batch handle), 32 (usage for tracing), 36 (warnings), 44 (suspend), 47 (replayed-cost bit).
- **Bears on:** OQ1. The honest fix is to widen the base contract (e.g. `call<T> -> Result<T>` where `Result` carries an extensible metadata bag) rather than bolt N parallel companions beside it. The proposal should decide this before the capability zoo calcifies.

### P2. Capability is a runtime promise — no compile-time guarantee, by design, and it hurts

Because `client` returns the existential `Provider`, "does this client stream / run tools / preserve reasoning / decode under a grammar / persist a session / batch / resume" is answered by a runtime `match`, never the type checker. The escape hatch (drop the sugar, return a concrete type) recovers the guarantee but forfeits combinators and dynamic selection — the exact ergonomics the model sells. This is acceptable for *degradable* capabilities (Streaming) but actively dangerous for *binary* ones where the whole point is a guarantee: a `.decode` on a hosted client compiles and throws; `Haiku().escalate_to(Opus(), 0.72)` compiles and *never escalates*; a cron wired to a non-batch client fails at runtime; a deployment platform can't reject a non-durable webhook agent at deploy time.

- **Demands it (a guarantee it cannot give):** 03 (constrained), 08 (enriched), 09 (tools), 22–26 (realtime), 27/34 (background/batch), 30 (confidence), 33 (reproducibility), 35 (transport/credential viability), 36 (the headline), 40 (resume), 41 (durability), 43/44 (workflow).
- **Bears on:** OQ5, OQ1. The recurring ask is a *middle ground* between "existential everywhere" and "concrete return kills portability" — e.g. a way to write `function F() -> Provider & Streaming` (an existential refined by a required capability) so the client stays swappable but the capability is statically guaranteed.

### P3. OQ5 (runtime interface-membership `match`) is load-bearing and unconfirmed — and sometimes needs *more* than membership

Almost every combinator and every capability companion routes through `match (p) { let s: Streaming => … }` over a concrete provider hidden behind a combinator wrapper. If value-level reflection can't test membership through a `Fallback`/`Quarantined`/`Traced` wrapper, **the entire combinator layer collapses** — this is assumed, never proven. Worse, several scenarios need *generic-method dispatch* through the existential, not just a boolean membership test: 36 needs `cap.structured_output<T>()` re-reflecting `T`; 37 needs to match a data *value* against a set of reified `type` values (value-level reflection + type equality). If OQ5 only guarantees membership, those are unsound.

- **Demands it:** essentially all of 03–47; load-bearing-and-called-out in 09, 11, 15, 16, 19, 29, 31, 33, 36, 37, 43, 47.
- **Bears on:** OQ5. This must be confirmed first; it is a precondition for the model existing at all.

### P4. Server-authoritative / durable / mutable cross-call state has no home

The model is "client drives provider," providers are pure-config immutable values, and combinators are stateless. This breaks for: server-owned conversation chains (`previous_response_id`), server-side cached resources billed while idle (Gemini `cachedContents`), load-balancer cursor + circuit-breaker health, budget cells, background/batch job state, durable workflow step logs, durable harness instances with single-writer leases, realtime sessions that already played audio and mutated server state. The recurring symptom: a `$rust_type` host handle is smuggled in to hold the mutable state, "quietly violating 'providers are pure-config values'," and **combinators silently do the wrong thing** (Fallback can't see `previous_response_id`; Retry double-submits a billed job; a forked session can't fail over mid-conversation).

- **Demands it:** 17, 18, 19, 20, 21 (Letta), 22/23 (realtime), 27, 29 (round-robin), 31 (managed cache), 34 (budget/batch), 41 (durable instance), 42, 43–47 (durable workflow).
- **Bears on:** OQ1, OQ3, OQ7. The model needs a first-class notion of *stateful provider / session object* (and *stateful combinator*) distinct from the stateless `call<T>(prompt)` value, plus a way to declare "this state is server-authoritative and non-retryable" so combinators refuse to silently re-drive it.

### P5. The opaque `PromptAst` (and opaque `Transcript`) is too opaque

The proposal keeps `PromptAst` as `{ _data: $rust_type }` that only ever passes through to `build_request`. But the *very first* scenario needs to read it (re-home the system turn into 3 placements), and the need recurs constantly: multimodal must read media parts, memory must read the last user turn and inject facts, compaction/few-shot must rewrite history, guardrails must extract the user span, multi-agent must concat/truncate. The same over-opacity hits the provider-owned `Transcript`: combinators erase it to `unknown`, per-turn tool filtering pokes it from outside, handoff smuggles control signals through `submit`. The model withheld a structured read-view that nearly everything needs.

- **Demands it:** 01, 05, 06, 14, 15, 17, 18, 21, 30 (judge render), 39.
- **Bears on:** OQ3 raised one level up — the missing thing is *semantic prompt structure* (a readable `PromptTurn[]` / media-part view), not transport. The proposal should ship a normative read-interface over `PromptAst` (roles, text, media parts) and a small prompt-algebra (concat/truncate/inject).

---

## Part B — New capability interfaces (deduplicated)

Each is a fresh `requires Provider` interface the model needs. Grouped by clustering; many are individually small but they reveal that the "base + a handful of capabilities" picture understates the real count (~25 distinct capabilities across the set).

| Capability interface | Scenarios | Notes / OQ |
|---|---|---|
| `Metered` / `Metering` (+ `Usage`, `Metered<T>`) | 01, 06, 32, 34 | The single most-demanded addition. Usage can't be a base field (most providers omit it) nor fit `call<T>`. Errors-on-fallback (can't fabricate). **OQ1.** |
| `Constrained` (regex/CFG/choice, no default) | 03 | First capability with *no* `call`-based default — a guarantee can't be synthesized. **OQ5.** |
| `MediaIngest` (accepts/upload/resolve) | 05 | Per-(kind,transport) acceptance + negotiation inside `build_request`. **OQ5.** |
| `Thinks` + `Continuity` (+ `WithReasoning<T>`, `ContinuationState`) | 07 | Out-of-band reasoning companion; continuity breaks under Fallback. **OQ1, OQ5.** |
| `Scored` / `Citable` / `Grounded` / `Annotated` | 08 | Four separate enrichment capabilities, no shared supertype, no honest fallback. **OQ1, OQ5.** |
| `Tools` (canonical: `Transcript`, `begin`/`step`/`submit`) | 09, 10, 11, 14 | The spine's own §7 capability. Combinators erase `Transcript` to `unknown`; no mid-loop failover. **OQ5.** |
| `MixedTools` / `SearchableTools` (refine `Tools`) | 12, 13 | Hosted/computer-use/MCP variants; deferred catalogs. Two-level `requires`. **OQ3, OQ5.** |
| `Guardrail` | 15 | `inspect<V>` over a value; deterministic guards must fake a throwing `call`. **OQ1.** |
| `Provenance` (requires `Tools`) | 16 | Taint labels below the `ToolCall` seam. **OQ5.** |
| `Conversational` / `Session` / `Branching` / `Chain` | 17, 19, 20 | The stateful-session family — base + fork + server-chain. **P4; OQ1, OQ3, OQ5.** |
| `Compaction` + `ContextEditing` | 18 | Two capabilities for one goal (replace vs prune) that can't share a signature. **OQ1.** |
| `MemoryStore` / `VectorStore` | 21 | Sibling abstractions that *consume* providers — not capabilities of one. **OQ7.** |
| `Realtime` + `LiveControl` + `RealtimeTools` + `Stt`/`Tts` | 22–25, 37 | The realtime/voice stack; two- and three-level `requires` chains; `call` is degenerate. **OQ1, OQ2.** |
| `Background` / `Batch` | 27, 34 | submit/poll/cancel lifecycle + persistable handle; inherited combinators unsafe. **P4; OQ1, OQ5.** |
| `Credential` / `Auth` | 28, 35 | Auth-shape divergence as a typed field-capability. (28 = the Clean one.) |
| `ConfidenceProvider` | 30 | Producing call that also reports a signal; absent signal = silent no-escalate. **OQ5.** |
| `ManagedCache` | 31 | Server-side cache resource with opaque handle. **P4; OQ1, OQ5.** |
| `UsageReporting` / `Traceable` | 32 | Tracing-facing usage + fluent `.with_traced`. **OQ4.** |
| `Deterministic` / `Scorer` / `PairwiseScorer` | 33 | Read-only metadata capability (odd fit for `requires Provider`) + scorer interfaces. **OQ1, OQ5.** |
| `Capabilities` (+ `Support` Yes/No/Maybe lattice) | 36 | The declarative, non-binary support table — pure read-only data forced to carry `call`. **OQ1, OQ5.** |
| `ControlPlane` / `Drivable` / `Durable` / `Harness` / `SessionCatalog` / `Skills` | 37, 39–42 | The harness stack: control-plane verbs, transport framings, durable instances, on-disk sessions. **P4; OQ1, OQ3, OQ7.** |
| `Suspendable` / `StepLog` / `Steppable` / `Workflow` | 43–47 | The workflow stack: pause/resume, durable replay, step-streaming. Several sit *outside* the Provider hierarchy. **OQ1, OQ5, OQ7.** |

---

## Part C — New types, fields, and result shapes

- **`Usage`** (normalized input/output/cache_read/cache_write/reasoning, with `add` for fan-out) — 01, 06, 32, 34. Reasoning-token normalization leaks (OpenAI itemizes, Anthropic folds → `reasoning_tokens: 0`).
- **Value+sidecar carriers:** `Metered<T>`, `Scored<T>`, `WithReasoning<T>`, `ChainTurn<T>`, `*$Result<T>`, `Settled<T>`, `Result<T,E>`, `Budget<T>`, `Suspend`, `Warned<T>` — all symptoms of **P1**.
- **`Refused` / `Refusal` / `SchemaRejected` / `Unsatisfiable` / `Unsupported` / `SessionExpired` / `ForkNotOwned` / `CannotForkHere`** — new variants on the typed `throws` channel; a refusal is HTTP-200-but-no-T and must be inspected *before* SAP. — 01, 02, 03, 19, 36, 42.
- **`Tool { name, description, parameters: type }`** + `Tool.effect` (ReadOnly/Write) + `ToolKind` union + `HarnessTool.source` + a parallel `ApprovalRule[]` (because `Tool` is frozen and can't carry approval/output type) — 09, 11, 12, 38, 39. **Tool *output* is untyped everywhere** (`ToolResult.output: unknown`) — the `type` primitive types inputs but the model offers nothing for outputs.
- **Opaque handles** (`ChainHandle`, `Job<T>`, `BatchHandle<T>`, `CacheHandle`, `SessionHandle`, `Snapshot`, `Cursor`, `ContinuationState`, `Window`) — all `{ _data: $rust_type }` with a stringly-typed `owner`/`provider_id` guard for non-portability. The guard is announce-not-enforce (an app can hand-construct a handle). — 18, 19, 20, 22, 27, 31, 34, 42, 44.
- **Readable prompt view** (`PromptTurn { role, text }`, `MediaPart`, `PromptView`) + a `Block`/`Turn`/`Role` transcript grammar — **P5** — 01, 05, 17.
- **Provider fields newly required:** `base_url`, `auth: Auth`, `headers`, `service_tier`, `cache_breakpoints`/`prompt_cache_key`, `effort`/`verbosity`/`budget`, `sandbox`/`permission_mode`/`allowed_tools`, `compact_threshold`, `store`. Mostly clean (the "options dissolve into fields" thesis holds), with two soundness conflicts: **SigV4 must sign the *final* request** but §4 says headers/auth wrap the provider at the combinator layer — applying auth before vs after the envelope silently invalidates the signature (28); and a `store: false` field makes a provider statically *be* a `Chain`/`Background` that throws at runtime (20, 27, 34).

---

## Part D — New host / `$rust` primitives

- **SAP made public** — `baml.sap.parse<T>` + `repair_parse<T>` + `parse_partial<T>` + `to_string<T>`/`to_json<T>`. Required by nearly every parse body. **OQ6**, blocking. — 01, 02, 04, 17, 31.
- **Schema lowering** — `baml.schema.json_schema(T, strict)`, a *separate* `gemini_schema(T)`, `strict_supports(T)`, plus reflection codecs `type_to_json_schema` / `type_to_gemini_openapi`. The "one schema, N dialects" problem is genuinely N distinct host lowerings, not a flag. — 02, 03, 09.
- **Concurrency** — `baml.async.gather` / `gather_until_error` / `join*` / `map_concurrent`, `Semaphore`, `baml.sys.spawn` / `Task`, `baml.sys.race_cancel`. BAML has *no* concurrency surface today; these must accept and schedule first-class BAML closures across the host boundary. **OQ7**, sharpest unconfirmed dependency. — 11, 15, 22, 43.
- **Streaming machinery** — `baml.llm.stream_unfold` + `StreamStep`, a *second* reader `fetch_json_seq`/`JsonSeq` for chunked-JSON, `parse_partial`, per-frame `baml.json.get_str/get_int`. The `Body` associated type does **not** carry streaming (SseStream-vs-JsonSeq is chosen *inside* the method body) — **OQ4 breaks for Streaming.** — 04, 26, 32, 46.
- **Duplex transport** — `baml.ws.connect`/`WsSocket` (mid-stream `.send`, exactly what `SseStream` lacks), `baml.webrtc.connect` + media/data-plane split, `baml.realtime.open`. Confirms there is **no shared transport abstraction**; Realtime invents its own opener. **OQ3.** — 22, 26, 35.
- **Media** — `baml.media.*` (download, to_data_url/base64, image dims/downscale, image_from_base64, audio_from_bytes), audio I/O (`play_and_measure`, `open_channel`), VAD (`silero_vad`). — 05, 06, 22, 23, 25, 36.
- **Time / durability / state** — `baml.sys.sleep_ms` (first-class blocking wait), `baml.rand.idempotency_key`, atomic `RunStore.claim` (compare-and-set), `StepLog`/`baml.flow` durable logs, `prompt_fingerprint`, host mutable cells (`KVCounter`, `BudgetCell`, `MemoryStore._cell`). All **P4**; all **OQ7**. — 27, 29, 34, 44, 45, 46, 47.
- **Cloud IAM** — `baml.cloud.sigv4_sign`, `gcp_access_token`. — 28, 35.
- **Observability** — `baml.obs.*` (start/finish/event/clock) + `ExecutionContext.emit` (the §7-flagged-but-undefined event surface, demanded concretely by 32, 39, 44, 46, 47).

---

## Part E — Unresolved tensions (cannot be closed by adding surface)

These are not "add a primitive" — they are places where the model and the requirement are fundamentally at odds. Honesty requires naming them.

1. **No compile-time information-flow / taint guarantee** (16) — the lethal-trifecta defense is a defeatable runtime `if`; the model has no information-flow types.
2. **No build-time secret hygiene** (35) — a `BearerKey` constructed in browser-targeted code typechecks and leaks; the existential model has no bundle-taint/provenance property.
3. **No portable graph IR** (43, 46) — topology is implicit in `let`/`match`/`while`, so a workflow can't be visualized, diffed, or statically checked; inter-step carry is `unknown`.
4. **Server-authoritative state is announce-not-own** (19, 20, 22, 23, 41, 42) — the model can *react* to reclaim/contention (catch sites) but cannot *own* eviction/single-writer/non-retryability; combinators silently mis-drive it.
5. **Combinators statically claim capabilities their members may not have** (07, 08, 09, 24, 29, 31, 42, 44) — `Fallback` is `Streaming`/`Tools`/`Harness` even when wrong; mid-loop/mid-session failover is impossible (the opaque `Transcript`/handle is owned by one member); for *binary* capabilities a combinator can never honestly *guarantee* what it advertises.
6. **Portability is shallow where wire semantics diverge** (07, 08, 13, 20, 23, 30, 40) — same interface, silently different *meaning* or *economics*: Gemini `truncate` is a no-op, Anthropic `avg` logprob is null, native-vs-emulated tool-deferral has different cache cost, fork-as-copy vs fork-as-pointer have different observable consequences. The type system calls them interchangeable; they are not.
7. **`requires Provider` misfits pure-I/O and pure-metadata capabilities** (25, 26, 33, 36, 44) — forcing `call<T>` onto Realtime/Stt/Tts/Capabilities/Deterministic/Suspendable produces a degenerate or dishonest method that exists only to satisfy the rule. Strong, repeated evidence that **OQ1's answer should be "no" for at least these classes.**

---

## Verdict distribution

| Verdict | Count |
|---|---|
| **Clean** | **1** (28 provider-diversity) |
| **Workable-with-additions** | **46** |
| **Awkward** | 0 |
| **Unsupported** | 0 |

## Honest assessment

The model holds up across the entire breadth — there is no scenario it cannot express — and that is a real result: one spine (provider + capability + client-as-function + combinators + runtime `match`) reaches from a single text call to durable multi-agent workflows without a kind-hierarchy. But "holds up" means "expressible with additions," and the additions are neither few nor incidental: roughly 25 distinct capability interfaces, a dozen value+sidecar carrier types, a concurrency surface BAML lacks entirely, public SAP, duplex transports, and a host of `$rust_type` state handles. Five structural pressures recur in nearly every scenario and deserve resolution *before* the capability zoo calcifies: `call<T> -> T` is too narrow to be the irreducible truth (P1 — usage/refusal/reasoning/handles fit nowhere); capability-as-runtime-promise is right for degradable features but dangerous for binary guarantees (P2); the whole combinator layer rests on unconfirmed value-level reflection (P3, OQ5); server-authoritative/mutable state has no first-class home and combinators silently mis-drive it (P4); and the opaque `PromptAst`/`Transcript` is too opaque for the read-access nearly everything needs (P5). The work it most needs, in order: (1) confirm OQ5 — nothing exists without it; (2) decide OQ1 honestly — pure-I/O and pure-metadata capabilities should *not* `requires Provider`; (3) widen the base result to carry a metadata sidecar (P1); (4) add a first-class stateful-session/durable-state notion with a "non-retryable, server-owned" marker so combinators stop lying (P4, P2); and (5) ship a structured read-view over `PromptAst` (P5). None of these breaks the thesis — they harden the one weak spot (the existential return) and the one over-aggressive deletion (the value's metadata).
