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
      throws baml.errors.CallError | baml.errors.UnknownError {
    let provider = match (self.inner) {
      let capability: GenerationProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot generate" },
    };
    let policy = ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null };
    let attempt = 1;
    while (attempt <= self.max_attempts) {
      let result = provider.generate<T>(task.with_provider(self.inner)) catch (e) {
        let failure: baml.errors.Failure => {
          if (attempt >= self.max_attempts || !may_replay(policy, failure)) { throw e; }
          attempt = attempt + 1;
          continue;
        },
        _ => throw e,
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }
}

implements DriveProvider for Retry {
  function drive<T>(self, task: Task<T>) -> Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    let provider = match (self.inner) {
      let capability: DriveProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot drive" },
    };
    let policy = provider.replay_policy<T>(task.with_provider(self.inner));
    let attempt = 1;
    while (attempt <= self.max_attempts) {
      let result = provider.drive<T>(task.with_provider(self.inner)) catch (e) {
        let failure: baml.errors.Failure => {
          if (attempt >= self.max_attempts) { throw e; }
          if (!may_replay(policy, failure)) {
            if (failure.is_retryable()) {
              throw baml.errors.CannotRetry {
                message: "retry refused to replay " + self.inner.name(),
              };
            }
            throw e;
          }
          attempt = attempt + 1;
          continue;
        },
        _ => throw e,
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
  Retry {
    inner: provider,
    max_attempts: if (max_attempts < 1) { 1 } else { max_attempts },
  }
}
