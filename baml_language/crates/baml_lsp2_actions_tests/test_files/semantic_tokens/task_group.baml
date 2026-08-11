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
    let a = spawn with baml.spawn.options(group = g) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
        1
    };
    let b = spawn with baml.spawn.options(group = g) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
        1
    };
    let c = spawn with baml.spawn.options(group = g) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
        1
    };
    let d = spawn with baml.spawn.options(group = g) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(5000n));
        1
    };
    let snapshot = g.active_count() * 100 + g.queued_count();
    let _ = g.cancel();
    snapshot
}

test "caps_active_and_queues_rest" {
    assert.equal(caps_active_and_queues_rest(), 202)
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
    assert.equal(queues_excess_and_all_complete(), 5)
}

// `group.cancel()` fires its members' tokens; awaiting a cancelled member
// re-throws `Cancelled`, which is catchable.
function group_cancel_cancels_member() -> int {
    let g = baml.spawn.TaskGroup.new(5);
    let f = spawn with baml.spawn.options(group = g) {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(10000n));
        42
    };
    let _ = g.cancel();
    (await f) catch (e) {
        baml.panics.Cancelled => 0
    }
}

test "group_cancel_cancels_member" {
    assert.equal(group_cancel_cancels_member(), 0)
}

// `limit()` / `set_limit()` round-trip.
function set_limit_updates_limit() -> int {
    let g = baml.spawn.TaskGroup.new(3);
    let before = g.limit();
    g.set_limit(7);
    before + g.limit()
}

