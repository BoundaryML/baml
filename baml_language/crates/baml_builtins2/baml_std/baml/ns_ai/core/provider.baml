// The capability marker every provider implements and every capability `requires`.

/// The bare capability marker. Carries no interaction method — `call`/`call_with`
/// live on `HttpProvider`, `stream` on `Streaming`, etc. It is the type a client
/// returns and the bound every capability `requires`. Hosts the inherited combinator
/// factories every provider gets for free (see `combinators.baml`).
interface Provider {
  /// D2 effect marker: true iff invoking this provider commits external effects
  /// (server-stored state, billed jobs, live sessions) that a blind re-drive would
  /// duplicate. Combinators consult it and refuse-then-error instead of silently
  /// re-driving. An author assertion (default: safe).
  function is_effectful(self) -> bool throws never {
    false
  }

  function with_retry(self, max: int) -> Retry throws never {
    Retry { inner: self, max: max }
  }
  function fallback_to(self, other: Provider) -> Fallback throws never {
    Fallback { members: [self, other] }
  }
}
