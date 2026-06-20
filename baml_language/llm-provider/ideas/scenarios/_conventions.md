# Scenario conventions — the canonical primitives

> This is the shared cheat-sheet every scenario in this folder builds on. It is a
> distillation of [`../01-providers-clients-capabilities.md`](../01-providers-clients-capabilities.md)
> (the authoritative proposal). When a scenario needs a primitive not listed here, it
> *introduces* it explicitly in its `implement.baml` and flags it in `evaluation.md`
> as **net-new surface the proposal must add**.

Each scenario folder contains:

- **`implement.baml`** — the provider classes, capability `interface`s, and combinators that make the scenario work. This is library/std-lib-author code.
- **`usage.baml`** — the `client` blocks and `function`s an *application* author writes to use it. This is end-user code.
- **`evaluation.md`** — does the proposed model actually express this cleanly? What is reused unchanged, what is net-new, what is awkward or unresolved. Adversarial: stress the proposal, don't sell it.
- **`README.md`** — one-paragraph orientation + the background source it maps to.

---

## The spine (assume these exist, exactly as written)

```baml
// The irreducible MARKER (see ../provider-as-marker.md). A `Provider` carries NO interaction method —
// `call`/`stream`/`run` each belong to the capability that owns that shape. `Provider` is the type
// `client` returns, the bound capabilities `requires`, and the host for the inherited combinator
// factories. "What you can DO" is per-capability; nothing is privileged or universal.
interface Provider {
  // Combinators — default methods inherited by EVERY provider (the Iterator.map pattern).
  function with_retry(self, max: int) -> Retry { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV) -> Cache { Cache { inner: self, store: store } }
}

// HTTP request/response codec — most chat models. OWNS call + call_with. CallError channel.
// build_request / send / parse / meta_of are all on the call path.
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function send(self, request: baml.http.Request) -> Body
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function parse<T>(self, from: Body) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError>
  function meta_of(self, from: Body) -> ResponseMeta            // normalize the sidecar (lazy)
      throws baml.ExtendUnknownError<baml.errors.CallError>

  // call_with is the PRIMITIVE value+sidecar method (see "Value + sidecar model" below); `project`
  // reads the normalized ResponseMeta, runs inside this CallError channel, and MAY throw on it.
  function call_with<T, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U) -> (T, U)
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let body = self.send(self.build_request<T>(prompt));
    (self.parse<T>(body), project(self.meta_of(body)))
  }
  // `call` DERIVES from call_with (project nothing, drop the sidecar).
  function call<T>(self, prompt: baml.llm.PromptAst) -> T
      throws baml.ExtendUnknownError<baml.errors.CallError> {
    let (v, _): (T, null) = self.call_with<T, null>(prompt, m => null);
    v
  }
}

// Incremental output → the StreamError channel. OWNS stream + the late-sidecar stream_with.
interface Streaming requires Provider {
  function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
      -> baml.llm.Stream<TStream, TFinal>
      throws baml.ExtendUnknownError<baml.errors.StreamError>
  // the sidecar is LATE (usage/finish-reason land at the end), so it rides the FINAL frame:
  function stream_with<TStream, TFinal, U>(self, prompt: baml.llm.PromptAst, project: (ResponseMeta) -> U)
      -> baml.llm.Stream<TStream, (TFinal, U)>
      throws baml.ExtendUnknownError<baml.errors.StreamError>
}

// A live, duplex interaction. The caller hands in a Channel (pass-in); the provider drives it.
interface Realtime requires Provider {
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
      throws baml.ExtendUnknownError<baml.errors.RealtimeError>
}

// The multi-turn tool loop is a capability → the ToolError channel. Transcript is provider-owned + opaque.
interface Tools requires Provider {
  type Transcript
  function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript
      throws baml.ExtendUnknownError<baml.errors.ToolError>
  function step<T>(self, t: Transcript) -> T | ToolCalls
      throws baml.ExtendUnknownError<baml.errors.ToolError>   // mirror of Iterator.next -> Item | Done
  function submit(self, t: Transcript, results: ToolResult[]) -> Transcript
      throws baml.ExtendUnknownError<baml.errors.ToolError>
  function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T
      throws baml.ExtendUnknownError<baml.errors.ToolError> {
    let t = self.begin<T>(prompt, tools);
    while (true) {
      match (self.step<T>(t)) {
        let calls: ToolCalls => { t = self.submit(t, ctx.dispatch(calls.calls)); },
        let value: T => { return value; },
      }
    }
    baml.sys.panic("unreachable")
  }
}

// Expose the built request without sending it (preview / testing). Building is on the call path.
interface Inspectable requires Provider {
  function request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>
}
```

