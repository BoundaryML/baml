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
  function generate<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    let policy = ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null };
    //# Track replayable failures so exhaustion rethrows something real
    let failures: root.ai.Failure[] = [];
    //# Try generation providers in order
    for (let member in self.providers()) {
      //## Select a generation-capable member
      match (member) {
        let provider: GenerationProvider => {
          //### Attempt generation
          let response = provider.generate<T>(task.with_provider(member)) catch (e) {
            let failure: root.ai.Failure => {
              //#### Continue only after a replay-safe failure
              if (root.ai.internal._may_replay(policy, failure)) {
                failures.push(failure);
                continue;
              }
              throw e;
            },
            _ => throw baml.errors.UnknownError.with_message<never>(e, "fallback(" + member.name() + ")"),
          };
          return response;
        },
        _ => {},
      }
    }
    //# Exhausted: the last member's failure IS the fallback's failure — outer
    //# layers still see its classification instead of an opaque wrapper.
    match (failures.at(failures.length() - 1)) {
      let failure: root.ai.Failure => throw failure,
      null => throw baml.errors.Unsupported { message: "no fallback member can generate" },
    }
  }
}

implements CompletionProvider for Fallback {
  function complete<T>(self, task: Task<T>) -> ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    let failures: root.ai.Failure[] = [];
    //# Try drive providers in order
    for (let member in self.providers()) {
      //## Select a drive-capable member
      match (member) {
        let provider: CompletionProvider => {
          let policy = provider.replay_policy<T>(task.with_provider(member));
          //### Attempt provider drive
          let response = provider.complete<T>(task.with_provider(member)) catch (e) {
            let failure: root.ai.Failure => {
              //#### Continue only after a replay-safe failure; anything else
              //#### is rethrown as itself, classification intact.
              if (root.ai.internal._may_replay(policy, failure)) {
                failures.push(failure);
                continue;
              }
              throw e;
            },
            _ => throw baml.errors.UnknownError.with_message<never>(e, "fallback(" + member.name() + ")"),
          };
          return response;
        },
        _ => {},
      }
    }
    //# Exhausted: rethrow the last real failure, not an opaque wrapper
    match (failures.at(failures.length() - 1)) {
      let failure: root.ai.Failure => throw failure,
      null => throw baml.errors.Unsupported { message: "no fallback member can drive" },
    }
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
