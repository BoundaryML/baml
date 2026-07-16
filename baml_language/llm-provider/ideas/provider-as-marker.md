# Provider as a marker — interaction is per-capability

A revision of the spine in [`01-providers-clients-capabilities.md`](01-providers-clients-capabilities.md):
**`Provider` is not "the thing that can `call<T>`" — it is a bare composable *marker*, and every
interaction (`call`, `stream`, `run`, `run_tools`) belongs to the capability that owns its shape.**
This resolves **OQ1** (no degenerate `call` forced onto realtime/harness) and removes the
`call_with` base-contract ripple, at the cost of retiring 01's headline thesis. Composes with
[`error-model.md`](error-model.md) and [`value-sidecar-model.md`](value-sidecar-model.md) unchanged.

---

## The revision

01 §1 says: *"A `Provider` is the irreducible base — an interface with one method, `call<T>(prompt) -> T`."*
The 47 scenarios undercut that twice:

- **`call` is degenerate off the HTTP path (OQ1).** A realtime session's `call` is a "best-effort single
  turn over a duplex socket"; a harness's `call` is "run the whole subprocess and hope." We kept writing
  fake `call`s to satisfy the base. The honest move: realtime/harness should simply *not* claim `call`.
- **`call` is too narrow even on the HTTP path (P1).** Resolved by `call_with` — but putting `call_with`
  on the base made *every* provider and combinator grow it (the ripple), including ones that never do
  request/response.

**New thesis:** `Provider` is the irreducible **marker** — composable, the host for combinators, the type
`client` returns and capabilities `requires`. **What you can *do* is per-capability.** No interaction is
privileged or universal; `call` becomes just another capability you `match` for.

---

## 1. The marker + the interaction surfaces

```baml
// The irreducible MARKER. No interaction method. Hosts the combinator factories every provider inherits.
interface Provider {
  function with_retry(self, max: int)         -> Retry    { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV)            -> Cache    { Cache { inner: self, store: store } }
}

// Request/response OWNS call + call_with (+ the codec). CallError channel (error-model.md).
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError>
  function send(self, request: baml.http.Request) -> Body                          throws baml.ExtendUnknownError<baml.errors.CallError>
  function parse<T>(self, from: Body) -> T                                         throws baml.ExtendUnknownError<baml.errors.CallError>
  function meta_of(self, from: Body)  -> ResponseMeta                              throws baml.ExtendUnknownError<baml.errors.CallError>

  // call_with is the PRIMITIVE here (value-sidecar-model.md); call derives from it.
  function call_with<T,U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let body = self.send(self.build_request<T>(prompt));
    (self.parse<T>(body), project(self.meta_of(body)))
  }
  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
    let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
    v
  }
}

// Streaming owns stream + its own late-sidecar variant. StreamError channel.
interface Streaming requires Provider {
  function stream<TS,TF>(self, prompt: baml.llm.PromptAst) -> baml.llm.Stream<TS,TF>
      throws baml.ExtendUnknownError<baml.errors.StreamError>
  // the sidecar is LATE, so it rides the FINAL frame (not a tuple-at-start):
  function stream_with<TS,TF,U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U)
      -> baml.llm.Stream<TS, (TF, U)>
      throws baml.ExtendUnknownError<baml.errors.StreamError>
}

// Realtime owns run. No honest `call` to fake. RealtimeError channel.
interface Realtime requires Provider {
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
      throws baml.ExtendUnknownError<baml.errors.RealtimeError>
}

// Harness, Tools, etc. each own their surface — none forced to carry `call`.
```

`call`/`call_with` are no longer on the base. `Provider` carries only the combinator factories (which
return combinators that *are* capability-implementers, §3).

## 2. Every interaction is a companion `match` — including the basic call

With nothing privileged, the basic `Foo(args)` companion becomes a capability `match` like every other —
the §5 negotiation story applied *universally*. And the degrade chains **both ways** (a call can be served
by draining a stream; a stream by buffering a call):

