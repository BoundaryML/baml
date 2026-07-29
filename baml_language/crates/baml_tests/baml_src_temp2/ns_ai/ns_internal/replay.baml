// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
/// A model step may be retried only when the failure says the attempt is
/// transient and definitely committed no effects. Unknown transport outcome
/// requires a future idempotency-aware policy rather than a blind retry.
function _may_retry_model_step(failure: root.ai.Failure) -> bool throws never {
    //# A terminal failure never replays
    if (!failure.is_transient()) {
        return false;
    }
    //# Effect-safety: what THIS attempt may have committed
    match (failure.effects()) {
        root.ai.Effects.Committed => {
            return false;
        },
        root.ai.Effects.Unknown => {
            return false;
        },
        root.ai.Effects.None => {},
    }
    true
}
