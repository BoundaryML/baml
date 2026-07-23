enum ReplayKind {
  Safe
  RequiresIdempotencyKey
  Never
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}

function may_replay(policy: ReplayPolicy, failure: Failure) -> bool throws never {
  match (failure.kind()) {
    FailureKind.InvalidRequest => return false,
    FailureKind.Refusal => return false,
    FailureKind.Unsupported => return false,
    FailureKind.Cancelled => return false,
    _ => {},
  }
  match (policy.kind) {
    ReplayKind.Safe => true,
    ReplayKind.RequiresIdempotencyKey => {
      policy.idempotency_key != null && failure.commit_state() != CommitState.Committed
    },
    ReplayKind.Never => false,
  }
}

// Bridge for the language's built-in thrown-error surface. Provider adapters
// should expose richer root.ai.Failure facts when possible; combinators can
// still conservatively honor the standard Failure predicates here.
function thrown_error_is_replayable(error: baml.errors.CallError | baml.errors.UnknownError) -> bool throws never {
  match (error) {
    let failure: baml.errors.Failure => {
      failure.is_retryable()
        && !failure.is_effectful()
        && !failure.is_policy_refusal()
        && !failure.is_unsupported()
    },
    _ => false,
  }
}
