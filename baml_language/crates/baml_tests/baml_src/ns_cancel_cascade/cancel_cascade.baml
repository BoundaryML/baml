// Tests for BEP-034 cancellation semantics.
//
// Helper functions live at top level to avoid the test-block local-boxing VM bug.

// `waiter` awaits `slow` (a 60s sibling, not a child of waiter). Cancelling
// `waiter` fires waiter's own cancel token; without the await-race fix,
// waiter's `await slow` would block on the SetOnce until `slow` settles 60s
// later. The engine race must observe the cancel and settle waiter
// immediately: `await waiter` throws Cancelled promptly (bound is generous
// for CI jitter; the fix lands in tens of ms). Compilation happens before the
// test body runs, so the measured window covers only the engine run. `slow`
// is torn down explicitly so B-650's end-of-run wait doesn't stall 60s.
function cancel_unblocks_await_on_non_descendant_fn() -> bool {
    let started = baml.time.Instant.now();
    let slow = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
    let waiter = spawn { await slow };
    let _ = waiter.cancel();
    // waiter's BexThread is parked in the engine's await on `slow`. cancel()
    // fires waiter's cancel token; awaiting waiter must observe Cancelled now,
    // not after `slow` settles.
    let got = (await waiter) catch (e) { baml.panics.Cancelled => 0 };
    let elapsed = started.elapsed();
    let _ = slow.cancel();
    got == 0 && elapsed.to_milliseconds() < 5000n
}

test "cancel_unblocks_await_on_non_descendant" {
    assert.is_true(cancel_unblocks_await_on_non_descendant_fn())
}

// BEP-034 Phase G: `let f = spawn { sleep(60s); 42 }; f.cancel(); await f
// catch (Cancelled) { 0 }` → 0. Confirms the full round-trip from user-side
// cancel → await throws Cancelled → user catches.
function cancel_from_handle_then_await_catches_cancelled() -> int {
    let f = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
    let _ = f.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "cancel_from_handle_then_await_catches_cancelled" {
    assert.equal(cancel_from_handle_then_await_catches_cancelled(), 0)
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
// catch yields 0. (This also pins that B-650's end-of-run wait does not stall
// on the now-cancelled, cancel-aware sleep.)
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
    assert.equal(cancel_after_gc_relocation_still_settles(), 0)
}

// State observation: after `waiter` is cancelled mid-await, its Future state
// must reflect Cancelled (not stay Pending).
//
// B-650: the root now WAITS for outstanding spawns at end of run. Cancelling
// `waiter` does NOT cascade to its sibling `slow` (cancelling an awaiter does
// not cancel the awaited future), so `slow`'s 60s sleep would keep the process
// alive at shutdown. Tear it down explicitly after capturing `waiter`'s state.
function cancelled_child_future_state_is_cancelled() -> baml.future.FutureState {
    let slow = spawn { baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n)); 42 };
    let waiter = spawn { await slow };
    let _ = waiter.cancel();
    let observed = waiter.state();
    let _ = slow.cancel();
    observed
}

test "cancelled_child_future_state_is_cancelled" {
    assert.equal(cancelled_child_future_state_is_cancelled(), baml.future.FutureState.Cancelled)
}
