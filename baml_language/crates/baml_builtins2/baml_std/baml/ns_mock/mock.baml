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

/// Activate `mock` for the dynamic extent of `body`, then deactivate it. If
/// `body` throws, the mock is still deactivated as the error unwinds, then the
/// error is re-raised (finally-style pop).
function scope<T, E>(mock: Mock<T>, body: () -> void throws E) -> void throws E {
  baml.mock.__enter(mock);
  body() catch (e) {
    _ => { baml.mock.__exit(mock); throw e }
  };
  baml.mock.__exit(mock)
}

/// Internal: build a `Mock<T>` from the target's runtime identity. Rejects
/// (panics on) a non-mockable runtime internal — see the Rust impl.
//baml:mut_vm
//baml:fallible
function __new<T>(original: T) -> Mock<T> throws never {
  $rust_function
}

/// Internal: push `mock` onto the active-mock table and reset its counter.
//baml:mut_vm
function __enter<T>(mock: Mock<T>) -> void throws never {
  $rust_function
}

/// Internal: pop `mock` off the active-mock table.
//baml:mut_vm
function __exit<T>(mock: Mock<T>) -> void throws never {
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
