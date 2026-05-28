/// BEP-034 — spawn options (`spawn ... with baml.spawn.options(...)`).
///
/// This namespace holds the values a `spawn` can be configured with: a
/// `CancelToken` for cooperative cancellation, a `TaskGroup` for rate
/// limiting, and the `detach` flag. They are assembled by `options(...)`
/// into an opaque `SpawnConfig` that the `with` clause hands to the runtime.

/// Opaque configuration produced by `baml.spawn.options(...)` and consumed by
/// a `spawn ... with` clause. Carries the (optional) cancel token, group, and
/// detach flag. Not meaningfully constructible any other way.
class SpawnConfig {
  _handle $rust_type
}

/// Caps how many spawned tasks run concurrently. Rate-limiting behavior lands
/// in a follow-up; the class is declared now so `options(group = ...)`
/// type-checks and the surface stays stable.
class TaskGroup {
  _handle $rust_type
}

/// A cooperative, one-shot cancellation handle backed by a runtime
/// cancellation token. Passing one via `options(cancel = ...)` links it into a
/// spawned task's effective token: once fired, the task's next `await` throws
/// `baml.panics.Cancelled`. Once cancelled, a token stays cancelled forever.
class CancelToken {
  _handle $rust_type

  /// Create a fresh, un-cancelled token.
  //baml:mut_vm
  function new() -> CancelToken {
    $rust_function
  }

  /// A new token that fires when ANY of `tokens` fires. Cancelling the
  /// returned token does not propagate back to the inputs (one-directional),
  /// so token composition cannot form cycles.
  //baml:mut_vm
  function any(tokens: CancelToken[]) -> CancelToken {
    $rust_function
  }

  /// Request cancellation. Returns `1` if this call transitioned the token
  /// from un-cancelled to cancelled, `0` if it was already cancelled.
  //baml:vm
  function cancel(self) -> int {
    $rust_function
  }

  /// `true` if this token has been cancelled.
  //baml:vm
  function is_cancelled(self) -> bool {
    $rust_function
  }
}

/// Assemble a `SpawnConfig` for a `spawn ... with` clause. In v1 only `cancel`
/// is honored at runtime; `group` is accepted for forward compatibility and
/// wired in a follow-up. (`detach` lands alongside its runtime support.)
//baml:mut_vm
function options(group: TaskGroup? = null, cancel: CancelToken? = null) -> SpawnConfig {
  $rust_function
}
