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
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    for (let member in self.members) {
      match (member) {
        let provider: GenerationProvider => {
          let response = provider.generate<T>(task.with_provider(member)) catch (e) {
            _ => continue,
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
      throws baml.errors.UnknownError | baml.errors.Unsupported {
    for (let member in self.members) {
      match (member) {
        let provider: DriveProvider => {
          let policy = provider.replay_policy<T>(task.with_provider(member));
          if (!can_replay(policy)) {
            throw baml.errors.Unsupported {
              message: "fallback refuses a provider without a replay-safe policy",
            };
          }
          let response = provider.drive<T>(task.with_provider(member)) catch (e) {
            _ => continue,
          };
          return response;
        },
        _ => {},
      }
    }
    throw baml.errors.UnknownError { data: null, message: ["all fallback providers failed"] }
  }

  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
  }
}

function fallback(providers: Provider[]) -> Fallback throws never {
  Fallback { members: providers }
}
