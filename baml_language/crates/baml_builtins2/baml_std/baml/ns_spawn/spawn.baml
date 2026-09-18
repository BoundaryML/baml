//! Spawning green threads: the `Plan` a `spawn` builds, the `Modifier`
//! values its `with` clause applies, and the `Execution` capability a
//! modifier's wrapper receives once the task runs.

/// A one-shot cancellation handle backed by a runtime cancellation token.
///
/// Using one as a spawn modifier links it into the task's own token: once
/// fired, the task's next checkpoint (an `await` or an IO call) panics with
/// `baml.panics.Cancelled`. Once cancelled, a token stays cancelled forever.
///
/// # Examples
///
/// ```baml
/// let token = baml.spawn.CancelToken.new()
/// spawn with token {
///     baml.sys.sleep(baml.time.Duration.from_seconds(5)); // cancellation triggers here inside `sleep`
///     baml.io.println("sleep finished");
/// }
/// token.cancel();
/// ```
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

    /// Returns `plan` with this token linked in: once it fires, the task is
    /// cancelled. A task spawned from the plan after that starts cancelled.
    //baml:mut_vm
    //baml:fallible
    function link<T, E>(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        $rust_function
    }
}

/// A value written after `with` in a `spawn`: it transforms the spawn's
/// [`Plan`] before the task starts.
///
/// `T` and `E` are the value and error types of the plan it is applied to.
/// Modifiers apply left to right, and one that wraps execution ends up outside
/// the ones before it.
interface Modifier<T, E> {
    /// The value type of the plan after this modifier. Unchanged by default.
    type Output = T;
    /// The error type of the plan after this modifier. Unchanged by default.
    type Error = E;
    /// What `apply` itself may throw. A throw happens at the `spawn`, before
    /// any task exists; modifiers to its right are not evaluated.
    type ApplyError = never;

    /// Returns `plan` transformed by this modifier.
    ///
    /// `apply` describes the task; it cannot start it. Work that can only fail
    /// once the task runs belongs in a [`Plan.wrap`] wrapper, with its failure
    /// in `Error`.
    function apply(self, plan: Plan<T, E>) -> Plan<Self.Output, Self.Error> throws Self.ApplyError;
}

/// A reusable recipe for a spawned task: a body, and the modifiers applied to
/// it.
///
/// A plan is immutable. `wrap` and `with` return a new plan and leave the
/// receiver as it was. The body cannot be read back or called; a `spawn`
/// expression builds a plan and starts it in one step.
///
/// Plans compare by identity and cannot be converted to JSON.
class Plan<T, E> {
    _handle: $rust_type,

    /// Creates a plan that runs `body`. `name` labels the task in traces and
    /// error reports.
    //baml:mut_vm
    //baml:fallible
    function new(body: () -> T throws E, name: string? = null) -> Plan<T, E> throws never {
        $rust_function
    }

    /// Returns a plan whose execution is `wrapper`.
    ///
    /// When the task runs, `wrapper` receives an [`Execution`] for the plan as
    /// it was before this call, and what it returns or throws is the task's
    /// outcome. A later `wrap` goes outside an earlier one.
    //baml:mut_vm
    //baml:fallible
    function wrap<Output, Error>(
        self,
        wrapper: (Execution<T, E>) -> Output throws Error,
    ) -> Plan<Output, Error> throws never {
        $rust_function
    }

    /// Returns this plan transformed by `modifier`.
    ///
    /// # Throws
    ///
    /// Whatever `modifier.apply` throws.
    function with<Output, Error, ApplyError>(
        self,
        modifier: Modifier<T, E, Output = Output, Error = Error, ApplyError = ApplyError>,
    ) -> Plan<Output, Error> throws ApplyError {
        modifier.apply(self)
    }

    /// The name given to `new`.
    //baml:vm
    //baml:fallible
    function name(self) -> string? throws never {
        $rust_function
    }
}

/// The rest of a running task, handed to a [`Plan.wrap`] wrapper.
///
/// An `Execution` is good for one `run`, and only while the wrapper that
/// received it is running. Copies are the same `Execution`, not new ones.
class Execution<T, E> {
    _plan: $rust_type,
    _handle: $rust_type,

    /// Runs the rest of the task and returns or throws its outcome.
    ///
    /// # Panics
    ///
    /// If `run` was already called, or the wrapper that received this
    /// `Execution` has returned.
    //baml:mut_vm
    //baml:may_yield
    function run(self) -> T throws E {
        $rust_function
    }

    /// The task's name, as given to [`Plan.new`].
    //baml:vm
    //baml:fallible
    function name(self) -> string? throws never {
        $rust_function
    }
}

/// Caps how many tasks spawned `with` it run at once.
///
/// Tasks over the cap wait in line; the `spawn` still returns its `Future` at
/// once. A waiting task can be cancelled. A task spawned with several limits
/// starts once every one of them has room, and holds none of them while it
/// waits. Two spawns share a cap only if they use the same `Limit` value.
///
/// # Examples
///
/// ```baml
/// let provider = baml.spawn.Limit.new(8);
/// let futures = ids.map((id) -> { spawn with provider { fetch(id) } });
/// ```
class Limit {
    _handle: $rust_type,

    /// Creates a limit that admits `capacity` tasks at a time.
    ///
    /// # Throws
    ///
    /// `InvalidArgument` if `capacity` is not positive.
    //baml:mut_vm
    function new(capacity: int) -> Limit throws baml.errors.InvalidArgument {
        $rust_function
    }

    /// How many tasks this limit admits at a time.
    //baml:vm
    function capacity(self) -> int throws never {
        $rust_function
    }

    /// How many tasks hold a slot right now.
    //baml:vm
    function active_count(self) -> int throws never {
        $rust_function
    }

    /// How many tasks are waiting for a slot right now.
    //baml:vm
    function queued_count(self) -> int throws never {
        $rust_function
    }

    /// Returns `plan` admitted through this limit as well. Applying the same
    /// limit again changes nothing.
    //baml:mut_vm
    //baml:fallible
    function admit<T, E>(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        $rust_function
    }
}

/// Spawns a task whose cancellation parent is the runtime, not the spawning
/// task: cancelling the spawner no longer cancels it.
///
/// Nothing else changes. The `Future` still reports the task's outcome and can
/// still cancel it, and an error nobody observes is still reported.
class Root {
    /// Creates a `Root`.
    function new() -> Root throws never {
        Root {}
    }

    /// Returns `plan` with the runtime as its cancellation parent.
    //baml:mut_vm
    //baml:fallible
    function reparent<T, E>(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        $rust_function
    }
}

implements<T, E> Modifier<T, E> for Limit {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        self.admit(plan)
    }
}

implements<T, E> Modifier<T, E> for Root {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        self.reparent(plan)
    }
}

implements<T, E> Modifier<T, E> for CancelToken {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        self.link(plan)
    }
}

/// (internal) The terminal a `spawn` expression compiles to, once its
/// modifiers have been applied. Not a way to start a task: write
/// `spawn ... { }`.
///
/// Starts `plan` and returns the new task's future. Never throws, and each
/// call is an independent task.
function __spawn<T, E>(plan: Plan<T, E>) -> baml.future.Future<T, E> throws never {
    $spawn
}
