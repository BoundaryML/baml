class Retry {
    inner: Provider,
    max_attempts: int,
}

implements Provider for Retry {
  function name(self) -> string throws never { "retry(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
}

implements GenerationProvider for Retry {
  function generate<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    //# Resolve the generation capability
    let provider = match (self.inner) {
      let capability: GenerationProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot generate" },
    };
    let policy = ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null };
    let attempt = 1;
    //# Retry replay-safe generation
    while (attempt <= self.max_attempts) {
      //## Attempt provider generation
      let result = provider.generate<T>(task.with_provider(self.inner)) catch (e) {
        let failure: root.ai.Failure => {
          //### A failure that cannot replay is rethrown as itself — its
          //### classification (is_transient, effects) already says why.
          if (attempt >= self.max_attempts || !root.ai.internal._may_replay(policy, failure)) { throw e; }
          attempt = attempt + 1;
          continue;
        },

        _ => throw baml.errors.UnknownError.with_message<never>(e, "retry(" + self.inner.name() + ")"),
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }
}

implements CompletionProvider for Retry {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    //# Resolve the drive capability
    let provider = match (self.inner) {
      let capability: CompletionProvider => capability,
      _ => throw baml.errors.Unsupported { message: "retry inner cannot drive" },
    };
    //# Load the provider replay policy
    let policy = provider.replay_policy<T>(task.with_provider(self.inner));
    let attempt = 1;
    //# Retry replay-safe drive calls
    while (attempt <= self.max_attempts) {
      //## Attempt provider drive
      let result = provider.complete<T>(task.with_provider(self.inner)) catch (e) {
        let failure: root.ai.Failure => {
          //### Rethrow anything that cannot replay: exhausted attempts, or a
          //### failure whose facts (terminal, effectful) make replay unsafe.
          if (attempt >= self.max_attempts || !root.ai.internal._may_replay(policy, failure)) { throw e; }
          attempt = attempt + 1;
          continue;
        },
        _ => throw baml.errors.UnknownError.with_message<never>(e, "retry(" + self.inner.name() + ")"),
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }

  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    match (self.inner) {
      let provider: CompletionProvider => provider.replay_policy<T>(task.with_provider(self.inner)),
      _ => ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null },
    }
  }
}

function retry(provider: Provider, max_attempts: int) -> Retry throws never {
    Retry {
        inner: provider,
        max_attempts: if (max_attempts < 1) {
            1
        } else {
            max_attempts
        },
    }
}
