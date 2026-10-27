// `baml.sync.Lock`: one task holds it at a time, the holding task takes it
// again freely, waiters enter in arrival order, and every exit releases it.
//
// Ordering between tasks is set up with short sleeps: a task that must be
// waiting for the lock is spawned, then given time to queue, before the
// holder lets go. A spawned holder is waited for (`sync_wait_for`) before it
// is cancelled, since under load a task can be cancelled before it starts.

class SyncCounter {
    n: int,
    lock: baml.sync.Lock,
}

class SyncLog {
    entries: string[],
    lock: baml.sync.Lock,
}

class SyncFailed {
    reason: string,
}

function sync_log() -> SyncLog {
    SyncLog { entries: [], lock: baml.sync.Lock.new() }
}

function sync_sleep(ms: int) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

function sync_push(log: SyncLog, entry: string) -> void {
    log.lock.with(() -> void {
        log.entries.push(entry);
    })
}

/// Holds `log`'s lock for `ms`, logging when it got it.
function sync_hold(log: SyncLog, name: string, ms: int) -> void {
    log.lock.with(() -> void {
        log.entries.push(name);
        sync_sleep(ms)
    })
}

/// Waits until `log` has `count` entries.
function sync_wait_for(log: SyncLog, count: int) -> void {
    while (log.entries.length() < count) {
        sync_sleep(5);
    }
}

// ─── mutual exclusion ────────────────────────────────────────────────────────

// Each task reads, yields, then writes. Without exclusion every task reads the
// same value and all but one update is lost.
function sync_count(tasks: int) -> int {
    let c = SyncCounter { n: 0, lock: baml.sync.Lock.new() };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn {
            c.lock.with(() -> void {
                let read = c.n;
                sync_sleep(1);
                c.n = read + 1;
            })
        });
        i += 1;
    }
    await baml.future.all(futures);
    c.n
}

test "no update is lost when every task holds the lock" {
    assert.equal(sync_count(100), 100)
}

function sync_count_with_guards(tasks: int) -> int {
    let c = SyncCounter { n: 0, lock: baml.sync.Lock.new() };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn {
            let guard = c.lock.acquire();
            defer { guard.release() }
            let read = c.n;
            sync_sleep(1);
            c.n = read + 1;
        });
        i += 1;
    }
    await baml.future.all(futures);
    c.n
}

test "acquire and a deferred release exclude like with" {
    assert.equal(sync_count_with_guards(100), 100)
}

function sync_returns() -> int {
    let lock = baml.sync.Lock.new();
    lock.with(() -> int { 41 + 1 })
}

test "with returns what its closure returns" {
    assert.equal(sync_returns(), 42)
}

// ─── re-entry ────────────────────────────────────────────────────────────────

class SyncLimiter {
    next: int,
    interval: int,
    lock: baml.sync.Lock,

    function reserve(self, now: int) -> int {
        self.lock.with(() -> int {
            if (self.next > now) {
                return self.next - now;
            }
            self.advance(now);
            0
        })
    }

    function advance(self, now: int) -> void {
        self.lock.with(() -> void {
            self.next = now + self.interval;
        })
    }
}

function sync_reentry() -> int[] {
    let limiter = SyncLimiter { next: 0, interval: 10, lock: baml.sync.Lock.new() };
    [limiter.reserve(5), limiter.reserve(7), limiter.reserve(15)]
}

test "a task holding the lock takes it again without waiting" {
    assert.equal(sync_reentry(), [0, 8, 0])
}

function sync_reentry_releases_at_outermost() -> string[] {
    let log = sync_log();
    let other = log.lock.with(() -> baml.future.Future<null, never> {
        log.lock.with(() -> void { log.entries.push("inner"); });
        // The inner exit must not have released the lock: the other task
        // waits until this outer hold ends.
        let other = spawn { sync_push(log, "other") };
        sync_sleep(50);
        log.entries.push("outer");
        other
    });
    await other;
    log.entries
}

test "nested holds release the lock only when the outermost ends" {
    assert.equal(sync_reentry_releases_at_outermost(), ["inner", "outer", "other"])
}

function sync_child_waits_for_parent() -> string[] {
    let log = sync_log();
    let child = log.lock.with(() -> baml.future.Future<null, never> {
        let child = spawn { sync_push(log, "child") };
        sync_sleep(50);
        log.entries.push("parent");
        child
    });
    await child;
    log.entries
}

test "a spawned task does not share its spawner's hold" {
    assert.equal(sync_child_waits_for_parent(), ["parent", "child"])
}

// ─── fairness ────────────────────────────────────────────────────────────────