Supporting types referenced above:

```baml
class ToolCall   { id: string, name: string, args: map<string, unknown> }  // id OPAQUE to the loop
class ToolResult { id: string, output: unknown }
class ToolCalls  { calls: ToolCall[] }                                      // sentinel, like Iterator's Done
class Tool       { name: string, description: string, parameters: type }   // uses the first-class `type`

interface Channel {
  function on(self, handler: (InEvent) -> void) -> null
  function send(self, data: OutEvent) -> null throws baml.errors.Io
  function close(self) -> null
}

// baml.http (ns_http/http.baml) — REAL, verified against baml_std. NOTE: Response has NO `.json_path`
// (that was invented); read JSON with `baml.json.parse(resp.text())` + navigation, or `baml.json.from_json<T>`.
class Request  { method: string, url: string, headers: map<string, string>, body: string }
class Response { status_code: int, headers: map<string, string>, _body: $rust_type }
                 // .text() -> string throws Io ; .bytes() -> uint8array ; .ok() -> bool
class SseStream{ _handle: $rust_type }   // .next() -> string? throws Io ; .close()
function send(request: Request, timeout: baml.time.Duration? = null) -> Response throws baml.errors.Io | baml.errors.Timeout
function fetch_sse(request: Request) -> SseStream throws baml.errors.Io | baml.errors.Timeout
```

---

## Error model (canonical — distilled from [`../error-model.md`](../error-model.md))

This **supersedes** the old `type Error = baml.errors.LlmClient` associated-type approach. There is
no `baml.errors.LlmClient` anymore. The channel is uniform per *capability*, not per *provider*.

```baml
// package baml — the universal wrapper. NON-GENERIC on purpose, so `E | UnknownError` collapses
// across layers instead of exploding into UnknownError<A> | UnknownError<B> | …
class baml.UnknownError {
  data: unknown        // the original thrown value, untouched
  message: string[]    // breadcrumb of context, accumulated as it bubbles up

  // Reassert the channel: a known T passes through; an UnknownError already wrapping a T is
  // unwrapped back to T; anything else is wrapped fresh.
  function from<T>(data: unknown) -> T | baml.UnknownError {
    match data {
      T => return data;
      Self { data: let inner: T } => return inner;
      _ => return baml.UnknownError { data: data, message: [] };
    }
  }
  // Same, but annotate context. KNOWN errors are NOT annotated (they keep their identity);
  // only unknown / already-wrapped values accumulate a message.
  function with_message<T>(data: unknown, message: string) -> T | baml.UnknownError {
    match data {
      T => return data;
      Self { data: let inner: T } => return inner;
      Self => { data.message.push(message); return data; }
      _ => return baml.UnknownError { data: data, message: [message] };
    }
  }
}

// type alias — the channel every fallible capability method declares.
type baml.ExtendUnknownError<E> = E | baml.UnknownError

// baml.errors — ONE capability error interface per capability (NOT a hierarchy). Each carries
// common classification methods so callers triage WITHOUT knowing the concrete class.
interface baml.errors.CallError {
  function is_network_error(self) -> bool
  function is_rate_limit(self) -> bool
  function is_parse_error(self) -> bool
}
interface baml.errors.StreamError   { function is_network_error(self) -> bool /* … */ }
interface baml.errors.ToolError     { /* classifiers as needed */ }
interface baml.errors.RealtimeError { /* classifiers as needed */ }

// Capability-negotiation error: thrown by a companion's `_` arm when the client's provider does
// NOT implement the requested capability. It implements ALL capability-error interfaces, so it is
// legal on ANY capability channel and is caught by its concrete type.
class baml.errors.Unsupported {
  message: string
  implements baml.errors.CallError     { /* all classifiers false */ }
  implements baml.errors.StreamError   { /* … */ }
  implements baml.errors.ToolError     { /* … */ }
  implements baml.errors.RealtimeError { /* … */ }
}
```

