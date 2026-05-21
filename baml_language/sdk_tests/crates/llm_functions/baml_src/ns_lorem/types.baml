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

// Return shape for `StreamE2ECollect`: every value produced by
// `stream.next()` (the `S` side of `Stream<T, S>` — `null | string` for
// the `StreamE2EExtract$stream` instantiation) plus the single `T`
// value from `stream.final()`. The class is concrete (no generics) so
// the FFI boundary doesn't depend on the engine's `Stream<T, S>`
// type-var substitution.
class StreamE2ECollectResult {
  next_calls (null | string)[]
  final_call string
}

// Minimum repro of the generic-method FFI plumbing bug that
// `test_stream_next_reaches_finished` surfaces against `baml.llm.Stream`.
// `Stream<T, S>.next() -> S | baml.stream.StreamFinished` is the
// engine-internal version of the same pattern: an instance method on a
// generic class whose return type mentions a class-level TypeVar.
//
// Cross-FFI symptom: when Python calls `w.get_value()`, the host-side
// `tir2_to_template` path can't substitute the class's `T` into the
// declared return type, so the lifted return type still mentions
// `Ty::TypeVar`. The lowering falls back to `Ty::Void`, and the
// concrete return value (a `string` here) fails the runtime decode
// with "Type mismatch: Value of type 'string' does not match any
// member of union [Void { ... }]".
//
// Exercised by `customizable/test_generic_method.py`.
class Wrapper<T> {
  value T

  // Plain TypeVar return. With the host-side lifting we have today,
  // this round-trips fine — the test under `customizable/
  // test_generic_method.py::test_generic_wrapper_get_value` passes.
  function get_value(self) -> T {
    self.value
  }

  // Union-with-TypeVar return. Mirrors the shape of
  // `baml.llm.Stream<T, S>.next(self) -> S | baml.stream.StreamFinished`:
  // a class-level TypeVar appears as one arm of a union with a concrete
  // class. This is the shape the streaming smoke fails on; whether the
  // user-defined analogue fails the same way isolates whether the bug
  // is in the union-of-TypeVar lifting path or in the engine-side
  // handle/Stream lifting specifically.
  function get_value_or_marker(self) -> T | WrapperMarker {
    self.value
  }
}

// Concrete sibling for the union arm above. Kept trivial — the marker
// path is not exercised; the test only proves that a `T`-shaped value
// decodes when the declared return is `T | WrapperMarker`.
class WrapperMarker {
  reason string
}
