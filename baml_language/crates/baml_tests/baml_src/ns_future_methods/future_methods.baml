// Tests for native methods on `baml.future.Future` (BEP-034 Phase F).
//
// Converted from crates/baml_tests/tests/future_methods.rs. All 4 tests assert
// on deterministic future state / variant — no timing assertions — so all are
// convertible.
//
// Helper functions live at top level to avoid the test-block local-boxing VM
// bug. Results that are enum variants are asserted via `assert.equal`
// against the enum value; bool-array results use a local-bound expected.

// All accessors after a successful settle: is_settled / is_result are true;
// is_error / is_cancelled are false; cancel() returns false (the future is
// already terminal, no transition performed).
function methods_after_successful_settle() -> bool[] {
    let f = spawn { 42 };
    let _ = await f;
    [
        f.is_settled(),
        f.is_result(),
        f.is_error(),
        f.is_cancelled(),
        f.cancel(),
    ]
}

test "methods_after_successful_settle" {
    // Compare against literal directly: a test-block local-bound expected
    // compares unequal to a function result (VM bug).
    assert.equal(methods_after_successful_settle(), [true, true, false, false, false])
}

// Pending future (long sleep keeps the spawn body parked): state() returns
// Pending. B-650: the root now WAITS for outstanding spawns at end of run, so
// this 5s-sleep future can no longer be left orphaned — it would stall
// shutdown. Capture the Pending state, then cancel it so it settles promptly
// (a cancelled sleep is interrupted immediately, not run to term).
function methods_on_pending_future() -> baml.future.FutureState {
    let f = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 42 };
    let _ = f.is_settled();
    let observed = f.state();
    let _ = f.cancel();
    observed
}

test "methods_on_pending_future" {
    assert.equal(methods_on_pending_future(), baml.future.FutureState.Pending)
}

// After f.cancel() on a pending future: state() returns Cancelled.
function methods_after_cancel() -> baml.future.FutureState {
    let f = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 42 };
    let _ = f.cancel();
    f.state()
}

test "methods_after_cancel" {
    assert.equal(methods_after_cancel(), baml.future.FutureState.Cancelled)
}

// Helper that throws so we can spawn it and observe an errored future.
function fail_io() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "x" }
}

// After awaiting an errored future (catch swallows the throw): state() returns Error.
function methods_on_errored_future() -> baml.future.FutureState {
    let f = spawn { fail_io() };
    let _ = (await f) catch (e) { baml.errors.Io => 0 };
    f.state()
}

test "methods_on_errored_future" {
    assert.equal(methods_on_errored_future(), baml.future.FutureState.Error)
}
