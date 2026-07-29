// The empty root capability. Specific operations require a narrower provider
// interface so a realtime channel cannot accidentally promise a final T.
interface Provider {
  function name(self) -> string throws never { "provider" }
  // The provider owns the primitive client used to render its prompt. There is
  // no portable default because role, media, and schema behavior vary by
  // provider.
  function render_shorthand(self) -> string throws never
}

// Bounded provider-default completion. The implementation may be one model
// interaction or a provider-managed loop, but it must finish with one typed
// ResponseWithMetadata<T> or throw.
interface CompletionProvider requires Provider {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}

interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
    throws root.ai.Failure | baml.errors.UnknownError
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws root.ai.Failure | baml.errors.UnknownError | baml.errors.Unsupported
}
