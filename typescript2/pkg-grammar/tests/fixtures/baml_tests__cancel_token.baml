// A `CancelToken` as a spawn modifier — `spawn ... with tok` (BEP-040).
//
// Helpers are top-level functions (test-block locals hit the known
// local-boxing VM bug; see ns_cancel_cascade/cancel_cascade.baml).
//
// Only deterministic, `await`-based assertions live here: a CancelToken fires
// the spawn's effective token via an async watcher, so reading `f.state()`
// immediately after `tok.cancel()` would race the watcher. `await` blocks
// until the future settles, so its outcome is deterministic.

// Awaiting a token-cancelled spawn re-throws `Cancelled`, which is catchable.
function await_token_cancelled_catches() -> int {
    let tok = baml.spawn.CancelToken.new();
    let f = spawn with tok {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
        42
    };
    let _ = tok.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "await_token_cancelled_catches" {
    assert.equal(await_token_cancelled_catches(), 0)
}

// An un-fired token leaves the spawn to complete normally — the option must
// not perturb the happy path.
function uncancelled_token_completes() -> int {
    let tok = baml.spawn.CancelToken.new();
    let f = spawn with tok { 42 };
    await f
}

test "uncancelled_token_completes" {
    assert.equal(uncancelled_token_completes(), 42)
}

// `CancelToken.cancel()` is synchronous on the token, so `is_cancelled()`
// observes the transition immediately (no future / watcher involved).
function token_is_cancelled_reflects_cancel() -> bool[] {
    let tok = baml.spawn.CancelToken.new();
    let before = tok.is_cancelled();
    let _ = tok.cancel();
    let after = tok.is_cancelled();
    [before, after]
}

test "token_is_cancelled_reflects_cancel" {
    assert.equal(token_is_cancelled_reflects_cancel(), [false, true])
}

// `Root` takes the task out of the parent's cancellation cascade but must not
// break normal completion (that it outlives a cancelled spawner is pinned in
// `ns_spawn_plan`).
function rooted_spawn_completes() -> int {
    let f = spawn with baml.spawn.Root.new() { 42 };
    await f
}

test "rooted_spawn_completes" {
    assert.equal(rooted_spawn_completes(), 42)
}
