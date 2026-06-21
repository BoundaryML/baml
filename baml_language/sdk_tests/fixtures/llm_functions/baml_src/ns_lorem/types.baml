// `lorem.Resume` from 09a §"Source BAML symbols".
class Resume {
  name string
  email string?
}

// `StreamingDoc` exists to force an always-different `$stream` branch:
// the optional `body` proxies a stream-state-altering field, so the
// synthesized `StreamingDoc$stream` class differs from the source
// class. Pins the `stream_types/lorem` leaf emission — the
// conditional-emit rule guarantees this companion shows up.
class StreamingDoc {
  title string
  body string?
  word_count int
}

// Return shape for `stream_e2e_collect`: every value produced by
// `stream.next()` (the `S` side of `Stream<T, S>` — `null | string` for
// the `stream_e2e_extract$stream` instantiation) plus the single `T`
// value from `stream.final()`. The class is concrete (no generics) so
// the FFI boundary doesn't depend on the engine's `Stream<T, S>`
// type-var substitution.
class StreamE2ECollectResult {
  next_calls (null | string)[]
  final_call string
}
