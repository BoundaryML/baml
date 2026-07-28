class EffectfulProvider {
  inner: root.ai.FakeProvider,
  implements root.ai.Provider {
    function name(self) -> string throws never { "effectful" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
  implements root.ai.CompletionProvider {
    function complete<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T>
        throws root.ai.CallError | baml.errors.UnknownError {
      self.inner.complete<T>(task.with_provider(self.inner))
    }
    function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
      root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Never, idempotency_key: null }
    }
  }
}
