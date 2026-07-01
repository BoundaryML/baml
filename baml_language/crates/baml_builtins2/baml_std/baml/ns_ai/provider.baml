// The provider/capability spine (see _plan/llm-provider-plan.md). A `Provider` is a
// bare marker; each capability is an interface that `requires Provider` and owns its
// own interaction method. Built-in providers (OpenAI, …) are ordinary classes that
// `implements` these — the same way a user's custom provider would.
//
// NOTE: `prompt` is a plain `string` for now. The design threads a `baml.llm.PromptAst`
// (the provider-agnostic rendered prompt); wiring that render pipeline in is a later step.

/// The value + out-of-band-metadata carrier returned by `call_with`. Replaces the
/// `(T, V)` tuple from the design (tuples are not yet a language feature).
class CallResult<T, V> {
    value: T,
    meta: V,
}

/// Token accounting for one call.
class Usage {
    input_tokens: int,
    output_tokens: int,
}

/// Out-of-band response metadata (finish reason, usage, …), normalized lazily by
/// each provider. Projected via `HttpProvider.call_with`. Metadata accessors are
/// best-effort (`throws never`): they return a sensible default if the wire shape
/// is missing, so a projection over them is infallible.
interface ResponseMeta {
  function finish_reason(self) -> string throws never
  function usage(self) -> Usage throws never
}

/// The bare capability marker. Carries no interaction method — `call`/`call_with`
/// live on `HttpProvider`, `stream` on `Streaming`, etc. It is the type a client
/// returns and the bound every capability `requires`. Hosts the inherited combinator
/// factories every provider gets for free (see `combinators.baml`).
interface Provider {
  function with_retry(self, max: int) -> Retry throws never {
    Retry { inner: self, max: max }
  }
  function fallback_to(self, other: Provider) -> Fallback throws never {
    Fallback { members: [self, other] }
  }
}

/// The HTTP request/response capability. Owns `call` and `call_with`. `Body` is the
/// provider's raw response type (associated), lifting the hardcoded `string` of the
/// old `parse<T>(body: string)` so non-text responses fit one interface.
interface HttpProvider requires Provider {
  type Body

  function build_request<T>(self, prompt: string) -> root.http.Request
      throws root.errors.CallError | root.errors.UnknownError
  function send(self, request: root.http.Request) -> Body
      throws root.errors.CallError | root.errors.UnknownError
  function parse<T>(self, from: Body) -> T
      throws root.errors.CallError | root.errors.UnknownError
  function meta_of(self, from: Body) -> ResponseMeta
      throws root.errors.CallError | root.errors.UnknownError

  // Primitive: value + projected sidecar. `E2` threads the projection's error
  // channel (the `Iterator.map` pattern).
  function call_with<T, V, E2>(
    self,
    prompt: string,
    project: (ResponseMeta) -> V throws E2,
  ) -> CallResult<T, V>
      throws root.errors.CallError | root.errors.UnknownError | E2 {
    let body = self.send(self.build_request<T>(prompt));
    CallResult<T, V> { value: self.parse<T>(body), meta: project(self.meta_of(body)) }
  }

  // Derived: value only (project nothing, drop the sidecar).
  function call<T>(self, prompt: string) -> T
      throws root.errors.CallError | root.errors.UnknownError {
    let r = self.call_with<T, null, never>(prompt, (m: ResponseMeta) -> null { null });
    r.value
  }
}

/// The incremental-output capability. Owns `stream`, returning the existing
/// `baml.llm.Stream<TStream, TFinal>` (pull-based: `.next()` for partials, `.final()`
/// for the completed value). `TStream` is the partial (streaming) shape; `TFinal` the
/// completed shape.
interface Streaming requires Provider {
  function stream<TStream, TFinal>(self, prompt: string) -> root.llm.Stream<TStream, TFinal>
      throws root.errors.StreamError | root.errors.UnknownError
}
