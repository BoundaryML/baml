// Cancellation unwinds (BEP-040 prerequisite). A cancelled task receives
// `baml.panics.Cancelled` as a throw at its next yield point — a sys-op such
// as `sleep`, or an `await` — so its body unwinds: `defer` bodies run,
// innermost first, and a `catch` may handle it, in which case the future
// settles ready with the body's value. Delivery is one-shot: once delivered,
// the task is shielded, so cleanup may itself sleep, spawn, and await.
//
// Each body parks on a long sleep the test never waits out; the spawner
// cancels after a short sleep of its own.

class Log { entries: string[] }

function sleep_ms(ms: bigint) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

function spawn_sleeper(log: Log, tok: baml.spawn.CancelToken) -> baml.future.Future<int, baml.errors.Io> {
    spawn with baml.spawn.options(cancel = tok) {
        defer { log.entries.push("cleanup") }
        log.entries.push("start")
        sleep_ms(5000n)
        log.entries.push("end")
        0
    }
}

function cancelled_body_runs_defer() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn_sleeper(log, tok)
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a cancelled body runs its defer and the future settles cancelled" {
    assert.equal(cancelled_body_runs_defer(), ["start", "cleanup", "outcome:-1"])
}

function nested_defers_unwind_innermost_first() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        defer { log.entries.push("outer") }
        {
            defer { log.entries.push("inner") }
            sleep_ms(5000n)
        }
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let _ = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries
}
test "nested defers unwind innermost first on cancellation" {
    assert.equal(nested_defers_unwind_innermost_first(), ["inner", "outer"])
}

function defer_that_suspends_completes() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        defer {
            sleep_ms(30n)
            let inner = spawn { 7 }
            let v = await inner
            log.entries.push(`cleanup:${v}`)
        }
        sleep_ms(5000n)
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a defer body may suspend during cancellation and completes" {
    assert.equal(defer_that_suspends_completes(), ["cleanup:7", "outcome:-1"])
}

function body_catching_cancelled_settles_ready() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        let v = { sleep_ms(5000n); 1 } catch (e) { baml.panics.Cancelled => 42 }
        log.entries.push("after catch")
        v
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a body that catches Cancelled settles ready with its value" {
    assert.equal(body_catching_cancelled_settles_ready(), ["after catch", "outcome:42"])
}

function cancelled_at_an_await_runs_defer() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        defer { log.entries.push("cleanup") }
        let never = spawn { sleep_ms(5000n); 1 }
        await never
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a body cancelled while awaiting runs its defer" {
    assert.equal(cancelled_at_an_await_runs_defer(), ["cleanup", "outcome:-1"])
}

function future_state_after_cancel_unwind() -> baml.future.State {
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        defer { let _ = 1 }
        sleep_ms(5000n)
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let _ = (await f) catch (e) { baml.panics.Cancelled => -1 }
    f.state()
}
test "an uncaught cancellation still settles the future cancelled" {
    assert.equal(future_state_after_cancel_unwind(), baml.future.State.Cancelled)
}

function cancelled_in_race_runs_defer() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with baml.spawn.options(cancel = tok) {
        defer { log.entries.push("cleanup") }
        let a = spawn { sleep_ms(5000n); 1 }
        let b = spawn { sleep_ms(5000n); 2 }
        await baml.future.race([a, b])
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a body cancelled while racing futures runs its defer" {
    assert.equal(cancelled_in_race_runs_defer(), ["cleanup", "outcome:-1"])
}
