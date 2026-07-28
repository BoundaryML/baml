enum ReplayKind {
    Safe,
    RequiresIdempotencyKey,
    Never,
}

class ReplayPolicy {
    kind: ReplayKind,
    idempotency_key: string?,
}

function _may_replay(policy: ReplayPolicy, failure: root.ai.Failure) -> bool throws never {
    //# Reject failures whose semantics make replay unsafe
    if (
        !failure.is_retryable()
            || failure.is_effectful()
            || failure.is_policy_refusal()
            || failure.is_unsupported()
    ) {
        return false;
    }
    //# Apply the provider's replay contract
    match (policy.kind) {
        ReplayKind.Safe => true,
        ReplayKind.RequiresIdempotencyKey => policy.idempotency_key != null,
        ReplayKind.Never => false,
    }
}
