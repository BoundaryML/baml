# Providers, Clients, and Capabilities

## Thesis

Three ideas, one deletion.

1. **A `Provider` is the irreducible base** — an interface with one method, `call<T>(prompt) -> T`: "produce a typed answer from a prompt." Everything is a provider.
2. **Capabilities are interfaces that `requires Provider`** — `HttpProvider`, `Streaming`, `Realtime`, `Tools`, … A concrete provider implements the base plus whichever capabilities it actually has. There's no taxonomy of provider *kinds* — just a base with optional, composable capabilities.
3. **A `client` is sugar for a function that returns a `Provider`.** `client Name(args) { body }` is *literally* rewritten to `function Name(args) -> Provider { body }`. Nothing more. Clients compose, take config parameters, and select dynamically — all because they're ordinary functions. (Live, execution-time handles like a realtime `Channel` are *not* carried by the client — they're parameters of the capability method, handed in at the call; see §3, §5.)

The **deletion**: today `provider` is a closed compiler enum (`match LlmProvider::from_str(...)`, `crates/sys_llm/src/build_request/mod.rs:31`, plus a construction-time check at `baml_std.rs:36` and a `// @providers:` annotation map at `ns_llm/llm_types.baml:356`). All three go away: a provider is anything that `implements Provider`, options are its fields, and the built-in OpenAI/Anthropic/Gemini providers become the first implementations.

Capability negotiation falls out for free: a function offers companions (`.call`, `.stream`, `.live`, …); each `match`es the client's provider against the capability it needs and **falls back to `.call`** where that's meaningful, or errors where it isn't.

This reuses machinery BAML already ships: `interface`/`implements` with associated types, `requires`, default methods (BEP-044/057, e.g. the whole `Iterator` stack in `ns_iter/iter.baml`); interface-membership pattern matching; the `type` primitive + `reflect`; `baml.http`; `baml.llm.PromptAst`; and `throws`/`catch`.

---

## 1. The core model: a base `Provider` and capabilities that require it

```baml
// The irreducible base. Every provider can do exactly this.
interface Provider {
  type Error = baml.errors.LlmClient
  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws Error
}
```

`T` is a generic — the output type, threaded by the compiler exactly as today's `PrimitiveClient.parse<T>(self, body) -> T` (`ns_llm/llm_types.baml:595`). `type Error` is the typed failure channel (default `baml.errors.LlmClient`, mirroring `Iterator`'s `type Error = never`), so combinators can reason about failures.

Capabilities are interfaces that `requires Provider`, so each one *is-a* `Provider` (the subtyping rule "A <: B where A requires B"):

```baml
// HTTP request/response codec — most chat models. Supplies a default `call`.
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws Error
  function send(self, request: baml.http.Request) -> Body throws Error
  function parse<T>(self, from: Body) -> T throws Error

  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws Error {
    self.parse<T>(self.send(self.build_request<T>(prompt)))      // positional calls
  }
}

// Incremental output of the SAME call.
interface Streaming requires Provider {
  function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
      -> baml.llm.Stream<TStream, TFinal> throws Error
}

// A live, duplex interaction (pass-in): the caller hands in a Channel, the provider
// drives it (send/on), and the call returns the final record when the session ends.
interface Realtime requires Provider {
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws Error
}

// Expose the built HTTP request without sending it (preview / testing).
interface Inspectable requires Provider {
  function request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws Error
}
```

A concrete provider implements the base plus its capabilities (a class can have multiple `implements` blocks, exactly as `ArrayIterator` implements both `Iterable` and `Iterator`):

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

The capability set *is* the type. A chat model is `HttpProvider + Streaming`. An image model is `HttpProvider` (output-specialized, §2). A realtime model is `Provider + Realtime`. A subprocess harness is `Provider` with no `HttpProvider` at all (§7). Nothing forces a kind hierarchy — you implement what you can do.

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
  // also implements Provider.call as a best-effort single turn (Realtime requires Provider)
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

## 5. Capability negotiation: companions `match`, and fall back to `.call`

A function decomposes into companions. The basic one uses `call`; the richer ones `match` the client's provider against the capability they need. Because pattern-matching a value against an interface checks interface-membership at runtime, the gate is just a `match` with `_` as the fallback:

```baml
// Foo(args)  -> always works: every provider has `call`
function Foo(args: ...) -> T { let p = client(); p.call<T>(render_prompt(args)) }

// Foo.stream(args)  -> degrades to `call`, because streaming is "same call, richer delivery"
function Foo$stream<TStream, TFinal>(args: ...) -> baml.llm.Stream<TStream, TFinal> {
  let p = client();
  match (p) {
    let s: Streaming => s.stream<TStream, TFinal>(render_prompt(args)),
    _               => buffer_as_stream(p.call<TFinal>(render_prompt(args))),    // fallback
  }
}

// Foo.live(args, io)  -> NO fallback: a one-shot call can't fake a duplex session
function Foo$live(args: ..., io: Channel) -> Transcript {
  let p = client();
  match (p) {
    let r: Realtime => r.run(render_prompt(args), io),
    _              => throw baml.errors.Unsupported { message: "client is not realtime" },
  }
}
```

So the rule you wanted holds exactly: **a companion works iff the provider implements the capability iff the client returns such a provider** — and the `_` arm decides what "doesn't support it" means.

The split that matters:

- **Delivery refinements** (`Streaming`, `Inspectable`) → degrade to `.call`. The `_` arm does something useful (buffer the whole answer; skip the preview).
- **Different interaction shapes** (`Realtime`) → no honest fallback. The `_` arm errors.