// The main task holds the lock until every waiter has been spawned, so only
// their arrival order (spaced by sleeps) decides who enters first.
function sync_queue_order() -> string[] {
    let log = sync_log();
    let waiters = log.lock.with(() -> baml.future.Future<null, never>[] {
        log.entries.push("holder");
        let first = spawn { sync_push(log, "first") };
        sync_sleep(30);
        let second = spawn { sync_push(log, "second") };
        sync_sleep(30);
        let third = spawn { sync_push(log, "third") };
        sync_sleep(30);
        [first, second, third]
    });
    await baml.future.all(waiters);
    log.entries
}

test "waiters enter in the order they arrived" {
    assert.equal(sync_queue_order(), ["holder", "first", "second", "third"])
}

// ─── exits ───────────────────────────────────────────────────────────────────

function sync_throwing(log: SyncLog) -> void throws SyncFailed {
    log.lock.with(() -> void throws SyncFailed {
        log.entries.push("threw");
        throw SyncFailed { reason: "boom" }
    })
}

function sync_released_after_throw() -> string[] {
    let log = sync_log();
    sync_throwing(log) catch (e) {
        SyncFailed => null
    };
    let other = spawn { sync_push(log, "after") };
    await other;
    log.entries
}

test "a throw propagates and releases the lock" {
    assert.equal(sync_released_after_throw(), ["threw", "after"])
}

function sync_released_after_panic() -> string[] {
    let log = sync_log();
    {
        log.lock.with(() -> void {
            log.entries.push("panicked");
            baml.sys.panic("boom")
        })
    } catch (e) {
        baml.panics.UserPanic => null
    };
    let other = spawn { sync_push(log, "after") };
    await other;
    log.entries
}

test "a panic propagates and releases the lock" {
    assert.equal(sync_released_after_panic(), ["panicked", "after"])
}

// The holder keeps the lock until it is cancelled itself, after the waiter.
function sync_cancelled_waiter() -> string[] {
    let log = sync_log();
    let holder = spawn { sync_hold(log, "holder", 10000) };
    sync_wait_for(log, 1);
    let waiter = spawn { sync_push(log, "waiter") };
    sync_sleep(20);
    let _ = waiter.cancel();
    let _ = holder.cancel();
    let after = spawn { sync_push(log, "after") };
    await after;
    log.entries
}

test "a task cancelled while waiting never runs and holds nothing" {
    assert.equal(sync_cancelled_waiter(), ["holder", "after"])
}

function sync_cancelled_acquire() -> string[] {
    let log = sync_log();
    let holder = spawn { sync_hold(log, "holder", 10000) };
    sync_wait_for(log, 1);
    let waiter = spawn {
        let guard = log.lock.acquire();
        defer { guard.release() }
        log.entries.push("waiter");
    };
    sync_sleep(20);
    let _ = waiter.cancel();
    let _ = holder.cancel();
    let after = spawn { sync_push(log, "after") };
    await after;
    log.entries
}

test "an acquire cancelled while waiting holds nothing" {
    assert.equal(sync_cancelled_acquire(), ["holder", "after"])
}

function sync_cancelled_holder() -> string[] {
    let log = sync_log();
    let holder = spawn { sync_hold(log, "holder", 10000) };
    sync_wait_for(log, 1);
    let _ = holder.cancel();
    let after = spawn { sync_push(log, "after") };
    await after;
    log.entries
}

test "cancelling the holder releases the lock" {
    assert.equal(sync_cancelled_holder(), ["holder", "after"])
}

// ─── guards ──────────────────────────────────────────────────────────────────

// Releasing a guard twice must not undo the other hold this task has.
function sync_double_release() -> string[] {
    let log = sync_log();
    let outer = log.lock.acquire();
    let inner = log.lock.acquire();
    inner.release();
    inner.release();
    let other = spawn { sync_push(log, "other") };
    sync_sleep(50);
    log.entries.push("main");
    outer.release();
    await other;
    log.entries
}

test "releasing a guard twice releases one hold" {
    assert.equal(sync_double_release(), ["main", "other"])
}

function sync_wrong_task_release() -> string {
    let lock = baml.sync.Lock.new();
    let guard = lock.acquire();
    let thief = spawn { guard.release() };
    let outcome = (await thief) catch (e) {
        baml.panics.LockNotHeld => "panicked",
    };
    guard.release();
    `${outcome ?? "released"}`
}

test "releasing another task's held guard panics" {
    assert.equal(sync_wrong_task_release(), "panicked")
}

// A task that ends holding the lock releases it then; its guard is spent, so
// releasing it later does nothing.
function sync_task_end_releases() -> string[] {
    let log = sync_log();
    let leaked = await spawn {
        let guard = log.lock.acquire();
        log.entries.push("leaked");
        guard
    };
    sync_push(log, "after");
    leaked.release();
    log.entries
}

test "a lock held when its task ends is released then" {
    assert.equal(sync_task_end_releases(), ["leaked", "after"])
}

// ─── scopes ──────────────────────────────────────────────────────────────────

