/// BEP-034 — concurrency via `spawn` / `await`.
///
/// `Future<T, E>` is the handle returned by `spawn { body }`. Awaiting
/// a future blocks the current thread until the future settles; on
/// settle, `await` either yields the value or re-throws the future's
/// error.
///
/// Sys-op calls (e.g. `http.fetch`, `fs.read`) are not futures — they
/// return their value directly and yield cooperatively under the hood.
/// To run one concurrently, wrap it in `spawn { ... }`.
///
/// The class is opaque to user code — its fields are runtime-managed.
/// All operations on a future go through the methods declared below.
///
/// State transitions: `Pending` → (`Ready` | `Error` | `Cancelled`).
/// Once settled, a future never changes state again.

/// Surface enumeration mirroring the runtime's `bex_vm_types::FutureRead`
/// states. Used by `Future.state(self)` to expose the settle status of
/// a future to BAML code.
enum FutureState {
  Pending
  Ready
  Error
  Cancelled
}

class Future<T, E> {
  /// Request cancellation of the future. If the future is already
  /// settled, this is a no-op and returns `false`. Otherwise the
  /// future's cancel token fires; the spawned thread will throw
  /// `baml.panics.Cancelled` at its next `await` point.
  ///
  /// Returns `true` if a transition to the Cancelled state was
  /// performed, `false` if the future was already settled.
  function cancel(self) -> bool {
    $rust_function
  }

  /// `true` if the future has reached a terminal state (Ready, Error,
  /// or Cancelled). Once `true`, this method's result never changes.
  function is_settled(self) -> bool {
    $rust_function
  }

  /// `true` if the future was cancelled before producing a value.
  /// Implies `is_settled() == true`.
  function is_cancelled(self) -> bool {
    $rust_function
  }

  /// `true` if the future settled with a successful value. Implies
  /// `is_settled() == true`.
  function is_result(self) -> bool {
    $rust_function
  }

  /// `true` if the future settled with an error. Implies
  /// `is_settled() == true`.
  function is_error(self) -> bool {
    $rust_function
  }

  /// Return the current state of the future as a [`FutureState`]
  /// variant.
  //baml:mut_vm
  function state(self) -> FutureState {
    $rust_function
  }
}