Capability is a **runtime promise**, not a static guarantee. Because a client returns the existential `Provider`, "does this client support `Realtime`?" is answered by the `match` above, not the type checker. If you genuinely need a compile-time guarantee, drop the `client` sugar and write the function longhand with a concrete return type (`function … -> SomeRealtimeProvider`); that's the only way to make a capability part of the type.

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

**Combinators as default methods on `Provider`** — fluent composition, inherited by *every* provider, exactly the `Iterator.map`/`filter`/`collect` pattern. This is also where fallback/round-robin/retry/cache live, replacing the hardcoded `ClientType` enum loop (`ns_llm/llm_types.baml:6,273`):
```baml
interface Provider {
  type Error = baml.errors.LlmClient
  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws Error

  function with_retry(self, max: int) -> Retry { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV) -> Cache { Cache { inner: self, store: store } }
}

class Fallback {
  strategy: Provider[]                          // plain, non-generic

  implements Provider {                          // .call — try each member in order
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        let r: T = p.call<T>(prompt) catch (e) { _ => { continue; } };   // typed throws decides retry
        return r;
      }
      throw baml.errors.LlmClient { message: "all providers failed" }
    }
  }

  implements Streaming {                         // .stream — route to a streaming member
    function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
        -> baml.llm.Stream<TStream, TFinal> throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        match (p) {
          let s: Streaming => { return s.stream<TStream, TFinal>(prompt); },
          _               => { continue; },     // skip non-streaming members
        }
      }
      throw baml.errors.LlmClient { message: "no streaming provider available" }
    }
  }

  implements Realtime {                          // .live — route to a realtime member
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
      for (let p in self.strategy) {
        match (p) {
          let r: Realtime => { return r.run(prompt, io); },
          _              => { continue; },       // skip non-realtime members
        }
      }
      throw baml.errors.LlmClient { message: "no realtime provider available" }
    }
  }
}
```

A combinator is a **plain (non-generic) class** that forwards capabilities by **runtime delegation** — each `implements` block just `match`es its members and routes to a capable one (with reliability fallback on `Provider.call`). So a `Fallback` can forward *all* of them at once — `.call`, `.stream`, `.live`, … — routing each companion to whichever members support it.

The tradeoff is the one we already accepted: a combinator statically *claims* every capability it forwards, so `Fallback` is `Streaming`/`Realtime` even when its members aren't — `.stream`/`.live` compile and then degrade or error at call time. Capability is a runtime promise. Each combinator returns a `Provider`, so chaining works like `Iterator` returning `Iterator`, and the two surfaces are equivalent: `Fallback { strategy: [GPT4(), Claude()] }` or `GPT4().fallback_to(Claude())`.

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

## 8. Harnesses (Claude Code) are providers with no HTTP

A harness is the payoff of the base-`Provider` model: it implements `Provider` (`call`) directly — its transport is a subprocess, not `baml.http` — and adds a control-plane capability. It is *not* an `HttpProvider`.

```baml
class ClaudeCode {
  model: string?
  sandbox: Sandbox                              // bound field: local | container | virtual
  allowed_tools: string[]?
  permission_mode: string?

  implements Provider {
    function call<T>(self, prompt: baml.llm.PromptAst) -> T throws baml.errors.LlmClient {
      $rust_io_function                          // drives the spawned `claude` subprocess
    }
  }
  implements Realtime {                          // the long-lived, steerable session
    // the control/event channel is handed in at the call (pass-in), not a field
    function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws baml.errors.LlmClient {
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
- A *base* `Provider` (`call<T>`) with capabilities as `requires Provider` interfaces, replacing the closed `provider` enum, the `ClientType` enum, and the `@providers:` map.
- Lifting `parse`'s hardcoded `string` to an associated `type Body`.
- Capability companions that `match` the provider and fall back to `.call`.
- The `Tools` capability (`type Transcript`, `step`/`submit`, the `ToolCall`/`ToolResult` seam); a `Tool` type; `ExecutionContext` gaining `dispatch` + an event-`emit` surface.

---

## Open questions

1. **Does every capability `requires Provider`?** `Realtime requires Provider` means a realtime model also implements `call` (a single-turn over its connection). Best-effort single turn, or should some capabilities stand alone rather than refine `call`?
2. **How does the live `io` channel surface at the call?** `Realtime.run(self, prompt, io)` takes the channel as a parameter, and it rides on the realtime companion: `VoiceChat.live(args, io)` — the `io` param propagates from `Realtime.run`'s signature onto the companion. Acceptable as-is (it appears only on the capability companion, never the plain `Foo(args)` call), or should a realtime function name the channel in its own signature?
3. **The request side is HTTP-specific.** `build_request` returns `baml.http.Request`; a non-HTTP provider (harness) sidesteps `HttpProvider` and implements `call`/`Realtime` directly. Is "HTTP is a capability, not the base" the whole story, or does the request side also want an associated type for non-HTTP transports?
4. **`parse` input is the full `Body`.** `parse<T>(self, from: Body)` widens today's `parse<T>(self, json: string)` — image/embeddings need `Body = Response` (bytes), streaming needs `Body = SseStream`. Confirm the seam is `Body`, not a string.
5. **Capability `match` over an interface-existential.** Runtime membership testing (`let s: Streaming => …`) needs the concrete provider's type at runtime. Confirm value-level reflection supports `match (provider) { let s: Streaming => … }`.
6. **SAP as a public primitive.** SAP today is `PrimitiveClient.parse<T>` / `__sap_parse_final` (`$rust_io_function`, internal). A pure-BAML provider that wants SAP needs a public `baml.sap.parse<T>(string) -> T`. Expose it?
7. **Host-backed providers.** A harness needs `$rust_io_function`/extern method bodies inside its `implements` blocks (like `PrimitiveClient` today). Is an `implements` block with native bodies the supported bridge boundary?
