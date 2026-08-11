// Tests for spawn body throws → `await` re-throws.
//
// Converted from crates/baml_tests/tests/spawn_throws.rs.
//
// `await_uncaught_bubbles_to_host`: the Rust test asserts on the host-level
// EngineError debug string (class name + message field). In BAML we cannot
// observe the host error object, but we can faithfully verify the substance:
// that `baml.errors.Io { message: "boom" }` propagates through the await
// and is catchable with the correct fields. The catch-arm assertion replaces
// the Rust `format!("{err:?}").contains(...)` checks.
//
// `await_rethrows_caught_by_user_catch`: direct value test → convertible.

function boom_io() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "boom" }
}

// The Rust test asserts on the host-level EngineError debug string, which we
// cannot observe in BAML. We verify the substance instead: that the error
// propagates through the await and the message field is intact. Because `await f`
// returns `int` on success and the catch arm returns `string`, we catch from
// the test block directly (avoids the mixed-type branch problem).
function await_uncaught_spawns_io_fn() -> int throws baml.errors.Io {
    let f = spawn { boom_io() };
    await f
}

test "await_uncaught_bubbles_to_host" {
    assert.equal(await_uncaught_spawns_io_fn() catch (e) { baml.errors.Io => e.message }, "boom")
}

function boom_io_x() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "x" }
}

function await_rethrows_caught_by_user_catch_fn() -> int {
    let f = spawn { boom_io_x() };
    (await f) catch (e) { baml.errors.Io => 99 }
}

test "await_rethrows_caught_by_user_catch" {
    assert.equal(await_rethrows_caught_by_user_catch_fn(), 99)
}

// B-612 regression lock: an erroring child whose error IS observed (awaited and
// caught) must NOT be re-surfaced by the end-of-run drain. The `sleep` lets the
// child throw and enqueue its error on the parent's fire-and-forget queue BEFORE
// the await, so the await path consumes the deferred error; the drain that runs
// when this function's root completes must then find nothing (the stash entry is
// gone) and return the caught value cleanly rather than double-surfacing.
function caught_after_sleep_not_double_surfaced_fn() -> int {
    let f = spawn { boom_io_x() };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(30n));
    (await f) catch (e) { baml.errors.Io => 99 }
}

test "caught_after_sleep_not_double_surfaced" {
    assert.equal(caught_after_sleep_not_double_surfaced_fn(), 99)
}

// B-650: a *racing* erroring child that IS observed (awaited + caught) with NO
// sleep must still not double-surface. Without the sleep the child has not run
// when the `await` lands, so the await itself drives the child (via
// `future_ready`) and consumes its deferred error into the `catch`. The
// end-of-run wait then finds this future already settled (not `Pending`) and
// the drain finds the stash entry already gone — returning the caught value
// cleanly rather than re-surfacing an already-handled error.
function racing_caught_not_double_surfaced_fn() -> int {
    let f = spawn { boom_io_x() };
    (await f) catch (e) { baml.errors.Io => 99 }
}

test "racing_caught_not_double_surfaced" {
    assert.equal(racing_caught_not_double_surfaced_fn(), 99)
}

// B-650 negative guard: a *racing* never-awaited spawn (NO sleep) whose body
// SUCCEEDS must return cleanly. The end-of-run wait drives the still-outstanding
// child to completion before the root's drain; a child that completes settles
// `Fulfilled`, so the wait must not false-surface an error or corrupt the
// returned value. (Positive surfacing of a racing *throw* is a host-level exit-1
// EngineError that a `test` block cannot observe; it is covered by the Rust
// engine tests in crates/baml_tests/tests/spawn_semantics.rs.)
function racing_successful_spawn_returns_cleanly_fn() -> int {
    let f = spawn { 42 };
    7
}

test "racing_successful_spawn_returns_cleanly" {
    assert.equal(racing_successful_spawn_returns_cleanly_fn(), 7)
}

function caught_not_preempted_by_unrelated_await_fn() -> int {
    let failing = spawn { boom_io_x() };
    let unrelated = spawn {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(50n));
        7
    };
    let _ = await unrelated;
    (await failing) catch (e) {
        baml.errors.Io => 99
    }
}

test "caught_not_preempted_by_unrelated_await" {
    assert.equal(caught_not_preempted_by_unrelated_await_fn(), 99)
}
