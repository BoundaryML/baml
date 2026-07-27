class Fallback {
    first: Provider,
    rest: Provider[],

    function providers(self) -> Provider[] throws never {
        [self.first].concat(self.rest)
    }
}

implements Provider for Fallback {
  function name(self) -> string throws never { "fallback" }
  function render_shorthand(self) -> string throws never { self.first.render_shorthand() }
}

implements GenerationProvider for Fallback {
  function generate<T>(self, task: Task<T>) -> Response<T>
      throws root.ai.CallError | baml.errors.UnknownError {
    let policy = ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null };
    //# Try generation providers in order
    for (let member in self.providers()) {
      //## Select a generation-capable member
      match (member) {
        let provider: GenerationProvider => {
          //### Attempt generation
          let response = provider.generate<T>(task.with_provider(member)) catch (e) {
            let failure: root.ai.Failure => {
              //#### Continue only after a replay-safe failure
              if (may_replay(policy, failure)) { continue; }
              throw e;
            },
            _ => throw e,
          };
          return response;
        },
        _ => {},
      }
    }
    throw baml.errors.UnknownError { data: null, message: ["all fallback providers failed"] }
  }
}

implements CompletionProvider for Fallback {
  function complete<T>(self, task: Task<T>) -> Response<T>
      throws root.ai.CallError | baml.errors.UnknownError {
    //# Try drive providers in order
    for (let member in self.providers()) {
      //## Select a drive-capable member
      match (member) {
        let provider: CompletionProvider => {
          let policy = provider.replay_policy<T>(task.with_provider(member));
          //### Attempt provider drive
          let response = provider.complete<T>(task.with_provider(member)) catch (e) {
            let failure: root.ai.Failure => {
              //#### Continue only after a replay-safe failure
              if (may_replay(policy, failure)) { continue; }
              if (failure.is_retryable()) {
                throw root.ai.CannotRetry {
                  message: "fallback refused to replay " + member.name(),
                };
              }
              throw e;
            },
            _ => throw e,
          };
          return response;
        },
        _ => {},
      }
    }
    throw baml.errors.UnknownError { data: null, message: ["all fallback providers failed"] }
  }

  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    //# Prove the entire fallback chain is replay-safe
    for (let member in self.providers()) {
      //## Inspect each member policy
      match (member) {
        let provider: CompletionProvider => {
          let policy = provider.replay_policy<T>(task.with_provider(member));
          if (policy.kind != ReplayKind.Safe) {
            return ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null };
          }
        },
        _ => return ReplayPolicy { kind: ReplayKind.Never, idempotency_key: null },
      }
    }
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
  }
}

function fallback(providers: Provider[]) -> Fallback throws baml.errors.InvalidArgument {
    match (providers.at(0)) {
        let first: Provider => {
            Fallback { first: first, rest: providers.slice(1, providers.length()) }
        },
        null => {
            throw baml.errors.InvalidArgument {
      message: "fallback requires at least one provider",
    }
        },
    }
}
