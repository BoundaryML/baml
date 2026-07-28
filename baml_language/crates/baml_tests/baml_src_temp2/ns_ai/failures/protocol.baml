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
