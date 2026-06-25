// BEP-034 spawn options — `TaskGroup` rate limiting via
// `spawn ... with baml.spawn.options(group = g)`.
//
// Deterministic tests only: a `TaskGroup` member is registered synchronously
// at `spawn`, so `active_count`/`queued_count` read right after spawning are
// exact, and `await`/`catch` outcomes don't depend on scheduler timing.

// The cap is enforced at admission: four members into a `limit = 2` group
// leave exactly two active and two queued (bodies not yet polled). `g.cancel()`
// tears the parked sleeps down.
function caps_active_and_queues_rest() -> int {
    let g = baml.spawn.TaskGroup.new(2);
    let a = spawn with baml.spawn.options(group = g) { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 1 };
    let b = spawn with baml.spawn.options(group = g) { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 1 };
    let c = spawn with baml.spawn.options(group = g) { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 1 };
    let d = spawn with baml.spawn.options(group = g) { baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n)); 1 };
    let snapshot = g.active_count() * 100 + g.queued_count();
    let _ = g.cancel();
    snapshot
}

test "caps_active_and_queues_rest" {
    assert.is_true(baml.deep_equals(caps_active_and_queues_rest(), 202))
}

// More spawns than the limit all complete — the queued ones start as slots free.
function queues_excess_and_all_complete() -> int {
    let g = baml.spawn.TaskGroup.new(2);
    let a = spawn with baml.spawn.options(group = g) { 1 };
    let b = spawn with baml.spawn.options(group = g) { 1 };
    let c = spawn with baml.spawn.options(group = g) { 1 };
    let d = spawn with baml.spawn.options(group = g) { 1 };
    let e = spawn with baml.spawn.options(group = g) { 1 };
    (await a) + (await b) + (await c) + (await d) + (await e)
}

test "queues_excess_and_all_complete" {
    assert.is_true(baml.deep_equals(queues_excess_and_all_complete(), 5))
}

// `group.cancel()` fires its members' tokens; awaiting a cancelled member
// re-throws `Cancelled`, which is catchable.
function group_cancel_cancels_member() -> int {
    let g = baml.spawn.TaskGroup.new(5);
    let f = spawn with baml.spawn.options(group = g) { baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n)); 42 };
    let _ = g.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "group_cancel_cancels_member" {
    assert.is_true(baml.deep_equals(group_cancel_cancels_member(), 0))
}

// `limit()` / `set_limit()` round-trip.
function set_limit_updates_limit() -> int {
    let g = baml.spawn.TaskGroup.new(3);
    let before = g.limit();
    g.set_limit(7);
    before + g.limit()
}

test "set_limit_updates_limit" {
    assert.is_true(baml.deep_equals(set_limit_updates_limit(), 10))
}
