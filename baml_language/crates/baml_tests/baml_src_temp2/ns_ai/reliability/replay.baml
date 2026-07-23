enum ReplayKind {
  Safe
  RequiresIdempotencyKey
  Never
}

class ReplayPolicy {
  kind: ReplayKind,
  idempotency_key: string?,
}

function may_replay(policy: ReplayPolicy, failure: root.ai.Failure) -> bool throws never {
  if (!failure.is_retryable()
      || failure.is_effectful()
      || failure.is_policy_refusal()
      || failure.is_unsupported()) {
    return false;
  }
  match (policy.kind) {
    ReplayKind.Safe => true,
    ReplayKind.RequiresIdempotencyKey => policy.idempotency_key != null,
    ReplayKind.Never => false,
  }
}
