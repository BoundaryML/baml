// Tests for BEP-034 cancellation semantics.
//
// Converted from crates/baml_tests/tests/cancel_cascade.rs. Three of the four
// tests assert on deterministic values/states → converted here. One test
// (`cancel_unblocks_await_on_non_descendant`) asserts on wall-clock timing
// (elapsed < 5s) and is left in the Rust file.
//
// Helper functions live at top level to avoid the test-block local-boxing VM bug.

function boom_io() -> int throws baml.errors.Io {
    throw baml.errors.Io { message: "boom" }
}

// BEP-034: "If a fire-and-forget task throws an unhandled error, the error
// propagates to the parent task at its next `await` point."
//
// NOT CONVERTIBLE: the fire-and-forget error propagation mechanism surfaces as
// a host-level `UnhandledThrow` that bypasses all user `catch` expressions in
// BAML (even wildcard `_ => ...`). The Rust test asserts `output.result.is_err()`
// which corresponds to a host-level error, not a user-catchable throw. There is
// no BAML test-block construct that can assert "this must produce a host error"
// rather than passing. Left in the Rust file.
//
// function fire_and_forget_error_fn() -> int {
//     let _ = spawn { boom_io() };
//     await spawn { 99 }
// }
//
// test "fire_and_forget_error_surfaces_at_next_await" {
//     {
//         fire_and_forget_error_fn();
//         baml.sys.panic("expected error propagation")
//     } catch (e) {
//         baml.errors.Io => null  // never reached; UnhandledThrow bypasses catch
//     }
// }

// BEP-034 Phase G: `let f = spawn { sleep(60s); 42 }; f.cancel(); await f
// catch (Cancelled) { 0 }` → 0. Confirms the full round-trip from user-side
// cancel → await throws Cancelled → user catches.
function cancel_from_handle_then_await_catches_cancelled() -> int {
    let f = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
    let _ = f.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "cancel_from_handle_then_await_catches_cancelled" {
    assert.is_true(baml.deep_equals(cancel_from_handle_then_await_catches_cancelled(), 0))
}

// B-665 regression: `f.cancel()` must still settle a spawn whose heap `Future`
// was RELOCATED by a garbage collection between the spawn and the cancel. The
// cancel reads the future through its (GC-updated) heap pointer and fires a
// cancel token shared across the collector's clone, so a Gen0 minor GC in the
// middle must not turn `cancel()` into a no-op. Concretely: spawn a long
// sleeper, allocate well past the ~10,000-alloc Gen0 `should_collect` threshold
// (one heap array per iteration) so the next safepoint collects and moves the
// pending future, hit that safepoint (a 1ms sleep), THEN cancel and `await`.
// If cancel-after-GC regressed, the future would never settle and `await f`
// would hang forever; a healthy runtime observes `Cancelled` promptly and the
// catch yields 0.
function cancel_after_gc_relocation_still_settles() -> int {
    let f = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };

    // Churn the heap past the Gen0 minor-GC threshold: 20,000 short-lived
    // array allocations (> the 10,000-alloc trigger) guarantee the collector
    // relocates the still-pending spawn future at the following safepoint.
    let sink = 0;
    let i = 0;
    while (i < 20000) {
        let junk = [i, i + 1, i + 2];
        sink += junk[0];
        i += 1;
    }
    // Safepoint: lets the collector run (allocs-since-GC is now past the
    // threshold) and move `f` before we cancel it.
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1n));

    let _ = f.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "cancel_after_gc_relocation_still_settles" {
    assert.is_true(baml.deep_equals(cancel_after_gc_relocation_still_settles(), 0))
}

// State observation: after `waiter` is cancelled mid-await, its Future state
// must reflect Cancelled (not stay Pending).
function cancelled_child_future_state_is_cancelled() -> baml.future.FutureState {
    let slow = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
    let waiter = spawn { await slow };
    let _ = waiter.cancel();
    waiter.state()
}

test "cancelled_child_future_state_is_cancelled" {
    assert.is_true(baml.deep_equals(cancelled_child_future_state_is_cancelled(), baml.future.FutureState.Cancelled))
}