```baml
// Foo(args)  — wants a single value. Prefer HttpProvider.call; else drain a Streaming provider.
function Foo(args: ...) -> T {
  let p = client();
  match (p) {
    let h: HttpProvider => h.call<T>(render_prompt(args)),
    let s: Streaming    => drain<T>(s.stream<T,T>(render_prompt(args))),   // value from a stream-only provider
    _                   => throw baml.errors.Unsupported { message: "client cannot produce a value" },
  }
}

// Foo.with(args, project)  — wants value + sidecar.
function Foo$with<U>(args: ..., project: (ResponseMeta) -> U) -> (T, U) {
  let p = client();
  match (p) {
    let h: HttpProvider => h.call_with<T,U>(render_prompt(args), project),
    _                   => throw baml.errors.Unsupported { message: "client has no request/response sidecar" },
  }
}

// Foo.stream(args)  — wants incremental. Prefer Streaming; else buffer an HttpProvider.call.
function Foo$stream<TS,TF>(args: ...) -> baml.llm.Stream<TS,TF> {
  let p = client();
  match (p) {
    let s: Streaming    => s.stream<TS,TF>(render_prompt(args)),
    let h: HttpProvider => buffer_as_stream<TS,TF>(h, render_prompt(args)),   // one-shot faked as a stream
    _                   => throw baml.errors.Unsupported { message: "client cannot stream" },
  }
}

// Foo.live(args, io) / Foo.run_tools(args, tools)  — NO degrade: a one-shot can't fake a duplex/loop.
function Foo$live(args: ..., io: Channel) -> Transcript {
  match (client()) {
    let r: Realtime => r.run(render_prompt(args), io),
    _               => throw baml.errors.Unsupported { message: "client is not realtime" },
  }
}
```

The split is the same as 01 §5: **delivery refinements** (call↔stream) degrade across the match;
**different interaction shapes** (realtime, tools) error. The only change is that `call` itself is now
*inside* this scheme rather than privileged beneath it.

## 3. Combinators implement the capabilities they forward (unchanged pattern)

A combinator `requires Provider` (gets the factories) and `implements` each capability it forwards by
runtime delegation — exactly the existing pattern, just no longer anchored to a base `call`:

```baml
class Fallback {
  strategy: Provider[]
  implements HttpProvider {            // forwards call / call_with to the first http-capable member
    type Body = unknown
    function call_with<T,U>(self, prompt, project: (ResponseMeta) -> U) -> (T,U)
        throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.strategy) {
        match (p) { let h: HttpProvider => { return h.call_with<T,U>(prompt, project); }, _ => {} }
      }
      throw baml.errors.AllFailed { };
    }
    // build_request/send/parse/meta_of/call inherited or delegated likewise
  }
  implements Streaming { /* route .stream to a streaming member */ }
  implements Realtime  { /* route .run to a realtime member */ }
}
```

A combinator statically *claims* every capability it forwards and routes/degrades at call time — the same
runtime-promise tradeoff 01 §6 already accepted. (And per the previous turn: `Fallback.call_with` can hand
`project` an `AggregateMeta` so usage/cost reflects the whole chain, not just the winner — combinators are
just code.)

## 4. What it resolves

- **OQ1 — closed as "no."** Realtime/harness/STT/TTS/Capabilities no longer `requires` a `call` they have to
  fake. A capability `requires Provider` (the marker) and adds *its own* interaction method. The repeated
  scenario complaint ("`call` is degenerate / dishonest here" — 22–26, 33, 36, 37–42) disappears.
- **The `call_with` base-contract ripple — gone.** Only `HttpProvider`s carry `call_with`. Harnesses,
  realtime providers, and combinators-over-non-http never grow it. The ~30 scenarios that "lacked"
  `call_with` were never request/response; there is nothing to add.
