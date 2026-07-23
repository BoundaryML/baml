// 29 — wrappers combine failure facts with an operation replay policy.
class s29_Transient {
  implements root.ai.Failure {
    function kind(self) -> root.ai.FailureKind throws never { root.ai.FailureKind.Transport }
    function commit_state(self) -> root.ai.CommitState throws never { root.ai.CommitState.NotCommitted }
  }
  implements baml.errors.Failure {
    function is_retryable(self) -> bool { true }
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { false }
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
  }
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { true }
    function is_rate_limit(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}

class s29_Flaky {
  reply: string,
  fail_until: int,
  calls: int,
  implements root.ai.Provider {}
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      if (self.calls <= self.fail_until) { throw s29_Transient {}; }
      root.ai.Response<T> {
        value: baml.json.from_string<T>(self.reply) catch (e) {
          _ => throw baml.errors.UnknownError { data: e, message: ["flaky parse"] },
        },
        meta: root.ai.Meta {
          provider: "flaky", model: "fake", request_id: null, finish_reason: "stop",
          usage: null, attributes: {}, raw: null,
        },
      }
    }
  }
}

class s29_Refusal {
  implements root.ai.Failure {
    function kind(self) -> root.ai.FailureKind throws never { root.ai.FailureKind.Refusal }
    function commit_state(self) -> root.ai.CommitState throws never { root.ai.CommitState.NotCommitted }
  }
  implements baml.errors.Failure {
    function is_retryable(self) -> bool { false }
    function is_effectful(self) -> bool { false }
    function is_policy_refusal(self) -> bool { true }
    function is_resumable(self) -> bool { false }
    function is_unsupported(self) -> bool { false }
  }
  implements baml.errors.CallError {
    function is_network_error(self) -> bool { false }
    function is_rate_limit(self) -> bool { false }
    function is_parse_error(self) -> bool { false }
  }
}

class s29_Refusing {
  calls: int,
  implements root.ai.Provider {}
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      throw s29_Refusal {}
    }
  }
}

class s29_Effectful {
  calls: int,
  implements root.ai.Provider {
    function is_effectful(self) -> bool throws never { true }
  }
  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      self.calls = self.calls + 1;
      root.ai.FakeGenerateProvider {
        name: "effectful",
        reply: `"committed"`,
        attributes: {},
      }.generate<T>(request)
    }
  }
}
