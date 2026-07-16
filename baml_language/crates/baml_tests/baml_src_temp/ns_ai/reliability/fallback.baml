class Fallback {
  members: Provider[],
}

implements Provider for Fallback {
  function name(self) -> string throws never { "fallback" }
  function render_shorthand(self) -> string throws never {
    match (self.members.at(0)) {
      let first: Provider => first.render_shorthand(),
      null => "openai/gpt-5.6-luna",
    }
  }
}

implements GenerationProvider for Fallback {
  function generate<T>(self, task: Task<T>) -> Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    let policy = ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null };
    for (let member in self.members) {
      match (member) {
        let provider: GenerationProvider => {
          let response = provider.generate<T>(task.with_provider(member)) catch (e) {
            let failure: baml.errors.Failure => {
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

implements DriveProvider for Fallback {
  function drive<T>(self, task: Task<T>) -> Response<T>
      throws baml.errors.CallError | baml.errors.UnknownError {
    for (let member in self.members) {
      match (member) {
        let provider: DriveProvider => {
          let policy = provider.replay_policy<T>(task.with_provider(member));
          let response = provider.drive<T>(task.with_provider(member)) catch (e) {
            let failure: baml.errors.Failure => {
              if (may_replay(policy, failure)) { continue; }
              if (failure.is_retryable()) {
                throw baml.errors.CannotRetry {
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
    for (let member in self.members) {
      match (member) {
        let provider: DriveProvider => {
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

function fallback(providers: Provider[]) -> Fallback throws never {
  Fallback { members: providers }
}
