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
    Pending,
    Ready,
    Error,
    Cancelled,
}

class Future<T, E> {
    /// Request cancellation of the future. If the future is already
    /// settled, this is a no-op and returns `false`. Otherwise the
    /// future's cancel token fires; the spawned thread will throw
    /// `baml.panics.Cancelled` at its next `await` point.
    ///
    /// Returns `true` if a transition to the Cancelled state was
    /// performed, `false` if the future was already settled.
    function cancel(self) -> bool throws never {
        $rust_function
    }

    /// `true` if the future has reached a terminal state (Ready, Error,
    /// or Cancelled). Once `true`, this method's result never changes.
    function is_settled(self) -> bool throws never {
        $rust_function
    }

    /// `true` if the future was cancelled before producing a value.
    /// Implies `is_settled() == true`.
    function is_cancelled(self) -> bool throws never {
        $rust_function
    }

    /// `true` if the future settled with a successful value. Implies
    /// `is_settled() == true`.
    function is_result(self) -> bool throws never {
        $rust_function
    }

    /// `true` if the future settled with an error. Implies
    /// `is_settled() == true`.
    function is_error(self) -> bool throws never {
        $rust_function
    }

    /// Return the current state of the future as a [`FutureState`]
    /// variant.
    //baml:mut_vm
    function state(self) -> FutureState throws never {
        $rust_function
    }
}

/// BEP-034 combinators over a homogeneous array of futures (all sharing value
/// type `T` and error type `E`). The inputs are already running, so awaiting
/// them — even in a loop — runs them concurrently; the combinator just decides
/// how to combine the results. Each returns a new `Future`, so they compose.

/// Await every future and return their values in input order. Like `all`, but
/// the losers KEEP RUNNING on a failure (they are not cancelled). Throws the
/// first error encountered in input order.
function all_complete<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T[], E> {
    spawn {
        futures.map((f) -> {
            await f
        })
    }
}

/// Await every future and return their values in input order, like
/// `all_complete`, but on the FIRST failure the remaining futures are
/// cancelled before the error is re-thrown (JS `Promise.all`). Use
/// `all_complete` instead when losers have side effects that must finish.
///
/// Note: failures are observed in input order (not strictly first-to-fail in
/// wall-clock time); the value contract — every value, or one error — is the
/// same either way.
function all<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T[], E> {
    spawn {
        futures.map((f) -> {
            // `(await f)` is parenthesized so `catch` binds to the await (not to the
            // un-awaited future `f`, which never throws).
            (await f) catch (e) {
                let e => {
                    // A future failed: cancel the rest, then re-throw. Cancelling an
                    // already-settled future is a no-op, so cancelling `f` itself is
                    // harmless.
                    futures.map((g) -> {
                        g.cancel()
                    });
                    // Drain inputs that ALREADY failed (settlement can outrun the
                    // in-order awaits): their errors are parked fire-and-forget and
                    // would otherwise bypass this rethrow and resurface at the
                    // spawner's own await. An unobserved failure reads `is_error()`
                    // (ErrorPending), and awaiting it consumes the parked error. The
                    // drained values are discarded — `all` reports the FIRST failure
                    // in input order, which is `e`.
                    let j = 0;
                    while (j < futures.length()) {
                        let g = futures[j];
                        if (g.is_error()) {
                            (await g) catch (drained) {
                                let drained => null
                            };
                        }
                        j = j + 1;
                    }
                    throw e
                }
            }
        })
    }
}

/// The error `any` throws when EVERY input future failed. `errors` holds the
/// per-future errors in input order. (BEP-034; JS calls this `AggregateError`.)
class AllFailed<E> {
    errors: E[],
}

/// (internal) BEP-034 internal primitive: suspend until the FIRST of `futures` settles —
/// whether it succeeds, fails, or is cancelled — and return its index in input
/// order. Compiles to a dedicated `AwaitAny` suspend point (it is `await` over
/// many futures), and is the building block for `race` and `any`. The inputs
/// are already running, so this only decides *which* settled first; it never
/// re-throws (callers `await futures[i]` to observe the value or error).
function __await_any<T, E>(futures: baml.future.Future<T, E>[]) -> int throws never {
    $await_any
}

