// `lorem.Resume` from 09a §"Source BAML symbols".
class Resume {
  name string
  email string?
}

// The multi-field class return type for class-typed streaming coverage.
// `word_count` is nullable so the class has a partial parse while it streams: an
// `int` field has no default (BEP-075), and the model emits it last.
class StreamingDoc {
  title string
  body string?
  word_count int?
}

// Static host model used only as a same-name trap for HS-7. The runtime test
// mints a distinct class named `Hs7Collision` with document fields; decoding
// that live nominal through this generated class would silently discard them.
class Hs7Collision {
  compiled_marker string?
}

// Return shape for `stream_e2e_collect`: every value produced by
// `stream.next()` (the partial side of `Stream<Partial, Final>` — `string`
// for the `stream_e2e_extract@stream` instantiation) plus the single final
// value from `stream.final()`. The class is concrete (no generics) so the
// FFI boundary doesn't depend on the engine's `Stream<Partial, Final>`
// type-var substitution.
class StreamE2ECollectResult {
  next_calls string[]
  final_call string
}

// Runtime-neutral host-projection coverage for LLM methods. SDK tests only
// inspect the generated bindings; they never invoke these provider-backed
// methods.
class MethodProjectionProbe {
  prefix string

  function extract(self, text: string) -> Resume {
    client: "openai/gpt-4o-mini"
    prompt: `${self.prefix}: ${text} ${ctx.output_format()}`
  }

  function summarize(text: string) -> Resume {
    client: "openai/gpt-4o-mini"
    prompt: `Summarize ${text} ${ctx.output_format()}`
  }
}
