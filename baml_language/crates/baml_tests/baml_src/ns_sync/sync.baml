// `baml.sync.atomic(key, fn)`: runs `fn` holding `key`, an object or a list of
// them taken together. Blocks on the same key exclude each other, blocks on
// different keys don't, the holding task re-enters freely, and every exit
// releases the key without undoing writes.
//
// A spawned holder is waited for (`sync_wait_for`) before anything depends on
// it holding the key, since under load a task can start late.

class SyncCounter {
    n: int,
}

class SyncLog {
    entries: string[],
}

class SyncFailed {
    reason: string,
}

function sync_sleep(ms: int) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

function sync_push(log: SyncLog, entry: string) -> void {
    baml.sync.atomic(log, () -> void {
        log.entries.push(entry);
    })
}

/// Waits until `log` has `count` entries.
function sync_wait_for(log: SyncLog, count: int) -> void {
    while (log.entries.length() < count) {
        sync_sleep(5);
    }
}

/// Holds `log` for `ms`, logging when it got it.
function sync_hold(log: SyncLog, name: string, ms: int) -> void {
    baml.sync.atomic(log, () -> void {
        log.entries.push(name);
        sync_sleep(ms)
    })
}

// ─── exclusion ───────────────────────────────────────────────────────────────

// Each task reads, yields, then writes. Without exclusion every task reads the
// same value and all but one update is lost.
function sync_count(tasks: int) -> int {
    let c = SyncCounter { n: 0 };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn {
            baml.sync.atomic(c, () -> void {
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

test "blocks on the same key exclude each other" {
    assert.equal(sync_count(100), 100)
}

function sync_different_keys() -> string[] {
    let a = SyncLog { entries: [] };
    let b = SyncLog { entries: [] };
    let holder = spawn { sync_hold(a, "holder", 10000) };
    sync_wait_for(a, 1);
    // `b` is a different key: free while `a` is held.
    sync_push(b, "b");
    let _ = holder.cancel();
    b.entries
}

test "blocks on different keys don't wait for each other" {
    assert.equal(sync_different_keys(), ["b"])
}

// ─── re-entry ────────────────────────────────────────────────────────────────

class SyncLimiter {
    next: int,
    interval: int,

    function reserve(self, now: int) -> int {
        baml.sync.atomic(self, () -> int {
            if (self.next > now) {
                return self.next - now;
            }
            self.advance(now);
            0
        })
    }

    function advance(self, now: int) -> void {
        baml.sync.atomic(self, () -> void {
            self.next = now + self.interval;
        })
    }
}

function sync_reentry() -> int[] {
    let limiter = SyncLimiter { next: 0, interval: 10 };
    [limiter.reserve(5), limiter.reserve(7), limiter.reserve(15)]
}

test "a task inside atomic on a key enters it again without waiting" {
    assert.equal(sync_reentry(), [0, 8, 0])
}

// ─── several keys ────────────────────────────────────────────────────────────

class SyncAccount {
    balance: int,

    function deposit(self, amount: int) -> void {
        baml.sync.atomic(self, () -> void {
            let read = self.balance;
            sync_sleep(1);
            self.balance = read + amount;
        })
    }
}

function sync_transfer(from: SyncAccount, to: SyncAccount, amount: int) -> void {
    baml.sync.atomic([from, to], () -> void {
        from.deposit(-amount);
        to.deposit(amount);
    })
}

// Transfers both ways at once. Nesting `atomic(from, ...)` around
// `atomic(to, ...)` would deadlock here; a list takes both or neither.
function sync_opposite_transfers(tasks: int) -> int[] {
    let a = SyncAccount { balance: 1000 };
    let b = SyncAccount { balance: 1000 };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn { sync_transfer(a, b, 1) });
        futures.push(spawn { sync_transfer(b, a, 2) });
        i += 1;
    }
    await baml.future.all(futures);
    [a.balance, b.balance]
}

test "a list key takes its objects together, in any order" {
    assert.equal(sync_opposite_transfers(50), [1050, 950])
}

function sync_list_holds_each() -> string[][] {
    let a = SyncLog { entries: [] };
    let b = SyncLog { entries: [] };
    let started = SyncLog { entries: [] };
    let holder = spawn {
        baml.sync.atomic([a, b], () -> void {
            started.entries.push("holder");
            sync_sleep(100);
            a.entries.push("list");
            b.entries.push("list");
        })
    };
    sync_wait_for(started, 1);
    // Each object is held by the list block until it exits.
    let on_a = spawn { sync_push(a, "single") };
    let on_b = spawn { sync_push(b, "single") };
    await on_a;
    await on_b;
    await holder;
    [a.entries, b.entries]
}

test "a block on one object waits for a list block holding it" {
    assert.equal(sync_list_holds_each(), [["list", "single"], ["list", "single"]])
}

// ─── waiting ─────────────────────────────────────────────────────────────────

/// Takes `key`, then logs `name`.
function sync_take(key: SyncLog, log: SyncLog, name: string) -> void {
    baml.sync.atomic(key, () -> void {
        log.entries.push(name);
    })
}

/// Holds `key` until `release` has an entry.
function sync_hold_until(key: SyncLog, release: SyncLog) -> void {
    baml.sync.atomic(key, () -> void {
        key.entries.push("held");
        sync_wait_for(release, 1);
    })
}

// A waiter cancelled while it waits never runs; the key goes to the next one.
function sync_cancelled_waiter() -> string[] {
    let key = SyncLog { entries: [] };
    let release = SyncLog { entries: [] };
    let log = SyncLog { entries: [] };
    let holder = spawn { sync_hold_until(key, release) };
    sync_wait_for(key, 1);
    let cancelled = spawn { sync_take(key, log, "cancelled") };
    let next = spawn { sync_take(key, log, "next") };
    sync_sleep(30);
    let _ = cancelled.cancel();
    let _ = await baml.future.all_settled([cancelled]);
    release.entries.push("go");
    await holder;
    await next;
    log.entries
}

test "a waiter cancelled while it waits never runs" {
    assert.equal(sync_cancelled_waiter(), ["next"])
}

// A list block waiting for a key that stays held does not hold up a block on
// one of its other keys once that key comes free.
function sync_passed_over() -> string[] {
    let a = SyncLog { entries: [] };
    let b = SyncLog { entries: [] };
    let release_a = SyncLog { entries: [] };
    let release_b = SyncLog { entries: [] };
    let log = SyncLog { entries: [] };
    let hold_a = spawn { sync_hold_until(a, release_a) };
    let hold_b = spawn { sync_hold_until(b, release_b) };
    sync_wait_for(a, 1);
    sync_wait_for(b, 1);
    let both = spawn { sync_take_both(a, b, log) };
    sync_sleep(30);
    let one = spawn { sync_take(b, log, "b alone") };
    sync_sleep(30);
    release_b.entries.push("go");
    // `a` is still held: the block on `b` alone must get it anyway.
    sync_wait_for(log, 1);
    release_a.entries.push("go");
    await hold_a;
    await hold_b;
    await both;
    await one;
    log.entries
}

/// Takes `a` and `b` together, then logs it.
function sync_take_both(a: SyncLog, b: SyncLog, log: SyncLog) -> void {
    baml.sync.atomic([a, b], () -> void {
        log.entries.push("both");
    })
}

test "a list block waiting for one key does not hold up its other keys" {
    assert.equal(sync_passed_over(), ["b alone", "both"])
}

// A task already holding `a` that asks for `[a, b]` keeps `a` while it waits
// for `b`.
function sync_list_keeps_held() -> string[] {
    let a = SyncLog { entries: [] };
    let b = SyncLog { entries: [] };
    let log = SyncLog { entries: [] };
    let hold_b = spawn {
        baml.sync.atomic(b, () -> void {
            b.entries.push("held");
            sync_sleep(100);
            log.entries.push("b released");
        })
    };
    sync_wait_for(b, 1);
    let both = spawn {
        baml.sync.atomic(a, () -> void {
            a.entries.push("held");
            sync_take_both(a, b, log);
        })
    };
    sync_wait_for(a, 1);
    let a_alone = spawn { sync_take(a, log, "a alone") };
    await hold_b;
    await both;
    await a_alone;
    log.entries
}

test "a list block keeps the keys it already holds while it waits" {
    assert.equal(sync_list_keeps_held(), ["b released", "both", "a alone"])
}

// A collection moves the objects a list block is waiting for; it still gets
// them once they come free.
function sync_list_wait_across_gc() -> string[] {
    let a = SyncLog { entries: [] };
    let b = SyncLog { entries: [] };
    let log = SyncLog { entries: [] };
    let holder = spawn { sync_hold(a, "held", 100) };
    sync_wait_for(a, 1);
    let both = spawn { sync_take_both(a, b, log) };
    sync_sleep(30);
    baml.sys.collect_garbage();
    await holder;
    await both;
    log.entries
}

test "a list block waiting across a collection gets its keys" {
    assert.equal(sync_list_wait_across_gc(), ["both"])
}

// ─── exits ───────────────────────────────────────────────────────────────────

function sync_throwing(log: SyncLog) -> void throws SyncFailed {
    baml.sync.atomic(log, () -> void throws SyncFailed {
        log.entries.push("written");
        throw SyncFailed { reason: "boom" }
    })
}

function sync_released_after_throw() -> string[] {
    let log = SyncLog { entries: [] };
    sync_throwing(log) catch (e) {
        SyncFailed => null
    };
    let other = spawn { sync_push(log, "after") };
    await other;
    log.entries
}

test "a throw releases the key and keeps earlier writes" {
    assert.equal(sync_released_after_throw(), ["written", "after"])
}

function sync_cancelled_holder() -> string[] {
    let log = SyncLog { entries: [] };
    let holder = spawn { sync_hold(log, "holder", 10000) };
    sync_wait_for(log, 1);
    let _ = holder.cancel();
    let after = spawn { sync_push(log, "after") };
    await after;
    log.entries
}

test "cancelling the holder releases the key" {
    assert.equal(sync_cancelled_holder(), ["holder", "after"])
}

// ─── garbage collection ──────────────────────────────────────────────────────

function sync_held_across_gc() -> string[] {
    let log = SyncLog { entries: [] };
    let waiter = baml.sync.atomic(log, () -> baml.future.Future<null, never> {
        // A full collection moves every survivor, `log` included.
        baml.sys.collect_garbage();
        // This task still holds `log` at its new address...
        baml.sync.atomic(log, () -> void {
            log.entries.push("reentered");
        });
        // ...and another task, which only knows the new address, still waits.
        let waiter = spawn { sync_push(log, "waiter") };
        sync_sleep(50);
        log.entries.push("holder");
        waiter
    });
    await waiter;
    log.entries
}

test "a key stays held when a collection moves its object" {
    assert.equal(sync_held_across_gc(), ["reentered", "holder", "waiter"])
}

// ─── Semaphore ───────────────────────────────────────────────────────────────

function sync_semaphore(capacity: int) -> baml.sync.Semaphore {
    baml.sync.Semaphore.new(capacity) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("the capacity is positive"),
    }
}

// Every holder counts itself in while it runs (under `atomic`), and the most
// seen running at once is kept.
function sync_semaphore_caps(tasks: int) -> int {
    let sem = sync_semaphore(2);
    let running = SyncCounter { n: 0 };
    let most = SyncCounter { n: 0 };
    let futures = [];
    let i = 0;
    while (i < tasks) {
        futures.push(spawn {
            sem.with(() -> void {
                baml.sync.atomic(running, () -> void {
                    running.n += 1;
                    if (running.n > most.n) {
                        most.n = running.n;
                    }
                });
                sync_sleep(10);
                baml.sync.atomic(running, () -> void {
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
    let log = SyncLog { entries: [] };
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
    let log = SyncLog { entries: [] };
    let permit = sem.acquire();
    let waiter = spawn {
        sem.with(() -> void {
            sync_push(log, "waiter");
        })
    };
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
