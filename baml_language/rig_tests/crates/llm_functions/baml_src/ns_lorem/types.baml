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
