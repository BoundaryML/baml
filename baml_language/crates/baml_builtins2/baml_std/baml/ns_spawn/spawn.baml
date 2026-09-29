//! Spawning green threads: the `Plan` a `spawn` builds, and the `Modifier`
//! values its `with` clause applies to it.

/// A one-shot cancellation handle backed by a runtime cancellation token.
///
/// Using one as a spawn modifier links it into the task: once fired, the
/// task's next checkpoint (an `await` or an IO call) panics with
/// `baml.panics.Cancelled`, and so does that of every task it spawns other
/// than a `Root` one, even after the task itself has finished. Once cancelled,
/// a token stays cancelled forever, and a task spawned with it after that
/// never starts: its future settles `Cancelled`.
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
}

/// A value written after `with` in a `spawn`: it transforms the spawn's
/// [`Plan`] before the task starts.
///
/// `T` and `E` are the value and error types of the plan it is applied to.
/// Modifiers apply left to right, each to the plan the one before it returned,
/// so a body wrapped later runs outside the wrappers before it.
interface Modifier<T, E> {
    /// The value type of the plan after this modifier. Unchanged by default.
    type Output = T;
    /// The error type of the plan after this modifier. Unchanged by default.
    type Error = E;
    /// What `apply` itself may throw. A throw happens at the `spawn`, before
    /// any task exists; modifiers to its right are not evaluated.
    type ApplyError = never;

    /// Returns `plan` transformed by this modifier.
    function apply(self, plan: Plan<T, E>) -> Plan<Self.Output, Self.Error> throws Self.ApplyError;
}

/// What a `spawn` starts: the task's body, and what its modifiers have added
/// to it.
///
/// A `spawn` builds a plan from its body and name, passes it through each
/// `with` modifier in turn, and starts the plan the last one returns.
class Plan<T, E> {
    /// What the task runs.
    body: () -> T throws E,
    /// The task's name in traces and error reports.
    name: string?,
    /// The limits the task waits on. It starts once every one of them has
    /// room, and holds none of them while it waits.
    limits: Limit[],
    /// Cancelling any of these tokens cancels the task.
    cancel_tokens: CancelToken[],
    /// Whether the task keeps running when its spawner is cancelled.
    root: bool,

    /// A plan that runs `body`, with nothing added to it.
    function new(body: () -> T throws E, name: string? = null) -> Plan<T, E> throws never {
        Plan { body: body, name: name, limits: [], cancel_tokens: [], root: false }
    }

    /// Returns this plan with a new body: `wrapper`, called with the old one.
    ///
    /// # Examples
    ///
    /// ```baml
    /// plan.wrap((body: () -> int throws E) -> int throws E { body() * 2 })
    /// ```
    function wrap<Output, Error>(
        self,
        wrapper: (body: () -> T throws E) -> Output throws Error,
    ) -> Plan<Output, Error> throws never {
        let body = self.body;
        Plan<Output, Error> {
            body: () -> Output throws Error {
                wrapper(body)
            },
            name: self.name,
            limits: self.limits,
            cancel_tokens: self.cancel_tokens,
            root: self.root,
        }
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
}

/// Caps how many tasks spawned `with` it run at once.
///
/// Tasks over the cap wait in line; the `spawn` still returns its `Future` at
/// once. A waiting task can be cancelled. A task spawned with several limits
/// starts once every one of them has room, and holds none of them while it
/// waits. Two spawns share a cap only if they use the same `Limit` value, and
/// a spawn that lists the same limit twice takes one slot.
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
}

implements<T, E> Modifier<T, E> for Limit {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        Plan { ...plan, limits: plan.limits.concat([self]) }
    }
}

implements<T, E> Modifier<T, E> for Root {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        Plan { ...plan, root: true }
    }
}

implements<T, E> Modifier<T, E> for CancelToken {
    function apply(self, plan: Plan<T, E>) -> Plan<T, E> throws never {
        Plan { ...plan, cancel_tokens: plan.cancel_tokens.concat([self]) }
    }
}

/// (internal) The terminal a `spawn` expression compiles to, once its
/// modifiers have been applied. Not a way to start a task: write
/// `spawn ... { }`.
///
/// Starts `plan` and returns the new task's future. Never throws.
function __spawn<T, E>(plan: Plan<T, E>) -> baml.future.Future<T, E> throws never {
    $spawn
}
