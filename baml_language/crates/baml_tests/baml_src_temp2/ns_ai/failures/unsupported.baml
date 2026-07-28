implements Failure for baml.errors.Unsupported {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { false }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { true }
}

implements CallError for baml.errors.Unsupported {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}
