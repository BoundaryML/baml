// The default failure vocabulary. Providers throw these unless they have
// provider-specific data to carry — in which case they define their own class
// and `implements Failure` for it (two methods). Apps catch these by name for
// precision, or catch `Failure` and consult the classification methods.

/// The provider said slow down (HTTP 429 and equivalents).
class RateLimited {
    provider: string,
    /// Backoff hint from the provider, when it sent one.
    retry_after_ms: int?,
}

implements Failure for RateLimited {
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> Effects throws never { Effects.None }
}

/// The transport failed: connect, reset, DNS, or the response never arrived.
/// The request may or may not have reached the provider.
class NetworkFailure {
    provider: string,
    detail: string,
}

implements Failure for NetworkFailure {
  function is_transient(self) -> bool throws never { true }
  function effects(self) -> Effects throws never { Effects.Unknown }
}

/// The provider rejected the request as malformed or unfulfillable
/// (HTTP 400/404/422 and equivalents). Retrying the same request cannot help.
class InvalidRequest {
    provider: string,
    status_code: int?,
    detail: string,
}

implements Failure for InvalidRequest {
  function is_transient(self) -> bool throws never { false }
  function effects(self) -> Effects throws never { Effects.None }
}

/// The model or provider refused the content (safety filter, policy block).
/// Terminal: an identical replay gets an identical refusal.
class Refused {
    provider: string,
    reason: string,
}

implements Failure for Refused {
  function is_transient(self) -> bool throws never { false }
  function effects(self) -> Effects throws never { Effects.None }
}

/// A response arrived but did not parse or validate as the requested type.
/// Carries the raw model text so callers can log it, repair it, or re-prompt
/// with it — blind re-send is a policy decision, not a default.
class ParseFailed {
    provider: string,
    raw_output: string,
}

implements Failure for ParseFailed {
  function is_transient(self) -> bool throws never { false }
  function effects(self) -> Effects throws never { Effects.None }
}

/// The attempt failed after committing a side effect (a job was enqueued, a
/// payment was taken). Blind replay would double the effect.
class EffectCommitted {
    provider: string,
    operation: string,
}

implements Failure for EffectCommitted {
  function is_transient(self) -> bool throws never { false }
  function effects(self) -> Effects throws never { Effects.Committed }
}
