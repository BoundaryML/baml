// Hidden plumbing for the agent runner. Nothing in ai.internal is part of
// the public surface: helpers here may change shape without notice and never
// appear in documentation examples.
/// A model step may be replayed only when the failure reports that THIS
/// attempt definitely committed no effects — a fact — and the caller's
/// judgment (if any) says the failure is worth replaying. Whether an error
/// is "transient" is never the error's self-report; it is the caller's
/// call, expressed through `retry_if`. Unknown transport outcome requires a
/// future idempotency-aware policy rather than a blind retry.
function _may_retry_model_step(
    failure: ai.Failure,
    retry_if: ((ai.Failure) -> bool throws never)?,
) -> bool throws never {
    //# Effect-safety first: what THIS attempt may have committed
    match (failure.effects()) {
        ai.Effects.Committed => {
            return false;
        },
        ai.Effects.Unknown => {
            return false;
        },
        ai.Effects.None => {},
    }
    //# Retryability is the caller's judgment. The DEFAULT judgment matches
    //# the documented guidance: replay cannot help a refusal, a malformed
    //# request, or a parse failure — an identical replay gets an identical
    //# answer. Callers override either direction with retry_if.
    match (retry_if) {
        null => match (failure) {
            let refused: ai.Refused => false,
            let invalid: ai.InvalidRequest => false,
            let parse: ai.ParseFailed => false,
            _ => true,
        },
        let judge: ((ai.Failure) -> bool throws never) => judge(failure),
    }
}
