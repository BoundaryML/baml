class s15_InputTripwire {
  reason: string,
}

implements baml.errors.Failure for s15_InputTripwire {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { false }
  function is_policy_refusal(self) -> bool { true }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { false }
}

implements baml.errors.CallError for s15_InputTripwire {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

function s15_request_text(request: root.ai.Request<string>) -> string throws never {
  let out = "";
  for (let message in request.messages()) {
    for (let part in message.parts) { out = out + (part.text ?? ""); }
  }
  out
}

// A guardrail that preserves the baseline Generate capability. It checks before
// delegating, so a refusal cannot accidentally issue the model request.
class s15_Guarded {
  inner: root.ai.Provider,
  blocked_phrase: string,

  implements root.ai.Provider {}

  implements root.ai.Generate {
    function generate<T>(self, request: root.ai.Request<T>) -> root.ai.Response<T>
        throws baml.errors.CallError | baml.errors.UnknownError {
      let text = "";
      for (let message in request.messages()) {
        for (let part in message.parts) { text = text + (part.text ?? ""); }
      }
      if (text.includes(self.blocked_phrase)) {
        throw s15_InputTripwire { reason: "blocked phrase" };
      }
      root.ai.run_with_meta<T>(request.for_provider(self.inner))
    }
  }
}
