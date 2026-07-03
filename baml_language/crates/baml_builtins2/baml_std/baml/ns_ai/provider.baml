// The provider/capability spine (see _plan/llm-provider-plan.md). A `Provider` is a
// bare marker; each capability is an interface that `requires Provider` and owns its
// own interaction method. Built-in providers (OpenAI, …) are ordinary classes that
// `implements` these — the same way a user's custom provider would.
//
// The provider exchange type is the NATIVE `ChatMessage[]` (roles + text/media parts) —
// the whole wire surface is BAML. The `string` prompt entry points are sugar for a single
// user message. `baml.llm.prompt_to_messages` bridges a rendered `PromptAst` (a host
// type) into `ChatMessage[]`; everything downstream of that is BAML.

/// One part of a chat message: text or one media item. A product type (exactly one field
/// set) rather than a union so it crosses the host boundary simply; use the
/// `MessagePart.from_*` constructors.
class MessagePart {
    text: string?,
    image: image?,
    audio: audio?,
    pdf: pdf?,
    video: video?,

    function from_text(text: string) -> MessagePart throws never {
        MessagePart { text: text }
    }
    function from_image(img: image) -> MessagePart throws never {
        MessagePart { image: img }
    }
    function from_audio(a: audio) -> MessagePart throws never {
        MessagePart { audio: a }
    }
}

/// One chat message: a role plus interleaved text/media parts. The native BAML
/// representation of what goes on the wire.
class ChatMessage {
    role: string,
    parts: MessagePart[],

    function user(text: string) -> ChatMessage throws never {
        ChatMessage { role: "user", parts: [MessagePart.from_text(text)] }
    }
    function system(text: string) -> ChatMessage throws never {
        ChatMessage { role: "system", parts: [MessagePart.from_text(text)] }
    }
    function assistant(text: string) -> ChatMessage throws never {
        ChatMessage { role: "assistant", parts: [MessagePart.from_text(text)] }
    }
}

/// Convert a rendered `root.llm.PromptAst` (a host type) into the native `ChatMessage[]`
/// representation — roles preserved, text and media parts interleaved. This is the ONE
/// leaf bridge between the render pipeline and a BAML-authored provider; everything
/// downstream (request building, wire encoding) is BAML.
function prompt_to_messages(prompt: root.llm.PromptAst) -> ChatMessage[] throws never {
    $rust_io_function
}

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

/// Marks a `ResponseMeta` dimension the provider genuinely cannot supply (vs empty this
/// call) — the design's `Supported<T> = T | Unsupported`. (Generic type aliases aren't a
/// language feature yet, so dimensions spell the union inline, e.g. `Logprob[] | Unavailable`.)
class Unavailable {
    reason: string,
}

/// A per-token logprob (scenario 08 enriched output).
class Logprob {
    token: string,
    logprob: float,
}

/// A grounding/citation reference (scenario 08).
class Citation {
    url: string,
    title: string,
}

/// Out-of-band response metadata (finish reason, usage, reasoning, logprobs, citations, …),
/// normalized lazily by each provider. Projected via `HttpProvider.call_with`. Accessors are
/// best-effort (`throws never`): a missing wire shape yields a default / `Unavailable`, so a
/// projection over them is infallible.
interface ResponseMeta {
  function finish_reason(self) -> string throws never
  function usage(self) -> Usage throws never
  function reasoning(self) -> string | Unavailable throws never
  function logprobs(self) -> Logprob[] | Unavailable throws never
  function citations(self) -> Citation[] | Unavailable throws never
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

/// The HTTP request/response capability. Owns `call`/`call_with` (and their
/// message-level primitives). `Body` is the provider's raw response type (associated),
/// lifting the hardcoded `string` of the old `parse<T>(body: string)` so non-text
/// responses fit one interface. The exchange type is the native `ChatMessage[]`; the
/// `prompt: string` entry points are sugar for a single user message.
interface HttpProvider requires Provider {
  type Body

  function build_request<T>(self, messages: ChatMessage[]) -> root.http.Request
      throws root.errors.CallError | root.errors.UnknownError
  function send(self, request: root.http.Request) -> Body
      throws root.errors.CallError | root.errors.UnknownError
  function parse<T>(self, from: Body) -> T
      throws root.errors.CallError | root.errors.UnknownError
  function meta_of(self, from: Body) -> ResponseMeta
      throws root.errors.CallError | root.errors.UnknownError

  // THE primitive: native messages in, value + projected sidecar out. `E2` threads the
  // projection's error channel (the `Iterator.map` pattern).
  function call_messages_with<T, V, E2>(
    self,
    messages: ChatMessage[],
    project: (ResponseMeta) -> V throws E2,
  ) -> CallResult<T, V>
      throws root.errors.CallError | root.errors.UnknownError | E2 {
    let body = self.send(self.build_request<T>(messages));
    CallResult<T, V> { value: self.parse<T>(body), meta: project(self.meta_of(body)) }
  }

  // Derived: native messages, value only.
  function call_messages<T>(self, messages: ChatMessage[]) -> T
      throws root.errors.CallError | root.errors.UnknownError {
    let r = self.call_messages_with<T, null, never>(messages, (m: ResponseMeta) -> null { null });
    r.value
  }

  // Sugar: a plain string prompt is a single user message. (The leading `let` keeps the
  // parser from mis-reading a `prompt`-param function as an LLM declarative body.)
  function call_with<T, V, E2>(
    self,
    prompt: string,
    project: (ResponseMeta) -> V throws E2,
  ) -> CallResult<T, V>
      throws root.errors.CallError | root.errors.UnknownError | E2 {
    let messages = [ChatMessage.user(prompt)];
    self.call_messages_with<T, V, E2>(messages, project)
  }

  // Sugar: value only from a plain string prompt.
  function call<T>(self, prompt: string) -> T
      throws root.errors.CallError | root.errors.UnknownError {
    let r = self.call_with<T, null, never>(prompt, (m: ResponseMeta) -> null { null });
    r.value
  }
}

/// The incremental-output capability. Owns `stream_messages` (native) with a `stream`
/// string sugar, returning the existing `baml.llm.Stream<TStream, TFinal>` (pull-based:
/// `.next()` for partials, `.final()` for the completed value). `TStream` is the partial
/// (streaming) shape; `TFinal` the completed shape.
interface Streaming requires Provider {
  function stream_messages<TStream, TFinal>(self, messages: ChatMessage[])
      -> root.llm.Stream<TStream, TFinal>
      throws root.errors.StreamError | root.errors.UnknownError

  function stream<TStream, TFinal>(self, prompt: string) -> root.llm.Stream<TStream, TFinal>
      throws root.errors.StreamError | root.errors.UnknownError {
    let messages = [ChatMessage.user(prompt)];
    self.stream_messages<TStream, TFinal>(messages)
  }
}

/// Constrained (grammar/regex/choice) decoding with a by-construction guarantee
/// (scenario 03). Deliberately has NO default method: a token-level guarantee cannot be
/// faked on top of an unconstrained `.call`, so a provider either has real constrained
/// decoding (self-hosted vLLM/llama.cpp/Outlines) or it does not implement this at all.
/// A client that lacks it fails the capability `match` at runtime — the guarantee is a
/// runtime promise the type system can't lift (design gap B1).
interface Constrained requires Provider {
  function decode<T>(self, prompt: string, pattern: string) -> T
      throws root.errors.CallError | root.errors.UnknownError
}
