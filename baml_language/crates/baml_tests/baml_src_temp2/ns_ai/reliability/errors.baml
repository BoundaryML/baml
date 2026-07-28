/// Raised when retrying would replay an effectful provider operation.
class CannotRetry {
    message: string,
}

implements Failure for CannotRetry {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { true }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { false }
}

implements CallError for CannotRetry {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}
