// Capability error model for provider/LLM interfaces. Concrete errors implement
// the capability channels they can cross and the shared Failure decision axis.

/// Universal wrapper for errors that do not implement a known capability channel.
class UnknownError {
    data: unknown,
    message: string[],
}

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

implements Failure for Unsupported {
  function is_retryable(self) -> bool { false }
  function is_effectful(self) -> bool { false }
  function is_policy_refusal(self) -> bool { false }
  function is_resumable(self) -> bool { false }
  function is_unsupported(self) -> bool { true }
}

implements CallError for Unsupported {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

implements StreamError for Unsupported {
  function is_network_error(self) -> bool { false }
}

implements ToolError for Unsupported {
  function is_network_error(self) -> bool { false }
}

implements RealtimeError for Unsupported {
  function is_network_error(self) -> bool { false }
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

implements StreamError for CannotRetry {
  function is_network_error(self) -> bool { false }
}

implements ToolError for CannotRetry {
  function is_network_error(self) -> bool { false }
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

implements CallError for LoopBudgetExceeded {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
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

implements CallError for HandoffUnresolved {
  function is_network_error(self) -> bool { false }
  function is_rate_limit(self) -> bool { false }
  function is_parse_error(self) -> bool { false }
}

implements ToolError for HandoffUnresolved {
  function is_network_error(self) -> bool { false }
}
