# Providers, Clients, and Capabilities

## Thesis

Three ideas, one deletion.

1. **A `Provider` is the irreducible *marker*** (see [`../provider-as-marker.md`](../provider-as-marker.md)) — it carries **no interaction method**. `call`/`call_with` belong to `HttpProvider`, `stream`/`stream_with` to `Streaming`, `run` to `Realtime`. Nothing is universal; **`call` is just a capability you `match` for**. The marker hosts only the inherited combinator factories and is the type `client` returns and capabilities `requires`. Everything is a provider only in the sense of *being composable*, not of sharing a `call`.
2. **Capabilities are interfaces that `requires Provider`** — `HttpProvider`, `Streaming`, `Realtime`, `Tools`, … A concrete provider implements the marker plus whichever capabilities it actually has. There's no taxonomy of provider *kinds* — just a bare marker with optional, composable capabilities, each owning its own interaction shape.
3. **A `client` is sugar for a function that returns a `Provider`.** `client Name(args) { body }` is *literally* rewritten to `function Name(args) -> Provider { body }`. Nothing more. Clients compose, take config parameters, and select dynamically — all because they're ordinary functions. (Live, execution-time handles like a realtime `Channel` are *not* carried by the client — they're parameters of the capability method, handed in at the call; see §3, §5.)

The **deletion**: today `provider` is a closed compiler enum (`match LlmProvider::from_str(...)`, `crates/sys_llm/src/build_request/mod.rs:31`, plus a construction-time check at `baml_std.rs:36` and a `// @providers:` annotation map at `ns_llm/llm_types.baml:356`). All three go away: a provider is anything that `implements Provider`, options are its fields, and the built-in OpenAI/Anthropic/Gemini providers become the first implementations.

Capability negotiation falls out for free: a function offers companions (the basic call, `.stream`, `.live`, …); each `match`es the client's provider against the capability it needs. The basic call is itself a companion `match` (prefer `HttpProvider.call`, else drain a `Streaming` provider); delivery refinements **degrade across the match** — call↔stream both ways — and different interaction shapes error where no honest degrade exists.

This reuses machinery BAML already ships: `interface`/`implements` with associated types, `requires`, default methods (BEP-044/057, e.g. the whole `Iterator` stack in `ns_iter/iter.baml`); interface-membership pattern matching; the `type` primitive + `reflect`; `baml.http`; `baml.llm.PromptAst`; and `throws`/`catch`.

---

## 1. The core model: a `Provider` *marker* and capabilities that own their interaction

```baml
// The irreducible MARKER. No interaction method — `call`/`stream`/`run` each live on the capability
// that owns that shape. The marker carries ONLY the combinator factories every provider inherits
// (the Iterator.map pattern), and is the type `client` returns and capabilities `requires`.
interface Provider {
  function with_retry(self, max: int)         -> Retry    { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV)            -> Cache    { Cache { inner: self, store: store } }
}
```

`Provider` has no `call`: nothing is privileged or universal. **What you can *do* is per-capability.** Each capability is an interface that `requires Provider`, so each one *is-a* `Provider` (the subtyping rule "A <: B where A requires B") and inherits the combinator factories — but supplies its own interaction method. The typed failure channel is per-capability too (`baml.ExtendUnknownError<baml.errors.CallError>` etc., see the error model), not a base `type Error`.

`HttpProvider` is the capability that **owns `call`** (and `call_with`), threading `T` exactly as today's `PrimitiveClient.parse<T>(self, body) -> T` (`ns_llm/llm_types.baml:595`):

```baml
// HTTP request/response codec — most chat models. OWNS call + call_with (+ the codec).
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.ExtendUnknownError<baml.errors.CallError>
  function send(self, request: baml.http.Request) -> Body                          throws baml.ExtendUnknownError<baml.errors.CallError>
  function parse<T>(self, from: Body) -> T                                         throws baml.ExtendUnknownError<baml.errors.CallError>

  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
    self.parse<T>(self.send(self.build_request<T>(prompt)))      // call is OWNED here, not inherited
  }
}

// Incremental output of the SAME call. OWNS stream.
interface Streaming requires Provider {
  function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
      -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError>
}

// A live, duplex interaction (pass-in): the caller hands in a Channel, the provider
// drives it (send/on), and the call returns the final record when the session ends. OWNS run.
interface Realtime requires Provider {
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
      throws baml.ExtendUnknownError<baml.errors.RealtimeError>
}

// Expose the built HTTP request without sending it (preview / testing).
interface Inspectable requires Provider {
  function request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request
      throws baml.ExtendUnknownError<baml.errors.CallError>
}
```