function sync_separate_locks() -> string[] {
    let a = sync_log();
    let b = sync_log();
    let holder = spawn { sync_hold(a, "holder", 10000) };
    sync_wait_for(a, 1);
    // `b` has its own lock: free.
    sync_push(b, "b");
    let _ = holder.cancel();
    b.entries
}

test "separate locks are independent" {
    assert.equal(sync_separate_locks(), ["b"])
}

// ─── top-level initializers ──────────────────────────────────────────────────

class SyncKeyStore {
    key: string,
    lock: baml.sync.Lock,

    function read(self) -> string {
        self.lock.with(() -> string {
            self.key
        })
    }
}

// Evaluated when the engine is created, before any task exists. If taking a
// lock there failed, engine creation would fail and no test here would run.
client SyncLockedClient = openai.ResponsesClient.new(
    model = "gpt-4o-mini",
    api_key = SyncKeyStore { key: "literal-key", lock: baml.sync.Lock.new() }.read(),
);

// ─── Semaphore ───────────────────────────────────────────────────────────────

function sync_semaphore(capacity: int) -> baml.sync.Semaphore {
    baml.sync.Semaphore.new(capacity) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("the capacity is positive"),
    }
}

// Every holder counts itself in while it runs (under a lock), and the most
// seen running at once is kept.
function sync_semaphore_caps(tasks: int) -> int {
    let sem = sync_semaphore(2);
    let running = SyncCounter { n: 0, lock: baml.sync.Lock.new() };
    let most = SyncCounter { n: 0, lock: baml.sync.Lock.new() };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn {
            sem.with(() -> void {
                running.lock.with(() -> void {
                    running.n += 1;
                    if (running.n > most.n) {
                        most.n = running.n;
                    }
                });
                sync_sleep(10);
                running.lock.with(() -> void {
                    running.n -= 1;
                });
            })
        });
        i += 1;
    }
    await baml.future.all(futures);
    most.n
}

test "with lets at most capacity holders run at once" {
    assert.equal(sync_semaphore_caps(8), 2)
}

function sync_permit_release_once() -> int[] {
    let sem = sync_semaphore(2);
    let permit = sem.acquire();
    let held = sem.active_count();
    permit.release();
    permit.release();
    let after = sem.active_count();
    let again = sem.acquire();
    let reacquired = sem.active_count();
    again.release();
    [held, after, reacquired, sem.active_count()]
}

test "a permit releases its slot once" {
    assert.equal(sync_permit_release_once(), [1, 0, 1, 0])
}

function sync_permit_other_task() -> int {
    let sem = sync_semaphore(1);
    let permit = sem.acquire();
    await spawn { permit.release() };
    sem.active_count()
}

test "any task can release a permit" {
    assert.equal(sync_permit_other_task(), 0)
}

// Slots taken by `spawn with` and by `acquire` come from one pool.
function sync_spawn_and_acquire_share() -> string {
    let sem = sync_semaphore(1);
    let log = sync_log();
    let permit = sem.acquire();
    let queued = spawn with sem { sync_push(log, "spawned") };
    let while_held = `${sem.active_count()}/${sem.queued_count()}`;
    sync_push(log, "held");
    permit.release();
    await queued;
    `${while_held}|${log.entries.join(",")}`
}

test "spawn with and acquire take slots from one pool" {
    assert.equal(sync_spawn_and_acquire_share(), "1/1|held,spawned")
}

function sync_semaphore_cancelled_wait() -> string {
    let sem = sync_semaphore(1);
    let log = sync_log();
    let permit = sem.acquire();
    let waiter = spawn { sem.with(() -> void { sync_push(log, "waiter"); }) };
    sync_sleep(20);
    let _ = waiter.cancel();
    let _ = (await waiter) catch (e) {
        baml.panics.Cancelled => null,
    };
    permit.release();
    `${sem.active_count()}/${sem.queued_count()}:${log.entries.length()}`
}

test "a semaphore wait cancelled holds nothing and leaves the queue" {
    assert.equal(sync_semaphore_cancelled_wait(), "0/0:0")
}

function sync_semaphore_throw() -> int {
    let sem = sync_semaphore(1);
    sem.with(() -> void throws SyncFailed {
        throw SyncFailed { reason: "boom" }
    }) catch (e) {
        SyncFailed => null
    };
    sem.active_count()
}

test "a throw inside with releases the slot" {
    assert.equal(sync_semaphore_throw(), 0)
}

// A permit nobody releases keeps its slot past its task (any task could still
// release it), until the garbage collector finds nobody can.
function sync_forgotten_permit() -> int[] {
    let sem = sync_semaphore(1);
    let _ = await spawn {
        let _permit = sem.acquire();
        1
    };
    let before = sem.active_count();
    baml.sys.collect_garbage();
    [before, sem.active_count()]
}

test "a forgotten permit gives its slot back when collected" {
    assert.equal(sync_forgotten_permit(), [1, 0])
}
