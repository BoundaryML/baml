// The empty root capability. Specific operations require a narrower provider
// interface so a realtime channel cannot accidentally promise a final T.
interface Provider {
  function name(self) -> string throws never { "provider" }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
}

// Bounded provider-default completion. The implementation may be one model
// interaction or a provider-managed loop, but it must finish with one typed
// Response<T> or throw.
interface CompletionProvider requires Provider {
  function complete<T>(self, task: Task<T>) -> Response<T>
    throws root.ai.CallError | baml.errors.UnknownError
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}

interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> Response<T>
    throws root.ai.CallError | baml.errors.UnknownError
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws root.ai.StreamError | baml.errors.UnknownError | baml.errors.Unsupported
}
