# LLM Provider Redesign — Implementation Plan

> **STATUS (2026-07-07): frozen design record — do not update; consult for the *why*.**
> Implementation reality is tracked in [`implementation-checklist.md`](./implementation-checklist.md)
> (live) and indexed in [`../llm-provider/REALIZED.md`](../llm-provider/REALIZED.md) /
> [`E2E_TESTS.md`](../llm-provider/E2E_TESTS.md). Built so far: Phases 0–3 nearly fully, plus large
> opportunistic parts of 4–5 (realtime over `baml.ws` live; chains/background live; stateful shapes
> compiled pending P8). The Phase-1 bullets on **`client`-as-sugar (P3) and the companion desugar**
> were deferred (orchestrator delegation shipped instead — see `deviations.md`) and are now specced
> in [`llm-desugar-capabilities-plan.md`](./llm-desugar-capabilities-plan.md), which **supersedes**
> those bullets and most of Part IV (migration), and adds the `//baml:llm_capability` open registry.
> Divergences from this doc: [`deviations.md`](./deviations.md). Directory guide: [`README.md`](./README.md).

**Status:** draft · **Scope:** full breadth (single-turn → realtime → harnesses → durable workflows) · **Shape:** phased, design-completion inline then build.

This plan turns the design exploration in [`../llm-provider/`](../llm-provider/) into an executable roadmap. It is grounded in two audits of the current tree (see the [Appendices](#appendix-a--current-code-change-map)):

- **Code reality** — how providers/clients work today (a triple-closed Rust enum system).
- **Language reality** — which BAML features the design leans on are already shipped (most of them).

The headline finding: **the language backbone the design needs is already implemented and tested** — interfaces + `requires`, associated types (BEP-057), interface default methods, interface-membership `match` + `reflect`, generics, `throws`/`catch`, the whole `spawn`/`await`/`Future`/`TaskGroup`/`CancelToken` stack, and scope-bound cleanup (`defer` + magic `cleanup()`, BEP-042). The redesign is buildable on today's compiler. What is *not* yet present is a small, concentrated surface (generic type aliases, a couple of error classes, `client`-as-sugar lowering) plus the genuinely-hard *design* questions the gap analysis flags. Those are resolved in [Part I](#part-i--blocking-design-decisions) before the build phases in [Part III](#part-iii--phased-build-roadmap).

---

## Source material

The authoritative design lives in `../llm-provider/ideas/`:

| Doc | Role |
|---|---|
| [`01-providers-clients-capabilities.md`](../llm-provider/ideas/01-providers-clients-capabilities.md) | The original spine (base `Provider` with `call`). |
| [`provider-as-marker.md`](../llm-provider/ideas/provider-as-marker.md) | **The current thesis** — `Provider` is a bare *marker*; interaction is per-capability. Supersedes 01 §1. |
| [`error-model.md`](../llm-provider/ideas/error-model.md) | Per-capability typed error channels + the `UnknownError` escape hatch. |
| [`value-sidecar-model.md`](../llm-provider/ideas/value-sidecar-model.md) | `call_with` / `ResponseMeta` for value + out-of-band metadata. |
| [`scenarios/_conventions.md`](../llm-provider/ideas/scenarios/_conventions.md) | The Rosetta stone: real stdlib spellings vs invented; the canonical spine. |
| [`scenarios/_gap-analysis.md`](../llm-provider/ideas/scenarios/_gap-analysis.md) | Where the model strains across 47 scenarios — the source of Part I. |
| [`scenarios/00-overview.md`](../llm-provider/ideas/scenarios/00-overview.md) | The 47-scenario index + verdicts. |

**Reading order for a newcomer:** `_conventions.md` (the model in one screen) → `provider-as-marker.md` → `_gap-analysis.md` → this plan.

---

## The target model in one screen

- **`Provider` is a bare marker.** It carries *no* interaction method — only the inherited combinator factories (`with_retry`, `fallback_to`, `cached`). It is the type a `client` returns and the bound every capability `requires`.
- **Capabilities are interfaces that `requires Provider`**, each owning its interaction: `HttpProvider` owns `call`/`call_with`, `Streaming` owns `stream`/`stream_with`, `Realtime` owns `run`, `Tools` owns `begin`/`step`/`submit`. **The capability set *is* the provider's type.** No degenerate `call` forced onto realtime/harness.
- **A `client` is sugar** for `function Name(args) -> Provider { body }`. Clients therefore compose, take params, select dynamically, and chain combinators — because they are ordinary functions.
- **Options dissolve into provider fields.** `Anthropic` has `max_tokens`, `Bedrock` has `region`. No `options:` blob, no `provider_options` union, no `@providers:` map, no closed enum.
- **Combinators** (`Fallback`, `Retry`, `Cache`, `RoundRobin`) are plain classes that forward each capability by runtime `match` over their members.
- **Errors** are one interface per capability (`CallError`/`StreamError`/`ToolError`/`RealtimeError`) on the channel `E | UnknownError`; foreign errors normalize into `UnknownError`.
- **Value + metadata** rides `call_with<T,U>(prompt, project) -> (T,U)`; `call` derives from it.
- **Capability negotiation is a runtime `match`** — every companion (`Foo(args)`, `Foo.stream`, `Foo.live`, `Foo.run_tools`) matches the client's provider for the capability it needs, degrading call↔stream both ways and erroring where no honest degrade exists.

---

## Part I — Blocking design decisions

The gap analysis grades all 47 scenarios **Workable-with-additions** — never impossible, but 0 are clean. The recurring strains cluster on two fault lines: *the per-call value-oriented model has no home for state*, and *the existential `client` makes every capability a runtime promise*. The decisions below resolve the ones that **block a coherent v1**; each states the problem, the options, and a recommendation. Decisions marked **[must-settle-before-build]** change the shape of core interfaces and cannot be retrofitted cheaply.

### D1 — Stateful / server-owned capabilities need a first-class handle with lifecycle · **[must-settle-before-build]**
*Gap A1 (fatal, 19 scenarios): conversation handles, server-stored chains, cache resources (Gemini `cachedContents`, billed-while-idle), background jobs, warm sockets, durable sessions, load-balancer cursors are mutable cross-call state the server owns, but the model is per-call. There is no lifecycle (RAII / TTL / compare-and-set / eviction), so state is smuggled through opaque `$rust_type` fields the app must keep aligned.*

**The finalizer mechanism already exists — this is BEP-042, not net-new.** BAML ships:
- **`defer { body }`** — runs on *every* exit of the enclosing block (normal completion, `return`, `break`/`continue`, **and error unwinding**) in LIFO order (`baml_compiler_parser/src/parser.rs:4551`). This is exactly the scope-bound cleanup the "billed-while-idle" concern needs: a handle acquired in a function is released even when the call throws.
- **magic `cleanup(self) -> void throws never`** — a by-name finalizer (like `to_json`), guaranteed to run **at most once** per instance whether called explicitly, via `defer`, or by the GC (BEP-042, `baml_compiler2_ast/src/cleanup_guard.rs:1`). A finalizer provably cannot fail (the `throws never` shape is enforced).

So the language primitive the gap analysis listed as "net-new #4 (RAII/`defer`/`Drop`)" is **already implemented**. There is no separate `Drop`/`Resource`/`using` interface — the idiomatic pattern is simply: give the handle class a `cleanup(self)` method, then `defer { handle.cleanup() }` at acquisition (canonical example: `crates/baml_tests/tests/cleanup.rs:36` and `tests/defer.rs`). What genuinely remains for D1 is a *convention*, not a compiler feature.

- **Options:** (a) leave state as opaque handle values threaded by the app (status quo of the scenarios — leaks); (b) define a lightweight **`Resource` convention** on top of BEP-042 — stateful handles define `cleanup()`, and capability call sites (or the app) use `defer { handle.cleanup() }`; (c) push all state to the host and expose only capability methods that take+return handles.
- **Recommendation:** (b). Adopt the shipped BEP-042 finalizer as the lifecycle spine: stateful capabilities (`Conversational`, `Chain`, `ManagedCache`, `Background`, `Realtime`) return handles that implement the magic `cleanup()`, and companions wrap acquisition in `defer`. This de-risks D1 substantially — it drops from "build a primitive" to "adopt a convention." The genuinely-hard residue is *not* the finalizer but (i) **cross-call, server-owned state** — a handle that must outlive any single function scope in a long-lived server, where `defer`-at-scope-exit is the wrong granularity and the app must hold the handle across HTTP requests — and (ii) **richer lifecycle** (TTL, compare-and-set, eviction), which stays per-capability library code. Everything in Phase 4–5 builds on this. **[downgraded from must-build to must-adopt]**

### D2 — Combinators must not silently forward a capability they cannot honestly re-drive · **[must-settle-before-build]**
*Gap A2 (fatal, 21 scenarios): `Retry` forwarding `submit` double-submits a billed job; `Retry` over a tool loop replays the whole loop and re-charges every side effect; `Fallback.run` re-drives a realtime provider after audio already streamed out; `Fallback` over an owner-bound session/chain can't fail over mid-conversation. A generic combinator statically claims every capability it forwards (`_conventions.md` §6), the type system permits it, and it does the wrong thing at runtime.*

The scenarios are the design record here — several **already prototyped mechanisms**, and their spread is the argument for converging them rather than inventing a new one. Grouped by what they contribute:

- **A per-combinator idempotency opt-out — actually coded.** [`29-reliability`](../llm-provider/ideas/scenarios/29-reliability/evaluation.md) ships `Retry.idempotent: bool` (`implement.baml:371`, guarded at `:397-401`; the app sets `idempotent: false` for side-effecting calls, `usage.baml:142`). Its own evaluation calls the bool **"a blunt instrument"** — it can't distinguish "the request never left the client" (safe) from "a 500 came back *after* the tool ran" (unsafe) — and points to request-level idempotency keys as the true fix.
- **Idempotency keys on the wire — the recurring "true fix."** [`27-background-jobs`](../llm-provider/ideas/scenarios/27-background-jobs/evaluation.md) (caller-owned `Idempotency-Key` header, `implement.baml:147`), [`45-workflow-durable`](../llm-provider/ideas/scenarios/45-workflow-durable/evaluation.md) (a key derived from the `(run, step)` coordinate + the tool call's opaque id, `implement.baml:225-257`), and `29` all converge on this. All three note the type system can't *tie* the key to the coordinate — it's by-convention.
- **A re-drive-vs-re-submit error classifier.** [`27-background-jobs`](../llm-provider/ideas/scenarios/27-background-jobs/evaluation.md) adds `is_terminal_job_failure()` on `BackgroundError` — the one shipped classifier that tells a combinator whether re-driving is safe (and it notes the signal is *absent* on the `UnknownError` half exactly when needed). **This is the same axis D8 wants.**
- **A per-tool effect marker.** [`11-parallel-tools`](../llm-provider/ideas/scenarios/11-parallel-tools/evaluation.md) proposes `Tool.effect: ToolEffect` (`ReadOnly` vs `Write`, defaulting to `Write`/safe) that a dispatcher consults before fan-out — conceded to be an unenforced author assertion.
- **Don't statically claim what you can't forward.** The stateful capabilities converge on *not* letting a generic combinator claim them: bespoke `ChainRetry` instead of generic `Retry` and an owner-tag + `ForeignChainHandle`/`is_handle_error()` guard ([`20-server-stored-chains`](../llm-provider/ideas/scenarios/20-server-stored-chains/evaluation.md)); `BoundTranscript { owner, inner }` so a tool transcript can't be submitted to a different member ([`09-tool-calling`](../llm-provider/ideas/scenarios/09-tool-calling/evaluation.md)); manual `PinnedSession` pinning ([`19-fork-branch`](../llm-provider/ideas/scenarios/19-fork-branch/evaluation.md)); opt-out-by-omission (`Poller` is deliberately not a `Provider` combinator, `27`; `Fallback` deliberately doesn't forward `ManagedCache`, [`31-caching`](../llm-provider/ideas/scenarios/31-caching/evaluation.md)); typed-trap methods ([`24-realtime-tools`](../llm-provider/ideas/scenarios/24-realtime-tools/evaluation.md)); a proposed-but-unbuilt `supported_lifecycle()` per-method split ([`42-harness-abstraction`](../llm-provider/ideas/scenarios/42-harness-abstraction/evaluation.md)).
- **The sharpest "no mechanism exists" exhibits.** [`10-agentic-loop`](../llm-provider/ideas/scenarios/10-agentic-loop/evaluation.md) — `Retry` over a tool loop "re-dispatch[es] every tool, re-charging side effects," and proposes nothing — and [`22-realtime-voice`](../llm-provider/ideas/scenarios/22-realtime-voice/evaluation.md), which names the absence directly: **"the model has no way to *say* 'this provider is non-retryable / its effects are external.'"** [`45`](../llm-provider/ideas/scenarios/45-workflow-durable/evaluation.md) names the same missing primitive as an "effect system to mark a function 'replay-safe.'"

- **Options:** (a) accept it as a documented runtime-promise footgun; (b) a single **effect/retryability classifier axis** on the capability + error surface that combinators consult before re-driving; (c) forbid generic combinators over stateful capabilities, forcing bespoke ones; (d) request-level **idempotency keys** as a wire-level safety net.
- **Recommendation: (b) as the spine, with (c) and (d) as the honest backstops — this is a convergence, not an invention.** Adopt one **effect/retryability classification** (`is_effectful` / `is_retryable` / `is_resumable`, the same axis as [D8](#d8--error-classifier-vocabulary-is-requestresponse-shaped-and-wrong-for-other-axes)) that lives on both capability methods and their error interfaces. `Retry`/`Fallback`/`RoundRobin` check it and **refuse-then-error** (a typed "cannot safely re-drive" error) instead of silently re-driving an effectful, non-idempotent, or owner-bound capability — generalizing `29`'s `idempotent` bool and `27`'s `is_terminal_job_failure()`. Promote `11`'s `Tool.effect` into that same axis for the dispatch layer. For genuinely non-portable state (sessions, chains, harnesses), follow `20`/`09`/`42`: the combinator does **not** statically claim the stateful capability — provide a bespoke, capability-aware combinator instead of forwarding the generic one. And carry an optional **idempotency key** on `HttpProvider`/`Tools` requests (`27`/`45`) as the wire-level safety net for the cases the marker can't make provably-once. **Accept the residual:** none of this is statically *enforced* (an effect marker is an author assertion, an idempotency key is by-convention) — but it moves the failure from *silent* to *typed-and-refused*, which is the achievable bar.

#### D2 worked example — what "one classifier axis on the capability + error surface" means concretely

The axis is a small set of yes/no methods answering **"is it safe to re-drive this?"**, added in the two places a combinator decides — and combinators *call* them instead of guessing. Today the only questions available (`is_network_error`/`is_rate_limit`/`is_parse_error`) don't answer that.

**Surface 1 — the error (a call *failed*; is retrying safe?).** A connection-refused (request never left) is safe; a `500` that returned *after* the server ran your tool is not — same "network-ish" failure, opposite safety. So the classifier rides the error:

```baml
interface baml.errors.CallError {
  function is_network_error(self) -> bool
  function is_rate_limit(self)    -> bool
  function is_parse_error(self)   -> bool
  function is_retryable(self)     -> bool   // NEW: were effects possibly committed?
}

class ConnectionRefused {                    // request never reached the server
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { true }
    function is_retryable(self)     -> bool { true }    // nothing happened → safe
  }
}
class ServerError500 {                       // server may have processed the request
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { false }
    function is_retryable(self)     -> bool { false }   // effect may have committed → unsafe
  }
}
```

`Retry` consults it in its forwarding loop instead of blindly re-looping (generalizing `29-reliability`'s `status_code == null` heuristic into a method every error answers):

```baml
class Retry {
  inner: Provider
  max: int
  implements HttpProvider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      let attempt = 0;
      while (true) {
        let r: T = (match (self.inner) {
          let h: HttpProvider => h.call<T>(prompt),
          _ => throw baml.errors.Unsupported { message: "not callable" },
        }) catch (e) {
          let ce: baml.errors.CallError => {
            if (attempt < self.max && ce.is_retryable()) { attempt = attempt + 1; continue; }  // consult axis
            throw e;                                        // unsafe or out of budget → surface it
          },
          _ => throw e,
        };
        return r;
      }
    }
  }
}
```

**Surface 2 — the capability/provider (no error; the operation itself must not be re-driven).** The double-submit case (`27`): `submit` *succeeds* and bills a job; the failure is later (in `wait_for`), and `Retry` wrapping the whole thing re-runs `submit`. There is no error to classify — the op is inherently effectful — so the marker rides the provider, and the combinator checks it *before* wrapping:

```baml
interface Provider {
  // ...combinator factories...
  function is_effectful(self) -> bool { false }   // default: safe. Effectful providers override.
}
class BackgroundJob {
  implements Provider  { function is_effectful(self) -> bool { true } }   // submit bills a job
  implements Background { /* submit, wait_for, ... */ }
}

// inside Retry/Fallback, before forwarding:
if (self.inner.is_effectful()) {
  throw baml.errors.CannotRetry { message: "provider is effectful; wrap the idempotent part or use a bespoke combinator" };
}
```

**Payoff.** The user writes the same thing either way:

| code | today | with the axis |
|---|---|---|
| `BigAnalysis().with_retry(3)` (effectful) | silently submits the billed job up to 3× | `is_effectful()` → typed `CannotRetry` at the wrap point |
| `GPT4().with_retry(3)` (stateless) | retries every failure, incl. post-effect 500s | enters the loop; `is_retryable()` retries the rate-limit, surfaces the 500 |

The failure moves from **silent** to **typed-and-refused**. `is_effectful()` is an author assertion (unchecked) — which is why it is paired with idempotency keys (wire backstop) and bespoke combinators for non-portable state — but it is the same method set D8 reuses for error triage, so it is **one axis, two consumers**.

### D3 — A signature must be able to demand a capability statically · **[must-settle-before-build]**
*Gap B1 (workable, 22 scenarios): because a client returns the existential `Provider`, "can this client stream / run tools / structured-output this `T`?" is only a runtime `match` that throws `Unsupported`. No signature can say "any provider, but it must do X."*

**What the examples explored** — three shapes, and the honest ceiling of each:
- **(i) Binary runtime-match-then-throw — the status quo in *every* scenario.** `match (p) { let x: SomeCapability => …, _ => throw Unsupported }`: `Constrained` ([`03`](../llm-provider/ideas/scenarios/03-constrained-decoding/usage.baml)), `Realtime` ([`22`](../llm-provider/ideas/scenarios/22-realtime-voice/implement.baml), [`25`](../llm-provider/ideas/scenarios/25-voice-pipelines/implement.baml)), `SearchableTools` ([`13`](../llm-provider/ideas/scenarios/13-searchable-tools/usage.baml)), `Deterministic` ([`33`](../llm-provider/ideas/scenarios/33-evaluation/implement.baml)), `Continuity`/`ReasoningMeta` ([`07`](../llm-provider/ideas/scenarios/07-reasoning/usage.baml)). Behavioral membership only — recovers the concrete type at runtime, proves nothing at compile time.
- **(ii) The only *graded* answer — [`36-capability-negotiation`](../llm-provider/ideas/scenarios/36-capability-negotiation/implement.baml)'s `Support` lattice, a library descriptor (not a type-system feature).** `enum Support { Yes; No; Maybe }` behind `interface Capabilities requires Provider { structured_output<T>() -> Support; image_input(dims) -> Support; parallel_tools() -> Support }`, fed by per-`T` host probes (`baml.schema.depth`/`uses_unions`/`uses_refs`) so the *same* `T` returns `No` on Gemini (unions/refs) but `Maybe` past depth-5 on OpenAI; a `Negotiated` combinator turns `Yes`/`No`/`Maybe` into send / skip-degrade-warn / probe-then-catch. Its own rationale: *"A `match` is binary; support is not."* This is the achievable-today mitigation — but built entirely on existing primitives, and `36`'s eval still concludes *"a compile-time capability contract is unsupported."*
- **(iii) The concrete-return escape hatch (`function … -> ConcreteHttpProvider`) — the only extant compile-time recovery, judged too coarse everywhere.** Named in `03`/`07`(`-> OpenAIReasoning`)/`22`/`25`/`33`/`30`. Every scenario reports the same two costs: you lose the `client` sugar + dynamic selection, and you can only demand **one named concrete provider** — never *"any provider that supports X for this `T`."*
- **(iv) The residue no mechanism solves — `implements` is binary, so it tests *presence, not quality*.** [`30-cascades-routing`](../llm-provider/ideas/scenarios/30-cascades-routing/implement.baml)'s Anthropic `implements ConfidenceProvider` but returns `Scored { confidence: 1.0, source: "none" }` — a satisfied interface that silently disables escalation (*"quality of implementation… is not a capability the type system can see"*); `03`'s hosted fallback returns post-hoc-validated JSON that is *"NOT a guarantee"*; `13`'s identical `SearchableTools` tags hide divergent cache economics; `33`'s `Deterministic` tag doesn't mean a run is actually reproducible. This is **B2-flavored (inherent)** — neither the lattice (grades *declared* support, still self-reported) nor the escape hatch (fixes presence, not honesty) closes it.

- **Options:** (a) runtime-only (status quo — every capability, incl. quality, is a runtime promise); (b) **intersection/refinement types** in signatures (`Provider & Streaming`) so a function can demand a capability at compile time — what every scenario is implicitly begging for, but a **type-system** feature none can express today; (c) ship `36`'s **`Support` lattice** as a library descriptor for the *graded* cases now (no type change); (d) the concrete-return escape hatch for the rare "I truly need one named provider" case.
- **Recommendation: (a)+(c) for v1, (b) as the flagged type-system direction, (d) as the documented escape hatch.** B1 does **not** block v1 — the runtime `match` works today, and `36`'s `Support` lattice covers the graded cases (structured-output-for-this-`T`, image-at-resolution) with existing primitives. Pursue (b) intersection types as the real fix, but it's a **type-system decision for the type-system owners**, not LLM-lib-local — so **decide the surface syntax now** (so companions and the escape hatch stay forward-compatible) and implement later. Be explicit in the docs that (iv) **quality/calibration is inherent** — a `Support: Yes` or an `implements` tag is a provider's self-report, not a proof; the model can offer runtime probes + telemetry, never a compile-time honesty guarantee. **[the *syntax direction* is must-settle-before-build; the implementation is deferrable]**

#### D3 worked example — binary `match` vs the graded `Support` lattice

```baml
// (i) BINARY — the status quo: present-or-absent, and only at runtime:
function Extract(doc: string) -> Resume {
  let p = client();
  match (p) {
    let h: HttpProvider => h.call<Resume>(render_prompt(doc)),
    _ => throw baml.errors.Unsupported { message: "client cannot produce a value" },
  }
}                                        // "can it structured-output THIS Resume?" — unknowable until it throws

// (ii) GRADED (scenario 36) — a library descriptor answers per-T, three-valued, before sending:
enum Support { Yes, No, Maybe }
interface Capabilities requires Provider {
  function structured_output<T>(self) -> Support          // computed from schema shape for THIS T
}
// app can branch up front instead of gambling on a throw:
match (cap.structured_output<Resume>()) {
  Yes   => Extract(doc),                                   // statically known to work on this provider
  No    => use_fallback(doc),                              // known to fail (e.g. Gemini + $ref cycle) — don't send
  Maybe => Extract(doc) catch (e) { let s: SchemaRejected => use_fallback(doc) },  // probe, downgrade on reject
}

// (iii) ESCAPE HATCH — the ONLY compile-time guarantee today, at the cost of the sugar + polymorphism:
function ExtractStrict(doc: string) -> Resume {
  client: OpenAI.from_model("gpt-4o")     // concrete return ⇒ the type checker knows it's Constrained/HttpProvider,
  prompt #"Extract from: {{ doc }}"#       // but you can no longer swap providers or say "any Constrained provider"
}
```

The lattice makes support *queryable and graded* (fixing "a flag can't capture the gradient"); it does **not** make it *provable* — that's option (b), a type-system ask, and even (b) can't reach (iv)'s calibration/honesty problem.

### D4 — Chain-wide provenance: the `(T,U)` sidecar projects one winner, not the chain
*Gap C1 (workable, 18 scenarios): `call_with`'s projection runs over exactly one `ResponseMeta` — the fallback winner, the last turn, the top provider — so tokens/cost/latency burned on retried members, multi-turn loops, and sub-calls (STT, judge, guard) have no slot.*

- **Options:** (a) accept single-winner provenance; (b) make combinators forward an **aggregate `ResponseMeta`** (an `AggregateMeta` that sums members) — the corpus already sketches `Fallback.call_with` handing `project` an aggregate; (c) a context-threaded usage accumulator (host cell) the whole call tree writes to.
- **Recommendation:** (b) as the default (combinators build aggregate metas), with (c) available as `ExecutionContext`-level usage accumulation for sub-calls that don't ride the main `call_with`. Not blocking for v1 spine; required by Phase 2 (combinators) and Phase 6 (observability).

### D5 — Non-error control outcomes must not be forced through `throws` or a frozen return
*Gap C2 (fatal, 7 scenarios): a budget-hit partial, a handoff, structured-output-as-tool are forced through `throws` (making `BudgetHit implements ToolError` with vacuous classifiers) or through a frozen `-> T` the combinator can't specialize.*

**What the examples show — the sum idiom already works where the signature allows it; only the *frozen* returns force the abuse.** BAML happily returns honest sums today wherever a capability method's signature was written wide:
- `step<T>(t) -> T | ToolCalls` ([`10-agentic-loop`](../llm-provider/ideas/scenarios/10-agentic-loop/implement.baml) `:67`) — the good pattern, *"the exact shape of `Iterator.next -> Item | Done`."*
- `submit -> Job<T>` ([`27-background-jobs`](../llm-provider/ideas/scenarios/27-background-jobs/implement.baml) `:133` — "a `Job<T>` is an opaque [handle]… come back later") and `start<T> -> T | Suspend` (`44`). The handle *replaces* the answer, honestly.

The abuse appears **only** where a method is frozen at `-> T`:
- **The smoking gun ([`10`](../llm-provider/ideas/scenarios/10-agentic-loop/implement.baml)):** `run_tools<T> -> T` is frozen, so a budget-hit partial *"escapes only via throw"* — `throw BudgetHit` (`:407`), and `BudgetHit implements baml.errors.ToolError` with all-false classifiers (`:447` — *"not a failure. All classifiers are false"*). The scenario then **hand-rolls** `run_to_budget<T> -> T | Budget<T>` (`:423`) as a separate static helper that catches the `BudgetHit` and re-exposes it as a sum — i.e. it *reconstructs by hand exactly the widened return the capability method should have had.*
- **Handoff through the wrong door ([`14-multi-agent`](../llm-provider/ideas/scenarios/14-multi-agent/implement.baml)):** with no `T | Handoff` return, a handoff is *"a Tools-combinator that… swaps providers"* smuggled through `submit`.
- **The inherent one ([`16-agent-security`](../llm-provider/ideas/scenarios/16-agent-security/implement.baml)):** `Tainted<T>` exists (`:34`), but `Quarantined.call` is stuck at `HttpProvider.call<T> -> T` — *"we can't change call's return type generically, so the obligation is on the CALLER to wrap"* — so taint-laundering is a one-line omission.

- **Options:** (a) keep abusing `throws` + vacuous classifiers (status quo — a budget cap is caught as a transport failure, per [D8](#d8--error-classifier-vocabulary-is-requestresponse-shaped-and-wrong-for-other-axes)); (b) widen the frozen capability returns to explicit **sum types** (`run_tools<T> -> T | Partial<T>`, `T | Handoff`), matching the `step`/`submit`/`start` idiom that already works; (c) a general "outcome" wrapper.
- **Recommendation: (b).** Sum returns are already idiomatic and already shipped where signatures permit — the fix is to stop freezing the other capability methods at `-> T`. Widen `run_tools` (→ `T | Partial<T>`), add `T | Handoff` for the multi-agent case, and keep refusal on `throws` (it *is* an error) — exactly the partition the value-sidecar doc draws (product → `call_with`; **sum → sentinel return**; refusal → `throws`). `10`'s hand-rolled `run_to_budget` becomes the *default* return, not a bolt-on. **Residue (inherent):** `HttpProvider.call`'s `-> T` genuinely can't be generically re-typed to `Tainted<T>` (`16`) — wrapping stays a caller obligation; the model can't force it. Settle the sentinel vocabulary per capability in Phase 3.

#### D5 worked example — widen the frozen return instead of throwing a fake error

```baml
// TODAY (scenario 10) — run_tools is frozen at -> T, so a budget cap can only ESCAPE via throw,
// and must masquerade as a ToolError with vacuously-false classifiers:
function run_tools<T>(self, prompt, tools, ctx) -> T throws baml.ExtendUnknownError<baml.errors.ToolError> {
  // ...loop...
  if (self.stop_when(info)) { throw BudgetHit { partial_json: ..., steps_taken: n }; }   // control signal as "error"
}
class BudgetHit {
  implements baml.errors.ToolError {                 // forced — it's the only way to be throwable here
    function is_network_error(self) -> bool { false } // all vacuously false: any `catch ToolError`
    function is_rate_limit(self)    -> bool { false } // swallows the budget cap as a transport failure
    function is_parse_error(self)   -> bool { false }
  }
}

// FIX — widen the return; the non-error outcome is a first-class arm, caught by `match`, not `catch`:
function run_tools<T>(self, prompt, tools, ctx) -> T | Partial<T> throws baml.ExtendUnknownError<baml.errors.ToolError> {
  if (self.stop_when(info)) { return Partial<T> { value_so_far: ..., steps_taken: n }; }   // honest sum
  // ...loop returns T on completion...
}
// caller distinguishes outcome from error cleanly — the same shape as `step -> T | ToolCalls`:
match (agent.run_tools<Answer>(prompt, tools, ctx)) {
  let a: Answer      => use(a),
  let p: Partial<Answer> => resume_or_report(p),        // NOT an error path
} catch (e) { let te: baml.errors.ToolError => retry(e) }  // real failures still on throws
```

The rule: **an outcome that has a value (partial, handoff, suspend, job-handle) is a sum arm; only a genuine failure is a `throw`.** `throws` stops carrying control signals, so [D8](#d8--error-classifier-vocabulary-is-requestresponse-shaped-and-wrong-for-other-axes)'s classifiers stop lying.

### D6 — Typed seams for tool args / output / resume payloads
*Gap D1 (workable, 15 scenarios): `Tool.parameters: type` is first-class, but `ToolCall.args: map<string,unknown>` and `ToolResult.output: unknown` erase the schema exactly at the handler boundary; resume payloads are untyped `unknown`.*

**What the scenarios actually do** (from the code, not the comments): the seam has two halves, and the first is already solved with real primitives while the second is quietly punted:
- **Outbound (emit schema) — real and type-driven.** Every tool scenario feeds the stored `type` to `baml.reflect.type_to_json_schema(tool.parameters)` / `type_to_gemini_openapi(tool.parameters)` ([`09-tool-calling`](../llm-provider/ideas/scenarios/09-tool-calling/implement.baml) `:212,:334,:476`; also `13`, `24`). `reflect.type_of` is real; the `type_to_*_schema` lowerings are the net-new bit (already tracked as [P7](#part-ii--language--compiler-prerequisites)).
- **Inbound (args → typed value) — punted to host code.** No scenario wires `Tool.parameters` back into a coercion in BAML. `09`/`24` push it into `ExecutionContext.invoke_one(c: ToolCall) { $rust_io_function }` whose comment says "SAP-validate `c.args` against the tool's `type`" but whose body doesn't even receive the `Tool`; `13` hand-plucks fields (`search_query_of(args: map<string,unknown>)`); `16` deliberately keeps `run_tool(call) -> unknown` so taint rides through. `09`'s own evaluation flags it: *"Argument validation lives nowhere canonical… an unlegislated policy decision sitting in net-new host code."*
- **SAP is the real bridge where typing IS recovered** — `baml.sap.parse<T>` / `baml.json.from_json<T>`, driven by the **lexical `T` in scope**, not the stored field: skill/agent output ([`39`](../llm-provider/ideas/scenarios/39-harness-extensibility/implement.baml) `:303,:326`), structured output ([`43`](../llm-provider/ideas/scenarios/43-workflow-graph/implement.baml)), final workflow carry ([`46`](../llm-provider/ideas/scenarios/46-workflow-observability/implement.baml) `baml.flow.coerce<T>` = "via SAP"). The one place `ToolCall.args → text → SAP` is used, it parses to `map<string,unknown>` (still untyped).
- **`baml.cast.checked<T>` is a red herring for this seam — and is `match` in disguise even where it appears.** It shows up in exactly **one** scenario ([`47`](../llm-provider/ideas/scenarios/47-workflow-agent-nesting/implement.baml) `:493`) as an *existential downcast* (`Workflow.Output → T`) — but that downcast needs **no new primitive**: `match (out) { let v: T => v, _ => throw Mismatch }` narrows a generic `T` at runtime today, and is the exact pattern the Iterator stack runs on (`ns_iter_impl_generics_only/ns_core/core.baml`; `ns_inferred_generic_type_args`). `14`'s analogous `baml.agent.coerce<T>` is called *"a smell… we launder through `unknown` and re-narrow"* — again, a `match` arm. The resume case ([`44`](../llm-provider/ideas/scenarios/44-workflow-suspend-resume/implement.baml)) carries `resume_schema: type` but **ignores it** at the coercion site — validation is delegated to the host/UI (`decision_is_approved(resume) { $rust_io_function }`).

- **Options:** (a) leave `unknown` + host hand-validation (status quo — invisible, unlegislated); (b) keep the wire seam `unknown` and standardize the inbound coercion on **SAP driven by the handler's declared type** (`baml.sap.parse<A>` — already real), making the step BAML-visible instead of a `$rust_io_function`; (c) additionally add a **dynamic-`type` SAP** (`baml.sap.parse_type(t: type, raw) -> unknown`) so `dispatch` can validate `c.args` against the *stored* `Tool.parameters` before invoking — the one thing no scenario does today; (d) mint a general `baml.cast.checked<T>` (rejected — unneeded: the existential downcast it was for is just a `match` arm).
- **Recommendation: (b) as the standard, (c) as the optional integrity check; explicitly not (d).** The wire seam stays `unknown` (providers return arbitrary JSON — honest). Standardize handler dispatch on SAP against the handler's declared arg type — this formalizes the intent the scenarios only wrote in comments, using primitives that already exist (`baml.sap.parse`, `reflect.type_of`). The one existential case (`47`) needs no primitive either — `match (out) { let v: T => v, _ => throw }`. No type-system change, no new cast primitive. Phase 3.

#### D6 worked example — SAP against the declared type, not a new cast

```baml
// The wire seam stays dynamic — providers genuinely return arbitrary JSON:
class ToolCall   { id: string, name: string, args: map<string, unknown> }
class ToolResult { id: string, output: unknown }
class Tool       { name: string, description: string, parameters: type }

// OUTBOUND — the stored `type` drives schema emission. Already the real idiom (scenario 09):
let schema: string = baml.reflect.type_to_json_schema(tool.parameters);   // uses tool.parameters

// INBOUND — coerce args to the handler's declared type A via SAP (already real). Today this hides
// inside a host `invoke_one`; D6 makes it explicit BAML and returns an error result on mismatch so
// the model can self-correct rather than aborting the loop:
function invoke<A>(self, handler: (A) -> unknown, c: ToolCall) -> ToolResult
    throws baml.ExtendUnknownError<baml.errors.ToolError> {
  let args: A = baml.sap.parse<A>(baml.json.to_string<map<string, unknown>>(c.args)) catch (e) {
    _ => { return ToolResult { id: c.id, output: { "error": "args did not match schema" } }; }
  };
  ToolResult { id: c.id, output: handler(args) }         // output stays `unknown` on the wire
}

// OPTIONAL integrity check (option c) — validate against the STORED parameters, so the advertised
// schema and the handler type can't silently drift. Needs a dynamic-`type` SAP (net-new):
let ok: unknown = baml.sap.parse_type(tool.parameters, baml.json.to_string<map<string, unknown>>(c.args));
```

The honest residue: (b) uses the handler's **lexical** type `A` — the stored `Tool.parameters` still only drives the *outbound* schema, so "advertised schema == handler type" is unchecked unless you also adopt (c). That gap is real but small, and closing it is one net-new host fn (`parse_type`), not a new type-system feature or a general checked-cast.

### D7 — A typed structural view over the opaque `PromptAst` / `Transcript`
*Gap D2 (workable, 9 scenarios): prompt-rewriting middleware (memory, compaction, guardrails, truncation) and per-turn tool filtering bottom out in untyped host pokes; a char-cut truncation can sever a `tool_call` from its `tool_result` and produce wire-invalid history.*

**What the examples explored** (nine scenarios, no fewer than six distinct workarounds — the spread itself is the signal that the seam is missing):
- **Opaque blob algebra with a char cut — the hazard, verbatim.** [`14-multi-agent`](../llm-provider/ideas/scenarios/14-multi-agent/implement.baml) threads history with `baml.llm.concat_prompts(parent, carry)` then `baml.llm.truncate_prompt(joined, n)` — a **char/token** truncation that can sever a `tool_use` from its `tool_result`. Its evaluation: *"opacity and history-threading are in tension… this is unresolved."*
- **Net-new opaque prompt pokes.** [`21-memory`](../llm-provider/ideas/scenarios/21-memory/implement.baml) (`last_user_text`, `with_system_suffix`) and [`15-guardrails`](../llm-provider/ideas/scenarios/15-guardrails/implement.baml) (`prompt_user_text`). `21`'s evaluation names the class directly: *"prompt-rewriting middleware (memory, but also compaction and few-shot injection) all bottom out in host functions… a whole class of middleware is un-typed BAML over an opaque AST."*
- **Host-poke-by-convention mutation.** [`10-agentic-loop`](../llm-provider/ideas/scenarios/10-agentic-loop/implement.baml) pokes the *inner* provider's opaque transcript with `inner_set_tools(...)` for per-turn tool filtering — its own evaluation calls this *"genuinely awkward"* and wishes for `set_tools` on the `Tools` interface so the provider re-renders its own envelope.
- **Opaque `Window` handle passed verbatim (safety by never decoding).** [`18-compaction`](../llm-provider/ideas/scenarios/18-compaction/implement.baml) round-trips `WindowItems { _data }` with a "DO NOT read, edit, or reorder" contract — safe, but zero inspectability, which its evaluation admits *"lies about Gemini"* (whose window is really readable prose).
- **A genuinely typed structural grammar — the thing D7 wants — but built app-side, not as a view over the provider transcript.** [`17-history-and-sessions`](../llm-provider/ideas/scenarios/17-history-and-sessions/implement.baml) defines `Turn { role, blocks }` + `type Block = TextBlock | ToolUseBlock | ToolResultBlock | ReasoningBlock` with **id-correlated tool pairs**, so windowing/summarizing operate on **whole `Turn`s** (`.slice`) and *cannot* orphan a `tool_result`. It reappears as the Wire/UI split in [`32-observability`](../llm-provider/ideas/scenarios/32-observability/implement.baml) and the id-addressed on-disk `SessionStore` (message-id truncate/fork) in [`40-harness-sessions`](../llm-provider/ideas/scenarios/40-harness-sessions/implement.baml). `17`'s honest caveat: the `Block` union is *"a normalization bet that will leak"* — a lossy lowest-common-denominator.
- **Proof the affordance is feasible:** [`36-capability-negotiation`](../llm-provider/ideas/scenarios/36-capability-negotiation/implement.baml) uses the **already-real** `baml.media.image_dims` / `downscale` view to safely pre-flight-rewrite a request — exactly the typed structural read the prompt/transcript lacks.

The evals converge on one wish (a typed messages/tool-pair view on the seam), and `17`/`32`/`40` independently reinvent it app-side — which both proves the shape and shows the cost (lossy normalization). **No scenario builds a `baml.llm.view` / `PromptView` over the provider's own prompt; that absence is the finding.**

- **Options:** (a) keep `PromptAst` fully opaque (host pokes only — the `10`/`14`/`15`/`21` status quo, admitted-awkward); (b) expose a **read-only typed view** `baml.llm.view(prompt) -> PromptView` (roles / messages / **id-correlated tool-pairs**, like `17`'s grammar) so middleware edits by *structural unit*, not chars; (c) an opaque **verbatim `Window` handle** for transcripts that genuinely don't normalize (`18`); (d) make `PromptAst` a fully transparent, mutable BAML value.
- **Recommendation: (b) as the default, (c) as the escape hatch; not (d).** Expose a read-only structural *view* (not a mutable AST) — the provider still **owns** the prompt, but middleware gets a safe unit (whole message, whole tool-pair) so truncation/compaction/memory-injection can't sever a pair. Where a provider's window truly is opaque prose (Gemini compaction), fall back to `18`'s verbatim handle rather than lying with a typed view. This is net-new host surface (`baml.media.view` is the real precedent). **Honest residue (inherent, B2-flavored):** a *portable* view is a lossy LCD — a provider block outside the union flattens to text (`17`'s "normalization bet"), so the view makes editing *safe* but not *lossless*. Phase 3–5 (needed by tools, memory, compaction, sessions).

#### D7 worked example — edit by structural unit, not by chars

```baml
// THE HAZARD (scenario 14) — opaque concat + a char cut that can orphan a tool_result:
function thread(self, parent: baml.llm.PromptAst, carry: baml.llm.PromptAst) -> baml.llm.PromptAst throws ... {
  let joined = baml.llm.concat_prompts(parent, carry);
  match (self.max_chars) {
    let n: int => baml.llm.truncate_prompt(joined, n),   // char-based → can sever tool_use/tool_result
    _          => joined,
  }
}

// THE FIX (scenario 17 shape, generalized as the D7 view) — a read-only typed view; edit whole turns:
enum Role { System, User, Assistant, Tool }
class ToolUseBlock    { id: string, name: string, args: map<string, unknown> }
class ToolResultBlock { id: string, output: unknown }
type  Block = TextBlock | ToolUseBlock | ToolResultBlock | ReasoningBlock
class Turn  { role: Role, blocks: Block[] }
interface PromptView { function turns(self) -> Turn[] }        // baml.llm.view(prompt) -> PromptView

function window(self, prompt: baml.llm.PromptAst, keep: int) -> Turn[] {
  let view = baml.llm.view(prompt);
  view.turns().slice(view.turns().length() - keep, view.turns().length())  // whole-Turn cut: a tool pair
}                                                                            // is never split — safe by construction
```

Char-truncation *can* leave a `ToolUseBlock` whose matching `ToolResultBlock` was cut (wire-invalid); slicing whole `Turn`s cannot. That is the entire point of the view.

### D8 — Error classifier vocabulary is request/response-shaped and wrong for other axes
*Gap E2 (workable, 16 scenarios): `is_network_error`/`is_rate_limit`/`is_parse_error` answer false for the failures that drive decisions — budget-hit, policy-refusal, session-not-found, security-denial, transport-drop-vs-server-teardown. A dropped realtime socket *is* a network error, so the classifier tells `Fallback` the one thing it must not retry is safe to retry.*

**What the examples invented** (16 scenarios; the pattern is uncannily consistent). Every scenario that hit a decision the trio couldn't name either **minted a capability-local classifier**, **lied** with a default bool, or **answered `false` to all three and vanished from interface-level triage**. Enumerated by the dimension they demand:

- **Retryability (`is_retryable`) — the load-bearing axis, motivated 5× independently.** [`22-realtime-voice`](../llm-provider/ideas/scenarios/22-realtime-voice/evaluation.md) states it verbatim: *"a dropped socket IS a network error by every honest definition, yet it is exactly the case where a silent reconnect-as-retry replays external effects… nothing in the channel marks an error as non-retryable."* Realized capability-locally as `is_terminal_job_failure` ([`27-background-jobs`](../llm-provider/ideas/scenarios/27-background-jobs/implement.baml) `:63` — "did the SERVER kill the job (re-submit) or did we time out polling (re-drive)?") and `is_non_determinism` ([`45-workflow-durable`](../llm-provider/ideas/scenarios/45-workflow-durable/implement.baml) `:32` — "no word for 'the replayed body changed'… retrying is futile"). Also the security stakes: [`16-agent-security`](../llm-provider/ideas/scenarios/16-agent-security/evaluation.md) — `catch ToolError => retry()` *"will happily retry a denied exfil attempt."*
- **Effect (`is_effectful`) — what makes "retryable" safe.** Audio already played (`22`), a double-submitted billed job (`27`), a committed durable step (`45`'s `is_step_log_error` read-vs-write proxy `:33`). This is the *provider-side* half of the same axis D2 uses.
- **Resumability / state-loss (`is_resumable`).** [`26-transports`](../llm-provider/ideas/scenarios/26-transports/evaluation.md): *"a server-authoritative session closing is state loss, not a network blip… a session the server tore down for a content violation and a Wi-Fi drop are indistinguishable at the catch."* Realized as `is_session_expired` ([`42-harness-abstraction`](../llm-provider/ideas/scenarios/42-harness-abstraction/implement.baml) `:93`), `is_not_found` ([`40-harness-sessions`](../llm-provider/ideas/scenarios/40-harness-sessions/implement.baml) `:52`), `is_store_unavailable` (`17`, which even *lies* `is_network_error→true` — "a lie of convenience").
- **Policy / refusal / budget / security (`is_policy_refusal`).** Faked with three-false classifiers by `Refused` ([`01`](../llm-provider/ideas/scenarios/01-single-turn-text/implement.baml) `:243`), tripwires ([`15-guardrails`](../llm-provider/ideas/scenarios/15-guardrails/implement.baml)), `BlockedOutboundUrl`/`HumanDeniedCall` (`16`). Minted as `is_budget_exceeded` ([`34-cost-and-batch`](../llm-provider/ideas/scenarios/34-cost-and-batch/implement.baml) `:74`); wished as `is_unviable_here` ([`35-deployment-shapes`](../llm-provider/ideas/scenarios/35-deployment-shapes/evaluation.md) `:37`); `is_auth_error` ([`12-tool-taxonomy`](../llm-provider/ideas/scenarios/12-tool-taxonomy/evaluation.md) — "auth-rejection, the one failure the background calls security-critical").
- **Handle-validity (`is_not_found` / `is_session_expired` / `is_wrong_owner`).** `40` `:52`, `42` `:93`, `WrongBatchOwner`/`WrongOwnerJob` (`34`/`27`).
- **Capability-gap (`is_unsupported`).** [`05-multimodal-input`](../llm-provider/ideas/scenarios/05-multimodal-input/evaluation.md): *"cannot tell 'you handed me audio Anthropic can't take' apart from 'the model declined'."* `baml.errors.Unsupported` already implements all four capability interfaces — a natural home for a shared `is_unsupported`.

**Genuinely capability-specific** (keep on their own interface, don't share): `is_unsatisfiable` (`03` `ConstraintError`), `is_unknown_price` / `is_partial_failure` (`34`), `is_runtime_error` (`42`), `is_media_error` (`05`).

**Two structural findings that shape the fix:**
1. **Every net-new capability silently drops or redefines the base three.** `SessionError` (`40`) and `HarnessError` (`42`) drop `is_rate_limit`/`is_parse_error`; `HarnessError` drops all three. So the trio is *not* a stable universal contract — the "triage without the concrete class" promise is already per-capability. A shared richer axis is what would restore it.
2. **Non-error control signals are repeatedly forced onto an error interface with vacuously-false classifiers** — `BudgetExceeded`→`CallError` (`34` `:142`), `NonDeterministicReplay`→`CallError` (`45` `:42`), `TransportUnviable`/`CredentialUnavailable` all-false (`35`). This is the **D5** overlap: the decision-driving fact is invisible to every classifier precisely because it isn't a transport failure.

- **Options:** (a) keep the three classifiers per-capability (status quo — every capability re-litigates its vocabulary and control signals fake-implement); (b) a **shared classification base** (`baml.errors.Failure`) carrying the cross-capability axis (`is_retryable` / `is_effectful` / `is_policy_refusal` / `is_resumable` / `is_unsupported`) that **every** capability error `requires`, with per-capability interfaces adding only genuinely-specific probes; (c) per-capability bespoke classifiers (status quo, formalized).
- **Recommendation: (b).** Put the decision axis on a shared `baml.errors.Failure` base that per-capability error interfaces `requires` (so they *can't* drop it), leaving the transport trio (`is_rate_limit`/`is_parse_error`/`is_network_error`) on the request/response capabilities where it's honest. This is the **same axis D2's combinators consult** (see the [D2 worked example](#d2-worked-example--what-one-classifier-axis-on-the-capability--error-surface-means-concretely) — `is_retryable` on the error, `is_effectful` on the provider): **one classification, two consumers** (error triage E2 + combinator forwarding A2). It also absorbs finding 2 — a budget-hit or non-determinism sets `is_policy_refusal`/`is_retryable=false` truthfully instead of faking three-false. Settle the `Failure` base in Phase 1; populate per-capability in later phases. **Residue:** classifiers remain author-asserted (a provider can mis-answer `is_effectful`), same honest bar as D2.

#### D8 worked example — a shared `Failure` base the capability errors `requires`

```baml
// The cross-capability DECISION axis — every capability error carries it, so a consumer can triage
// (and a combinator can decide retry/forward) WITHOUT knowing the concrete class.
interface baml.errors.Failure {
  function is_retryable(self)      -> bool   // safe to re-drive? false if effects may have committed
  function is_effectful(self)      -> bool   // did/will this commit an external side effect?
  function is_policy_refusal(self) -> bool   // deliberate decline: model refusal, guardrail, security, budget
  function is_resumable(self)      -> bool   // transport-drop (resumable) vs server-teardown / handle-invalid
  function is_unsupported(self)    -> bool   // the backend cannot do this at all
}

// Request/response keeps the transport trio — but REQUIRES the shared axis (can't drop it):
interface baml.errors.CallError requires baml.errors.Failure {
  function is_network_error(self) -> bool
  function is_rate_limit(self)    -> bool
  function is_parse_error(self)   -> bool
}
// A net-new capability adds ONLY its specific probe on top of the shared base:
interface baml.errors.BackgroundError requires baml.errors.Failure {
  function is_terminal_job_failure(self) -> bool   // = is_retryable=false + is_effectful=true, named for the domain
}

// A control signal (scenario 34) now tells the TRUTH instead of faking three-false transport classifiers:
class BudgetExceeded {
  spent: float
  implements baml.errors.CallError {
    function is_policy_refusal(self) -> bool { true }    // it's a deliberate stop —
    function is_retryable(self)      -> bool { false }   // — retrying won't help,
    function is_effectful(self)      -> bool { false }   // — and it committed nothing.
    function is_unsupported(self)    -> bool { false }
    function is_resumable(self)      -> bool { false }
    function is_network_error(self)  -> bool { false }
    function is_rate_limit(self)     -> bool { false }
    function is_parse_error(self)    -> bool { false }
  }
}
```

A consumer catching `baml.errors.Failure` can now branch on `is_policy_refusal()` / `is_retryable()` across *any* capability, and `catch Failure => retry()` stops re-driving denied exfils and budget caps — the exact bug scenarios 16/34/45 hit.

### D9 — Resolved / accepted-as-inherent (no action, documented)
- **OQ1 (degenerate `call` on realtime/harness) — resolved** by the marker model: realtime/harness `requires Provider` and expose only `run`; there is no base `call` to fake.
- **Gap B2 (shallow portability, fatal-inherent, 18 scenarios) — accept.** Two providers can satisfy the same interface and type identically while their *meaning/cost/cache-economics/side-effects* differ (OpenAI vs Anthropic finish-reason vocab; native vs emulated tool-search cache busting; destructive vs non-destructive `ResumeAt`). No addition fixes this — the types are honestly equal, the world is not. The plan's job is to make it *visible* (normalized-where-possible + documented divergence), not to erase it.
- **§3 one-offs** (SigV4-vs-envelope-header ordering, browser secret taint, MCP tool-poisoning, no portable graph IR) — track as per-feature risks in the phase that touches them; none blocks the spine.

---

## Part II — Language & compiler prerequisites

Small, concentrated, and mostly independent of the LLM library. These land in **Phase 0**.

| # | Prerequisite | Status today | Work | Blocks |
|---|---|---|---|---|
| P1 | **Generic type aliases** `type ExtendUnknownError<E> = E \| UnknownError` | **Not implemented** — `TypeAliasDef` has no type-param field (`baml_compiler2_ast/src/ast.rs:1776`); no `.baml` uses `type Name<…> = …` | Parser + HIR + TIR support for parameterized aliases. **Fallback if deferred:** inline the union in every `throws` (union errors in `throws` already work) — ergonomic cost only. | Error model ergonomics (all phases) |
| P2 | **`UnknownError` + per-capability error interfaces** (`CallError`/`StreamError`/`ToolError`/`RealtimeError`) in stdlib | **Do not exist** (0 hits in `baml_std`) | Add plain classes/interfaces to `baml/ns_errors/` — trivial, they mirror existing error classes. Include the D8 classifier axis. | Phase 1 |
| P3 | **`client` as function-sugar** | `client` is a first-class **config block** today (`ClientDef`, `ast.rs:1783`), not `function → Provider` sugar | New lowering: rewrite `client Name(args) { body }` → `function Name(args) -> Provider { body }`; keep the config-block form as back-compat surface (Part IV). | Phase 1 |
| P4 | **`type` as a stored class field** (`class Tool { parameters: type }`) | **Unverified** — `type` as param/return/local is proven & tested; the field position has no test | One-line confirmation test; almost certainly already works. | Phase 3 (Tools) |
| P5 | **Public `baml.sap.parse<T>` / `parse_partial<T>`** | Engine is real but internal (`__sap_parse_final`/`_partial`, `ns_llm/llm_types.baml:796`) | Expose a public wrapper (this is OQ6 = *expose*, not build). | Phase 1 (pure-BAML providers) |
| P6 | **Scope-bound cleanup + finalizer** (`defer` + magic `cleanup()`) | ✅ **shipped (BEP-042)** — `defer` runs on all block exits incl. error unwinding (`parser.rs:4551`); `cleanup(self)->void throws never` is an at-most-once finalizer (`cleanup_guard.rs:1`) | None (build on it). Only a `Resource` *convention* (D1) rides on top — no compiler work. | Phase 4–5 |
| P7 | **Schema-lowering seams with a typed failure channel** (`baml.schema.json_schema`/`gemini_schema`, `reflect.type_to_*_schema`) | Net-new (flagged in conventions) | Host fns that lower a `type` to each wire dialect and `throws CallError` on inexpressible types (Gemini `$ref` cycles). | Phase 1 (structured output) |
| P8 | **Duplex transport** (`baml.ws`/`webrtc`/`realtime` over raw `baml.net`) + **inbound control-inversion** (webhook handler) + **resumable mid-stream offset** + **UUID** | Net-new host surface (gap analysis §2) | Sequenced with the phases that need them (Realtime = Phase 4; background jobs / A2A push = Phase 5). | Phase 4–5 |

**What is already solid and needs no work** (build directly on these): interfaces + `requires`, multiple `implements` blocks, associated types incl. defaults/projection (BEP-057), interface default methods, interface-membership `match` + `reflect.type_of`/`.implements`/`.implementors`, generic classes/interfaces/methods, `throws`/`catch`/union-error mechanism, and the full BEP-034 concurrency stack (`spawn`/`await`/`Future`/`TaskGroup`/`CancelToken`). All have passing test projects under `crates/baml_tests/`.

---

## Part III — Phased build roadmap

Full breadth, sequenced so each phase ships something usable and de-risks the next. Scenario numbers reference `../llm-provider/ideas/scenarios/`.

### Phase 0 — Prerequisites & scaffolding
**Goal:** land the language/host surface the spine needs; no behavior change to existing clients.
- Ship P1 (or accept the inline-union fallback), P2 (error stdlib + classifier axis from D8), P4 (verify `type` field), P5 (public SAP), P7 (schema lowering).
- Decide D3 syntax (static capability refinement) even if implementation is deferred — companions must be forward-compatible.
- **Exit:** error classes exist; `baml.sap.parse<T>` public; schema-lowering fns callable; a throwaway `.baml` proves `class Tool { parameters: type }` compiles + runs.

### Phase 1 — The spine (HttpProvider + client-sugar + error model)
**Goal:** replace the closed provider enum with the marker model for basic request/response. This is the make-or-break phase.
- Define `interface Provider` (marker) + `interface HttpProvider requires Provider` with `build_request`/`send`/`parse`/`meta_of`/`call_with`/`call` (per `_conventions.md`).
- Port **OpenAI, Anthropic, Gemini** from Rust `match` arms to BAML `class`es implementing `HttpProvider`. The whole point of the redesign is that a provider is *just* a class that implements `HttpProvider`; a built-in must be written the same way a user's custom provider is, or the model isn't real. So the **per-provider request/response *logic* lives in BAML** — `build_request` assembles the body with `baml.json.*` + string methods and picks the endpoint/headers; `parse` decodes via `baml.sap.parse<T>` / `baml.json.from_json<T>`; schema emission via `baml.reflect.type_to_json_schema`. What goes away is the **closed Rust `match provider`** that owns per-provider shaping — not host functions in general. Options become **class fields**; delete the `provider_options` union.
- **Leaf host primitives legitimately stay in Rust — that's fine, and expected.** BAML provider logic calls generic, provider-agnostic host functions: SAP itself (`baml.sap.parse`), `PromptAst` construction (`render_prompt` / `baml.llm.*`), the HTTP transport (`baml.http.send`), and the auth/crypto below. The distinction is *orchestration vs leaf*: the per-provider control flow (which endpoint, how the body is shaped, how the response threads to `parse`) is BAML; the irreducible primitives it calls are Rust. Much of the orchestration is **already BAML today** (the strategy loop + `call`/`stream` flow in `ns_llm/llm.baml` + `llm_types.baml`); this phase extends that to the per-provider request building currently trapped in `sys_llm/src/build_request/*` Rust arms.
- **The only native residue is auth/crypto that genuinely needs a Rust crate** — AWS SigV4 request signing, GCP/Vertex OAuth access-token minting, Azure AD tokens. Expose these as narrow, provider-agnostic host functions the BAML `implements Auth`/`build_request` calls — e.g. `baml.cloud.sigv4_sign(request, service, region)`, `baml.cloud.gcp_access_token(...)` (both already flagged net-new in `_conventions.md`). A provider's auth is then ordinary BAML: bearer/header providers (OpenAI/Anthropic/Gemini-API-key) set a header inline; Bedrock/Vertex call the one crypto/IAM primitive. This shrinks the native surface for providers from "three exhaustive Rust `match`es" to "a handful of stateless signing/token functions," and lets a user add e.g. a SigV4-authed provider without touching Rust. *(Caveat from `28`: SigV4 signs the whole request, so it must run **after** any envelope-header merge — see [D9 §3](#d9--resolved--accepted-as-inherent-no-action-documented) / Part V.)*
- Implement P3: `client` sugar → `function → Provider`. Keep shorthand `"provider/model"`.
- Companion desugar: `Foo(args)` → the `match (client()) { let h: HttpProvider => h.call<T>(...) , _ => throw Unsupported }`.
- Wire the per-capability error channels (P2) through `call`/`call_with`.
- **Files:** `sys_llm/src/provider.rs` (retire enum), `build_request/`, `parse_response/`, `auth_request/` (per-provider arms become BAML/host bodies), `baml_std.rs` (`apply_provider_defaults` → class field defaults; drop `ProviderOptions`), `ns_llm/llm_types.baml` (`PrimitiveClient` → capability interfaces + provider classes), `lower_cst.rs` (`synthesize_client_*`, `is_valid_provider`, `append_default_client_param`), `build.rs` (`@providers` codegen), `sys_ops/src/lib.rs` (`IoClassLlmClient`). See [Appendix A](#appendix-a--current-code-change-map).
- **Scenarios covered:** 01 (single-turn text), 02 (structured output), 28 (provider diversity — the one "Clean" scenario), 35 (deployment shapes, partial).
- **Exit:** a hand-written custom provider class (not in any enum) works end-to-end via a `client`; the three built-ins pass the existing LLM test suite; `provider_options`/`ClientType`/`@providers` removed or shimmed (Part IV).

### Phase 2 — Streaming, combinators, value+sidecar
**Goal:** everything the current `Client` strategy layer does, on the new model.
- `interface Streaming requires Provider` (`stream`/`stream_with`) over the existing `baml.llm.Stream<TStream,TFinal>`; the call↔stream degrade both ways in companions.
- Combinators `Fallback`/`Retry`/`Cache`/`RoundRobin` as plain classes forwarding by `match` — replacing the `ClientType` enum loop (`llm_types.baml` `execute_once_*` continue-catch loops). Apply **D2** (effect-aware forwarding) and **D4** (aggregate `ResponseMeta`).
- `call_with`/`ResponseMeta` + `Supported<T>` (value-sidecar model). Requires tuples + closures-as-params (already available).
- **Scenarios:** 04 (streaming), 29 (reliability), 30 (cascades/routing), 31 (caching, partial), 32 (observability), 34 (cost/tokens).
- **Exit:** `GPT4().fallback_to(Claude()).with_retry(2)` works; usage/logprobs project via `call_with`; retry refuses to re-drive an effectful member.

### Phase 3 — Tools & the agentic loop
**Goal:** the `Tools` capability and typed handler seams.
- `interface Tools requires Provider` with `type Transcript`, `begin`/`step`/`submit`, default `run_tools`. `Tool { name, description, parameters: type }` (P4).
- **D5:** widen `run_tools`/`step` returns to explicit sum outcomes (`T | Partial<T>`, `T | Handoff`). **D6:** `ExecutionContext.dispatch` coerces args via `baml.sap.parse` against the handler's declared type (no new cast primitive — existential downcast is just `match`). **D7:** `PromptView` structural read for per-turn tool filtering.
- Parallel tools via the shipped concurrency stack (`spawn` + `baml.future.all`, `TaskGroup` to cap) — no new host surface.
- **Scenarios:** 09–16 (tool calling, agentic loop, parallel tools, taxonomy, searchable, multi-agent, guardrails, security).
- **Exit:** a multi-tool agent loop with parallel dispatch, approval gate, and a budget-hit partial that surfaces as a sum outcome (not a fake `ToolError`).

### Phase 4 — Realtime, channels & harnesses
**Goal:** duplex/long-lived providers with no fake `call`.
- P6 (`Resource`/lifecycle) + P8 (duplex transport). `interface Realtime requires Provider` with `run(prompt, io: Channel)`; `Channel` pass-in at the `.live` companion. `LiveControl requires Realtime` (barge-in/cancel/truncate).
- Harnesses (Claude Code) as marker-only providers implementing `Realtime` over a subprocess; control plane as an `OutEvent`/`InEvent` union over `Channel`. **Track §3 gap:** no `PermissionReply {ask_id}` down-channel — decide the union shape.
- **D1** lifecycle governs the session handle; **D3** decides whether a realtime function statically demands `Realtime`.
- **Scenarios:** 22–26 (realtime/voice/barge-in/tools/pipelines/transports), 37–42 (harnesses).
- **Exit:** a voice session and a Claude-Code harness run through the same `client`+companion machinery; channel lifecycle is scope-bound, not by-convention.

### Phase 5 — Stateful capabilities & durable workflows
**Goal:** the state-heavy tail — the fault line the model strains on most.
- On D1's `Resource`/handle foundation: `Conversational`/`Session`, `Chain`, `ManagedCache`, `Background`/`Job<T>`, `Suspendable`, `Durable`. Inbound control-inversion (P8) for job completion + A2A push.
- **D2** guards these hard: mark stateful/effectful capabilities non-retryable so combinators error instead of double-submitting.
- Durable workflows: steps are typed functions, model calls are `provider.call<T>`, fan-in via structured concurrency; the net-new layer is durable resume (a `Checkpoint`/replay layer outside the Provider hierarchy). **Accept** the "no portable graph IR" §3 gap.
- **Scenarios:** 17–21 (history/sessions/fork/chains/memory), 27 (background jobs), 33 (evaluation), 43–47 (workflows).
- **Exit:** a resumable HITL workflow and a server-stored chat session with lifecycle; `BigAnalysis().with_retry(3)` refuses to double-bill.

### Phase 6 — Hardening, migration completion, docs
- Complete Part IV migration (remove shims), normalize `ResponseMeta` vocabularies where honest (finish-reason), document the B2 divergences that can't be normalized, ship the D3 static-refinement ergonomics if deferred, and land the observability aggregate provenance (D4) end-to-end.

---

## Part IV — Migration & compatibility

The current surface is triple-closed (enum + `ClientType` + `provider_options` + `@providers`). Migration must not break existing `.baml` projects.

1. **Keep `client<llm> Name { provider "openai" options {…} }` working** as a compatibility front-end that lowers to a built-in provider *class* construction (Phase 1 keeps the `synthesize_client_*` path, retargeted). Emit a deprecation note pointing at the class form once it's stable.
2. **Preserve `"provider/model"` shorthand** — it inlines a built-in provider class instead of a `Client{from_shorthand}`.
3. **Retire in order:** `provider_options` union → per-provider class fields (Phase 1); `ClientType` enum + `Client.execute_once_*` loops → combinator classes (Phase 2); `LlmProvider` Rust enum + the three exhaustive `match`es → provider classes with host bodies (Phase 1, but the enum can linger as an internal detail of the built-in classes until Phase 6); `@providers` annotation map + compile-time `is_valid_provider` → *any class implementing `HttpProvider` is valid* (Phase 1 removes the closed-set check).
4. **Back-compat test:** the existing LLM test corpus must pass unchanged through Phase 1–2 (behavior parity), with new tests added per phase.

---

## Part V — Risks & non-goals

- **B2 shallow portability is inherent** — do not promise it away. Two providers can type-check identically and behave/cost differently. Mitigation is visibility (normalize where honest, document where not), not erasure.
- **D3 (static capability guarantee) is a type-system decision**, not LLM-lib-local. If interface intersection types aren't feasible soon, v1 ships runtime-only and the guarantee is the "drop-the-sugar, concrete-return" escape hatch. Decide the *syntax* early regardless.
- **P1 (generic type aliases) may slip** — the inline-union fallback keeps the model correct at an ergonomic cost; don't let it block Phase 1.
- **Host-surface tail (P8)** — duplex transport, inbound webhooks, resumable stream offsets are real engineering; they gate Phases 4–5 only, so the spine (1–3) is unblocked.
- **§3 one-offs** (SigV4/header ordering, browser secret taint, MCP tool-poisoning, permission-reply down-channel) are per-feature; log them in the owning phase, don't let them expand scope.
- **Non-goals for this plan:** compile-time information-flow/taint typing (agent security), a distributed workflow scheduler, and dependent typing of tool args. All are acknowledged in the gap analysis as beyond the model's reach.

---

## Appendix A — Current code change map

Ranked by centrality to the redesign (from the code audit; paths under `crates/`):

1. `sys_llm/src/provider.rs` — the closed `LlmProvider` enum (`:5-58`); source of the closed set.
2. `sys_llm/src/build_request/mod.rs` (+ arms `anthropic.rs`, `bedrock.rs`, `google.rs`, `openai/*`) — the `match LlmProvider::from_str` dispatch (`mod.rs:31`) and HTTP-request building.
3. `sys_llm/src/parse_response/mod.rs` (+arms) and `sys_llm/src/auth_request/mod.rs` (+`vertex.rs`, `bedrock.rs`) — the other two exhaustive provider matches.
4. `sys_llm/src/baml_std.rs` — `PrimitiveClient::new` check (`:36`), `apply_provider_defaults` (`:157-293`), `ProviderOptions` enum + `resolve_provider_options` (`:109-146`).
5. `baml_builtins2/baml_std/baml/ns_llm/llm_types.baml` — `PrimitiveClient` (`:629-711`), `PrimitiveClientOptions` + `provider_options` union (`:453-471`), per-provider option classes + `@providers` annotations (`:426-510`), `Client`/`ClientType`/`RetryPolicy` strategy logic (`:78-425`), `Stream` (`:564-627`), `PromptAst` (`:1-3`).
6. `baml_builtins2/baml_std/baml/ns_llm/llm.baml` — `ExecutionContext` (`:6-14`), call/stream entry points, `PlannerState`.
7. `baml_compiler2_ast/src/lower_cst.rs` — `synthesize_client_items`/`_let`/`_new_companion` (`:2393+`), `is_valid_provider` (`:2917`), `provider_config_for` (`:2910`), `append_default_client_param` (`:611-633`).
8. `baml_compiler2_ast/build.rs` — `extract_provider_configs` / `@providers` codegen.
9. `sys_ops/src/lib.rs` — `IoClassLlmClient` dispatch (`build_request` `:224`, `parse` `:264`, `render_prompt` `:164`, `__sap_parse_*` `:747-786`, `from_shorthand` `:711`).
10. `sys_llm/src/lib.rs` — `execute_*` owned entry points; `execute_sap_parse_*` (`:975`, `:1000`).
11. `baml_compiler_parser/src/parser.rs` — `client<llm>` recognition (`:1110-1127`) — only if surface syntax changes.
12. **Likely unchanged (build on, don't replace):** `bex_sap/*` (provider-agnostic structured parsing), `ns_http/*.baml` + host HTTP impls (general primitives).

## Appendix B — Language-feature readiness

| Feature | Status | Evidence |
|---|---|---|
| `interface` + `requires` | ✅ shipped | `ns_iter/iter.baml:3,10`; `interfaces.rs` |
| Multiple `implements` blocks/class | ✅ shipped | `ns_iter/iter.baml:141,148,498,507` |
| Associated types (BEP-057) incl. defaults | ✅ shipped | `ns_iter/iter.baml:4-5,142,149`; `interfaces_associated_types.rs` |
| Interface default methods | ✅ shipped | `ns_iter/iter.baml:16-126` |
| Interface-membership `match` + `reflect` | ✅ shipped | `interfaces.rs:1144-1310`; `reflect/reflect.baml:15`; `type_class.baml:30` |
| Generics (class/interface/method) | ✅ shipped | `ns_iter/iter.baml:16,129,133`; `interfaces_class_generics.rs` |
| `throws`/`catch`/union errors | ✅ mechanism shipped | `ns_iter/iter.baml:16,247`; `ns_catch_arm_return`, `ns_spawn_throws` |
| `spawn`/`await`/`Future`/`TaskGroup`/`CancelToken` | ✅ shipped | `bytecode.rs:456-486`; `ns_future/future.baml`; `ns_spawn/spawn.baml`; `spawn_*.rs` |
| `type` as param/return/local | ✅ shipped | `reflect.baml:8`; `ns_reflect_type_of` |
| `defer` + magic `cleanup()` finalizer (BEP-042) | ✅ shipped | `parser.rs:4551` (defer, all exits incl. error); `cleanup_guard.rs:1` (at-most-once); `lexical_scoping.baml:52` |
| Generic type alias `Name<E>` | ❌ **net-new (P1)** | `ast.rs:1776` (no type-param field); 0 `.baml` uses |
| `UnknownError`/`CallError` stdlib | ❌ **net-new (P2)** | 0 hits in `baml_std` |
| `client` as `function→Provider` sugar | ⚠️ **redesign (P3)** | `ClientDef` config block, `ast.rs:1783` |
| `type` as stored class field | ⚠️ **unverified (P4)** | proven elsewhere; field position untested |

## Appendix C — Scenario → phase coverage

| Phase | Scenarios |
|---|---|
| 1 — Spine | 01, 02, 28, 35(partial) |
| 2 — Streaming/combinators/sidecar | 04, 29, 30, 31(partial), 32, 34 |
| 3 — Tools | 09, 10, 11, 12, 13, 14, 15, 16 |
| 4 — Realtime/harness | 22, 23, 24, 25, 26, 37, 38, 39, 40, 41, 42 |
| 5 — Stateful/workflows | 17, 18, 19, 20, 21, 27, 33, 43, 44, 45, 46, 47 |
| Cross-cutting (03,05,06,07,08,36) | land with their owning capability (structured/multimodal/reasoning/enriched/negotiation) across Phases 1–3 |
