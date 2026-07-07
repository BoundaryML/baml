// The request/response capability — owns `call`/`call_with` and their message-level
// primitives. This is the capability most providers implement first.

//baml:llm_capability
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

// ── Companion drivers (DCP §1.2) ────────────────────────────────────────────
// The generated `Foo(args…, client?)` / `Foo$with(…)` companions of an LLM
// declarative function delegate here. All capability negotiation lives in
// these bodies — the compiler only splices names and substitutes `T`.

//baml:llm_companion(call)
/// Drive a value-producing call: prefer `HttpProvider`, else drain a
/// `Streaming` provider to its final value, else the capability is absent.
function drive_call<T>(client: Provider, prompt: root.llm.PromptAst) -> T
    throws root.errors.CallError | root.errors.StreamError | root.errors.UnknownError {
  let messages = prompt_to_messages(prompt);
  match (client) {
    let h: HttpProvider => h.call_messages<T>(messages),
    let s: Streaming => s.stream_messages<T, T>(messages).final() catch (e) {
      let se: root.errors.StreamError => throw se,
      _ => throw root.errors.UnknownError {
        data: e,
        message: ["stream drain failed while degrading call -> stream"],
      },
    },
    _ => throw root.errors.Unsupported {
      message: "client's provider supports neither HttpProvider nor Streaming",
    },
  }
}

//baml:llm_companion(with)
/// Drive a value+sidecar call (`CallResult<T, V>`): the projection's value and
/// error types (`V`, `E2`) are passthrough generics inferred at the call site.
function drive_with<T, V, E2>(
  client: Provider,
  prompt: root.llm.PromptAst,
  project: (ResponseMeta) -> V throws E2,
) -> CallResult<T, V>
    throws root.errors.CallError | root.errors.UnknownError | E2 {
  let messages = prompt_to_messages(prompt);
  match (client) {
    let h: HttpProvider => h.call_messages_with<T, V, E2>(messages, project),
    _ => throw root.errors.Unsupported {
      message: "client's provider does not support HttpProvider (call_with)",
    },
  }
}
