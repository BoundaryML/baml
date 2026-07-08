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

/// D4 chain-wide provenance: a `ResponseMeta` over SEVERAL member calls
/// (retry attempts, fallback members, loop turns). `usage()` sums the parts;
/// `finish_reason()` is the LAST part's (the one whose value the caller got);
/// the per-part dimensions (reasoning/logprobs/citations) are not merged —
/// read them off the individual parts instead.
///
/// NOTE the honesty bar: today a FAILED attempt throws without carrying its
/// meta, so combinators can only aggregate the calls they observed succeed —
/// use `UsageMeter`/`Traced` (combinators.baml) to collect across a chain.
class AggregateMeta {
    parts: ResponseMeta[],

    implements ResponseMeta {
        function finish_reason(self) -> string throws never {
            match (self.parts.at(self.parts.length() - 1)) {
                null => "unknown",
                let last: ResponseMeta => last.finish_reason(),
            }
        }
        function usage(self) -> Usage throws never {
            let inp = 0;
            let out = 0;
            for (let p in self.parts) {
                let u = p.usage();
                inp = inp + u.input_tokens;
                out = out + u.output_tokens;
            }
            Usage { input_tokens: inp, output_tokens: out }
        }
        function reasoning(self) -> string | Unavailable throws never {
            Unavailable { reason: "aggregate: read per-part dimensions off .parts" }
        }
        function logprobs(self) -> Logprob[] | Unavailable throws never {
            Unavailable { reason: "aggregate: read per-part dimensions off .parts" }
        }
        function citations(self) -> Citation[] | Unavailable throws never {
            Unavailable { reason: "aggregate: read per-part dimensions off .parts" }
        }
    }
}
