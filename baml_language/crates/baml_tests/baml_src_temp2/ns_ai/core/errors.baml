// Capability error model for provider/LLM interfaces. These types are local
// to the root.ai fixture namespace rather than part of the baml.errors stdlib.

/// Cross-capability decision axis used by retry and fallback policies.
interface Failure {
  function is_retryable(self) -> bool throws never
  function is_effectful(self) -> bool throws never
  function is_policy_refusal(self) -> bool throws never
  function is_resumable(self) -> bool throws never
  function is_unsupported(self) -> bool throws never
}

/// Request/response failures.
interface CallError requires Failure {
  function is_network_error(self) -> bool throws never
  function is_rate_limit(self) -> bool throws never
  function is_parse_error(self) -> bool throws never
}

/// Incremental streaming failures.
interface StreamError requires Failure {
  function is_network_error(self) -> bool throws never
}

/// Failures on a multi-turn tool-calling path.
interface ToolError requires Failure {
  function is_network_error(self) -> bool throws never
}

/// Failures on a live/duplex session.
interface RealtimeError requires Failure {
  function is_network_error(self) -> bool throws never
}

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

/// Raised when a tool-loop stop policy is reached before a final value exists.
class LoopBudgetExceeded {
    steps_taken: int,
}

implements Failure for LoopBudgetExceeded {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { true }
  function is_policy_refusal(self) -> bool { true }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { false }
}

implements ToolError for LoopBudgetExceeded {
  function is_network_error(self) -> bool { false }
}

/// Raised when a handoff is produced on a plain call path that cannot return it.
class HandoffUnresolved {
    to: string,
    steps_taken: int,
}

implements Failure for HandoffUnresolved {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { true }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { true }
  function is_unsupported(self) -> bool { false }
}

implements ToolError for HandoffUnresolved {
  function is_network_error(self) -> bool { false }
}
