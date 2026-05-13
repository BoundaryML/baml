// Mock — wrap an existing function (e.g. `baml.http.fetch`) with a replacement.
//
// Inside `mock.scope(body)`, direct calls to the original (e.g.
// `baml.http.fetch("...")`) are transparently redirected to the replacement
// via a VM-level function-override stack (see `push_override`/`pop_override`).
//
// Usage:
//
//   let m = baml.mock.Mock.new(baml.http.fetch, (url: string) -> Response {
//     Response { status_code: 200, headers: {}, url: url, _body: "fake" }
//   });
//
//   m.scope(() -> void {
//     // Calls baml.http.fetch here go through the replacement.
//     baml.http.fetch("https://example.com");
//   });
//
//   // Outside the scope, the real builtin runs.
//   baml.http.fetch("https://example.com");

/// VM primitive: push an override mapping `target → replacement` onto the
/// active mock stack. The override stays in effect until a matching
/// `pop_override` call. Pairs MUST be balanced — `Mock.scope` is the intended
/// caller and handles the bookkeeping.
//baml:mut_vm
function push_override<T>(target: T, replacement: T) -> null {
  $rust_function
}

/// VM primitive: pop the top entry from the active mock stack.
//baml:mut_vm
function pop_override() -> null {
  $rust_function
}

/// A Mock wraps a function value `T` (any arity, any return, any throws)
/// and supports scoped replacement via [`scope`]. `T` is left opaque to the
/// class so that the same `Mock` definition works for every function
/// signature.
class Mock<T> {
  _impl T
  _original T
  call_count int

  /// Construct a Mock from an original function and a replacement implementation.
  function new(original: T, impl: T) -> Mock<T> {
    Mock<T> { _impl: impl, _original: original, call_count: 0 }
  }

  /// Construct a Mock that initially passes through to `original`. Use
  /// [`replace`] to swap in a real replacement before entering [`scope`].
  function spy(original: T) -> Mock<T> {
    Mock<T> { _impl: original, _original: original, call_count: 0 }
  }

  /// Swap the replacement implementation. Does not reset `call_count`.
  //baml:mut_self
  function replace(self, impl: T) -> void {
    self._impl = impl
  }

  /// Reset the call counter to zero.
  //baml:mut_self
  function reset(self) -> void {
    self.call_count = 0
  }

  /// Run `body` with this mock active: any direct call to the original
  /// function within `body` is redirected to the current `_impl`. The
  /// override is removed when `body` returns or throws.
  ///
  /// NOTE: `call_count` is **not** automatically incremented for intercepted
  /// calls inside `scope` in this version — track interceptions from inside
  /// the replacement closure itself if you need that today.
  //baml:mut_self
  function scope<U, EU>(self, body: () -> U throws EU) -> U throws EU {
    let _ = baml.mock.push_override<T>(self._original, self._impl)
    let result = {
      body()
    } catch_all (e) {
      _ => {
        let _ = baml.mock.pop_override()
        throw e
      }
    }
    let _ = baml.mock.pop_override()
    result
  }
}