A concrete provider implements the marker plus its capabilities (a class can have multiple `implements` blocks, exactly as `ArrayIterator` implements both `Iterable` and `Iterator`):

```baml
class OpenAI {
  model: string
  api_key: string

  implements HttpProvider {                  // -> gets `call` for free from the codec default
    type Body = baml.http.Response
    function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws baml.errors.LlmClient {
      $rust_io_function
    }
    function send(self, request: baml.http.Request) -> baml.http.Response throws baml.errors.LlmClient {
      baml.http.send(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "http failed" }; } }
    }
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.sap.parse<T>(from.text())          // SAP is ONE parse mechanism (see §2)
    }
  }

  implements Streaming {
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient { /* ... */ }
  }
}
```

The capability set *is* the type. A chat model is `HttpProvider + Streaming`. An image model is `HttpProvider` (output-specialized, §2). A realtime model is `Realtime` (the marker + `run`, no fake `call`). A subprocess harness is a marker-only provider with no `HttpProvider` at all (§7). Nothing forces a kind hierarchy, and nothing forces a degenerate `call` onto a provider that doesn't do request/response — you implement what you can do.

---

## 2. The HTTP codec: `T` generic, `Body` associated, SAP as one mechanism

`render_prompt` stays provider-agnostic — it yields a `baml.llm.PromptAst` (`class PromptAst { _data: $rust_type }`, opaque). The `HttpProvider` owns `build_request`/`send`/`parse`, over the real `baml.http` types:

```baml
// baml.http  (ns_http/http.baml)
class Request  { method: string, url: string, headers: map<string, string>, body: string }
class Response { status_code: int, headers: map<string, string>, url: string, _body: $rust_type }
                 // .text() -> string throws Io ; .bytes() -> uint8array ; .ok() -> bool
class SseStream{ url: string, _handle: $rust_type }    // .next() -> string? throws Io ; .close()
function send(request: Request) -> Response throws baml.errors.Io | baml.errors.Timeout
function fetch_sse(request: Request) -> SseStream throws baml.errors.Io | baml.errors.Timeout
```

Two type roles:

- **`T` generic** — the output. It flows into `build_request` *and* `parse`, which is what captures the four wire encodings of one schema (prompt-text for chat+SAP, `response_format`, Gemini `responseSchema`, Anthropic forced-tool) — `build_request` decides where the schema goes; the function still writes `-> Resume`.
- **`Body` associated** — the provider's raw response. It lifts the hardcoded `string` in today's `parse<T>(self, http_response_body: string)` so non-text responses fit one interface.

SAP (Schema-Aligned Parsing) is one branch of `parse`, not the seam itself. Different `Body`, different decode:

```baml
class GptImage {                              // output-specialized: Body is HTTP, parse decodes bytes
  model: string
  api_key: string
  implements HttpProvider {
    type Body = baml.http.Response
    function parse<T>(self, from: baml.http.Response) -> T throws baml.errors.LlmClient {
      baml.media.image_from_base64(from.json_path("data.0.b64_json"))    // T = image; no SAP
    }
    // build_request / send omitted
  }
}

class OpenAIStream {                          // Body is a live stream
  model: string
  api_key: string
  implements HttpProvider {
    type Body = baml.http.SseStream
    function send(self, request: baml.http.Request) -> baml.http.SseStream throws baml.errors.LlmClient {
      baml.http.fetch_sse(request) catch (e) { _ => { throw baml.errors.LlmClient { message: "sse failed" }; } }
    }
    function parse<T>(self, from: baml.http.SseStream) -> T throws baml.errors.LlmClient {
      // drain `from`, SAP-parse the final value
    }
  }
}
```

A provider can be generic over its output the way `class ArrayIterator<T> { implements Iterator { type Item = T } }` is: `OpenAI` works for any `T` (SAP handles any schema); `GptImage`'s `parse` only ever yields `image`. The difference is the body, not the interface.

---

## 3. Clients are functions that return a `Provider`

`client` is **pure sugar** — a literal AST rewrite, no new semantics:

```
client $name($args) { $body }
   ⟿  function $name($args) -> Provider { $body }
```

That single fact gives everything:

