class Retry {
  inner: Provider,
  max_attempts: int,
}

implements Provider for Retry {
  function name(self) -> string throws never { "retry(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements GenerationProvider for Retry {
  function generate<T>(self, task: Task<T>) -> Response<T>
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    let provider = match (self.inner) {
      let capability: GenerationProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot generate" },
    };
    let attempt = 1;
    while (attempt <= self.max_attempts) {
      let result = provider.generate<T>(task.with_provider(self.inner)) catch (e) {
        _ => {
          if (attempt < self.max_attempts) {
            attempt = attempt + 1;
            continue;
          }
          throw e;
        },
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }
}

implements DriveProvider for Retry {
  function drive<T>(self, task: Task<T>) -> Response<T>
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    let provider = match (self.inner) {
      let capability: DriveProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot drive" },
    };
    let policy = provider.replay_policy<T>(task.with_provider(self.inner));
    if (!can_replay(policy)) {
      throw baml.errors.Unsupported {
        message: "retry requires a safe or idempotent replay policy",
      };
    }
    let attempt = 1;
    while (attempt <= self.max_attempts) {
      let result = provider.drive<T>(task.with_provider(self.inner)) catch (e) {
        _ => {
          if (attempt < self.max_attempts) {
            attempt = attempt + 1;
            continue;
          }
          throw e;
        },
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }

  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    match (self.inner) {
      let provider: DriveProvider => provider.replay_policy<T>(task.with_provider(self.inner)),
      _ => ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null },
    }
  }
}

function retry(provider: Provider, max_attempts: int) -> Retry throws never {
  Retry { inner: provider, max_attempts: max_attempts }
}