/// Settle with the FIRST future to settle, whether it succeeds or fails; the
/// remaining futures are cancelled (JS `Promise.race`). A fast failure from
/// one input therefore wins the race — use `any` when you want the first
/// SUCCESS. Racing an empty array never settles (matching JS).
function race<T, E>(futures: baml.future.Future<T, E>[]) -> baml.future.Future<T, E> {
    spawn {
        let i = baml.future.__await_any(futures);
        // The winner is already settled; cancelling it is a no-op, so a blanket
        // cancel cleanly stops just the losers.
        futures.map((f) -> {
            f.cancel()
        });
        await futures[i]
    }
}

/// Settle with the first future to SUCCEED; the remaining futures are then
/// cancelled (JS `Promise.any`). If every future fails, throws `AllFailed<E>`
/// carrying all the errors in input order. Cancelled inputs are skipped (they
/// neither win nor contribute an error).
function any<T, Err>(
    futures: baml.future.Future<T, Err>[],
) -> baml.future.Future<T, baml.future.AllFailed<Err>> {
    spawn {
        // `winners` holds the first success (0 or 1 element — an array stands in
        // for an optional so we avoid a nullable local). `remaining` shrinks as
        // settled losers are dropped so the next `__await_any` doesn't keep
        // re-reporting the same one.
        let winners: T[] = [];
        let remaining = futures;
        while (remaining.length() > 0) {
            let i = baml.future.__await_any(remaining);
            let f = remaining[i];
            if (f.is_result()) {
                // `f` succeeded; record its value and stop. The catch is unreachable
                // and only there to keep the throws type `AllFailed<Err>` (not `Err`).
                winners.push(
                    (await f) catch (e) {
                        let e => {
                            let failure: baml.future.AllFailed<Err> = baml.future.AllFailed { errors: [e] };
                            throw failure
                        }
                    },
                );
                break;
            }
            if (f.is_error()) {
                // Drain the settled error (returns immediately). This CONSUMES the
                // child's deferred fire-and-forget error so it cannot resurface at
                // the spawner; the value itself is re-read in INPUT order below —
                // a settled Error future rethrows on every await.
                (await f) catch (e) {
                    let e => null
                };
            }
            // Drop `f` (failed or cancelled) and keep going.
            remaining = remaining.slice(0, i).concat(remaining.slice(i + 1, remaining.length()));
        }
        if (winners.length() == 0) {
            // Everything settled without a success: collect the errors by walking
            // the ORIGINAL futures so `AllFailed.errors` is in INPUT order (the
            // main loop observes settlement order). Cancelled inputs are skipped —
            // they neither win nor contribute an error.
            let errors: Err[] = [];
            let j = 0;
            while (j < futures.length()) {
                let g = futures[j];
                if (g.is_error()) {
                    (await g) catch (e) {
                        let e => {
                            errors.push(e);
                            null
                        }
                    };
                }
                j = j + 1;
            }
            // The typed let drives bidirectional checking: a BARE generic literal
            // records no class type args (TIR does not infer literal type args from
            // fields), but checking it against the annotation records
            // `AllFailed<Err>`, which MIR threads to the runtime instance — typed
            // arms like `let e: AllFailed<string>` compare them.
            let failure: baml.future.AllFailed<Err> = baml.future.AllFailed { errors: errors };
            throw failure;
        }
        // A success was recorded; cancel the losers and return it.
        futures.map((g) -> {
            g.cancel()
        });
        winners[0]
    }
}

/// Run `body` under a deadline.
function with_timeout<T, E>(
    limit: baml.time.Duration,
    body: () -> T throws E,
) -> T throws E | baml.errors.Timeout | baml.panics.Cancelled {
    let token = baml.spawn.CancelToken.new();
    let work = spawn with baml.spawn.options(cancel = token) { body() };
    let deadline = spawn {
        baml.sys.sleep(limit);
        token.cancel()
    };
    defer {
        deadline.cancel()
    }
    (await work) catch (e) {
        baml.panics.Cancelled => {
            if (!token.is_cancelled()) {
                throw e;
            }
            let limit_ms = limit.to_milliseconds();
            let reported = limit_ms.to_int() catch (conversion) {
                baml.errors.InvalidArgument => null,
            };
            throw baml.errors.Timeout {
                message: `operation timed out after ${limit_ms}ms`,
                duration_ms: reported,
            }
        },
    }
}