```baml
// one-shot: a zero-arg client
client GPT4() {
  OpenAI { model: "gpt-4o", api_key: env.OPENAI_API_KEY }
}

// composition is a function call
client Resilient() {
  GPT4().fallback_to(Claude())                    // combinators, §6
}

// dynamic selection — impossible with a static block
client Smart(tenant: string) {
  if (tenant == "enterprise") { OpenAI { model: "gpt-4o", api_key: env.ENTERPRISE_KEY } }
  else                        { OpenAI { model: "gpt-4o-mini", api_key: env.DEFAULT_KEY } }
}
```

**A live channel is a parameter of the capability method, not the client.** A realtime model needs a channel that doesn't exist until you run — so it's a parameter of `Realtime.run` (pass-in), handed in at the realtime call. It is *not* a provider field and *not* a client-function parameter; the client function stays config-only.

```baml
interface Channel {
  function on(self, handler: (InEvent) -> void) -> null
  function send(self, data: OutEvent) -> null throws baml.errors.Io
  function close(self) -> null
}

class OpenAIRealtime {
  voice: string
  api_key: string

  implements Realtime {
    // io is an explicit PARAMETER (pass-in): the caller hands in the channel; the provider drives it.
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      io.on(event => { /* feed user audio to the LLM, accumulate */ });
      io.send(...);                            // drive the duplex via the handed-in channel
    }
  }
  // Realtime requires only the Provider MARKER — no `call` to fake. A realtime client is value-callable
  // only via the §5 degrade (and only if it ALSO implements HttpProvider/Streaming); otherwise Foo(args) errors.
}

client Voice() {                              // config only — the live channel is supplied at the call
  OpenAIRealtime { voice: "alloy", api_key: env.OPENAI_API_KEY }
}
```

The `client:` clause in a function takes *any* expression of provider type — a call, a factory, a combinator chain:

```baml
function VoiceChat(system: string) -> Transcript {
  client: Voice()              // config-only; the live channel is handed in at the realtime call
  prompt #"{{ system }}"#
}
// the channel arrives at the realtime call via the .live companion (§5):
//   VoiceChat.live("be terse", io)   ->   OpenAIRealtime.run(prompt, io)

function Extract(resume: string) -> Resume {
  client: OpenAI.from_model("gpt-4o").with_retry(2)
  prompt #"Extract from: {{ resume }}"#
}
```

Because the rewrite is literal, the return is the **existential `Provider`** — capability is therefore a *runtime* concern (§5), matching the status quo (the closed enum also only failed at runtime). When you want compile-time precision instead, drop the sugar and write the function longhand with a concrete or capability return type — that's the escape hatch, and it's free because `client` was never anything but sugar:

```baml
function Dalle() -> GptImage { GptImage { model: "gpt-image-2", api_key: env.OPENAI_API_KEY } }   // precise
```

---

## 4. Options dissolve into provider fields

There is no `options: PrimitiveClientOptions` blob and no `provider_options: AnthropicOptions | … | null` union (`ns_llm/llm_types.baml:365`). That union existed only because the old design couldn't let a provider declare its own fields. Now:

- **Provider-specific params** are the provider class's own fields — `Anthropic` has `max_tokens`, `Bedrock` has `region`, `Azure` has `api_version`. Validation checks the construction against the class directly (no `@providers:` annotation map needed).
- **Standard params** (`model`, `api_key`) are an optional shared *read-interface* the provider's fields satisfy, so tooling reads them uniformly without a monolith:
  ```baml
  interface ChatModelOptions { model: string, api_key: string? }
  class Anthropic {
    model: string
    api_key: string?
    max_tokens: int                                   // bespoke
    implements ChatModelOptions { model as model, api_key as api_key }
  }
  ```
- **Transport envelope** (`headers`, `query_params`, `timeout`) and **orchestration** (retry, fallback) are *not* provider fields. The current code already applies header-merge / query-params / `auth` / retry *outside* the per-provider arm — they wrap the provider. They live at the combinator/framework layer (§6), which is why a custom provider never has to re-declare `headers`.

---

## 5. Capability negotiation: every companion `match`es — *including the basic call*

A function decomposes into companions. With nothing privileged on the marker, **the basic call is itself a companion `match`**, like every other — the negotiation story applied *universally*. Because pattern-matching a value against an interface checks interface-membership at runtime, each gate is just a `match` with `_` as the fallback. And the call↔stream degrade chains **both ways** — a value can be served by draining a stream; a stream by buffering a call:

