// === Generic function inference over nominal wrappers ===
// T should be inferred from the nominal wrapper's type_args.

class Wrapper<T> {
  inner: T
}

function unwrap<T>(w: Wrapper<T>) -> T {
  w.inner
}

function test_unwrap_int() -> int {
  unwrap(Wrapper<int> { inner: 42 })
}

function test_unwrap_string() -> string {
  unwrap(Wrapper<string> { inner: "hello" })
}
