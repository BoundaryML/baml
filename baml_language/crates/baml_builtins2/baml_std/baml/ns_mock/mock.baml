// Mock — wrap an existing function (e.g. `baml.http.fetch`) with a replacement
// that records call counts and forwards to a user-provided implementation.
//
// Inside `mock.scope(body)`, direct calls to the original (e.g.
// `baml.http.fetch("...")`) are transparently redirected to the replacement
// via a VM-level function-override stack (see `push_override`/`pop_override`).
//
// Construction is two-step: `Mock.new(original)` returns a passthrough spy
// (its `_impl` field is the original, so `scope` runs the real function but
// ticks `call_count`). Call `m.replace(lambda)` to install an interception
// replacement. Inside the replacement, invoke `m.original(args)` to call the
// un-mocked target while the override is active — bypasses the mock stack
// so the recursion guard doesn't fire.
//
//   let m = baml.mock.Mock.new(baml.http.fetch)
//   m.replace((url: string) -> Response {
//     log.push("fetching " + url)
//     m.original(url)              // dispatches the real baml.http.fetch
//   })
//   m.scope(() -> {
//     baml.http.fetch("https://example.com")   // intercepted
//   })

/// VM primitive: push an override mapping `target → replacement` onto the
/// active mock stack. The override stays in effect until a matching
/// `pop_override` call. Pairs MUST be balanced — `Mock.scope` is the intended
/// caller and handles the bookkeeping.
///
/// `counter_owner` is the `Mock<T>` instance whose `call_count` field
/// should be incremented when this override fires. Pass `null` to skip
/// counter tracking (e.g. tests pushing overrides outside of a `Mock`).
//baml:mut_vm
function push_override<T>(target: T, replacement: T, counter_owner: Mock<T>?) -> null {
  $rust_function
}

/// VM primitive: pop the top entry from the active mock stack.
//baml:mut_vm
function pop_override() -> null {
  $rust_function
}

/// VM primitive: wrap a callable in an `UnmockedRef` so that any
/// `CallIndirect`/`YieldToCall` on the returned value bypasses the active
/// mock stack and dispatches the inner callable directly. Used by
/// `Mock.new` to construct the `original` field. The returned value is
/// typed as `T` (the wrapped callable's type) — the bypass is a runtime
/// property invisible to the type system.
///
/// Throws `baml.errors.InvalidArgument` if `target` isn't a callable
/// (`Function`/`Closure`/`BoundMethod`) — a runtime stand-in for the
/// generic-bounds constraint we'd write if BAML supported them
/// (`Mock<T> where T: callable`, à la Rust's `T: Fn(...)`). Remove this
/// check and the `throws` annotation once generic bounds land.
//baml:mut_vm
function wrap_unmocked<T>(target: T) -> T throws baml.errors.InvalidArgument {
  $rust_function
}

class Mock<T> {
  // Field order is irrelevant — the VM resolves `call_count` by name at
  // init and the codegen handles the rest, so this can be reorganised
  // for readability without breaking anything.
  call_count int
  /// Callable that dispatches the un-mocked target regardless of any
  /// active mock override. Use inside a replacement to wrap-and-delegate:
  ///
  ///   m.replace((url: string) -> Response {
  ///     log.push(url)
  ///     m.original(url)        // calls the real fetch, bypassing the mock
  ///   })
  original T
  _impl T
  _original T

  /// Construct a `Mock` that initially passes through to `original`. Use
  /// [`replace`] before entering `scope` to install an actual replacement
  /// implementation. Without `replace`, the mock just observes calls
  /// (counter ticks on every intercepted invocation).
  function new(original: T) -> Mock<T> {
    Mock<T> {
      call_count: 0,
      original: baml.mock.wrap_unmocked<T>(original),
      _impl: original,
      _original: original,
    }
  }

  /// Swap the replacement implementation. Does not reset `call_count`.
  //baml:mut_self
  function replace(self, impl: T) -> void {
    self._impl = impl
  }

  /// Run `body` with this mock active: any direct call to the original
  /// function within `body` is redirected to the current `_impl`. The
  /// override is removed when `body` returns or throws. `call_count` is
  /// incremented on each intercepted call.
  //baml:mut_self
  function scope<U, EU>(self, body: () -> U throws EU) -> U throws EU {
    let _ = baml.mock.push_override<T>(self._original, self._impl, self)
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
