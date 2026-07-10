enum ReplayKind {
  Safe
  RequiresIdempotencyKey
  Never
}

enum FailureKind {
  Transport
  RateLimit
  InvalidRequest
  Refusal
  Parse
  Unsupported
  Cancelled
}

enum CommitState {
  NotCommitted
  Committed
  Unknown
}

interface Failure {
  function kind(self) -> FailureKind throws never
  function commit_state(self) -> CommitState throws never
  function retry_after(self) -> baml.time.Duration? throws never { null }
  function is_resumable(self) -> bool throws never { false }
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}

function may_replay(policy: ReplayPolicy, failure: Failure) -> bool throws never {
  match (failure.kind()) {
    FailureKind.InvalidRequest => return false,
    FailureKind.Refusal => return false,
    FailureKind.Unsupported => return false,
    FailureKind.Cancelled => return false,
    _ => {},
  }
  match (policy.kind) {
    ReplayKind.Safe => true,
    ReplayKind.RequiresIdempotencyKey => {
      policy.idempotency_key != null && failure.commit_state() != CommitState.Committed
    },
    ReplayKind.Never => false,
  }
}

class Retry {
  inner: Provider,
  max: int,

  implements Provider {
    function display_name(self) -> string throws never { "retry" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let attempt = 0;
      while (true) {
        match (self.inner) {
          let generate: Generate => {
            let response = generate.generate<T>(request.for_provider(self.inner)) catch (e) {
              _ => {
                if (attempt < self.max) {
                  attempt = attempt + 1;
                  continue;
                }
                throw e;
              },
            };
            return response;
          },
          _ => throw baml.errors.Unsupported { message: "retry inner cannot generate" },
        }
      }
      baml.sys.panic("unreachable")
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      match (self.inner) {
        let streaming: Streaming => streaming.stream<TPartial, T>(request.for_provider(self.inner)),
        _ => throw baml.errors.Unsupported { message: "retry inner cannot stream" },
      }
    }
  }
}

class Fallback {
  members: Provider[],

  implements Provider {
    function display_name(self) -> string throws never { "fallback" }
    function render_shorthand(self) -> string throws never {
      match (self.members.at(0)) {
        let first: Provider => first.render_shorthand(),
        null => "openai/gpt-5.6-luna",
      }
    }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      for (let member in self.members) {
        match (member) {
          let generate: Generate => {
            let response = generate.generate<T>(request.for_provider(member)) catch (e) {
              _ => continue,
            };
            return response;
          },
          _ => {},
        }
      }
      throw baml.errors.UnknownError { data: null, message: ["fallback: all providers failed"] }
    }
  }

  implements Streaming {
    function stream<TPartial, T>(self, request: Request<T>) -> baml.llm.Stream<TPartial, T>
        throws baml.errors.StreamError | baml.errors.UnknownError {
      for (let member in self.members) {
        match (member) {
          let streaming: Streaming => {
            let result = streaming.stream<TPartial, T>(request.for_provider(member)) catch (e) {
              _ => continue,
            };
            return result;
          },
          _ => {},
        }
      }
      throw baml.errors.UnknownError { data: null, message: ["fallback: no streaming provider"] }
    }
  }
}

class UsageMeter {
  entries: Meta[],

  function record(self, meta: Meta) -> null throws never {
    let _ = self.entries.push(meta);
    null
  }
  function calls(self) -> int throws never { self.entries.length() }
  function total(self) -> Usage throws never {
    let input = 0;
    let output = 0;
    for (let entry in self.entries) {
      match (entry.usage) {
        let usage: Usage => {
          input = input + usage.input_tokens;
          output = output + usage.output_tokens;
        },
        null => {},
      }
    }
    Usage { input_tokens: input, output_tokens: output }
  }
}

class Traced {
  inner: Provider,
  meter: UsageMeter,

  implements Provider {
    function display_name(self) -> string throws never { "traced" }
    function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  }

  implements Generate {
    function generate<T>(self, request: Request<T>) -> Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      match (self.inner) {
        let generate: Generate => {
          let response = generate.generate<T>(request.for_provider(self.inner));
          self.meter.record(response.meta);
          response
        },
        _ => throw baml.errors.Unsupported { message: "traced inner cannot generate" },
      }
    }
  }
}
