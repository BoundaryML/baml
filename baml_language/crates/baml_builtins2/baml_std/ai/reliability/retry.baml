/// Delay policy between replayed model steps. Exponential:
/// `initial_ms * multiplier^(attempt-1)`, capped at `max_ms`. When the
/// failure is `RateLimited` and the provider sent `retry_after_ms`, that
/// hint overrides the computed delay — the provider knows its own window.
class Backoff {
    initial_ms: int,
    multiplier: int,
    max_ms: int,

    function default() -> Backoff throws never {
        Backoff { initial_ms: 250, multiplier: 2, max_ms: 10000 }
    }

    /// The delay to sleep after the failure of `attempt` (1-based).
    function delay_ms(self, attempt: int, failure: ai.Failure) -> int throws never {
        match (failure) {
            let limited: ai.RateLimited => match (limited.retry_after_ms) {
                let hint: int => return hint.max(0),
                null => {},
            },
            _ => {},
        }
        let delay = self.initial_ms;
        let i = 1;
        while (i < attempt && delay < self.max_ms) {
            delay = delay * self.multiplier;
            i = i + 1;
        }
        delay.clamp(0, self.max_ms)
    }
}

class Retry {
    inner: Provider,
    max_attempts: int,
    // The caller's judgment of which failures are worth replaying. Null
    // uses the default judgment (decline Refused/InvalidRequest/ParseFailed,
    // replay the rest); effect safety (`Effects.None`) is always enforced
    // regardless.
    retry_if: ((ai.Failure) -> bool throws never)?,
    // Null disables sleeping between attempts.
    backoff: Backoff?,
}

class RetryConversation {
    owner: Retry,
    inner: Conversation,

    implements Conversation {
        function provider(self) -> Provider throws never {
            self.owner
        }

        function messages(self) -> Messages throws never {
            self.inner.messages()
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.inner.output_type_fingerprint()
        }
    }
}

implements Provider for Retry {
  function name(self) -> string throws never { "retry(" + self.inner.name() + ")" }
  function render_shorthand(self) -> string throws never { self.inner.render_shorthand() }
  // Retry is a thin wrapper: ownership checks may walk through it.
  function delegate(self) -> Provider? throws never { self.inner }
}

// Retry wraps individual provider turns. It never replays the Agent lifecycle
// or any application tool that has already run.
implements AgentProvider for Retry {
  function begin<T>(self, task: Task<T>) -> Conversation
      throws ai.Failure | baml.errors.UnknownError {
    let provider = match (self.inner) {
      let capability: AgentProvider => capability,
      _ => throw baml.errors.Unsupported {
        message: "retry inner cannot run an Agent: " + self.inner.name(),
      },
    };
    RetryConversation {
      owner: self,
      inner: provider.begin<T>(task.with_provider(self.inner)),
    }
  }

  function step<T>(
    self,
    conversation: Conversation,
    tools: ai.tools.Tool[],
  ) -> ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: RetryConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received a conversation owned by another retry instance",
      )
    }
    let provider = match (self.inner) {
      let capability: AgentProvider => capability,
      _ => throw baml.errors.Unsupported {
        message: "retry inner cannot run an Agent: " + self.inner.name(),
      },
    };
    let attempt = 1;
    //# Retry only a replay-safe provider turn
    while (attempt <= self.max_attempts) {
      let result = provider.step<T>(state.inner, tools) catch (e) {
        let failure: ai.Failure => {
          if (
            attempt >= self.max_attempts
              || !ai.internal._may_retry_model_step(failure, self.retry_if)
          ) {
            throw e;
          }
          //# Back off before the replay; a provider retry_after hint wins.
          match (self.backoff) {
            let policy: Backoff => {
              let _ = baml.sys.sleep(baml.time.Duration.from_milliseconds(policy.delay_ms(attempt, failure))) catch (io) {
                _ => null,
              };
            },
            null => {},
          }
          attempt = attempt + 1;
          continue;
        },
        _ => throw baml.errors.UnknownError.with_message<never>(
          e,
          "retry(" + self.inner.name() + ") step",
        ),
      };
      return result;
    }
    baml.sys.panic("retry exhausted without returning or throwing")
  }

  function submit(
    self,
    conversation: Conversation,
    results: ai.tools.ToolResult[],
  ) -> Conversation throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: RetryConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received a conversation owned by another retry instance",
      )
    }
    let provider = match (self.inner) {
      let capability: AgentProvider => capability,
      _ => throw baml.errors.Unsupported {
        message: "retry inner cannot run an Agent: " + self.inner.name(),
      },
    };
    state.inner = provider.submit(state.inner, results);
    state
  }
}

implements ConversationAppendProvider for Retry {
  function append_messages(
    self,
    conversation: Conversation,
    messages: Messages,
  ) -> null throws ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: RetryConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received another provider's conversation",
      ),
    };
    if (!ai.internal._same_provider_instance(state.owner, self)) {
      throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received a conversation owned by another retry instance",
      )
    }
    let provider = match (self.inner) {
      let capability: ConversationAppendProvider => capability,
      _ => throw baml.errors.Unsupported {
        message: "retry inner cannot append exact conversation messages: " + self.inner.name(),
      },
    };
    provider.append_messages(state.inner, messages)
  }
}

function retry(
    provider: Provider,
    max_attempts: int,
    retry_if: ((ai.Failure) -> bool throws never)? = null,
    backoff: Backoff? = Backoff.default(),
) -> Retry throws never {
    Retry {
        inner: provider,
        max_attempts: if (max_attempts < 1) {
            1
        } else {
            max_attempts
        },
        retry_if: retry_if,
        backoff: backoff,
    }
}