**Channel each capability method declares** (drop `type Error`; write the channel explicitly):

| capability method | channel |
|---|---|
| `HttpProvider.{call,call_with,build_request,send,parse,meta_of}`, `Inspectable.request` | `baml.ExtendUnknownError<baml.errors.CallError>` |
| `Streaming.{stream,stream_with}` (+ the stream's `final`) | `baml.ExtendUnknownError<baml.errors.StreamError>` |
| `Realtime.run` | `baml.ExtendUnknownError<baml.errors.RealtimeError>` |
| `Tools.{begin,step,submit,run_tools}` | `baml.ExtendUnknownError<baml.errors.ToolError>` |
| any NEW capability | a NEW `baml.errors.<Cap>Error` interface, same shape (flag it net-new in `evaluation.md`) |

**Concrete provider errors** `implement` whichever capability interfaces apply (the *same* error
often arises on the call path *and* mid-stream, so it implements both):

```baml
// A refusal: HTTP 200, no usable T, the decline lands in a side field (Anthropic stop_reason
// "refusal" / OpenAI message.refusal / Gemini finishReason "SAFETY"). It is neither the answer
// nor a transport failure — a distinct typed error the app branches on.
class Refused {
  category: string
  message: string
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { return false; }
    function is_rate_limit(self)    -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* same error can surface mid-stream */ }
}

// A classified transport failure (HTTP 429). Use a concrete class when classification matters;
// otherwise let the trailing `with_message` catch route the foreign Io/Timeout into UnknownError.
class OpenAiRateLimitError {
  retry_after_secs: int
  implements baml.errors.CallError {
    function is_rate_limit(self)    -> bool { return true; }
    function is_network_error(self) -> bool { return false; }
    function is_parse_error(self)   -> bool { return false; }
  }
  implements baml.errors.StreamError { /* … */ }
}
```

**Error hygiene — keep crossings typed with `implements` (a Rust-like trait relation).** Boxing into
`baml.UnknownError` at a capability boundary happens *only when a thrown error does not implement the
destination channel's interface*. Two `implements` patterns keep crossings typed, and BAML allows
**external impls in both directions** (implement a parent-package interface for your type, *and* implement
your interface for a parent-package type — like Rust trait coherence):

- **Implement the common interfaces on your errors.** stdlib errors (`baml.errors.Io`/`Timeout`,
  `baml.http.Error`) already implement all four core interfaces; do the same for bespoke errors (the
  canonical `Refused`/rate-limit implement both `CallError` and `StreamError`). So `Refused` passes through
  the `buffer_as_stream` degrade and a fallback's `.stream` *unboxed*, and `let r: Refused =>` matches directly.
- **Implement *your* channel's interface for the upstream errors you forward.** A capability that defines a
  net-new channel (`baml.errors.StepError`) can declare, in its own package,
  `implements baml.errors.StepError for baml.errors.Refused` (local trait, foreign type) — and likewise
  `implements baml.errors.StepError for baml.errors.Unsupported`. Now an upstream `Refused`/`Unsupported`
  rides the `StepError` channel as its typed self, caught concretely, **no box**. This dissolves the
  "forward-reference" worry: a downstream channel can retro-admit upstream error types.

So minimizing boxing is a **coverage convention**, not a wall — either side of a boundary can supply the
`implements` rows. Where no row exists a crossing still boxes (data survives in `.data`, recover with an
explicit `from<…>` probe, below); and a *genuinely foreign* value (host panic, unmapped JSON) has no inner
typed error, so the `baml.UnknownError` escape hatch is correct there.

**The five rules** (verbatim from `error-model.md`):
1. Every fallible method's channel is `baml.ExtendUnknownError<CapErr>` = `CapErr | baml.UnknownError`.
2. A throw is legal **iff** it `implements` the capability interface **or** is routed through `baml.UnknownError`.
3. Normalize foreign errors (`baml.errors.Io`/`Timeout` from `baml.http.send`, host panics, JSON failures) with a trailing
   `catch (e) { _ => throw baml.UnknownError.with_message<CapErr>(e, "<provider> <op> failed"); }`. Because `with_message<CapErr>`
   returns a known `CapErr` unchanged, this trailing catch-all NEVER swallows a deliberately-thrown typed error (`Refused`, a rate-limit).
4. Combinators (`Fallback`, `Retry`, `Cache`, …) declare the **SAME** channel — no narrow / widen / forward. Concreteness is
   recovered at the consumer's `catch`, so combinators never do error-type variance gymnastics.
5. Consumers (`usage.baml`) catch by **runtime match, most-specific first**: concrete (`Refused`, `OpenAiRateLimitError`) →
   interface (`baml.errors.CallError`, using `is_network_error()`/`is_rate_limit()`) → `baml.UnknownError` (the escape hatch).

**Canonical method body** (the shape every fallible provider method takes):

```baml
function call<T>(self, prompt: baml.llm.PromptAst) -> T
    throws baml.ExtendUnknownError<baml.errors.CallError> {
  if (rate_limited) { throw OpenAiRateLimitError { retry_after_secs: 5 }; }   // concrete CallError
  let resp = self.send(self.build_request<T>(prompt));                        // foreign Io possible
  self.parse<T>(resp)                                                         // may throw Refused
} catch (e) {
  _ => throw baml.UnknownError.with_message<baml.errors.CallError>(e, "openai call failed");
}
```

**Canonical consumer catch** (`usage.baml`):

```baml
Summarize(work) catch (e) {
  let r: Refused                  => "[declined: " + r.category + "]";   // concrete, when known
  let rl: OpenAiRateLimitError    => backoff(rl.retry_after_secs);       // concrete, when known
  let c: baml.errors.CallError    => { if (c.is_network_error()) { retry(); } else { fail(); } }
  let u: baml.UnknownError        => report(u.message);                  // the escape hatch
}
```

**Recovering a boxed error across a capability boundary (settled rule).** A `catch` arm tests the
runtime type of `e` *itself* — it does **not** auto-unwrap. When a typed error crosses into a
*different* capability's channel (a combinator/harness/workflow whose method delegates into a method
on another channel), rule 3's `with_message<OuterErr>` boxes it into `baml.UnknownError` (because it
does not implement `OuterErr`), preserving the original untouched in `.data`. The down-stack error is
therefore **not** matched by a plain `let c: baml.errors.CallError =>` arm (the value is an
`UnknownError`); recover it **explicitly** inside the `UnknownError` arm:

```baml
RunStep(input) catch (e) {                                   // channel is StepError | baml.UnknownError
  let s: baml.errors.StepError => handleStep(s);
  let u: baml.UnknownError     => {
    // a CallError raised three frames down was boxed here — re-assert to pull it back out:
    match baml.UnknownError.from<baml.errors.CallError>(e) {
      let c: baml.errors.CallError => { if (c.is_rate_limit()) { backoff(); } else { fail(c); } }
      _                            => report(u.message);     // genuinely foreign — only the breadcrumb
    }
  }
}
```

The data survives; the *static* channel does not advertise that a `CallError` is reachable inside, so
this recovery is by convention (you must know to probe), not type-directed — the error-channel form of
"capability is a runtime promise."

---

## Value + sidecar model (canonical — distilled from [`../value-sidecar-model.md`](../value-sidecar-model.md))

A response is often *answer + out-of-band metadata* (usage, timings, logprobs, citations, grounding,
reasoning text, revised_prompt, a replayed-echo bit). `call<T> -> T` has no room for it, so metadata
rides a sibling method on **`HttpProvider`**, `call_with` (and `Streaming.stream_with` for the streaming
path — see [`../provider-as-marker.md`](../provider-as-marker.md)), which returns a **tuple `(T, U)`** —
and **`ResponseMeta` is an interface** (same interface/concrete/external-impl/`Supported` shape as the error model).

