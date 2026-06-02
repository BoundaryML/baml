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

/// Caps how many spawns referencing it run concurrently. Excess spawns queue
/// FIFO and start as earlier ones settle; the `spawn` still returns its
/// `Future` immediately, so queueing is invisible. Two spawns share a limit iff
/// they reference the same `TaskGroup` value — there is no global registry.
class TaskGroup {
  _handle $rust_type

  /// Create a group capping concurrency at `limit`. `name` is an optional
  /// debug label (no semantic meaning).
  //baml:mut_vm
  function new(limit: int, name: string? = null) -> TaskGroup throws never {
    $rust_function
  }

  /// Fire the cancel tokens of the group's members. `pending` selects queued
  /// members, `active` selects running ones (both default `true`). Returns the
  /// number signalled. The group stays usable afterward.
  //baml:vm
  function cancel(self, pending: bool? = true, active: bool? = true) -> int throws never {
    $rust_function
  }

  /// Change the concurrency cap. Increasing admits queued members up to the new
  /// cap immediately; decreasing never preempts running members; `0` pauses.
  //baml:vm
  function set_limit(self, limit: int) -> void throws never {
    $rust_function
  }

  /// The current concurrency cap.
  //baml:vm
  function limit(self) -> int throws never {
    $rust_function
  }

  /// The optional debug name.
  //baml:vm
  function name(self) -> string? throws never {
    $rust_function
  }

  /// Instantaneous count of running members.
  //baml:vm
  function active_count(self) -> int throws never {
    $rust_function
  }

  /// Instantaneous count of queued members.
  //baml:vm
  function queued_count(self) -> int throws never {
    $rust_function
  }
}

/// A cooperative, one-shot cancellation handle backed by a runtime
/// cancellation token. Passing one via `options(cancel = ...)` links it into a
/// spawned task's effective token: once fired, the task's next `await` throws
/// `baml.panics.Cancelled`. Once cancelled, a token stays cancelled forever.
class CancelToken {
  _handle $rust_type

  /// Create a fresh, un-cancelled token.
  //baml:mut_vm
  function new() -> CancelToken throws never {
    $rust_function
  }

  /// A new token that fires when ANY of `tokens` fires. Cancelling the
  /// returned token does not propagate back to the inputs (one-directional),
  /// so token composition cannot form cycles.
  //baml:mut_vm
  function any(tokens: CancelToken[]) -> CancelToken throws never {
    $rust_function
  }

  /// Request cancellation. Returns `1` if this call transitioned the token
  /// from un-cancelled to cancelled, `0` if it was already cancelled.
  //baml:vm
  function cancel(self) -> int throws never {
    $rust_function
  }

  /// `true` if this token has been cancelled.
  //baml:vm
  function is_cancelled(self) -> bool throws never {
    $rust_function
  }
}

/// Assemble a `SpawnConfig` for a `spawn ... with` clause. `cancel` links a
/// cancel token into the spawn's effective token; `detach = true` opts the
/// spawn out of the parent→child cancel cascade and routes its unhandled
/// errors to the root task instead of the spawner; `group` enrolls the spawn
/// in a `TaskGroup`, so it parks until the group admits it (FIFO).
//baml:mut_vm
function options(group: TaskGroup? = null, cancel: CancelToken? = null, detach: bool? = null) -> SpawnConfig throws never {
  $rust_function
}
