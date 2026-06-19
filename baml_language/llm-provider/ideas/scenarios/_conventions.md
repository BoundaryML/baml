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
// The irreducible base. Every provider can do exactly this.
interface Provider {
  type Error = baml.errors.LlmClient
  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws Error

  // Combinators — default methods inherited by EVERY provider (the Iterator.map pattern).
  function with_retry(self, max: int) -> Retry { Retry { inner: self, max: max } }
  function fallback_to(self, other: Provider) -> Fallback { Fallback { strategy: [self, other] } }
  function cached(self, store: KV) -> Cache { Cache { inner: self, store: store } }
}

// HTTP request/response codec — most chat models. Supplies a default `call`.
interface HttpProvider requires Provider {
  type Body
  function build_request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws Error
  function send(self, request: baml.http.Request) -> Body throws Error
  function parse<T>(self, from: Body) -> T throws Error
  function call<T>(self, prompt: baml.llm.PromptAst) -> T throws Error {
    self.parse<T>(self.send(self.build_request<T>(prompt)))
  }
}

// Incremental output of the SAME call.
interface Streaming requires Provider {
  function stream<TStream, TFinal>(self, prompt: baml.llm.PromptAst)
      -> baml.llm.Stream<TStream, TFinal> throws Error
}

// A live, duplex interaction. The caller hands in a Channel (pass-in); the provider drives it.
interface Realtime requires Provider {
  function run(self, prompt: baml.llm.PromptAst, io: Channel) -> Transcript throws Error
}

// The multi-turn tool loop is a capability. Transcript is provider-owned + opaque.
interface Tools requires Provider {
  type Transcript
  function begin<T>(self, prompt: baml.llm.PromptAst, tools: Tool[]) -> Transcript throws Error
  function step<T>(self, t: Transcript) -> T | ToolCalls throws Error   // mirror of Iterator.next -> Item | Done
  function submit(self, t: Transcript, results: ToolResult[]) -> Transcript throws Error
  function run_tools<T>(self, prompt: baml.llm.PromptAst, tools: Tool[], ctx: ExecutionContext) -> T throws Error {
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

// Expose the built request without sending it (preview / testing).
interface Inspectable requires Provider {
  function request<T>(self, prompt: baml.llm.PromptAst) -> baml.http.Request throws Error
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

// baml.http (ns_http/http.baml) — real, existing
class Request  { method: string, url: string, headers: map<string, string>, body: string }
class Response { status_code: int, headers: map<string, string>, url: string, _body: $rust_type }
                 // .text() -> string throws Io ; .bytes() -> uint8array ; .ok() -> bool ; .json_path(p) -> string
class SseStream{ url: string, _handle: $rust_type }   // .next() -> string? throws Io ; .close()
function send(request: Request) -> Response throws baml.errors.Io | baml.errors.Timeout
function fetch_sse(request: Request) -> SseStream throws baml.errors.Io | baml.errors.Timeout
```

---

## The three load-bearing ideas

1. **A `Provider` is the irreducible base** — one method, `call<T>(prompt) -> T`. Everything is a provider: chat models, image models, harnesses, combinators.
2. **Capabilities are interfaces that `requires Provider`** (`Streaming`, `Realtime`, `Tools`, `HttpProvider`, …). A concrete provider implements the base plus whatever it can actually do. The capability set *is* the type. No taxonomy of provider "kinds".
3. **A `client` is pure sugar for a function returning a `Provider`:**
   ```
   client $name($args) { $body }   ⟿   function $name($args) -> Provider { $body }
   ```
   So clients compose, take params, select dynamically, and chain combinators — because they are ordinary functions.

## Rules that recur

- **Options dissolve into provider fields.** `Anthropic` has `max_tokens`, `Bedrock` has `region`. No `options:` blob, no `provider_options` union, no `@providers:` map. Standard params (`model`, `api_key`) are an optional shared read-interface (`ChatModelOptions`).
- **Transport envelope (`headers`, `timeout`) and orchestration (retry, fallback) are NOT provider fields** — they live at the combinator layer and *wrap* the provider.
- **Capability negotiation = a runtime `match`.** A companion (`.stream`, `.live`, …) `match`es the client's provider against the capability it needs, then either **falls back to `.call`** (delivery refinements like `Streaming`/`Inspectable`) or **errors** (different interaction shapes like `Realtime`). Capability is a *runtime* promise because `client` returns the existential `Provider`. The escape hatch for compile-time precision: drop the sugar and write `function … -> ConcreteProvider`.
- **Companions decompose a function:** `Foo(args)` uses `.call`; `Foo.stream(args)`, `Foo.live(args, io)`, `Foo.run_tools(args, tools)` are the capability companions. A live `io: Channel` is a parameter of the *capability method*, handed in at the call — never a client field.
- **Combinators (`Fallback`, `Retry`, `Cache`, `RoundRobin`, …) are plain non-generic classes** that forward capabilities by runtime delegation: each `implements` block `match`es its members and routes to a capable one. A combinator statically *claims* every capability it forwards (and degrades/errors at call time if a member can't).
- **Host-backed bodies** use `$rust_io_function` / `$rust_type` inside `implements` blocks, exactly like today's `PrimitiveClient`.

## Machinery reused from today's BAML (cite it, don't reinvent it)

- `interface`/`implements` with associated `type`s, `requires`, default methods, interface-membership `match` (BEP-044/057; the `Iterator` stack in `ns_iter/iter.baml` is the template top to bottom — `next() -> Item | Done`, `map`/`filter`/`collect`).
- `render_prompt → baml.llm.PromptAst` (opaque), `build_request → baml.http.Request`, `parse<T>(body) -> T` (already generic), `baml.llm.Stream<TStream, TFinal>`.
- The `type` primitive + `reflect.type_of<T>()` + `TypeValue.implements`; `throws`/`catch`.
- SAP (Schema-Aligned Parsing): one branch of `parse`; assume a public `baml.sap.parse<T>(string) -> T` exists where a pure-BAML provider needs it.

## BAML syntax reminders

- `throws Error` on a signature; `catch (e) { _ => { ... } }` to handle. `for (let x in xs)`. `match (v) { let s: Iface => ..., _ => ... }`. Static class function = no `self`.
- Keep examples realistic and fully written — no `/* ... */` where real code is the point; use `$rust_io_function` for genuine host boundaries only.
</content>
</invoke>
