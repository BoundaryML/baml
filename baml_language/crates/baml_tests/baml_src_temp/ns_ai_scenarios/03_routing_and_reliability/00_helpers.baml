class TierRouter {
  fast: root.ai.Provider,
  careful: root.ai.Provider,
  implements root.ai.Router {
    function choose(self, task_name: string, tags: map<string, string>) -> root.ai.Route throws never {
      if (tags.get("tier") == "pro") {
        root.ai.Route { name: "careful", provider: self.careful, reason: "pro customer" }
      } else {
        root.ai.Route { name: "fast", provider: self.fast, reason: "default" }
      }
    }
  }
}

class EffectfulProvider {
  inner: root.ai.FakeProvider,
  implements root.ai.Provider {
    function name(self) -> string throws never { "effectful" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }
  implements root.ai.DriveProvider {
    function drive<T>(self, task: root.ai.Task<T>) -> root.ai.Response<T>
        throws baml.errors.UnknownError {
      self.inner.drive<T>(task.with_provider(self.inner))
    }
    function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
      root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Never, idempotency_key: null }
    }
  }
}

