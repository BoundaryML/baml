# The value + sidecar model (`call_with` + `ResponseMeta`)

How a provider hands back **an answer *and* its out-of-band metadata** (token usage, timings,
logprobs, citations, grounding, a reasoning block, a revised prompt, a "this was a replayed echo"
bit) without inventing a parallel companion method per sidecar. This is the proposed resolution to
**P1** in [`scenarios/_gap-analysis.md`](scenarios/_gap-analysis.md) (*"`call<T> -> T` is too narrow
to be the irreducible truth — the value+sidecar problem"*).

Builds on [`01-providers-clients-capabilities.md`](01-providers-clients-capabilities.md) (the spine)
and reuses the exact shape of [`error-model.md`](error-model.md) (interface + concrete impls +
external impls + `Supported`/`Unsupported`).

---

## The problem (P1)

The base method returns exactly `T`. But a huge fraction of real responses are *value + metadata*,
and none of it fits `-> T`. Today each scenario bolts on a parallel companion (`call_metered`,
`.think`, `.logprobs`) returning a bespoke `Wrapper<T>` (`Metered<T>`, `Scored<T>`,
`WithReasoning<T>`, …), each duplicating send/parse plumbing and — critically — **silently dropping
the sidecar when routed through the inherited `.call`** (a `Fallback` that forwards `Metered`
degrades to zero-usage). The fix is to put metadata on the **base contract** so combinators forward
it, and to give it **one normalized, extensible shape** instead of N wrappers.

## Thesis

1. **`call<T> -> T` stays** (the 90% path, zero tax). Add **one** companion on the base:
   `call_with<T, U>(prompt, project: (ResponseMeta) -> U) -> (T, U)`. Because it lives on the base,
   every combinator forwards it — no silent drop.
2. **`ResponseMeta` is an *interface*, not a record.** Concrete provider types (`OpenAIResponse`,
   `AnthropicResponse`) `implement` it by normalizing their wire response *lazily*. Framework facts
   (timing, attempt, replayed) are added by a wrapper that **forwards** provider dimensions and
   **answers** its own. This is the same interface/concrete/external-impl pattern as the error model.
3. **`call_with` is for *product* sidecars only** (value **and** metadata). Outcomes that are *sums*
   (value **or** something — refusal, suspend, tool-calls, async handle) or *stateful round-trips*
   (reasoning threaded back, chain/session handles) are **explicitly out of scope** — they belong on
   the `throws` channel, on `T | Sentinel` returns, or on their own capabilities. `call_with` does
   not widen to cover them, and pretending it does is the mistake P1 invites.

---

## 1. The base contract

```baml
interface Provider {
  // The metadata path — the PRIMITIVE. `project` reads the normalized ResponseMeta; U is whatever the
  // caller wants. On the BASE so combinators forward it — this is the actual P1 fix.
  // `project` runs inside this method's CallError channel and MAY throw on it (a lazy accessor can fail).
  function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError>

  // The 90% path — DERIVED from call_with (project nothing, drop the sidecar). A combinator that
  // implements Provider directly therefore need only define call_with; HttpProvider overrides both.
  function call<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
    v
  }
}
```

`call_with` is the primitive; `call` derives from it (so a bare-`Provider` combinator defines only
`call_with`). A combinator forwards `call_with` structurally, so usage/timings/etc. survive
`Fallback`/`Retry`/`Cache`:

```baml
class Fallback {
  members: Provider[]
  implements Provider {
    function call_with<T,U>(self, prompt, project: (ResponseMeta) -> U) -> (T,U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.members) {
        let r: (T,U) = p.call_with<T,U>(prompt, project) catch (e) { _ => { continue; } };
        return r;                                  // metadata forwarded, NOT dropped — the P1 win
      }
      throw baml.errors.AllFailed { };
    }
  }
}
```

## 2. `ResponseMeta` is an interface; providers normalize lazily

```baml
interface ResponseMeta {
  function usage(self)         -> Usage
  function finish_reason(self) -> string
  function logprobs(self)      -> Supported<Logprob[]>
  function citations(self)     -> Supported<Citation[]>
  function reasoning(self)     -> Supported<string>
}

class OpenAIResponse {
  raw: baml.http.Response                          // holds the wire response; normalizes LAZILY
  implements ResponseMeta {
    function usage(self) -> Usage {
      Usage { input:  self.raw.json_path_int("usage.prompt_tokens"),
              output: self.raw.json_path_int("usage.completion_tokens") }
    }
    function finish_reason(self) -> string            { self.raw.json_path("choices.0.finish_reason") }
    function logprobs(self)      -> Supported<Logprob[]>  { parse_logprobs(self.raw.json_path("choices.0.logprobs")) }
    function citations(self)     -> Supported<Citation[]> { Unsupported { reason: "openai chat" } }
    function reasoning(self)     -> Supported<string>     { Unsupported { reason: "gpt-4" } }
  }
}

class AnthropicResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      Usage { input:  self.raw.json_path_int("usage.input_tokens"),
              output: self.raw.json_path_int("usage.output_tokens") }
    }
    function finish_reason(self) -> string            { self.raw.json_path("stop_reason") }
    function logprobs(self)      -> Supported<Logprob[]>  { Unsupported { reason: "anthropic: none" } }
    function citations(self)     -> Supported<Citation[]> { parse_citations(self.raw.json_path("content")) }
    function reasoning(self)     -> Supported<string>     { extract_thinking(self.raw.json_path("content")) }
  }
}
```

Interface beats a record here on three counts: **lazy** (`m => m.usage()` never parses logprobs),
**co-located** (normalizers live with the provider, not in a central struct everyone edits), and
**externally extensible** (§7).

## 3. Provider-half + framework-half compose by delegation

A provider can normalize only what is *in the response*. `timing` / `attempt` / `replayed` are facts
about the *call*, which the provider never sees. The interface composes the two sources cleanly: a
framework wrapper **forwards** provider dimensions and **answers** its own.

```baml
class FrameworkMeta {
  inner: ResponseMeta            // the provider's OpenAIResponse / AnthropicResponse
  measured_timing: Timing
  measured_attempt: int
  was_replayed: bool
  implements ResponseMeta {
    function usage(self)         -> Usage                 { self.inner.usage() }          // forward
    function finish_reason(self) -> string                { self.inner.finish_reason() }
    function logprobs(self)      -> Supported<Logprob[]>  { self.inner.logprobs() }
    function citations(self)     -> Supported<Citation[]> { self.inner.citations() }
    function reasoning(self)     -> Supported<string>     { self.inner.reasoning() }
    // ...plus its own framework dimensions (a Timed/Replayable interface — see §7)
  }
}

// framework-level call_with assembles the layers:
function call_with<T,U>(p: Provider, prompt, project: (ResponseMeta) -> U) -> (T,U)
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  let t0 = clock.now();
  let (value, pm): (T, ResponseMeta) = p.call_with_raw<T>(prompt);   // provider returns its own ResponseMeta
  let wrapped = FrameworkMeta { inner: pm, measured_timing: Timing { ms: clock.now() - t0 },
                                measured_attempt: 1, was_replayed: false };
  (value, project(wrapped))
}
```

Layers stack: a `CacheMeta` that sets `replayed = true` wraps a `FrameworkMeta` wraps an
`OpenAIResponse` — three layers, one `ResponseMeta` the caller projects.

## 4. `Supported<T>`: "can't" vs "empty"

```baml
type Supported<T> = T | Unsupported
class Unsupported { reason: string }
```

Without this, `m.logprobs() == null` conflates *"Anthropic cannot produce logprobs"* with *"logprobs
were empty this call"* — re-inheriting **P2** (capability-as-runtime-promise) per field. `Supported<T>`
makes the absence typed and explains itself.

## 5. The partition — what `call_with` is, and is not, for

`(T, U)` is a **product**: you got a `T` *and* a `U`. That is the right shape for sidecars that
*accompany* the answer, and the wrong shape for everything else. Running all of P1 through the code:

| P1 item | shape | mechanism |
|---|---|---|
| usage, timings, logprobs/citations/grounding, revised_prompt, warnings, replayed-bit, reasoning *text* | **product** (accompanies) | ✅ `call_with` + `ResponseMeta` |
| refusal | sum (no `T`) | `throws baml.errors.CallError` (`Refused`) — already off to the side |
| suspend, tool-calls | sum (`T \| Sentinel`) | sentinel return `T \| Suspend` / `T \| ToolCalls` (like `Iterator.Done`) |
| background / batch handle | sum (handle *replaces* answer) | a `submit -> Job<T>` method, not `(T, U)` |
| reasoning *threaded back*, chain/session handle | stateful round-trip | a capability (`Continuity`, `Chain`); the hard part is *state* (**P4**) |

The two failure shapes `call_with` must **not** absorb:

```baml
// SUSPEND — call_with's (T, U) PROMISES a T; a paused run has none. Use a sentinel:
function reenter<T>(self, snap: Snapshot) -> T | Suspend throws baml.ExtendUnknownError<baml.errors.SuspendError>

// BACKGROUND — the handle REPLACES the answer (it isn't ready). Asking call_with for it
// inverts T/U (the "answer" T can't be returned yet). Use a dedicated lifecycle method:
function submit<T>(self, prompt: baml.llm.PromptAst) -> Job<T> throws baml.ExtendUnknownError<baml.errors.BackgroundError>
```

And the one it *covers but does not finish* — reasoning: `m.reasoning()` hands you the **text** for
display, but re-sending a signed thinking block on the next turn is a stateful `Continuity` job that
no return-shape tweak expresses.

## 6. Net-new language surface required

- **Tuples `(T, U)`** — REQUIRED. The whole model returns `(T, U)`; `call_with_raw` returns
  `(T, ResponseMeta)`. *We need first-class tuple types and tuple destructuring* (`let (v, u) = …`).
  They already appear in the streaming folds (`StreamStep<(StreamAcc, baml.http.SseStream), …>` in
  scenario 04), so this is a shared dependency, not bespoke to metadata. (A named `Output<T, U>
  { value: T, extra: U }` struct is the fallback if tuples are rejected, at an ergonomic cost.)
- **Closures passed as method params** — `project: (ResponseMeta) -> U`. First-class function values
  handed into a method and invoked inside the provider/framework body.
- **`Supported<T> = T | Unsupported`** — a one-line std type (§4).
- **OQ5 keystone (already required by capabilities + errors): value-level `match` through wrappers.**
  Both the escape hatch (§8) and layer delegation depend on testing/threading a concrete type behind
  a wrapper. Nothing new is asked of OQ5 here — but `call_with` *also* rides on it.
- **Host timing** — `clock.now()` (or equivalent) for the framework-observed dimensions.
- **Streaming carries `ResponseMeta` too.** For the streaming path, usage/finish-reason arrive only at
  the end, so the stream's terminal value (`baml.llm.StreamDone` / the stream's `final`) must expose a
  `ResponseMeta` — the streaming analog of `meta_of(body)` — so `Streaming` consumers project sidecars
  the same way (`done.meta.usage()`). Net-new on the `Stream`/`StreamDone` surface (surfaced by 32).
