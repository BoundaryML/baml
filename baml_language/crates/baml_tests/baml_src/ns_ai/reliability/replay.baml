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