```baml
// ResponseMeta is an INTERFACE; concrete provider types normalize their wire response LAZILY.
interface ResponseMeta {
  function usage(self)         -> Usage
  function finish_reason(self) -> string
  function logprobs(self)      -> Supported<Logprob[]>     // "can't" is typed, not null
  function citations(self)     -> Supported<Citation[]>
  function reasoning(self)     -> Supported<string>
}
type Supported<T> = T | Unsupported                        // "this provider CAN'T" vs "empty this call"
class Unsupported { reason: string }

class OpenAIResponse {
  raw: baml.http.Response
  implements ResponseMeta {
    function usage(self) -> Usage {
      Usage { input: self.raw.json_path_int("usage.prompt_tokens"),
              output: self.raw.json_path_int("usage.completion_tokens") }
    }
    function logprobs(self)  -> Supported<Logprob[]> { parse_logprobs(self.raw.json_path("choices.0.logprobs")) }
    function citations(self) -> Supported<Citation[]> { Unsupported { reason: "openai chat" } }
    function reasoning(self) -> Supported<string>     { Unsupported { reason: "gpt-4" } }
    function finish_reason(self) -> string            { self.raw.json_path("choices.0.finish_reason") }
  }
}

// Framework-observed dimensions (timing/attempt/replayed) compose by DELEGATION: a wrapper forwards
// the provider dimensions and answers its own. (Layers stack: a CacheMeta over a FrameworkMeta over OpenAIResponse.)
class FrameworkMeta {
  inner: ResponseMeta
  measured_timing: Timing
  was_replayed: bool
  implements ResponseMeta {
    function usage(self)         -> Usage                 { self.inner.usage() }          // forward
    function finish_reason(self) -> string                { self.inner.finish_reason() }
    function logprobs(self)      -> Supported<Logprob[]>  { self.inner.logprobs() }
    function citations(self)     -> Supported<Citation[]> { self.inner.citations() }
    function reasoning(self)     -> Supported<string>     { self.inner.reasoning() }
  }
}
```

App-side: project exactly what you want; combinators forward it (no silent drop), and refusal stays on `throws`:

```baml
let (answer, usage): (string, Usage) = Summarize.call_with(work, m => m.usage())
  catch (e) { let r: Refused => /* refusal is on the throws channel, not in U */ ... };

// compose sidecars in one call via a tuple:
let (ans, extras) = Extract.call_with(doc, m => (m.usage(), m.logprobs()));
```

**Scope (the partition — `call_with` is for *product* sidecars only):**
- **In scope (value AND metadata):** usage, timings, logprobs, citations, grounding, reasoning *text*, revised_prompt, warnings, replayed-bit → `call_with` + `ResponseMeta`.
- **NOT `call_with` — *sum* outcomes (value OR something):** refusal → the `throws` channel (`Refused`); suspend / tool-calls → a sentinel return `T | Suspend` / `T | ToolCalls`; background/batch → a `submit -> Job<T>` method (the handle *replaces* the answer). `(T, U)` promises a `T`; these have none.
- **NOT `call_with` — *stateful round-trips*:** reasoning threaded back into the next turn, chain/session handles → their own capability (`Continuity`, `Chain`); the hard part is *state* (P4), not the carrier shape.

**Net-new language surface this needs:** **tuples `(T, U)`** + destructuring (REQUIRED; also used by the streaming folds), **closures as method params** (`project: (ResponseMeta) -> U`), the `Supported<T>` std type, and a host `clock`. The escape hatch (`match (m) { let o: OpenAIResponse => o.raw... }`) and layer delegation both ride the same **OQ5 wrapper-transparent `match`** the rest of the model needs.

---

## Concurrency (BEP-034 — a real, SHIPPED language feature, *not* net-new)

BAML has first-class structured concurrency in the runtime today (`baml_std/baml/ns_spawn`, `ns_future`).
**Do not invent `baml.async.*`, `Semaphore`, `baml.sys.spawn`/`Task`, or `baml.sys.race_cancel`, and do not
claim "BAML has no concurrency surface" — it does.** Use the real primitives:

```baml
// `spawn { body }` starts `body` concurrently and returns a Future immediately. body is () -> T throws E.
let f: baml.future.Future<T, E> = spawn { do_work() };
let v: T = await f;                              // blocks until settled; yields T or RE-THROWS E

// Sys-ops (baml.http.send, fs reads, …) are NOT futures — they return directly and yield cooperatively.
// To run several concurrently, wrap each in spawn:
let a = spawn { baml.http.send(r1) };
let b = spawn { baml.http.send(r2) };
let both: (Resp, Resp) = (await a, await b);     // ran concurrently
```

**Future combinators** (`baml.future.*`, over a homogeneous `Future<T,E>[]`; inputs already running):
- `all(fs) -> Future<T[], E>` — `Promise.all`: values in input order; **cancels losers** on first failure, re-throws.
- `all_complete(fs) -> Future<T[], E>` — like `all` but losers **keep running** (when they have side effects that must finish).
- `race(fs) -> Future<T, E>` — first to settle (success OR failure) wins; losers cancelled (`Promise.race`).
- `any(fs) -> Future<T, AllFailed<E>>` — first **success** wins; if all fail, throws `baml.future.AllFailed<E> { errors: E[] }` (`Promise.any`).