- **A default `call` on the base.** `call` is a default method deriving from `call_with` (§1), so a
  combinator implementing `Provider` directly need only supply `call_with` (surfaced by 36).

## 7. One mechanism: metadata = errors = capabilities

Making `ResponseMeta` an interface collapses three subsystems onto one pattern:

```baml
// novel dimension added LATER, in a downstream package, with an EXTERNAL impl (Rust-like traits):
interface Grounded { function grounding(self) -> Source[] }
implements Grounded for AnthropicResponse { function grounding(self) -> Source[] { parse_sources(self.raw) } }

// the caller reaches it by runtime match — exactly the capability/error pattern:
let (ans, g) = call_with(Claude(), p, m => match (m) {
  let gm: Grounded => gm.grounding(),
  _                => [],
});
```

This is the **same fork** the error model faced — fat base interface (every response implements every
dimension, `Unsupported` when absent) vs. small interfaces reached by `match` — and it resolves the
**same way**: common dimensions in a base everyone implements, genuinely-novel dimensions as
external-impl interfaces. So capabilities (`Streaming`/`Tools`/…), errors (`CallError`/…), and
response metadata (`ResponseMeta`/`Grounded`/…) are now **one** interface + concrete + external-impl +
`Supported` mechanism. That consolidation is a first-class argument *for* the proposal.

