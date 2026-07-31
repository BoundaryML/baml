// Failure classification for provider/model capability channels. These types
// belong to the `ai` package rather than the core `baml.errors` package.
//
// The channel every fallible capability method declares is
// `ai.Failure | baml.errors.UnknownError`: a classified error the policy
// layer can reason about, or the universal wrapper for anything foreign. A
// throw is legal iff the value implements `Failure` or is routed through
// `baml.errors.UnknownError.with_message` (see the trailing normalize catch in
// each provider).

/// Whether a failed attempt may have committed side effects.
enum Effects {
    /// Failed before anything could commit — replay is safe.
    None,
    /// The request was sent but the outcome was never observed — replay only
    /// with an idempotency key.
    Unknown,
    /// Effects definitely happened — never blind-replay.
    Committed,
}

/// Errors carry FACTS; callers make judgments. The one method here reports
/// a fact only the failing layer can know: what this attempt may have
/// committed before it failed. Whether a failure is worth retrying is the
/// caller's judgment — expressed by catching concrete error types, or by
/// passing `retry_if` to `ai.retry` — never a self-report by the error.
interface Failure {
  /// What this attempt may have committed before it failed.
  function effects(self) -> Effects throws never
}

/// The shared HTTP status→failure table (429 → RateLimited, 5xx/408 →
/// NetworkFailure, other 4xx → InvalidRequest). Public so third-party
/// provider adapters classify identically to the built-ins instead of
/// copying the table and drifting — retry and fallback behavior depends on
/// this classification. Effectful endpoints (background jobs, batch
/// submission) construct their failures directly instead.
function classify_http(
    provider: string,
    status_code: int,
    body: string,
) -> Failure throws never {
    ai.internal._classify_http(provider, status_code, body)
}
