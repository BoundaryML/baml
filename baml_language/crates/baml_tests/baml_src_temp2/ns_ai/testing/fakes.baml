enum FakeFailureMode {
    Retryable,
    Terminal,
    Unknown,
}

// A recognized strategy shorthand gives deterministic providers a prompt
// rendering context without coupling the portable ai namespace to a model
// vendor.
function fake_render_shorthand() -> string throws never {
    "baml-fallback/fake"
}

class FakeCallFailure {
    message: string,
    retryable: bool,
}

implements root.ai.Failure for FakeCallFailure {
  function is_retryable(self) -> bool throws never { self.retryable }
  function is_effectful(self) -> bool throws never { false }
  function is_policy_refusal(self) -> bool throws never { false }
  function is_resumable(self) -> bool throws never { false }
  function is_unsupported(self) -> bool throws never { false }
}

implements root.ai.CallError for FakeCallFailure {
  function is_network_error(self) -> bool throws never { self.retryable }
  function is_rate_limit(self) -> bool throws never { false }
  function is_parse_error(self) -> bool throws never { false }
}

class FakeProvider {
    provider_name: string,
    output: string,
    calls: int,
    failures_remaining: int,
    failure_mode: FakeFailureMode,
}

implements Provider for FakeProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { fake_render_shorthand() }
}

implements GenerationProvider for FakeProvider {
  function generate<T>(self, task: Task<T>) -> Response<T>
      throws root.ai.CallError | baml.errors.UnknownError {
    self.calls = self.calls + 1;
    if (self.failures_remaining > 0) {
      self.failures_remaining = self.failures_remaining - 1;
      match (self.failure_mode) {
        FakeFailureMode.Retryable => throw FakeCallFailure {
          message: "injected retryable fake failure",
          retryable: true,
        },
        FakeFailureMode.Terminal => throw FakeCallFailure {
          message: "injected terminal fake failure",
          retryable: false,
        },
        FakeFailureMode.Unknown => throw baml.errors.UnknownError {
          data: null,
          message: ["injected unclassified fake failure"],
        },
      }
    }
    let value = baml.sap.parse<T>(self.output) catch (e) {
      _ => throw baml.errors.UnknownError { data: e, message: ["fake output parse failed"] },
    };
    Response<T> {
      value: value,
      meta: Meta {
        provider: self.provider_name,
        model: "fake",
        request_id: "fake-" + self.calls.to_string(),
        finish_reason: "stop",
        usage: Usage {
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

implements CompletionProvider for FakeProvider {
  function complete<T>(self, task: Task<T>) -> Response<T>
      throws root.ai.CallError | baml.errors.UnknownError {
    self.generate<T>(task)
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
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