## 8. Residual tensions (honest)

1. **Wrapper-transparency — for the escape hatch *and* external dimensions.** `m => match (m) { let o:
   OpenAIResponse => … }` silently misses, because `m` is a `FrameworkMeta` wrapping the
   `OpenAIResponse`, not the response itself — the *same* occlusion as `UnknownError` boxing a concrete
   error and `Fallback` hiding its member. The sharper version: `FrameworkMeta` forwards only the
   *known base* `ResponseMeta` methods, so an **external-impl dimension** (`Grounded`/`ReasoningMeta`
   added downstream) is *also* occluded — `match (m) { let g: Grounded => … }` misses through the
   wrapper even on a provider that supports it, silently yielding the `_`/`Unsupported` arm. Cure is the
   same for both: expose `inner` / probe through the layers (OQ5). This is the third place the model
   leans on wrapper-transparent `match`; metadata does not escape it.
2. **Fallback aggregate has no slot.** `project` runs over **one** `ResponseMeta` — the winner's. The
   tokens you burned on the members that failed first (you paid for them) have nowhere to go; the
   product shape carries the winner's view, not the chain's. Same aggregate-provenance gap as the
   error side.
3. **Per-field absence is a runtime promise.** `Supported<T>` makes "can't" typed and honest, but
   *which* dimensions a given client supports is still answered at runtime (`Unsupported` / a `match`
   miss), never by the static type of the client — P2, localized to metadata.
