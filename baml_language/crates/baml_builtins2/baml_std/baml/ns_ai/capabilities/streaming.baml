// The incremental-output capability.

//baml:llm_capability
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

//baml:llm_companion(stream)
/// Drive a streaming call (the generated `Foo$stream` companion delegates
/// here). `TPartial` is the stream-expanded return-type slot. No honest
/// call->stream degrade exists yet: buffering an `HttpProvider.call` into a
/// `Stream` needs a stream constructor (`stream_unfold` — net-new host
/// surface), so a non-Streaming provider is a typed `Unsupported`.
function drive_stream<TPartial, T>(client: Provider, prompt: root.llm.PromptAst)
    -> root.llm.Stream<TPartial, T>
    throws root.errors.StreamError | root.errors.UnknownError {
  let messages = prompt_to_messages(prompt);
  match (client) {
    let s: Streaming => s.stream_messages<TPartial, T>(messages),
    _ => throw root.errors.Unsupported {
      message: "client's provider does not support Streaming",
    },
  }
}
