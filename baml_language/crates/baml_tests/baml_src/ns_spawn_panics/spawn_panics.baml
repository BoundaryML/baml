// A panic in a spawned task, seen from whoever waits for the task.
//
// A panic is RAISED, not thrown (see ns_panic_channel): it is no part of the
// task's `Future<T, E>` error type, a `_` arm does not trap it, and only an
// arm that names a `baml.panics.*` class does. These tests pin how that
// carries across `spawn`:
//
//   - `await` re-raises the task's panic in the awaiter, payload intact;
//   - `catch_all` around the `await` does not catch it;
//   - `baml.future.all_settled` turns it into a `baml.future.Panicked` value;
//   - a `defer` in the task still runs while the panic unwinds.
//
// Every test observes the task (awaits it, directly or through `all_settled`)
// before it reads a side effect, so nothing here races the task. A panicking
// task that NOBODY observes is reported by the host instead; that cannot be
// asserted from inside the corpus, which has to exit 0, so it lives in
// crates/baml_cli/tests/exit_code_e2e.rs.

function sp_explode() -> int {
    baml.sys.panic("boom")
}

function sp_await_reraises() -> string {
    let task = spawn { sp_explode() };
    let value = (await task) catch (raised) {
        let panic: baml.panics.UserPanic => return `panic: ${panic.message}`,
    };
    `value: ${value}`
}

test "await re-raises a spawned task's panic with its payload" {
    assert.equal(sp_await_reraises(), "panic: boom")
}

// The inner `catch_all` is exhaustive over errors and still lets the panic
// through; the outer arm that names the panic class is what traps it.
function sp_await_inside_catch_all() -> string {
    let task = spawn { sp_explode() };
    let guarded = () -> string {
        let value = (await task) catch_all (e) {
            _ => return "caught as an error",
        };
        `value: ${value}`
    };
    guarded() catch (raised) {
        let panic: baml.panics.UserPanic => `panic: ${panic.message}`,
    }
}

test "catch_all around an await does not catch the task's panic" {
    assert.equal(sp_await_inside_catch_all(), "panic: boom")
}

function sp_settled_outcome() -> string {
    let task = spawn { sp_explode() };
    let outcomes = await baml.future.all_settled([task]);
    match (outcomes[0]) {
        let success: baml.future.Success<int> => `value: ${success.value}`,
        let panicked: baml.future.Panicked => match (panicked.panic) {
            let panic: baml.panics.UserPanic => `panicked: ${panic.message}`,
            _ => "panicked: another panic",
        },
        _ => "failure",
    }
}

test "all_settled reports a single panicked task as Panicked" {
    assert.equal(sp_settled_outcome(), "panicked: boom")
}

function sp_defer_then_settled() -> string[] {
    let log: string[] = [];
    let task = spawn {
        defer { log.push("deferred") }
        log.push("started");
        sp_explode()
    };
    let outcomes = await baml.future.all_settled([task]);
    log.push(match (outcomes[0]) {
        baml.future.Panicked => "settled: panicked",
        _ => "settled: other",
    });
    log
}

test "a defer in a panicking task runs before the task settles" {
    assert.equal(sp_defer_then_settled(), [
        "started",
        "deferred",
        "settled: panicked",
    ])
}

function sp_defer_then_await() -> string[] {
    let log: string[] = [];
    let task = spawn {
        defer { log.push("deferred") }
        sp_explode()
    };
    let _ = (await task) catch (raised) {
        baml.panics.UserPanic => {
            log.push("awaiter saw the panic");
            0
        },
    };
    log
}

test "a defer in a panicking task runs before the awaiter sees the panic" {
    assert.equal(sp_defer_then_await(), ["deferred", "awaiter saw the panic"])
}
