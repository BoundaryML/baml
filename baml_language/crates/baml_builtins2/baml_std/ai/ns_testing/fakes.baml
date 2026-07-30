enum FakeFailureMode {
    Retryable,
    Terminal,
    Unknown,
}

// A recognized strategy shorthand gives deterministic providers a prompt
// rendering context without coupling the portable ai namespace to a model
// vendor.
function _fake_render_shorthand() -> string throws never {
    "baml-fallback/fake"
}

class FakeProvider {
    provider_name: string,
    output: string,
    calls: int,
    failures_remaining: int,
    failure_mode: FakeFailureMode,
}

class FakeConversation {
    owner: FakeProvider,
    history: ai.MessageHistory,
    declared_output_type: string,

    implements ai.Conversation {
        function provider(self) -> ai.Provider throws never {
            self.owner
        }

        function messages(self) -> ai.Messages throws never {
            self.history
        }
        function output_type_fingerprint(self) -> string? throws never {
            self.declared_output_type
        }
    }
}

implements ai.Provider for FakeProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements ai.AgentProvider for FakeProvider {
  function begin<T>(self, task: ai.Task<T>) -> ai.Conversation throws never {
    FakeConversation {
      owner: self,
      history: task.messages().to_history(),
      declared_output_type: ai.internal._output_type_fingerprint<T>(),
    }
  }

  function step<T>(
    self,
    conversation: ai.Conversation,
    tools: ai.tools.Tool[],
  ) -> ai.ModelStep<T> throws ai.Failure | baml.errors.UnknownError {
    ai.internal._require_conversation_output_type<T>(
      conversation,
      self.name(),
    );
    match (conversation) {
      let state: FakeConversation => {
        if (!ai.internal._same_provider_instance(state.owner, self)) {
          throw baml.errors.UnknownError.with_message<never>(
            state.owner.name(),
            "fake conversation belongs to another provider",
          )
        }
      },
      _ => throw baml.errors.UnknownError.with_message<never>(
        null,
        "fake provider received another provider's conversation",
      ),
    };
    //# Count this provider attempt
    self.calls = self.calls + 1;
    //# Inject the configured retryable, terminal, or unknown failure. The
    //# fakes throw the SAME default types real providers throw, so retry and
    //# fallback tests exercise the live classification path.
    if (self.failures_remaining > 0) {
      self.failures_remaining = self.failures_remaining - 1;
      match (self.failure_mode) {
        FakeFailureMode.Retryable => throw ai.RateLimited {
          provider: self.provider_name,
          retry_after_ms: 10,
        },
        FakeFailureMode.Terminal => throw ai.Refused {
          provider: self.provider_name,
          reason: "injected terminal fake failure",
        },
        FakeFailureMode.Unknown => throw baml.errors.UnknownError.with_message<never>(
          "injected unclassified fake failure",
          "fake provider",
        ),
      }
    }
    //# Parse the deterministic fixture as the task's declared output type
    let value = baml.sap.parse<T>(self.output) catch (e) {
      _ => throw baml.errors.UnknownError.with_message<never>(e, "fake output parse failed"),
    };
    //# Return the typed value from exactly one fake model turn
    ai.ModelStep<T> {
      outcome: value,
      metadata: ai.ResponseMetadata {
        provider: self.provider_name,
        model: "fake",
        request_id: "fake-" + self.calls.to_string(),
        finish_reason: "stop",
        usage: ai.Usage {
          input_tokens: 1,
          output_tokens: 1,
          cached_input_tokens: 0,
          reasoning_tokens: 0,
          cost_usd: 0.0,
        },
        attributes: {},
        raw: null,
      },
    }
  }

  function submit(
    self,
    conversation: ai.Conversation,
    results: ai.tools.ToolResult[],
  ) -> ai.Conversation throws ai.Failure {
    if (results.length() != 0) {
      throw ai.InvalidRequest {
        provider: self.name(),
        status_code: null,
        detail: "fake provider has no pending tool calls",
      }
    }
    conversation
  }
}

function fake_output_provider(output: string) -> FakeProvider throws never {
    FakeProvider {
        provider_name: "fake",
        output: output,
        calls: 0,
        failures_remaining: 0,
        failure_mode: FakeFailureMode.Retryable,
    }
}
