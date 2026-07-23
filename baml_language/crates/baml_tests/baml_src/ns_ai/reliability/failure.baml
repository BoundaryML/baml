enum FailureKind {
  Transport
  RateLimit
  InvalidRequest
  Refusal
  Parse
  Unsupported
  Cancelled
}

enum CommitState {
  NotCommitted
  Committed
  Unknown
}

interface Failure {
  function kind(self) -> FailureKind throws never
  function commit_state(self) -> CommitState throws never
  function retry_after(self) -> baml.time.Duration? throws never { null }
  function is_resumable(self) -> bool throws never { false }
}
