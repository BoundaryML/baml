// BEP-034 spawn options — `spawn ... with baml.spawn.options(cancel = tok)`.
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
    let f = spawn with baml.spawn.options(cancel = tok) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
        42
    };
    let _ = tok.cancel();
    (await f) catch (e) { baml.panics.Cancelled => 0 }
}

test "await_token_cancelled_catches" {
    assert.is_true(baml.deep_equals(await_token_cancelled_catches(), 0))
}

// An un-fired token leaves the spawn to complete normally — the option must
// not perturb the happy path.
function uncancelled_token_completes() -> int {
    let tok = baml.spawn.CancelToken.new();
    let f = spawn with baml.spawn.options(cancel = tok) { 42 };
    await f
}

test "uncancelled_token_completes" {
    assert.is_true(baml.deep_equals(uncancelled_token_completes(), 42))
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
    assert.is_true(baml.deep_equals(token_is_cancelled_reflects_cancel(), [false, true]))
}

// `detach = true` opts out of the parent cascade but must not break normal
// completion (its survival/cascade-independence is verified by construction in
// the engine: a detached spawn gets a fresh effective token).
function detached_spawn_completes() -> int {
    let f = spawn with baml.spawn.options(detach = true) { 42 };
    await f
}

test "detached_spawn_completes" {
    assert.is_true(baml.deep_equals(detached_spawn_completes(), 42))
}