```baml
// Foo(args)  -> wants a single value. Prefer HttpProvider.call; else drain a Streaming provider.
function Foo(args: ...) -> T {
  let p = client();
  match (p) {
    let h: HttpProvider => h.call<T>(render_prompt(args)),
    let s: Streaming    => drain<T>(s.stream<T, T>(render_prompt(args))),   // value from a stream-only provider
    _                   => throw baml.errors.Unsupported { message: "client cannot produce a value" },
  }
}

// Foo.stream(args)  -> wants incremental. Prefer Streaming; else buffer an HttpProvider.call.
function Foo$stream<TStream, TFinal>(args: ...) -> baml.llm.Stream<TStream, TFinal> {
  let p = client();
  match (p) {
    let s: Streaming    => s.stream<TStream, TFinal>(render_prompt(args)),
    let h: HttpProvider => buffer_as_stream<TStream, TFinal>(h, render_prompt(args)),   // one-shot faked as a stream
    _                   => throw baml.errors.Unsupported { message: "client cannot stream" },
  }
}

// Foo.live(args, io)  -> NO degrade: a one-shot call can't fake a duplex session
function Foo$live(args: ..., io: Channel) -> Transcript {
  let p = client();
  match (p) {
    let r: Realtime => r.run(render_prompt(args), io),
    _              => throw baml.errors.Unsupported { message: "client is not realtime" },
  }
}
```

So the rule you wanted holds exactly: **a companion works iff the provider implements the capability iff the client returns such a provider** — and the `_` arm decides what "doesn't support it" means. The only change from the marker model is that `call` is now *inside* this scheme rather than a privileged base method sitting underneath it.

The split that matters:

- **Delivery refinements** (call↔stream, `Inspectable`) → degrade across the match. The `_`/sibling arm does something useful (drain a stream into a value; buffer the whole answer into a stream; skip the preview).
- **Different interaction shapes** (`Realtime`, `Tools`) → no honest degrade. The `_` arm errors.

Capability is a **runtime promise**, not a static guarantee — and this now touches the basic case too: `Foo(args)` can `throw Unsupported` for a pure-realtime/pure-harness client, because the marker carries no `call`. Because a client returns the existential `Provider` marker, "does this client support `HttpProvider`/`Realtime`?" is answered by the `match` above, not the type checker. If you genuinely need a compile-time guarantee, drop the `client` sugar and write the function longhand with a concrete return type (`function … -> ConcreteHttpProvider`); that's the only way to make a capability part of the type.

---

## 6. Methods: static factories and fluent combinators

Providers are classes, so they get the full method machinery (the std lib already uses no-`self` class functions like `ArrayIterator.new(...)` as statics, and `self` interface default methods like `Iterator.map`):

**Static factories** — named constructors, mirroring the existing `from_shorthand`:
```baml
class OpenAI {
  model: string
  api_key: string
  function from_model(model: string) -> OpenAI {       // no self => static: OpenAI.from_model("gpt-4o")
    OpenAI { model: model, api_key: env.OPENAI_API_KEY }
  }
}
```

**Combinators as default methods on the `Provider` marker** — fluent composition, inherited by *every* provider, exactly the `Iterator.map`/`filter`/`collect` pattern. The marker carries *only* these factories; this is where fallback/round-robin/retry/cache live, replacing the hardcoded `ClientType` enum loop (`ns_llm/llm_types.baml:6,273`):
```baml
interface Provider {                            // the MARKER — combinator factories only, no `call`
  function with_retry(self, max: int) -> Retry { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV) -> Cache { Cache { inner: self, store: store } }
}

class Fallback {
  strategy: Provider[]                          // plain, non-generic

  implements HttpProvider {                      // .call — try each http-capable member in order
    type Body = unknown
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.ExtendUnknownError<baml.errors.CallError> {
      for (let p in self.strategy) {
        match (p) {
          let h: HttpProvider => {
            let r: T = h.call<T>(prompt) catch (e) { _ => { continue; } };   // typed throws decides retry
            return r;
          },
          _ => { continue; },                     // skip non-http members
        }
      }
      throw baml.errors.Unsupported { message: "all providers failed" }
    }
    // build_request / send / parse delegated or inherited likewise
  }

  implements Streaming {                         // .stream — route to a streaming member
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.ExtendUnknownError<baml.errors.StreamError> {
      for (let p in self.strategy) {
        match (p) {
          let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
          _               => { continue; },     // skip non-streaming members
        }
      }
      throw baml.errors.Unsupported { message: "no streaming provider available" }
    }
  }

  implements Realtime {                          // .live — route to a realtime member
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      for (let p in self.strategy) {
        match (p) {
          let r: Realtime => { return r.run(prompt, io); },
          _              => { continue; },       // skip non-realtime members
        }
      }
      throw baml.errors.Unsupported { message: "no realtime provider available" }
    }
  }
}
```

