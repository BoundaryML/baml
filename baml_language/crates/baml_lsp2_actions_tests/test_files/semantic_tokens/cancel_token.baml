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
    assert.equal(await_token_cancelled_catches(), 0)
}

// An un-fired token leaves the spawn to complete normally — the option must
// not perturb the happy path.
function uncancelled_token_completes() -> int {
    let tok = baml.spawn.CancelToken.new();
    let f = spawn with baml.spawn.options(cancel = tok) { 42 };
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

// `detach = true` opts out of the parent cascade but must not break normal
// completion (its survival/cascade-independence is verified by construction in
// the engine: a detached spawn gets a fresh effective token).
function detached_spawn_completes() -> int {
    let f = spawn with baml.spawn.options(detach = true) { 42 };
    await f
}

test "detached_spawn_completes" {
    assert.equal(detached_spawn_completes(), 42)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// cancel_token.baml:1:1 (comment) len=79 "// BEP-034 spawn options — `spawn ... with baml.spawn.options(cancel = tok)`."
// cancel_token.baml:2:1 (comment) len=2 "//"
// cancel_token.baml:3:1 (comment) len=67 "// Helpers are top-level functions (test-block locals hit the known"
// cancel_token.baml:4:1 (comment) len=67 "// local-boxing VM bug; see ns_cancel_cascade/cancel_cascade.baml)."
// cancel_token.baml:5:1 (comment) len=2 "//"
// cancel_token.baml:6:1 (comment) len=78 "// Only deterministic, `await`-based assertions live here: a CancelToken fires"
// cancel_token.baml:7:1 (comment) len=75 "// the spawn's effective token via an async watcher, so reading `f.state()`"
// cancel_token.baml:8:1 (comment) len=74 "// immediately after `tok.cancel()` would race the watcher. `await` blocks"
// cancel_token.baml:9:1 (comment) len=61 "// until the future settles, so its outcome is deterministic."
// cancel_token.baml:11:1 (comment) len=78 "// Awaiting a token-cancelled spawn re-throws `Cancelled`, which is catchable."
// cancel_token.baml:12:1 (keyword) len=8 "function"
// cancel_token.baml:12:10 (function) [declaration] len=29 "await_token_cancelled_catches"
// cancel_token.baml:12:45 (type) [defaultLibrary] len=3 "int"
// cancel_token.baml:13:5 (keyword) len=3 "let"
// cancel_token.baml:13:9 (variable) [declaration] len=3 "tok"
// cancel_token.baml:13:13 (operator) len=1 "="
// cancel_token.baml:13:15 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:13:19 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:13:20 (keyword) len=5 "spawn"
// cancel_token.baml:13:26 (class) [defaultLibrary] len=11 "CancelToken"
// cancel_token.baml:13:38 (method) len=3 "new"
// cancel_token.baml:14:5 (keyword) len=3 "let"
// cancel_token.baml:14:9 (variable) [declaration] len=1 "f"
// cancel_token.baml:14:11 (operator) len=1 "="
// cancel_token.baml:14:13 (keyword) len=5 "spawn"
// cancel_token.baml:14:19 (keyword) len=4 "with"
// cancel_token.baml:14:24 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:14:28 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:14:29 (keyword) len=5 "spawn"
// cancel_token.baml:14:35 (function) len=7 "options"
// cancel_token.baml:14:43 (parameter) len=6 "cancel"
// cancel_token.baml:14:50 (operator) len=1 "="
// cancel_token.baml:14:52 (variable) len=3 "tok"
// cancel_token.baml:15:9 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:15:13 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:15:14 (namespace) [defaultLibrary] len=3 "sys"
// cancel_token.baml:15:17 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:15:18 (function) len=5 "sleep"
// cancel_token.baml:15:24 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:15:28 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:15:29 (namespace) [defaultLibrary] len=4 "time"
// cancel_token.baml:15:33 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:15:34 (class) [defaultLibrary] len=8 "Duration"
// cancel_token.baml:15:43 (method) len=17 "from_milliseconds"
// cancel_token.baml:15:61 (number) len=6 "60000n"
// cancel_token.baml:16:9 (number) len=2 "42"
// cancel_token.baml:18:5 (keyword) len=3 "let"
// cancel_token.baml:18:11 (operator) len=1 "="
// cancel_token.baml:18:13 (variable) len=3 "tok"
// cancel_token.baml:18:17 (method) len=6 "cancel"
// cancel_token.baml:19:6 (keyword) len=5 "await"
// cancel_token.baml:19:12 (variable) len=1 "f"
// cancel_token.baml:19:15 (keyword) len=5 "catch"
// cancel_token.baml:19:22 (parameter) [declaration] len=1 "e"
// cancel_token.baml:19:27 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:19:31 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:19:32 (namespace) [defaultLibrary] len=6 "panics"
// cancel_token.baml:19:38 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:19:39 (class) [defaultLibrary] len=9 "Cancelled"
// cancel_token.baml:19:52 (number) len=1 "0"
// cancel_token.baml:22:1 (keyword) len=4 "test"
// cancel_token.baml:22:6 (string) len=31 "\"await_token_cancelled_catches\""
// cancel_token.baml:23:5 (namespace) [defaultLibrary] len=6 "assert"
// cancel_token.baml:23:11 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:23:12 (function) len=5 "equal"
// cancel_token.baml:23:18 (function) len=29 "await_token_cancelled_catches"
// cancel_token.baml:23:51 (number) len=1 "0"
// cancel_token.baml:26:1 (comment) len=78 "// An un-fired token leaves the spawn to complete normally — the option must"
// cancel_token.baml:27:1 (comment) len=30 "// not perturb the happy path."
// cancel_token.baml:28:1 (keyword) len=8 "function"
// cancel_token.baml:28:10 (function) [declaration] len=27 "uncancelled_token_completes"
// cancel_token.baml:28:43 (type) [defaultLibrary] len=3 "int"
// cancel_token.baml:29:5 (keyword) len=3 "let"
// cancel_token.baml:29:9 (variable) [declaration] len=3 "tok"
// cancel_token.baml:29:13 (operator) len=1 "="
// cancel_token.baml:29:15 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:29:19 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:29:20 (keyword) len=5 "spawn"
// cancel_token.baml:29:26 (class) [defaultLibrary] len=11 "CancelToken"
// cancel_token.baml:29:38 (method) len=3 "new"
// cancel_token.baml:30:5 (keyword) len=3 "let"
// cancel_token.baml:30:9 (variable) [declaration] len=1 "f"
// cancel_token.baml:30:11 (operator) len=1 "="
// cancel_token.baml:30:13 (keyword) len=5 "spawn"
// cancel_token.baml:30:19 (keyword) len=4 "with"
// cancel_token.baml:30:24 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:30:28 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:30:29 (keyword) len=5 "spawn"
// cancel_token.baml:30:35 (function) len=7 "options"
// cancel_token.baml:30:43 (parameter) len=6 "cancel"
// cancel_token.baml:30:50 (operator) len=1 "="
// cancel_token.baml:30:52 (variable) len=3 "tok"
// cancel_token.baml:30:59 (number) len=2 "42"
// cancel_token.baml:31:5 (keyword) len=5 "await"
// cancel_token.baml:31:11 (variable) len=1 "f"
// cancel_token.baml:34:1 (keyword) len=4 "test"
// cancel_token.baml:34:6 (string) len=29 "\"uncancelled_token_completes\""
// cancel_token.baml:35:5 (namespace) [defaultLibrary] len=6 "assert"
// cancel_token.baml:35:11 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:35:12 (function) len=5 "equal"
// cancel_token.baml:35:18 (function) len=27 "uncancelled_token_completes"
// cancel_token.baml:35:49 (number) len=2 "42"
// cancel_token.baml:38:1 (comment) len=74 "// `CancelToken.cancel()` is synchronous on the token, so `is_cancelled()`"
// cancel_token.baml:39:1 (comment) len=70 "// observes the transition immediately (no future / watcher involved)."
// cancel_token.baml:40:1 (keyword) len=8 "function"
// cancel_token.baml:40:10 (function) [declaration] len=34 "token_is_cancelled_reflects_cancel"
// cancel_token.baml:40:50 (type) [defaultLibrary] len=4 "bool"
// cancel_token.baml:41:5 (keyword) len=3 "let"
// cancel_token.baml:41:9 (variable) [declaration] len=3 "tok"
// cancel_token.baml:41:13 (operator) len=1 "="
// cancel_token.baml:41:15 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:41:19 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:41:20 (keyword) len=5 "spawn"
// cancel_token.baml:41:26 (class) [defaultLibrary] len=11 "CancelToken"
// cancel_token.baml:41:38 (method) len=3 "new"
// cancel_token.baml:42:5 (keyword) len=3 "let"
// cancel_token.baml:42:9 (variable) [declaration] len=6 "before"
// cancel_token.baml:42:16 (operator) len=1 "="
// cancel_token.baml:42:18 (variable) len=3 "tok"
// cancel_token.baml:42:22 (method) len=12 "is_cancelled"
// cancel_token.baml:43:5 (keyword) len=3 "let"
// cancel_token.baml:43:11 (operator) len=1 "="
// cancel_token.baml:43:13 (variable) len=3 "tok"
// cancel_token.baml:43:17 (method) len=6 "cancel"
// cancel_token.baml:44:5 (keyword) len=3 "let"
// cancel_token.baml:44:9 (variable) [declaration] len=5 "after"
// cancel_token.baml:44:15 (operator) len=1 "="
// cancel_token.baml:44:17 (variable) len=3 "tok"
// cancel_token.baml:44:21 (method) len=12 "is_cancelled"
// cancel_token.baml:45:6 (variable) len=6 "before"
// cancel_token.baml:45:14 (variable) len=5 "after"
// cancel_token.baml:48:1 (keyword) len=4 "test"
// cancel_token.baml:48:6 (string) len=36 "\"token_is_cancelled_reflects_cancel\""
// cancel_token.baml:49:5 (namespace) [defaultLibrary] len=6 "assert"
// cancel_token.baml:49:11 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:49:12 (function) len=5 "equal"
// cancel_token.baml:49:18 (function) len=34 "token_is_cancelled_reflects_cancel"
// cancel_token.baml:49:57 (boolean) len=5 "false"
// cancel_token.baml:49:64 (boolean) len=4 "true"
// cancel_token.baml:52:1 (comment) len=75 "// `detach = true` opts out of the parent cascade but must not break normal"
// cancel_token.baml:53:1 (comment) len=79 "// completion (its survival/cascade-independence is verified by construction in"
// cancel_token.baml:54:1 (comment) len=62 "// the engine: a detached spawn gets a fresh effective token)."
// cancel_token.baml:55:1 (keyword) len=8 "function"
// cancel_token.baml:55:10 (function) [declaration] len=24 "detached_spawn_completes"
// cancel_token.baml:55:40 (type) [defaultLibrary] len=3 "int"
// cancel_token.baml:56:5 (keyword) len=3 "let"
// cancel_token.baml:56:9 (variable) [declaration] len=1 "f"
// cancel_token.baml:56:11 (operator) len=1 "="
// cancel_token.baml:56:13 (keyword) len=5 "spawn"
// cancel_token.baml:56:19 (keyword) len=4 "with"
// cancel_token.baml:56:24 (namespace) [defaultLibrary] len=4 "baml"
// cancel_token.baml:56:28 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:56:29 (keyword) len=5 "spawn"
// cancel_token.baml:56:35 (function) len=7 "options"
// cancel_token.baml:56:43 (parameter) len=6 "detach"
// cancel_token.baml:56:50 (operator) len=1 "="
// cancel_token.baml:56:52 (boolean) len=4 "true"
// cancel_token.baml:56:60 (number) len=2 "42"
// cancel_token.baml:57:5 (keyword) len=5 "await"
// cancel_token.baml:57:11 (variable) len=1 "f"
// cancel_token.baml:60:1 (keyword) len=4 "test"
// cancel_token.baml:60:6 (string) len=26 "\"detached_spawn_completes\""
// cancel_token.baml:61:5 (namespace) [defaultLibrary] len=6 "assert"
// cancel_token.baml:61:11 (namespace) [defaultLibrary] len=1 "."
// cancel_token.baml:61:12 (function) len=5 "equal"
// cancel_token.baml:61:18 (function) len=24 "detached_spawn_completes"
// cancel_token.baml:61:46 (number) len=2 "42"
