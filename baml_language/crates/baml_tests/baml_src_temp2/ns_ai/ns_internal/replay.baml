// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
/// The whole replay judgment, in one place: the failure's facts (transience,
/// effect state) combined with the provider's standing replay contract.
function _may_replay(policy: root.ai.ReplayPolicy, failure: root.ai.Failure) -> bool throws never {
    //# A terminal failure never replays
    if (!failure.is_transient()) {
        return false;
    }
    //# Effect-safety: what THIS attempt may have committed
    match (failure.effects()) {
        root.ai.Effects.Committed => return false,
        root.ai.Effects.Unknown => {
            if (policy.idempotency_key == null) {
                return false;
            }
        },
        root.ai.Effects.None => {},
    }
    //# The provider's replay contract
    match (policy.kind) {
        root.ai.ReplayKind.Safe => true,
        root.ai.ReplayKind.RequiresIdempotencyKey => policy.idempotency_key != null,
        root.ai.ReplayKind.Never => false,
    }
}
