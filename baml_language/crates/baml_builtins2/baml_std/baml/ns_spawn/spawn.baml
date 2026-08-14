/// BEP-034 — spawn options and `with` middleware.
///
/// This namespace holds the values a `spawn` can be configured with: a
/// `CancelToken` for cooperative cancellation, a `TaskGroup` for rate
/// limiting, and the `detach` flag — plus `SpawnParams`, the value a
/// `spawn ... with` middleware pipeline transforms before the runtime
/// creates the future.

/// Every `spawn` implicitly constructs a `SpawnParams` from its name and
/// body; the `with` pipeline transforms it left-to-right before the runtime
/// creates the future. Each transformer is an ordinary function
/// `(SpawnParams<T, E>) -> SpawnParams<U, F>` — it may set config fields,
/// wrap `body` (retry, timing), or replace it entirely (type-changing
/// transformers like a fallback that erases the error type).
///
/// NOTE for the runtime: the engine reads these fields BY INDEX at the spawn
/// dispatch site (body=0, name=1, group=2, cancel=3, detach=4) — keep the
/// declaration order in sync with `bex_engine`'s `read_spawn_params`.
class SpawnParams<T, E> {
    body: () -> T throws E,
    name: string?,
    group: TaskGroup?,
    cancel: CancelToken?,
    detach: bool,
}

/// Caps how many spawns referencing it run concurrently. Excess spawns queue
/// FIFO and start as earlier ones settle; the `spawn` still returns its
/// `Future` immediately, so queueing is invisible. Two spawns share a limit iff
/// they reference the same `TaskGroup` value — there is no global registry.
class TaskGroup {
    _handle: $rust_type,

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
    _handle: $rust_type,

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

/// The built-in config transformer for a `spawn ... with` clause: sets the
/// provided fields, leaves everything else (including `body`) untouched, so
/// types are unchanged. `cancel` links a cancel token into the spawn's
/// effective token; `detach = true` opts the spawn out of the parent→child
/// cancel cascade and routes its unhandled errors to the root task instead
/// of the spawner; `group` enrolls the spawn in a `TaskGroup`, so it parks
/// until the group admits it (FIFO).
function options<T, E>(
    group: TaskGroup? = null,
    cancel: CancelToken? = null,
    detach: bool? = null,
) -> (SpawnParams<T, E>) -> SpawnParams<T, E> throws never {
    (params: SpawnParams<T, E>) -> SpawnParams<T, E> {
        SpawnParams {
            body: params.body,
            name: params.name,
            group: group ?? params.group,
            cancel: cancel ?? params.cancel,
            detach: detach ?? params.detach,
        }
    }
}