A combinator `requires Provider` (inheriting the factories) and forwards each capability by **runtime delegation** — each `implements` block just `match`es its members and routes to a capable one (reliability fallback now lives inside the `HttpProvider` arm). So a `Fallback` can forward *all* of them at once — `HttpProvider.call`, `.stream`, `.live`, … — routing each companion to whichever members support it.

The tradeoff is the one we already accepted: a combinator statically *claims* every capability it forwards, so `Fallback` is `HttpProvider`/`Streaming`/`Realtime` even when its members aren't — `.call`/`.stream`/`.live` compile and then degrade or error at call time. Capability is a runtime promise. Each combinator returns a `Provider` marker, so chaining works like `Iterator` returning `Iterator`, and the two surfaces are equivalent: `Fallback { strategy: [GPT4(), Claude()] }` or `GPT4().fallback_to(Claude())`.

---

## 7. The agentic tool loop is a capability

Tool-calling is another capability: a provider that can run a multi-turn loop. The thing that makes it tricky — wire threading (call ids, Gemini's lack of them, Anthropic's positional `tool_result` order, assistant-turn/`phase` preservation, cache-stable ordering) — **must stay inside the provider**, because none of it can be expressed once for all providers. So the transcript is provider-owned and opaque (an associated type), and the loop speaks a tiny vocabulary with a correlation id it never interprets:

```baml
class ToolCall   { id: string, name: string, args: map<string, unknown> }   // id OPAQUE to the loop
class ToolResult { id: string, output: unknown }
class ToolCalls  { calls: ToolCall[] }                                       // sentinel, like Iterator's Done

interface Tools requires Provider {
  type Transcript

  function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript throws Error
  function step<T>(self, t: Transcript) -> T | ToolCalls throws Error        // mirror of next() -> Item | Done
  function submit(self, t: Transcript, results: ToolResult[]) -> Transcript throws Error

  function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T throws Error {
    let t = self.begin<T>(prompt, tools);
    while (true) {
      match (self.step<T>(t)) {
        let calls: ToolCalls => { t = self.submit(t, ctx.dispatch(calls.calls)); },   // YOUR code in dispatch
        let value: T => { return value; },
      }
    }
    baml.sys.panic("unreachable")
  }
}
```

`step` returning `T | ToolCalls` is the exact shape of `Iterator.next -> Item | Done`. Overriding the loop changes *dispatch policy* (parallel, approval, budget, your code between turns) and never touches threading — all of which stays below the `ToolCall`/`ToolResult` seam, inside `begin`/`step`/`submit`.

> **Net-new flags:** there is no `Tool` type today and `call`/`build_request` have no tools param. A natural shape using the first-class `type`: `class Tool { name: string, description: string, parameters: type }`. `ExecutionContext` (today `{ jinja_string, args, function_name }`, `ns_llm/llm.baml:6`) needs a `dispatch` and, for realtime/harnesses, an event-`emit` surface.

---

## 8. Harnesses (Claude Code) are marker-only providers with no HTTP

A harness is the payoff of the marker model: it is a `Provider` marker that implements `Realtime` (its long-lived, steerable session) directly — its transport is a subprocess, not `baml.http` — and is *not* an `HttpProvider`. Crucially, it does **not** fake a `call`: under the marker model there is no base `call` to satisfy, so the harness claims only the interaction it actually does (OQ1 resolved). Its value-shaped use comes through the §5 degrade only if it *also* implements a value-producing capability.

```baml
class ClaudeCode {
  model: string?
  sandbox: Sandbox                              // bound field: local | container | virtual
  allowed_tools: string[]?
  permission_mode: string?

  // marker-only + Realtime — no degenerate `call` to fake.
  implements Realtime {                          // the long-lived, steerable session
    // the control/event channel is handed in at the call (pass-in), not a field
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript
        throws baml.ExtendUnknownError<baml.errors.RealtimeError> {
      $rust_io_function
    }
  }
}

client CodeAgent() {                            // config only
  ClaudeCode {
    model: "claude-sonnet-4-6",
    sandbox: container,                          // just a bound field, like `model`
    permission_mode: "acceptEdits",
    allowed_tools: ["Read", "Edit", "Bash", "Grep"],
  }
}

function Fix(task: string) -> Patch {
  client: CodeAgent()
  prompt #"Fix the bug: {{ task }}"#
}
// the live control/event channel is handed in at the realtime call:  Fix.live("...", io)
```