4. **`call_with` does not finish the stateful cases.** It hands you reasoning text / a handle as a
   value; threading reasoning back and the server-authoritative life of a handle (P4) are untouched.

None of these is introduced by the value+sidecar model — they are the model's *existing* tensions
(OQ5, aggregate provenance, P2, P4) reappearing, which is the point: the metadata path lives on the
same grain as everything else.

## 9. Downstream changes (what to update)

- **`scenarios/_conventions.md`** — add `call`/`call_with` to the `Provider` spine; add the
  `ResponseMeta` interface, `Supported<T>`/`Unsupported`, and the `FrameworkMeta` delegation pattern;
  note tuples as required surface.
- **`scenarios/_gap-analysis.md`** — recast **P1** as *resolved-with-a-named-mechanism* (this doc):
  `call_with` + `ResponseMeta` settles the product-sidecar pile; sums (refusal/suspend/background/
  tool-calls) and stateful round-trips (continuity/handles) are explicitly *not* P1's problem. Fold
  the metadata/error/capability **unification** into the assessment. Keep the residuals (§8) as
  cross-references to OQ5 / P2 / P4, not new pressures.
- **Scenarios that invented a bespoke `Wrapper<T>` companion** — 01 (`Metered`), 06 (usage/
  revised_prompt), 07 (reasoning *text* half), 08 (logprobs/citations/grounding), 32 (usage for
  tracing), 34 (metering), 36 (warnings), 47 (replayed-bit): replace the parallel companion with
  `call_with` + a `ResponseMeta` projection. Leave their *sum*/*stateful* parts (07 continuity, 20
  chain handle, 27/34 job/batch handle, 44 suspend) on their own mechanisms — those were never P1.
- **Net-new surface** — register **tuples** (and closures-as-params, `Supported<T>`, host `clock`) in
  the gap-analysis "New host / language primitives" list.