`Future` methods: `.cancel() -> bool`, `.is_settled()`, `.is_result()`, `.is_error()`, `.is_cancelled()`, `.state() -> baml.future.FutureState` (`Pending|Ready|Error|Cancelled`).

**Rate limiting — `baml.spawn.TaskGroup`** (the real "Semaphore"): spawns sharing a group are capped; excess queue FIFO; each `spawn` still returns its `Future` immediately.
```baml
let g = baml.spawn.TaskGroup.new(5);                                              // cap concurrency at 5
let fs = items.map(i => spawn with baml.spawn.options(group = g) { handle(i) });
let results = await baml.future.all(fs);
// g.set_limit(n) · g.cancel(pending?, active?) -> int · g.active_count() · g.queued_count()
```

**Cooperative cancellation — `baml.spawn.CancelToken`** (one-shot; once fired, the task's next `await` throws `baml.panics.Cancelled`):
```baml
let tok = baml.spawn.CancelToken.new();
let f = spawn with baml.spawn.options(cancel = tok) { long_job() };
tok.cancel();                                                  // or f.cancel()
let r = (await f) catch (e) { baml.panics.Cancelled => fallback() };
// CancelToken.any([t1, t2]) fires when ANY input fires. `detach = true` opts a spawn out of the
// parent→child cancel cascade (errors route to the root task, not the spawner).
```

**Structured concurrency:** a spawn is a child of its spawner; cancelling/failing the parent cascades to children (unless `detach`). `baml.sys.sleep(ms)` is the cooperative sleep.

**Mapping invented names → the real model** (apply when fixing a scenario):

| invented (WRONG) | real (BEP-034) |
|---|---|
| `baml.async.gather` / `join_all` / `join2` | `spawn` each + `baml.future.all` (or `all_complete` if losers must finish) |
| `baml.async.gather_until_error` | `baml.future.all` (fail-fast — cancels losers) |
| `baml.async.join_settled` | `baml.future.all_complete` (losers keep running) |
| `baml.async.map_concurrent(xs, n, f)` | `let g = baml.spawn.TaskGroup.new(n); baml.future.all(xs.map(x => spawn with baml.spawn.options(group=g) { f(x) }))` |
| `Semaphore(n)` | `baml.spawn.TaskGroup.new(n)` |
| `baml.sys.spawn(body) -> Task` | `spawn { body } -> Future`; `Task.join()` → `await`; `Task.abort()` → `f.cancel()` |
| `baml.sys.race_cancel` | `baml.future.race` (+ a `CancelToken` for the trip signal) |

---

## Standard library — use the REAL spellings (verified against `baml_std/`)

Most "host primitives" the scenarios reach for **already exist** — usually as **methods** on `string` /
`Array` / `Map`, or under a correctly-named namespace. The invented free-function spellings (`baml.str.*`,
`baml.list.*`, `baml.json.obj`, `Response.json_path`, …) are **wrong**. Use these:

**Strings** — methods on `string` (NOT `baml.str.*` / `baml.string.*` / `baml.strings.*`):
| invented | real |
|---|---|
| `baml.str.concat(a, b)` | `a + b` |
| `baml.str.contains(s, x)` | `s.includes(x)` |
| `baml.str.eq(a, b)` | `a == b` |
| `baml.str.starts_with` / `ends_with` | `s.starts_with(x)` / `s.ends_with(x)` |
| `baml.str.replace` | `s.replace(a, b)` / `s.replace_all(a, b)` |
| `baml.str.{before,after,split}` | `s.split(sep)` + index |
| `baml.str.join(xs, sep)` | `xs.join(sep)` |
| `baml.string.from_int(i)` / `baml.str.to_int(s)` | `i.to_string()` / `int.from_string(s)` |

**Lists** — methods on `Array` (NOT `baml.arr` / `array` / `list` / `collections.*`):
| invented | real |
|---|---|
| `baml.list.push(xs, x)` | `xs.push(x)` |
| `baml.{list,array}.len(xs)` | `xs.length()` |
| `baml.list.concat(a, b)` | `a.concat(b)` |
| `baml.collections.map(xs, f)` | `xs.map(f)` |
| `baml.list.{at,take,tail,...}` | `xs.at(i)` / `xs.slice(...)` ; `.filter/.find/.reduce/.includes/.index_of/.for_each/.some/.every/.sort_by` are all `Array` methods |

**Maps** — methods on `Map` (NOT `baml.map.*`):
| invented | real |
|---|---|
| `baml.map.get(m, k)` / `get_str` | `m.get(k)` |
| `baml.map.set` / `insert` | `m.set(k, v)` / `m.insert(k, v)` |
| `baml.map.has` / `keys` / `values` / `merge` | `m.has(k)` / `m.keys()` / `m.values()` |
| `baml.sys.new_map()` | a `{}` map literal |

**JSON** — `baml.json` (real). `type json = null | bool | int | float | string | json[] | map<string, json>`.
Build with native literals + serialize; read with parse + navigation or typed coercion. (NOT the invented
`obj`/`object`/`arr`/`array`/`raw`/`get_str`/`get_int`/`encode`/`string`.)
| invented | real |
|---|---|
| `baml.json.object({...})` / `obj` / `encode(v)` | build a `map<string, json>` literal → `baml.json.stringify(m)`; or `baml.json.to_string<T>(v)` for a typed value |
| `baml.json.array([...])` / `arr` | a native `json[]` literal |
| `baml.json.string(s)` / `int(i)` / `bool(b)` / `null` | the bare value — `s` / `i` / `b` / `null` ARE `json` |
| `baml.json.raw(serialized)` (nest a fragment) | nest the `json` value directly — no re-stringify |
| `from.json_path("a.b.c")` / `baml.json.get_str(raw, p)` | `baml.json.parse(resp.text())` then navigate the `map`/`json[]`; **preferred:** `baml.json.from_json<RespClass>(resp.text())` into a typed class and read fields |
| `baml.json.{parse_map,merge,has,len}` | `baml.json.parse` + `Map`/`Array` methods |

**SAP** — the engine is REAL but **internal** (`__sap_parse_final` / `__sap_parse_partial` in `ns_llm`).
Keep `baml.sap.parse<T>` / `parse_partial<T>` as the **public wrapper** (this is OQ6 = *expose* it, not build it).

**Other real** (NOT the invented spellings):
- `baml.math.{min,max,pow,abs,clamp,random,trunc}` (NOT `min_int`/`pow_int`/`rand_int`).
- `baml.io.{print,println,eprint}` (NOT `baml.sys.print`); `baml.log.{info,warn,error,debug}` (NOT `baml.sys.log`).
- `baml.sys.{now_ms,sleep,panic,exit}` — `sleep` takes ms (NOT `sleep_ms`); `baml.time.*` for durations.
- `baml.id.new()` → a runtime id `baml_id_1_…` (the closest real thing to the invented `uuid`/`idempotency_key`; a general UUID is a real gap).
- `baml.reflect.type_of` (only — `type_to_json_schema`/`type_to_gemini_openapi` are net-new).
- `baml.media.{Image,Audio,Pdf,Video}` with `from_base64`/`from_url`/`from_file`, `.base64()`/`.url()`/`.mime_type()`.
- `baml.errors.{Io,Timeout,Unsupported,LlmClient,ParseError}` are the real base error classes.

**Genuinely net-new — keep, but flag as such (these are NOT real yet):** schema lowering (`baml.schema.*`,
`reflect.type_to_*_schema`); duplex-transport framings (`baml.ws`/`webrtc`/`realtime` over the real raw
`baml.net` TCP/UDP); streaming-construction helpers (`baml.llm.stream_unfold` / `baml.http.fetch_json_seq`);
cloud IAM (`baml.cloud.sigv4_sign` / `gcp_access_token`); image transforms (`media.image_dims` / `downscale`);
durability/state (`baml.durable` / `flow` / `kv` / `agent` — the P4 cluster); `baml.vec.cosine` / `hash.sha`.
**Provider wire-shapers** (`baml.llm.openai_*` / `anthropic_*` / `gemini_*`) are NOT stdlib primitives —
they are provider-*implementation* code that belongs inside `build_request`.

---

## The three load-bearing ideas

1. **A `Provider` is the irreducible *marker*** (see [`../provider-as-marker.md`](../provider-as-marker.md)) — it carries NO interaction method. `call`/`call_with` belong to `HttpProvider`, `stream`/`stream_with` to `Streaming`, `run` to `Realtime`. Nothing is universal; `call` is just a capability you `match` for. Everything is a provider (chat models, image models, harnesses, combinators) only in the sense of *being composable*, not of sharing a `call`.
2. **Capabilities are interfaces that `requires Provider`** (`HttpProvider`, `Streaming`, `Realtime`, `Tools`, …). A concrete provider implements the marker plus whatever interactions it actually does. The capability set *is* the type. No taxonomy of provider "kinds", and no degenerate `call` forced onto realtime/harness (OQ1 resolved).
3. **A `client` is pure sugar for a function returning a `Provider`:**
   ```
   client $name($args) { $body }   ⟿   function $name($args) -> Provider { $body }
   ```
   So clients compose, take params, select dynamically, and chain combinators — because they are ordinary functions.

## Rules that recur

- **Options dissolve into provider fields.** `Anthropic` has `max_tokens`, `Bedrock` has `region`. No `options:` blob, no `provider_options` union, no `@providers:` map. Standard params (`model`, `api_key`) are an optional shared read-interface (`ChatModelOptions`).
- **Transport envelope (`headers`, `timeout`) and orchestration (retry, fallback) are NOT provider fields** — they live at the combinator layer and *wrap* the provider.
- **Capability negotiation = a runtime `match`, *including the basic call*.** Every companion `match`es the client's provider for the capability it needs, with the degrade chaining both ways: `Foo(args)` prefers `HttpProvider.call`, else drains a `Streaming` provider, else errors; `Foo.stream(args)` prefers `Streaming.stream`, else buffers an `HttpProvider.call`, else errors; `Foo.live`/`Foo.run_tools` have no honest degrade and error if the capability is absent. Capability (now including `call`) is a *runtime* promise because `client` returns the existential marker `Provider`. Escape hatch for compile-time precision: drop the sugar and write `function … -> ConcreteHttpProvider`.
- **Companions decompose a function:** `Foo(args)` matches `HttpProvider` (value); `Foo.with(args, project)` matches `HttpProvider` (value+sidecar via `call_with`); `Foo.stream(args)` matches `Streaming`; `Foo.live(args, io)` matches `Realtime`; `Foo.run_tools(args, tools)` matches `Tools`. A live `io: Channel` is a parameter of the *capability method*, handed in at the call — never a client field.
- **Combinators (`Fallback`, `Retry`, `Cache`, `RoundRobin`, …) are plain non-generic classes** that forward capabilities by runtime delegation: each `implements` block `match`es its members and routes to a capable one. A combinator statically *claims* every capability it forwards (and degrades/errors at call time if a member can't).
- **Host-backed bodies** use `$rust_io_function` / `$rust_type` inside `implements` blocks, exactly like today's `PrimitiveClient`.

## Machinery reused from today's BAML (cite it, don't reinvent it)

- `interface`/`implements` with associated `type`s, `requires`, default methods, interface-membership `match` (BEP-044/057; the `Iterator` stack in `ns_iter/iter.baml` is the template top to bottom — `next() -> Item | Done`, `map`/`filter`/`collect`).
- `render_prompt → baml.llm.PromptAst` (opaque), `build_request → baml.http.Request`, `parse<T>(body) -> T` (already generic), `baml.llm.Stream<TStream, TFinal>`.
- The `type` primitive + `reflect.type_of<T>()` + `TypeValue.implements`; `throws`/`catch`.
- SAP (Schema-Aligned Parsing): one branch of `parse`; assume a public `baml.sap.parse<T>(string) -> T` exists where a pure-BAML provider needs it.

## BAML syntax reminders

- `throws baml.ExtendUnknownError<baml.errors.CallError>` on a signature; `catch (e) { _ => { ... } }` to handle (see "Error model"). `for (let x in xs)`. `match (v) { let s: Iface => ..., _ => ... }`. Static class function = no `self`.
- Keep examples realistic and fully written — no `/* ... */` where real code is the point; use `$rust_io_function` for genuine host boundaries only.
</content>
</invoke>