`sandbox`/`permission_mode`/`model` are bound config fields; the live control/event channel is the `io` parameter of `Realtime.run`, handed in at the `.live` call (steer/interrupt/rewind flow over that channel). Because `ClaudeCode` is a `Provider`, the literal `client → -> Provider` rewrite covers it with no special case.

---

## What this reuses vs what it adds

**Reused, unchanged:**
- `interface`/`implements` with associated types, `requires`, default methods, interface-membership `match` (BEP-044/057; the `Iterator` stack is the template top to bottom).
- `render_prompt → PromptAst`, `build_request → http.Request`, `parse<T>(body) → T` companions; `parse<T>` is already generic.
- The combinator loop already exists in BAML — `Client.execute_*` with `catch (e) { _ => { continue; } }`.
- `baml.http`, `baml.llm.PromptAst`, `baml.llm.Stream`, the `type` primitive + `reflect.type_of<T>()` + `TypeValue.implements`, the `throws`/`catch` channel.

**Genuinely new:**
- `client` as literal sugar for `function … -> Provider { … }` (so clients are functions: params, composition, dynamic selection, factories, combinators).
- A *marker* `Provider` (combinator factories only, no interaction method) with capabilities as `requires Provider` interfaces that each own their interaction (`HttpProvider.call`/`call_with`, `Streaming.stream`, `Realtime.run`, …), replacing the closed `provider` enum, the `ClientType` enum, and the `@providers:` map.
- Lifting `parse`'s hardcoded `string` to an associated `type Body`.
- Capability companions that `match` the provider — *including the basic call* — with the call↔stream degrade chaining both ways.
- The `Tools` capability (`type Transcript`, `step`/`submit`, the `ToolCall`/`ToolResult` seam); a `Tool` type; `ExecutionContext` gaining `dispatch` + an event-`emit` surface.

---

## Open questions

1. **~~Does every capability `requires Provider`?~~ — RESOLVED by the marker model.** Earlier this asked whether `Realtime requires Provider` forces a realtime model to fake a degenerate `call`. With `Provider` now a bare *marker* (no `call`), the answer is *no*: every capability `requires` only the marker (for the combinator factories) and supplies its own interaction. Realtime/harness providers expose `run` and never a fake `call`; a value-shaped use comes only through the §5 degrade when the provider *also* implements a value-producing capability. See [`../provider-as-marker.md`](../provider-as-marker.md).
2. **How does the live `io` channel surface at the call?** `Realtime.run(self, prompt, io)` takes the channel as a parameter, and it rides on the realtime companion: `VoiceChat.live(args, io)` — the `io` param propagates from `Realtime.run`'s signature onto the companion. Acceptable as-is (it appears only on the capability companion, never the plain `Foo(args)` call), or should a realtime function name the channel in its own signature?
3. **The request side is HTTP-specific.** `build_request`/`call` live on `HttpProvider` and return/consume `baml.http.Request`; a non-HTTP provider (harness) sidesteps `HttpProvider` entirely and implements only the capabilities it actually does (e.g. `Realtime`). Is "HTTP is a capability, never the base" the whole story, or does a non-HTTP transport want its own value-producing capability (a marker-`requires` interface with a subprocess `call`) rather than leaning on the §5 degrade?
4. **`parse` input is the full `Body`.** `parse<T>(self, from: Body)` widens today's `parse<T>(self, json: string)` — image/embeddings need `Body = Response` (bytes), streaming needs `Body = SseStream`. Confirm the seam is `Body`, not a string.
5. **Capability `match` over an interface-existential.** Runtime membership testing (`let s: Streaming => …`) needs the concrete provider's type at runtime. Confirm value-level reflection supports `match (provider) { let s: Streaming => … }`.
6. **SAP as a public primitive.** SAP today is `PrimitiveClient.parse<T>` / `__sap_parse_final` (`$rust_io_function`, internal). A pure-BAML provider that wants SAP needs a public `baml.sap.parse<T>(string) -> T`. Expose it?
7. **Host-backed providers.** A harness needs `$rust_io_function`/extern method bodies inside its `implements` blocks (like `PrimitiveClient` today). Is an `implements` block with native bodies the supported bridge boundary?