- **Uniformity.** `call` is just a capability; the §5 `match` is the *whole* interaction story, with no
  privileged base method sitting underneath it as a special case.

## 5. What it costs (named honestly)

- **Retires 01's headline thesis.** "A `Provider` is the irreducible base with one method `call<T>`" becomes
  "a `Provider` is the irreducible *marker*; interaction is per-capability." 01 §1 must be rewritten. This is
  the conclusion the scenarios already earned, but it *is* a change to the central claim.
- **"Degrade to `.call`" is now a companion `match`, not a free default.** Capabilities that leaned on a
  base `call` (e.g. `buffer_as_stream` calling `p.call`) now `match` for `HttpProvider` first. Honest (you can
  only buffer-as-stream something that *has* a call), but the degrade lives in the companion, not in a
  `requires`-inherited default.
- **Basic call is no longer guaranteed.** `Foo(args)` can `throw Unsupported` for a client that is
  pure-realtime/pure-harness — the type system won't stop you wiring such a client into a value function.
  Same runtime-promise property as every other capability (P2); now it touches the basic case too.

## 6. Open design decisions

1. **`requires Provider` (marker) vs `requires HttpProvider` for capabilities that degrade.** Recommendation:
   keep all capabilities `requires Provider` (the bare marker) and put the degrade in the *companion* `match`
   (§2). This keeps capabilities decoupled — `Streaming` doesn't force `HttpProvider` — and lets the degrade
   chain both directions (call↔stream). The alternative (`Streaming requires HttpProvider`) hard-couples the
   `requires` graph and forbids a stream-only provider.
2. **Should basic `call` degrade to draining a `Streaming`-only provider?** §2 says yes (symmetry: a value is
   a fully-drained stream). Cheap and honest. Confirm it's wanted, or make `Foo(args)` http-only.
3. **Where do the combinator factories live?** Shown on the `Provider` marker so every provider inherits
   `with_retry`/`fallback_to`. Fine as the marker's only members; alternatively they move to a separate
   `Composable` interface. Marker-hosts-factories is simplest.

## 7. Composes with the other two models

- **Error model — unchanged.** Channels are already per-capability: `call`/`call_with` on `CallError`,
  `stream`/`stream_with` on `StreamError`, `run` on `RealtimeError`. Moving the methods onto their owning
  capability *aligns* the method and its channel (each capability = its methods + its error interface).
- **Value+sidecar — unchanged, just relocated.** `call_with` + `ResponseMeta` live on `HttpProvider`;
  `stream_with` is the streaming analog (sidecar on the final frame, §1). `Supported<T>`, `FrameworkMeta`
  delegation, and the partition (product → `*_with`; sum → throws/sentinel; stateful → capability) all hold.

## 8. Downstream changes (what to update)

- **`01-providers-clients-capabilities.md` §1** — rewrite the thesis: marker base, interaction per-capability.
  §5 (companion `match`) becomes the universal interaction story, not a refinement on top of `call`.
- **`scenarios/_conventions.md`** — move `call`/`call_with` off `Provider` onto `HttpProvider`; make `Provider`
  the marker (combinator factories only); add `Streaming.stream_with`; show the §2 companion-as-match desugar
  (with the call↔stream degrade).
- **`scenarios/_gap-analysis.md`** — record **OQ1 → resolved** ("no — pure-I/O and pure-metadata capabilities
  do not `requires` a `call`; `Provider` is a marker"); strike the `call_with` base-contract ripple note (moot);
  fold the "degenerate `call`" entries in P2/§7 of Part E into "resolved by marker-`Provider`."
- **Scenarios** — realtime (22–26), harness (37–42), STT/TTS (25), Capabilities (36), Deterministic (33) drop
  their fake/degenerate `call`; they `requires Provider` and expose only their real interaction method. The 8
  value+sidecar scenarios keep `call_with` (now via `HttpProvider`). Combinator scenarios keep implementing the
  capabilities they forward (unchanged).
