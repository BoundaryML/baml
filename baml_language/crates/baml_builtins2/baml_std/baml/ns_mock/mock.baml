/// BEP-058 — function mocking for tests.
///
/// `baml.mock.new(original)` creates a `Mock<T>` wrapping a callable target; the
/// runtime records the target's identity. `Mock.replace` sets a stand-in, and
/// `baml.mock.scope(m, body)` activates the mock for the dynamic extent of `body`.
/// Calls to the mocked target inside the scope run the replacement and increment
/// `call_count`; outside it there are no effects.

/// Create a mock of the callable `original`.
function new<T>(original: T) -> Mock<T> throws never {
  baml.mock.__new(original)
}

/// Activate `mocks` for the dynamic extent of `body`, then deactivate them.
/// `mocks` is a single `Mock` or an array of `Mock`s (applied in order, so later
/// entries sit innermost). If `body` throws, the mocks are still deactivated as
/// the error unwinds, then the error is re-raised (finally-style pop).
function scope<T, E>(mocks: Mock<T>, body: () -> void throws E) -> void throws E {
  baml.mock.__enter(mocks);
  body() catch (e) {
    _ => { baml.mock.__exit(mocks); throw e }
  };
  baml.mock.__exit(mocks)
}

/// Internal: build a `Mock<T>` from the target's runtime identity. Rejects
/// (panics on) a non-mockable runtime internal — see the Rust impl.
//baml:mut_vm
//baml:fallible
function __new<T>(original: T) -> Mock<T> throws never {
  $rust_function
}

/// Internal: push `mocks` (a single mock or an array) onto the active-mock
/// table and reset their counters.
//baml:mut_vm
function __enter<T>(mocks: Mock<T>) -> void throws never {
  $rust_function
}

/// Internal: pop `mocks` off the active-mock table.
//baml:mut_vm
function __exit<T>(mocks: Mock<T>) -> void throws never {
  $rust_function
}

/// A mock of a callable of type `T`. The class is opaque to user code; its state is
/// runtime-managed.
class Mock<T> {
  /// Number of times the mocked target was called within the current scope.
  /// Read-only; runtime-managed (backed by an atomic counter on the heap object).
  call_count int

  /// Set the replacement to run in place of the original while this mock is active.
  //baml:mut_vm
  function replace(self, replacement: T) -> void throws never {
    $rust_function
  }
}
