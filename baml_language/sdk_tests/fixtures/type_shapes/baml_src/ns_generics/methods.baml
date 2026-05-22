// Generic-instance-method FFI fixture (moved from llm_functions/ns_lorem).
// Lives in a separate file so the bare generic-class shape coverage in
// types.baml stays method-free.
//
// Minimum repro of the generic-method FFI plumbing bug surfaced by
// `Stream<T, S>.next() -> S | baml.stream.StreamFinished`: an instance
// method on a generic class whose return type mentions a class-level
// TypeVar. When Python calls `w.get_value()`, the host-side
// `tir2_to_template` path can't substitute the class's `T` into the
// declared return type, so the lifted return type still mentions
// `Ty::TypeVar`. The lowering falls back to `Ty::Void`, and the
// concrete return value (a `string` here) fails the runtime decode
// with "Type mismatch: Value of type 'string' does not match any
// member of union [Void { ... }]".
//
// Renamed from `Wrapper<T>` to `WrapperMethods<T>` to avoid colliding
// with the bare `Wrapper<T>` shape fixture in types.baml.
//
// Exercised by `customizable/test_generic_method.py`.
class WrapperMethods<T> {
  value T

  // Plain TypeVar return. With the host-side lifting we have today,
  // this round-trips fine — `test_generic_wrapper_get_value` passes.
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

// Host-side construction of a `WrapperMethods<string>` so Python can
// call `.get_value()` on it. Mirrors the host-driven path that
// `test_stream_next_reaches_finished` takes against `Stream<T, S>` —
// builds the generic instance on one side, calls the generic instance
// method from the other.
function MakeWrapperMethods(text: string) -> WrapperMethods<string> {
  WrapperMethods<string> { value: text }
}
