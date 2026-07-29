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

implements root.ai.Provider for FakeProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { _fake_render_shorthand() }
}

implements root.ai.GenerationProvider for FakeProvider {
  function generate<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    //# Count this provider attempt
    self.calls = self.calls + 1;
    //# Inject the configured retryable, terminal, or unknown failure. The
    //# fakes throw the SAME default types real providers throw, so retry and
    //# fallback tests exercise the live classification path.
    if (self.failures_remaining > 0) {
      self.failures_remaining = self.failures_remaining - 1;
      match (self.failure_mode) {
        FakeFailureMode.Retryable => throw root.ai.RateLimited {
          provider: self.provider_name,
          retry_after_ms: 10,
        },
        FakeFailureMode.Terminal => throw root.ai.Refused {
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
    //# Return the typed value with deterministic response metadata
    root.ai.ResponseWithMetadata<T> {
      value: value,
      metadata: root.ai.ResponseMetadata {
        provider: self.provider_name,
        model: "fake",
        request_id: "fake-" + self.calls.to_string(),
        finish_reason: "stop",
        usage: root.ai.Usage {
          input_tokens: 1,
          output_tokens: 1,
          cached_input_tokens: 0,
          reasoning_tokens: 0,
          cost_usd: 0.0,
        },
        attributes: {},
        raw: null,
      },
      conversation: null,
    }
  }
}

implements root.ai.CompletionProvider for FakeProvider {
  function complete<T>(self, task: root.ai.Task<T>) -> root.ai.ResponseWithMetadata<T>
      throws root.ai.Failure | baml.errors.UnknownError {
    self.generate<T>(task)
  }
  function replay_policy<T>(self, task: root.ai.Task<T>) -> root.ai.ReplayPolicy throws never {
    root.ai.ReplayPolicy { kind: root.ai.ReplayKind.Safe, idempotency_key: null }
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