test "set_limit_updates_limit" {
    assert.equal(set_limit_updates_limit(), 10)
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// task_group.baml:1:1 (comment) len=58 "// BEP-034 spawn options — `TaskGroup` rate limiting via"
// task_group.baml:2:1 (comment) len=50 "// `spawn ... with baml.spawn.options(group = g)`."
// task_group.baml:3:1 (comment) len=2 "//"
// task_group.baml:4:1 (comment) len=77 "// Deterministic tests only: a `TaskGroup` member is registered synchronously"
// task_group.baml:5:1 (comment) len=77 "// at `spawn`, so `active_count`/`queued_count` read right after spawning are"
// task_group.baml:6:1 (comment) len=72 "// exact, and `await`/`catch` outcomes don't depend on scheduler timing."
// task_group.baml:8:1 (comment) len=74 "// The cap is enforced at admission: four members into a `limit = 2` group"
// task_group.baml:9:1 (comment) len=80 "// leave exactly two active and two queued (bodies not yet polled). `g.cancel()`"
// task_group.baml:10:1 (comment) len=32 "// tears the parked sleeps down."
// task_group.baml:11:1 (keyword) len=8 "function"
// task_group.baml:11:10 (function) [declaration] len=27 "caps_active_and_queues_rest"
// task_group.baml:11:43 (type) [defaultLibrary] len=3 "int"
// task_group.baml:12:5 (keyword) len=3 "let"
// task_group.baml:12:9 (variable) [declaration] len=1 "g"
// task_group.baml:12:11 (operator) len=1 "="
// task_group.baml:12:13 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:12:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:12:18 (keyword) len=5 "spawn"
// task_group.baml:12:24 (class) [defaultLibrary] len=9 "TaskGroup"
// task_group.baml:12:34 (method) len=3 "new"
// task_group.baml:12:38 (number) len=1 "2"
// task_group.baml:13:5 (keyword) len=3 "let"
// task_group.baml:13:9 (variable) [declaration] len=1 "a"
// task_group.baml:13:11 (operator) len=1 "="
// task_group.baml:13:13 (keyword) len=5 "spawn"
// task_group.baml:13:19 (keyword) len=4 "with"
// task_group.baml:13:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:13:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:13:29 (keyword) len=5 "spawn"
// task_group.baml:13:35 (function) len=7 "options"
// task_group.baml:13:43 (parameter) len=5 "group"
// task_group.baml:13:49 (operator) len=1 "="
// task_group.baml:13:51 (variable) len=1 "g"
// task_group.baml:14:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:14:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:14:14 (namespace) [defaultLibrary] len=3 "sys"
// task_group.baml:14:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:14:18 (function) len=5 "sleep"
// task_group.baml:14:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:14:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:14:29 (namespace) [defaultLibrary] len=4 "time"
// task_group.baml:14:33 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:14:34 (class) [defaultLibrary] len=8 "Duration"
// task_group.baml:14:43 (method) len=17 "from_milliseconds"
// task_group.baml:14:61 (number) len=5 "5000n"
// task_group.baml:15:9 (number) len=1 "1"
// task_group.baml:17:5 (keyword) len=3 "let"
// task_group.baml:17:9 (variable) [declaration] len=1 "b"
// task_group.baml:17:11 (operator) len=1 "="
// task_group.baml:17:13 (keyword) len=5 "spawn"
// task_group.baml:17:19 (keyword) len=4 "with"
// task_group.baml:17:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:17:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:17:29 (keyword) len=5 "spawn"
// task_group.baml:17:35 (function) len=7 "options"
// task_group.baml:17:43 (parameter) len=5 "group"
// task_group.baml:17:49 (operator) len=1 "="
// task_group.baml:17:51 (variable) len=1 "g"
// task_group.baml:18:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:18:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:18:14 (namespace) [defaultLibrary] len=3 "sys"
// task_group.baml:18:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:18:18 (function) len=5 "sleep"
// task_group.baml:18:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:18:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:18:29 (namespace) [defaultLibrary] len=4 "time"
// task_group.baml:18:33 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:18:34 (class) [defaultLibrary] len=8 "Duration"
// task_group.baml:18:43 (method) len=17 "from_milliseconds"
// task_group.baml:18:61 (number) len=5 "5000n"
// task_group.baml:19:9 (number) len=1 "1"
// task_group.baml:21:5 (keyword) len=3 "let"
// task_group.baml:21:9 (variable) [declaration] len=1 "c"
// task_group.baml:21:11 (operator) len=1 "="
// task_group.baml:21:13 (keyword) len=5 "spawn"
// task_group.baml:21:19 (keyword) len=4 "with"
// task_group.baml:21:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:21:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:21:29 (keyword) len=5 "spawn"
// task_group.baml:21:35 (function) len=7 "options"
// task_group.baml:21:43 (parameter) len=5 "group"
// task_group.baml:21:49 (operator) len=1 "="
// task_group.baml:21:51 (variable) len=1 "g"
// task_group.baml:22:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:22:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:22:14 (namespace) [defaultLibrary] len=3 "sys"
// task_group.baml:22:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:22:18 (function) len=5 "sleep"
// task_group.baml:22:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:22:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:22:29 (namespace) [defaultLibrary] len=4 "time"
// task_group.baml:22:33 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:22:34 (class) [defaultLibrary] len=8 "Duration"
// task_group.baml:22:43 (method) len=17 "from_milliseconds"
// task_group.baml:22:61 (number) len=5 "5000n"
// task_group.baml:23:9 (number) len=1 "1"
// task_group.baml:25:5 (keyword) len=3 "let"
// task_group.baml:25:9 (variable) [declaration] len=1 "d"
// task_group.baml:25:11 (operator) len=1 "="
// task_group.baml:25:13 (keyword) len=5 "spawn"
// task_group.baml:25:19 (keyword) len=4 "with"
// task_group.baml:25:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:25:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:25:29 (keyword) len=5 "spawn"
// task_group.baml:25:35 (function) len=7 "options"
// task_group.baml:25:43 (parameter) len=5 "group"
// task_group.baml:25:49 (operator) len=1 "="
// task_group.baml:25:51 (variable) len=1 "g"
// task_group.baml:26:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:26:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:26:14 (namespace) [defaultLibrary] len=3 "sys"
// task_group.baml:26:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:26:18 (function) len=5 "sleep"
// task_group.baml:26:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:26:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:26:29 (namespace) [defaultLibrary] len=4 "time"
// task_group.baml:26:33 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:26:34 (class) [defaultLibrary] len=8 "Duration"
// task_group.baml:26:43 (method) len=17 "from_milliseconds"
// task_group.baml:26:61 (number) len=5 "5000n"
// task_group.baml:27:9 (number) len=1 "1"
// task_group.baml:29:5 (keyword) len=3 "let"
// task_group.baml:29:9 (variable) [declaration] len=8 "snapshot"
// task_group.baml:29:18 (operator) len=1 "="
// task_group.baml:29:20 (variable) len=1 "g"
// task_group.baml:29:22 (method) len=12 "active_count"
// task_group.baml:29:37 (operator) len=1 "*"
// task_group.baml:29:39 (number) len=3 "100"
// task_group.baml:29:43 (operator) len=1 "+"
// task_group.baml:29:45 (variable) len=1 "g"
// task_group.baml:29:47 (method) len=12 "queued_count"
// task_group.baml:30:5 (keyword) len=3 "let"
// task_group.baml:30:11 (operator) len=1 "="
// task_group.baml:30:13 (variable) len=1 "g"
// task_group.baml:30:15 (method) len=6 "cancel"
// task_group.baml:31:5 (variable) len=8 "snapshot"
// task_group.baml:34:1 (keyword) len=4 "test"
// task_group.baml:34:6 (string) len=29 "\"caps_active_and_queues_rest\""
// task_group.baml:35:5 (namespace) [defaultLibrary] len=6 "assert"
// task_group.baml:35:11 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:35:12 (function) len=5 "equal"
// task_group.baml:35:18 (function) len=27 "caps_active_and_queues_rest"
// task_group.baml:35:49 (number) len=3 "202"
// task_group.baml:38:1 (comment) len=83 "// More spawns than the limit all complete — the queued ones start as slots free."
// task_group.baml:39:1 (keyword) len=8 "function"
// task_group.baml:39:10 (function) [declaration] len=30 "queues_excess_and_all_complete"
// task_group.baml:39:46 (type) [defaultLibrary] len=3 "int"
// task_group.baml:40:5 (keyword) len=3 "let"
// task_group.baml:40:9 (variable) [declaration] len=1 "g"
// task_group.baml:40:11 (operator) len=1 "="
// task_group.baml:40:13 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:40:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:40:18 (keyword) len=5 "spawn"
// task_group.baml:40:24 (class) [defaultLibrary] len=9 "TaskGroup"
// task_group.baml:40:34 (method) len=3 "new"
// task_group.baml:40:38 (number) len=1 "2"
// task_group.baml:41:5 (keyword) len=3 "let"
// task_group.baml:41:9 (variable) [declaration] len=1 "a"
// task_group.baml:41:11 (operator) len=1 "="
// task_group.baml:41:13 (keyword) len=5 "spawn"
// task_group.baml:41:19 (keyword) len=4 "with"
// task_group.baml:41:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:41:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:41:29 (keyword) len=5 "spawn"
// task_group.baml:41:35 (function) len=7 "options"
// task_group.baml:41:43 (parameter) len=5 "group"
// task_group.baml:41:49 (operator) len=1 "="
// task_group.baml:41:51 (variable) len=1 "g"
// task_group.baml:41:56 (number) len=1 "1"
// task_group.baml:42:5 (keyword) len=3 "let"
// task_group.baml:42:9 (variable) [declaration] len=1 "b"
// task_group.baml:42:11 (operator) len=1 "="
// task_group.baml:42:13 (keyword) len=5 "spawn"
// task_group.baml:42:19 (keyword) len=4 "with"
// task_group.baml:42:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:42:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:42:29 (keyword) len=5 "spawn"
// task_group.baml:42:35 (function) len=7 "options"
// task_group.baml:42:43 (parameter) len=5 "group"
// task_group.baml:42:49 (operator) len=1 "="
// task_group.baml:42:51 (variable) len=1 "g"
// task_group.baml:42:56 (number) len=1 "1"
// task_group.baml:43:5 (keyword) len=3 "let"
// task_group.baml:43:9 (variable) [declaration] len=1 "c"
// task_group.baml:43:11 (operator) len=1 "="
// task_group.baml:43:13 (keyword) len=5 "spawn"
// task_group.baml:43:19 (keyword) len=4 "with"
// task_group.baml:43:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:43:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:43:29 (keyword) len=5 "spawn"
// task_group.baml:43:35 (function) len=7 "options"
// task_group.baml:43:43 (parameter) len=5 "group"
// task_group.baml:43:49 (operator) len=1 "="
// task_group.baml:43:51 (variable) len=1 "g"
// task_group.baml:43:56 (number) len=1 "1"
// task_group.baml:44:5 (keyword) len=3 "let"
// task_group.baml:44:9 (variable) [declaration] len=1 "d"
// task_group.baml:44:11 (operator) len=1 "="
// task_group.baml:44:13 (keyword) len=5 "spawn"
// task_group.baml:44:19 (keyword) len=4 "with"
// task_group.baml:44:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:44:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:44:29 (keyword) len=5 "spawn"
// task_group.baml:44:35 (function) len=7 "options"
// task_group.baml:44:43 (parameter) len=5 "group"
// task_group.baml:44:49 (operator) len=1 "="
// task_group.baml:44:51 (variable) len=1 "g"
// task_group.baml:44:56 (number) len=1 "1"
// task_group.baml:45:5 (keyword) len=3 "let"
// task_group.baml:45:9 (variable) [declaration] len=1 "e"
// task_group.baml:45:11 (operator) len=1 "="
// task_group.baml:45:13 (keyword) len=5 "spawn"
// task_group.baml:45:19 (keyword) len=4 "with"
// task_group.baml:45:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:45:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:45:29 (keyword) len=5 "spawn"
// task_group.baml:45:35 (function) len=7 "options"
// task_group.baml:45:43 (parameter) len=5 "group"
// task_group.baml:45:49 (operator) len=1 "="
// task_group.baml:45:51 (variable) len=1 "g"
// task_group.baml:45:56 (number) len=1 "1"
// task_group.baml:46:6 (keyword) len=5 "await"
// task_group.baml:46:12 (variable) len=1 "a"
// task_group.baml:46:15 (operator) len=1 "+"
// task_group.baml:46:18 (keyword) len=5 "await"
// task_group.baml:46:24 (variable) len=1 "b"
// task_group.baml:46:27 (operator) len=1 "+"
// task_group.baml:46:30 (keyword) len=5 "await"
// task_group.baml:46:36 (variable) len=1 "c"
// task_group.baml:46:39 (operator) len=1 "+"
// task_group.baml:46:42 (keyword) len=5 "await"
// task_group.baml:46:48 (variable) len=1 "d"
// task_group.baml:46:51 (operator) len=1 "+"
// task_group.baml:46:54 (keyword) len=5 "await"
// task_group.baml:46:60 (variable) len=1 "e"
// task_group.baml:49:1 (keyword) len=4 "test"
// task_group.baml:49:6 (string) len=32 "\"queues_excess_and_all_complete\""
// task_group.baml:50:5 (namespace) [defaultLibrary] len=6 "assert"
// task_group.baml:50:11 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:50:12 (function) len=5 "equal"
// task_group.baml:50:18 (function) len=30 "queues_excess_and_all_complete"
// task_group.baml:50:52 (number) len=1 "5"
// task_group.baml:53:1 (comment) len=74 "// `group.cancel()` fires its members' tokens; awaiting a cancelled member"
// task_group.baml:54:1 (comment) len=45 "// re-throws `Cancelled`, which is catchable."
// task_group.baml:55:1 (keyword) len=8 "function"
// task_group.baml:55:10 (function) [declaration] len=27 "group_cancel_cancels_member"
// task_group.baml:55:43 (type) [defaultLibrary] len=3 "int"
// task_group.baml:56:5 (keyword) len=3 "let"
// task_group.baml:56:9 (variable) [declaration] len=1 "g"
// task_group.baml:56:11 (operator) len=1 "="
// task_group.baml:56:13 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:56:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:56:18 (keyword) len=5 "spawn"
// task_group.baml:56:24 (class) [defaultLibrary] len=9 "TaskGroup"
// task_group.baml:56:34 (method) len=3 "new"
// task_group.baml:56:38 (number) len=1 "5"
// task_group.baml:57:5 (keyword) len=3 "let"
// task_group.baml:57:9 (variable) [declaration] len=1 "f"
// task_group.baml:57:11 (operator) len=1 "="
// task_group.baml:57:13 (keyword) len=5 "spawn"
// task_group.baml:57:19 (keyword) len=4 "with"
// task_group.baml:57:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:57:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:57:29 (keyword) len=5 "spawn"
// task_group.baml:57:35 (function) len=7 "options"
// task_group.baml:57:43 (parameter) len=5 "group"
// task_group.baml:57:49 (operator) len=1 "="
// task_group.baml:57:51 (variable) len=1 "g"
// task_group.baml:58:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:58:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:58:14 (namespace) [defaultLibrary] len=3 "sys"
// task_group.baml:58:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:58:18 (function) len=5 "sleep"
// task_group.baml:58:24 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:58:28 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:58:29 (namespace) [defaultLibrary] len=4 "time"
// task_group.baml:58:33 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:58:34 (class) [defaultLibrary] len=8 "Duration"
// task_group.baml:58:43 (method) len=17 "from_milliseconds"
// task_group.baml:58:61 (number) len=6 "10000n"
// task_group.baml:59:9 (number) len=2 "42"
// task_group.baml:61:5 (keyword) len=3 "let"
// task_group.baml:61:11 (operator) len=1 "="
// task_group.baml:61:13 (variable) len=1 "g"
// task_group.baml:61:15 (method) len=6 "cancel"
// task_group.baml:62:6 (keyword) len=5 "await"
// task_group.baml:62:12 (variable) len=1 "f"
// task_group.baml:62:15 (keyword) len=5 "catch"
// task_group.baml:62:22 (parameter) [declaration] len=1 "e"
// task_group.baml:63:9 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:63:13 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:63:14 (namespace) [defaultLibrary] len=6 "panics"
// task_group.baml:63:20 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:63:21 (class) [defaultLibrary] len=9 "Cancelled"
// task_group.baml:63:34 (number) len=1 "0"
// task_group.baml:67:1 (keyword) len=4 "test"
// task_group.baml:67:6 (string) len=29 "\"group_cancel_cancels_member\""
// task_group.baml:68:5 (namespace) [defaultLibrary] len=6 "assert"
// task_group.baml:68:11 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:68:12 (function) len=5 "equal"
// task_group.baml:68:18 (function) len=27 "group_cancel_cancels_member"
// task_group.baml:68:49 (number) len=1 "0"
// task_group.baml:71:1 (comment) len=40 "// `limit()` / `set_limit()` round-trip."
// task_group.baml:72:1 (keyword) len=8 "function"
// task_group.baml:72:10 (function) [declaration] len=23 "set_limit_updates_limit"
// task_group.baml:72:39 (type) [defaultLibrary] len=3 "int"
// task_group.baml:73:5 (keyword) len=3 "let"
// task_group.baml:73:9 (variable) [declaration] len=1 "g"
// task_group.baml:73:11 (operator) len=1 "="
// task_group.baml:73:13 (namespace) [defaultLibrary] len=4 "baml"
// task_group.baml:73:17 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:73:18 (keyword) len=5 "spawn"
// task_group.baml:73:24 (class) [defaultLibrary] len=9 "TaskGroup"
// task_group.baml:73:34 (method) len=3 "new"
// task_group.baml:73:38 (number) len=1 "3"
// task_group.baml:74:5 (keyword) len=3 "let"
// task_group.baml:74:9 (variable) [declaration] len=6 "before"
// task_group.baml:74:16 (operator) len=1 "="
// task_group.baml:74:18 (variable) len=1 "g"
// task_group.baml:74:20 (method) len=5 "limit"
// task_group.baml:75:5 (variable) len=1 "g"
// task_group.baml:75:7 (method) len=9 "set_limit"
// task_group.baml:75:17 (number) len=1 "7"
// task_group.baml:76:5 (variable) len=6 "before"
// task_group.baml:76:12 (operator) len=1 "+"
// task_group.baml:76:14 (variable) len=1 "g"
// task_group.baml:76:16 (method) len=5 "limit"
// task_group.baml:79:1 (keyword) len=4 "test"
// task_group.baml:79:6 (string) len=25 "\"set_limit_updates_limit\""
// task_group.baml:80:5 (namespace) [defaultLibrary] len=6 "assert"
// task_group.baml:80:11 (namespace) [defaultLibrary] len=1 "."
// task_group.baml:80:12 (function) len=5 "equal"
// task_group.baml:80:18 (function) len=23 "set_limit_updates_limit"
// task_group.baml:80:45 (number) len=2 "10"
