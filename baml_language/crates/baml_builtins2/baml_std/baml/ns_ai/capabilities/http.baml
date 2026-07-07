// The request/response capability — owns `call`/`call_with` and their message-level
// primitives. This is the capability most providers implement first.

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
