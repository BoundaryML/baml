// Two return-type classes for the LLM functions.
//   - `Resume`: simple class. Its `$stream` PPIR expansion is a no-op
//     (no stream-state-altering fields), so depending on the
//     conditional-emit rule it may or may not appear under
//     `stream_types/`. The customizable test asserts on whatever the
//     pipeline actually produces.
//   - `StreamingDoc`: includes a `stream_state<...>`-shaped field
//     (proxied via optional in this fixture) so the synthesized
//     `StreamingDoc$stream` differs from the source class — exercises
//     the always-different branch of the stream-companion rule.
class Resume {
  name string
  email string?
}

class StreamingDoc {
  title string
  body string?
  word_count int
}
