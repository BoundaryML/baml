class FakeProvider {
  provider_name: string,
  output: string,
  calls: int,
  failures_remaining: int,
}

implements Provider for FakeProvider {
  function name(self) -> string throws never { self.provider_name }
  function render_shorthand(self) -> string throws never { "openai/gpt-5.6-luna" }
}

implements GenerationProvider for FakeProvider {
  function generate<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.calls = self.calls + 1;
    if (self.failures_remaining > 0) {
      self.failures_remaining = self.failures_remaining - 1;
      throw baml.errors.UnknownError { data: null, message: ["injected fake failure"] };
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
      transcript: null,
    }
  }
}

implements DriveProvider for FakeProvider {
  function drive<T>(self, task: Task<T>) -> Response<T> throws baml.errors.UnknownError {
    self.generate<T>(task)
  }
  function replay_policy<T>(self, task: Task<T>) -> ReplayPolicy throws never {
    ReplayPolicy { kind: ReplayKind.Safe, idempotency_key: null }
  }
}

function fake(output: string) -> FakeProvider throws never {
  FakeProvider {
    provider_name: "fake",
    output: output,
    calls: 0,
    failures_remaining: 0,
  }
}
