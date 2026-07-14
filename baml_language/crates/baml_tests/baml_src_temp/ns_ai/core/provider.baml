// The empty root capability. Specific operations require a narrower provider
// interface so a realtime channel cannot accidentally promise a final T.
interface Provider {
  function name(self) -> string throws never { "provider" }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
}

interface DriveProvider requires Provider {
  function drive<T>(self, task: Task<T>) -> Response<T>
    throws baml.errors.UnknownError | baml.errors.Unsupported
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null }
  }
}

interface GenerationProvider requires Provider {
  function generate<T>(self, task: Task<T>) -> Response<T>
    throws baml.errors.UnknownError | baml.errors.Unsupported
}

interface StreamingProvider requires Provider {
  function stream<TPartial, T>(self, task: Task<T>) -> baml.llm.Stream<TPartial, T>
    throws baml.errors.StreamError | baml.errors.UnknownError | baml.errors.Unsupported
}
