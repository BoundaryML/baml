// Out-of-band response metadata: the value+sidecar carrier (`CallResult`) and the
// lazily-normalized `ResponseMeta` every provider exposes through `call_with`.

/// The value + out-of-band-metadata carrier returned by `call_with`. Replaces the
/// `(T, V)` tuple from the design (tuples are not yet a language feature).
class CallResult<T, V> {
    value: T,
    meta: V,
}

/// Token accounting for one call.
class Usage {
    input_tokens: int,
    output_tokens: int,
}

/// Marks a `ResponseMeta` dimension the provider genuinely cannot supply (vs empty this
/// call) — the design's `Supported<T> = T | Unsupported`. (Generic type aliases aren't a
/// language feature yet, so dimensions spell the union inline, e.g. `Logprob[] | Unavailable`.)
class Unavailable {
    reason: string,
}

/// A per-token logprob (scenario 08 enriched output).
class Logprob {
    token: string,
    logprob: float,
}

/// A grounding/citation reference (scenario 08).
class Citation {
    url: string,
    title: string,
}

/// Out-of-band response metadata (finish reason, usage, reasoning, logprobs, citations, …),
/// normalized lazily by each provider. Projected via `HttpProvider.call_with`. Accessors are
/// best-effort (`throws never`): a missing wire shape yields a default / `Unavailable`, so a
/// projection over them is infallible.
interface ResponseMeta {
  function finish_reason(self) -> string throws never
  function usage(self) -> Usage throws never
  function reasoning(self) -> string | Unavailable throws never
  function logprobs(self) -> Logprob[] | Unavailable throws never
  function citations(self) -> Citation[] | Unavailable throws never
}
