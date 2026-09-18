// Cancellation unwinds (BEP-034 cancellation; BEP-040 prerequisite). A
// cancelled task receives `baml.panics.Cancelled` as a throw at its next
// checkpoint — an IO call such as `sleep`, or an `await` — so its body
// unwinds: `defer` bodies run, innermost first, and a `catch` may handle it.
// Catching does not un-cancel the task: every later checkpoint throws again,
// except inside a `defer` body, which runs shielded so cleanup may itself
// sleep, spawn, and await; a task spawned from a shield gets a fresh token.
// The task settles its future: Cancelled once it unwinds, Ready with its value
// if it handled the panic and completed or had no checkpoint left.
//
// Each body parks on a long sleep the test never waits out; the spawner
// cancels after a short sleep of its own.

class Log { entries: string[] }

function sleep_ms(ms: bigint) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

function spawn_sleeper(log: Log, tok: baml.spawn.CancelToken) -> baml.future.Future<int, baml.errors.Io> {
    spawn with tok {
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
    let f = spawn with tok {
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
    let f = spawn with tok {
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
    let f = spawn with tok {
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
    let f = spawn with tok {
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
    let f = spawn with tok {
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
    let f = spawn with tok {
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

// ─── Repeated delivery, scoped shields ───────────────────────────────────────

// Catching `Cancelled` does not un-cancel the task: the next checkpoint
// outside a shield throws again.
function catch_then_next_checkpoint_throws_again() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with tok {
        { sleep_ms(5000n) } catch (e) { baml.panics.Cancelled => { log.entries.push("caught") } }
        log.entries.push("between")
        sleep_ms(5000n)
        log.entries.push("unreachable")
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "catching Cancelled does not un-cancel: the next checkpoint throws again" {
    assert.equal(catch_then_next_checkpoint_throws_again(), ["caught", "between", "outcome:-1"])
}

// A `catch` arm is not shielded (a checkpoint inside it throws), a `defer`
// body is.
function catch_arm_unshielded_defer_shielded() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with tok {
        defer {
            sleep_ms(20n)
            log.entries.push("defer slept")
        }
        { sleep_ms(5000n) } catch (e) {
            baml.panics.Cancelled => {
                { sleep_ms(20n); log.entries.push("arm slept") } catch (e2) {
                    baml.panics.Cancelled => { log.entries.push("arm re-cancelled") }
                }
            }
        }
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a catch arm is not shielded, a defer body is" {
    assert.equal(catch_arm_unshielded_defer_shielded(), ["arm re-cancelled", "defer slept", "outcome:0"])
}

// Cleanup may delegate: a task spawned from a shield gets a fresh token, so
// it is not born cancelled, and its future is awaited to completion.
function shielded_cleanup_delegates() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with tok {
        defer {
            let child = spawn { sleep_ms(30n); 7 }
            let v = (await child) catch (e) { baml.panics.Cancelled => -7 }
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
test "a task spawned from a shielded defer is not born cancelled" {
    assert.equal(shielded_cleanup_delegates(), ["cleanup:7", "outcome:-1"])
}

// The shield reaches a callee through the call itself: as the last statement
// of the defer body, the call continues to a block outside the body, and the
// callee's checkpoint must still be shielded.
function push_after_sleep(log: Log, entry: string) -> void {
    sleep_ms(20n)
    log.entries.push(entry)
}

function callee_is_shielded_through_its_caller() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with tok {
        defer { push_after_sleep(log, "cleanup") }
        sleep_ms(5000n)
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "a callee is shielded through the defer body that calls it, even as its last statement" {
    assert.equal(callee_is_shielded_through_its_caller(), ["cleanup", "outcome:-1"])
}

// Nested defers nest their shields; a throw out of a defer body leaves its
// shield, so the enclosing defer is still shielded and the code after it is
// not.
function nested_shields_and_a_throwing_defer() -> string[] {
    let log = Log { entries: [] }
    let tok = baml.spawn.CancelToken.new()
    let f = spawn with tok {
        defer {
            sleep_ms(10n)
            log.entries.push("outer")
        }
        {
            defer {
                { sleep_ms(10n); throw "from inner" } catch (e) { string => { log.entries.push(`inner:${e}`) } }
                sleep_ms(10n)
                log.entries.push("inner done")
            }
            sleep_ms(5000n)
        }
        0
    }
    sleep_ms(50n)
    let _ = tok.cancel()
    let outcome = (await f) catch (e) { baml.panics.Cancelled => -1 }
    log.entries.push(`outcome:${outcome}`)
    log.entries
}
test "nested defers nest their shields and a throw leaves only its own" {
    assert.equal(nested_shields_and_a_throwing_defer(), ["inner:from inner", "inner done", "outer", "outcome:-1"])
}

// ─── The task settles its future ─────────────────────────────────────────────

// `f.cancel()` fires the token; the state stays Pending until the task
// observes it, then reflects how the task ended.
function cancel_then_state_is_pending_until_observed() -> baml.future.State[] {
    let f = spawn { sleep_ms(5000n); 0 }
    sleep_ms(20n)
    let _ = f.cancel()
    let before = f.state()
    let _ = (await f) catch (e) { baml.panics.Cancelled => -1 }
    let states = [before, f.state()]
    states
}
test "cancel fires the token and the task settles the future" {
    assert.equal(
        cancel_then_state_is_pending_until_observed(),
        [baml.future.State.Pending, baml.future.State.Cancelled],
    )
}

// A task with no checkpoint left when the token fires runs to completion,
// and its value stands. Whether `cancel()` still finds the future pending
// depends on scheduling; the value does not.
function cancel_after_last_checkpoint_completes() -> string {
    let f = spawn { sleep_ms(20n); 42 }
    sleep_ms(120n)
    let _ = f.cancel()
    let v = (await f) catch (e) { baml.panics.Cancelled => -1 }
    `${v}:${f.state() == baml.future.State.Ready}`
}
test "a task cancelled after its last checkpoint completes with its value" {
    assert.equal(cancel_after_last_checkpoint_completes(), "42:true")
}
