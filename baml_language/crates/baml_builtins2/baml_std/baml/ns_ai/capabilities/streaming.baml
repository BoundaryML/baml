// The incremental-output capability.

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
