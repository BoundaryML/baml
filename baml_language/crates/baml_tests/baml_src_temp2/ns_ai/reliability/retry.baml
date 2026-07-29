class Retry {
    inner: Provider,
    max_attempts: int,
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
}

// Retry wraps individual provider turns. It never replays the Agent lifecycle
// or any application tool that has already run.
implements AgentProvider for Retry {
  function begin<T>(self, task: Task<T>) -> Conversation
      throws root.ai.Failure | baml.errors.UnknownError {
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
    tools: root.ai.tools.Tool[],
  ) -> ModelStep<T> throws root.ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: RetryConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received another provider's conversation",
      ),
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
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
        let failure: root.ai.Failure => {
          if (
            attempt >= self.max_attempts
              || !root.ai.internal._may_retry_model_step(failure)
          ) {
            throw e;
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
    results: root.ai.tools.ToolResult[],
  ) -> Conversation throws root.ai.Failure | baml.errors.UnknownError {
    let state = match (conversation) {
      let value: RetryConversation => value,
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "retry received another provider's conversation",
      ),
    };
    if (!root.ai.internal._same_provider_instance(state.owner, self)) {
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
