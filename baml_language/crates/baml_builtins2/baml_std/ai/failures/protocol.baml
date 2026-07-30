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

/// The one classification axis generic policy code consumes. Errors carry
/// facts; retry and fallback decide what to do with them. Precision beyond
/// these two methods comes from catching concrete error types, not from
/// interface getters.
interface Failure {
  /// Could an identical attempt plausibly succeed? Rate limits and transport
  /// blips: yes. Refusals, invalid requests, parse failures: no.
  function is_transient(self) -> bool throws never

  /// What this attempt may have committed before it failed.
  function effects(self) -> Effects throws never
}
