// Spawn plans and modifiers (BEP-040). `spawn name with m1, m2 { body }` builds
// a `baml.spawn.Plan` from the body and the name, hands it to each modifier in
// turn, and starts the plan the last one returns. A plan is plain data and a
// modifier returns a transformed one; `Plan.wrap` is the helper for a modifier
// that only replaces the body. Modifiers apply left to right, so a body wrapped
// later runs outside the wrappers before it.
//
// Every value under test is produced by a helper function and asserted on
// directly. A modifier built with an object literal is bound to a variable
// first: after `with`, a `{` opens the body. Tasks that must not finish park on a long sleep the test never
// waits out.

class Log { entries: string[] }

function sleep_ms(ms: bigint) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

function limit_of(capacity: int) -> baml.spawn.Limit {
    baml.spawn.Limit.new(capacity) catch (e) {
        baml.errors.InvalidArgument => baml.sys.panic("the capacity is positive"),
    }
}

// ─── plans ───────────────────────────────────────────────────────────────────

function spawn_runs_its_body() -> int {
    let f = spawn "sum" { 41 + 1 };
    await f
}

test "a spawn runs its body and settles its future" {
    assert.equal(spawn_runs_its_body(), 42)
}

function plan_names() -> string[] {
    let named = baml.spawn.Plan.new(body = () -> int { 1 }, name = "fetch");
    let bare = baml.spawn.Plan.new(body = () -> int { 1 });
    let names = [named.name ?? "<none>", bare.name ?? "<none>"];
    names
}

test "a plan keeps the name it was given" {
    assert.equal(plan_names(), ["fetch", "<none>"])
}

// ─── wrapped bodies ──────────────────────────────────────────────────────────

class Tag { log: Log, name: string }

implements<T, E> baml.spawn.Modifier<T, E> for Tag {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        plan.wrap((body: () -> T throws E) -> T throws E {
            self.log.entries.push(`${self.name}:before`);
            defer { self.log.entries.push(`${self.name}:after`) }
            body()
        })
    }
}

function wrappers_nest_last_outermost() -> string[] {
    let log = Log { entries: [] };
    let inner = Tag { log: log, name: "inner" };
    let outer = Tag { log: log, name: "outer" };
    let f = spawn with inner, outer { log.entries.push("body"); 1 };
    let _ = await f;
    log.entries
}

test "modifiers apply left to right and the last wrapper is outermost" {
    assert.equal(
        wrappers_nest_last_outermost(),
        ["outer:before", "inner:before", "body", "inner:after", "outer:after"],
    )
}

class Double {}

implements<E> baml.spawn.Modifier<int, E> for Double {
    function apply(self, plan: baml.spawn.Plan<int, E>) -> baml.spawn.Plan<int, E> throws never {
        plan.wrap((body: () -> int throws E) -> int throws E { body() * 2 })
    }
}

class AddOne {}

implements<E> baml.spawn.Modifier<int, E> for AddOne {
    function apply(self, plan: baml.spawn.Plan<int, E>) -> baml.spawn.Plan<int, E> throws never {
        plan.wrap((body: () -> int throws E) -> int throws E { body() + 1 })
    }
}

// `Double, AddOne` → addOne(double(10)) = 21, not 22.
function chained_wrappers_compose_in_order() -> int {
    let double = Double {};
    let add_one = AddOne {};
    let f = spawn with double, add_one { 10 };
    await f
}

test "chained wrappers compose in application order" {
    assert.equal(chained_wrappers_compose_in_order(), 21)
}

// A modifier held in a variable applies the same as one written inline.
function variable_bound_modifier_still_applies() -> int {
    let double = Double {};
    let f = spawn with double { 21 };
    await f
}

test "a modifier bound to a variable still applies" {
    assert.equal(variable_bound_modifier_still_applies(), 42)
}

// A wrapper is any `(() -> T throws E) -> O` value, a bound method included.
class Offset {
    by: int,

    function around(self, body: () -> int throws never) -> int throws never {
        body() + self.by
    }

    function produce(self) -> int throws never {
        self.by * 2
    }
}

implements baml.spawn.Modifier<int, never> for Offset {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
        plan.wrap(self.around)
    }
}

function bound_method_wrapper() -> int {
    let offset = Offset { by: 1 };
    await spawn with offset { 41 }
}

test "a modifier may wrap with a bound method" {
    assert.equal(bound_method_wrapper(), 42)
}

// A generic parameter bounded by `Modifier` proves the operand from its bound,
// and the bound's pins decide the task's types: here `Output = int`.
function spawn_through_bound<M extends baml.spawn.Modifier<int, never, Output = int, Error = never, ApplyError = never>>(m: M) -> int {
    let f = spawn with m { 21 };
    await f
}

test "a modifier behind a bounded generic applies" {
    assert.equal(spawn_through_bound(Double {}), 42)
}

// An existential modifier applies through its own interface.
function spawn_through_existential(m: baml.spawn.Modifier<int, never, Output = int, Error = never, ApplyError = never>) -> int {
    let f = spawn with m { 21 };
    await f
}

test "a modifier held as an existential applies" {
    assert.equal(spawn_through_existential(Double {}), 42)
}

// ─── modifiers that change the types ─────────────────────────────────────────

class Fallback<T> { value: T }

implements<T, E> baml.spawn.Modifier<T, E> for Fallback<T> {
    type Error = never
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, never> throws never {
        plan.wrap((body: () -> T throws E) -> T throws never {
            body() catch (e) {
                let e => self.value,
            }
        })
    }
}

function flaky(n: int) -> int throws string {
    if (n > 0) {
        throw "boom"
    }
    n
}

function fallback_erases_the_error_type() -> int {
    let fallback = Fallback<int> { value: 99 };
    let f: baml.future.Future<int, never> = spawn with fallback { flaky(1) };
    // `Future<int, never>`: awaiting needs no catch.
    await f
}

test "a modifier may change the error type" {
    assert.equal(fallback_erases_the_error_type(), 99)
}

class Stringify {}

implements<T, E> baml.spawn.Modifier<T, E> for Stringify {
    type Output = string
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<string, E> throws never {
        plan.wrap((body: () -> T throws E) -> string throws E {
            let _ = body();
            "wrapped"
        })
    }
}

function output_type_changes() -> string {
    let stringify = Stringify {};
    let f: baml.future.Future<string, never> = spawn with stringify { 1 };
    await f
}

test "a modifier may change the output type" {
    assert.equal(output_type_changes(), "wrapped")
}

// The future's runtime type is the last plan's, generic arguments realized.
function future_type_after(v: bool) -> string {
    let stringify = Stringify {};
    let f = spawn with stringify { v };
    reflect.Type.of_value(f).to_string()
}

function generic_future_type_after<T>(v: T) -> string {
    let stringify = Stringify {};
    let f = spawn with stringify { v };
    reflect.Type.of_value(f).to_string()
}

test "a modifier's types are the future's at runtime" {
    assert.equal(future_type_after(true), "baml.future.Future<string, never>");
    assert.equal(generic_future_type_after<int>(1), "baml.future.Future<string, never>")
}

// ─── apply errors ────────────────────────────────────────────────────────────

class Refuses { log: Log }

implements<T, E> baml.spawn.Modifier<T, E> for Refuses {
    type ApplyError = string
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws string {
        self.log.entries.push("refuses.apply");
        throw "refused"
    }
}

function logged_limit(log: Log) -> baml.spawn.Limit {
    log.entries.push("make limit");
    limit_of(1)
}

// A modifier that throws does so at the `spawn`: no task exists, the body never
// runs, and the operand to its right is never evaluated.
function apply_error_throws_at_the_spawn() -> string[] {
    let log = Log { entries: [] };
    let outcome = {
        let refuses = Refuses { log: log };
        let _ = spawn with refuses, logged_limit(log) { log.entries.push("body"); 1 };
        "spawned"
    } catch (e) {
        string => e,
    };
    log.entries.push(outcome);
    log.entries
}

test "an apply error throws at the spawn and stops the modifier chain" {
    assert.equal(apply_error_throws_at_the_spawn(), ["refuses.apply", "refused"])
}

// ─── transforming the plan ───────────────────────────────────────────────────

// A wrapper calls the old body as often as it likes: here, until it succeeds
// or the last attempt's error propagates.
class Retry { attempts: int }

implements<T, E> baml.spawn.Modifier<T, E> for Retry {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        plan.wrap((body: () -> T throws E) -> T throws E {
            for (let tries = 1; tries < self.attempts; tries += 1) {
                let value = body() catch (e) {
                    _ => { continue; },
                };
                return value;
            }
            body()
        })
    }
}

function fails_until(log: Log, successes_after: int) -> int throws string {
    log.entries.push("try");
    if (log.entries.length() <= successes_after) {
        throw "not yet"
    }
    log.entries.length()
}

function retry_calls_the_body_again() -> string {
    let log = Log { entries: [] };
    let retry = Retry { attempts: 3 };
    let value = await spawn with retry { fails_until(log, 2) };
    `${value}:${log.entries.length()}`
}

test "a wrapper may call the old body more than once" {
    assert.equal(retry_calls_the_body_again(), "3:3")
}

function retry_gives_up() -> string {
    let log = Log { entries: [] };
    let retry = Retry { attempts: 2 };
    let outcome = (await spawn with retry { fails_until(log, 5) }) catch (e) {
        string => e,
    };
    `${outcome}:${log.entries.length()}`
}

test "a wrapper's own throw is the task's error" {
    assert.equal(retry_gives_up(), "not yet:2")
}

// A wrapper may return without calling the old body at all.
class Skip {}

implements baml.spawn.Modifier<int, never> for Skip {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
        plan.wrap((body: () -> int throws never) -> int throws never { 7 })
    }
}

function skipped_body_never_runs() -> string {
    let log = Log { entries: [] };
    let skip = Skip {};
    let value = await spawn with skip { log.entries.push("body"); 1 };
    `${value}:${log.entries.length()}`
}

test "a wrapper may skip the body" {
    assert.equal(skipped_body_never_runs(), "7:0")
}

// A modifier may set the plan's fields directly; a later one sees the result.
class Rename { to: string }

implements<T, E> baml.spawn.Modifier<T, E> for Rename {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        baml.spawn.Plan { ...plan, name: self.to }
    }
}

class ReadName { log: Log }

implements<T, E> baml.spawn.Modifier<T, E> for ReadName {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        self.log.entries.push(plan.name ?? "<none>");
        plan
    }
}

function renamed_plan() -> string[] {
    let log = Log { entries: [] };
    let rename = Rename { to: "renamed" };
    let read = ReadName { log: log };
    let _ = await spawn "original" with read, rename, read { 1 };
    log.entries
}

test "a modifier may set a plan's fields" {
    assert.equal(renamed_plan(), ["original", "renamed"])
}

// A plan's body is any `() -> T` value, a bound method included: the task
// enters it with the receiver as `self`.
class UseProduce { offset: Offset }

implements baml.spawn.Modifier<int, never> for UseProduce {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
        baml.spawn.Plan { ...plan, body: self.offset.produce }
    }
}

function bound_method_body() -> int {
    let use_produce = UseProduce { offset: Offset { by: 21 } };
    await spawn with use_produce { 0 }
}

test "a bound method is a task's body" {
    assert.equal(bound_method_body(), 42)
}

// ─── Limit ───────────────────────────────────────────────────────────────────

function limit_rejects_zero() -> string {
    let made = baml.spawn.Limit.new(0) catch (e) {
        baml.errors.InvalidArgument => null,
    };
    if (made == null) { "rejected" } else { "accepted" }
}

test "a limit's capacity must be positive" {
    assert.equal(limit_rejects_zero(), "rejected")
}

// Admission registers at the `spawn`, so the counts read right after it are
// exact: four tasks into a capacity of two leave two running and two waiting.
// A count read AFTER an `await` is not exact -- a finished task releases its
// slot on its own task, which may not have run yet -- so nothing below reads
// one there.
function caps_active_and_queues_the_rest() -> string {
    let limit = limit_of(2);
    let a = spawn with limit { sleep_ms(5000n); 1 };
    let b = spawn with limit { sleep_ms(5000n); 1 };
    let c = spawn with limit { sleep_ms(5000n); 1 };
    let d = spawn with limit { sleep_ms(5000n); 1 };
    let snapshot = `${limit.capacity()}:${limit.active_count()}:${limit.queued_count()}`;
    let futures = [a, b, c, d];
    futures.map((f) -> { f.cancel() });
    snapshot
}

test "a limit admits up to its capacity and queues the rest" {
    assert.equal(caps_active_and_queues_the_rest(), "2:2:2")
}

function queued_tasks_all_complete() -> int {
    let limit = limit_of(2);
    let a = spawn with limit { 1 };
    let b = spawn with limit { 1 };
    let c = spawn with limit { 1 };
    let d = spawn with limit { 1 };
    let e = spawn with limit { 1 };
    (await a) + (await b) + (await c) + (await d) + (await e)
}

test "queued tasks start as slots free and all complete" {
    assert.equal(queued_tasks_all_complete(), 5)
}

// The same limit applied twice is one slot, through a composite too.
class Pool { limit: baml.spawn.Limit, token: baml.spawn.CancelToken }

implements<T, E> baml.spawn.Modifier<T, E> for Pool {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        plan.with(self.limit).with(self.token)
    }
}

function the_same_limit_twice_is_one_slot() -> string {
    let limit = limit_of(2);
    let pool = Pool { limit: limit, token: baml.spawn.CancelToken.new() };
    let f = spawn with limit, pool, limit { sleep_ms(5000n); 1 };
    let snapshot = `${limit.active_count()}:${limit.queued_count()}`;
    let _ = pool.token.cancel();
    let outcome = (await f) catch (e) {
        baml.panics.Cancelled => -1,
    };
    `${snapshot}:${outcome}`
}

test "a composite modifier applies its parts and a repeated limit is one slot" {
    assert.equal(the_same_limit_twice_is_one_slot(), "1:0:-1")
}

// A task under two limits waits for both and holds neither meanwhile.
function two_limits_are_taken_together() -> string {
    let a = limit_of(1);
    let b = limit_of(1);
    let hold_a = spawn with a { sleep_ms(60n); 1 };
    let both = spawn with a, b { 2 };
    let while_waiting = `${a.active_count()}/${a.queued_count()} ${b.active_count()}/${b.queued_count()}`;
    let only_b = await spawn with b { 3 };
    let total = (await hold_a) + (await both) + only_b;
    `${while_waiting}|${total}`
}

test "several limits are taken together or not at all" {
    assert.equal(two_limits_are_taken_together(), "1/1 0/1|6")
}

function cancelled_while_queued_never_runs() -> string {
    let limit = limit_of(1);
    let log = Log { entries: [] };
    let first = spawn with limit { log.entries.push("ran"); sleep_ms(60n); 1 };
    let queued = spawn with limit { log.entries.push("ran"); 1 };
    let before = limit.queued_count();
    let _ = queued.cancel();
    let outcome = (await queued) catch (e) {
        baml.panics.Cancelled => -1,
    };
    let _ = await first;
    // One body ran: the queued task was cancelled before it was admitted.
    `${before}:${outcome}:${log.entries.length()}`
}

test "a task cancelled while queued never runs" {
    assert.equal(cancelled_while_queued_never_runs(), "1:-1:1")
}

// The limit holds back the WORK, not just its counts: under a capacity of one,
// no body starts until the previous one has finished its IO, so the log
// alternates strictly whatever the timing. (Capacity one also keeps the shared
// log free of racing writes: only one body ever runs at a time.)
function a_limit_serializes_real_io() -> string {
    let limit = limit_of(1);
    let log = Log { entries: [] };
    let a = spawn with limit { log.entries.push("start"); sleep_ms(30n); log.entries.push("end"); 1 };
    let b = spawn with limit { log.entries.push("start"); sleep_ms(30n); log.entries.push("end"); 1 };
    let c = spawn with limit { log.entries.push("start"); sleep_ms(30n); log.entries.push("end"); 1 };
    let total = (await a) + (await b) + (await c);
    `${total}:${log.entries.join(",")}`
}

test "a limit holds back the work, not just the counts" {
    assert.equal(a_limit_serializes_real_io(), "3:start,end,start,end,start,end")
}

// ─── Root and CancelToken ────────────────────────────────────────────────────

function linked_token_cancels() -> int {
    let token = baml.spawn.CancelToken.new();
    let f = spawn with token { sleep_ms(5000n); 1 };
    sleep_ms(30n);
    let _ = token.cancel();
    let outcome = (await f) catch (e) {
        baml.panics.Cancelled => -1,
    };
    outcome
}

test "a token modifier cancels the task when it fires" {
    assert.equal(linked_token_cancels(), -1)
}

// Cancelling a token frees the slot its running task held; the task queued
// behind it under the same token must not take that slot and run.
function token_cancels_its_queued_task() -> string {
    let limit = limit_of(1);
    let token = baml.spawn.CancelToken.new();
    let log = Log { entries: [] };
    let running = spawn with limit, token { sleep_ms(5000n); 1 };
    let queued = spawn with limit, token { log.entries.push("ran"); 2 };
    let _ = token.cancel();
    let first = (await running) catch (e) {
        baml.panics.Cancelled => -1,
    };
    let second = (await queued) catch (e) {
        baml.panics.Cancelled => -1,
    };
    `${first}:${second}:${log.entries.length()}`
}

test "a token's queued task is cancelled, not admitted" {
    assert.equal(token_cancels_its_queued_task(), "-1:-1:0")
}

// Cancelling the spawner cancels its ordinary child; a `Root` child runs on.
function root_survives_its_spawner() -> string {
    let log = Log { entries: [] };
    let outer = spawn {
        let rooted = spawn with baml.spawn.Root.new() { sleep_ms(80n); log.entries.push("rooted done"); 1 };
        let child = spawn { sleep_ms(80n); log.entries.push("child done"); 1 };
        sleep_ms(5000n);
        0
    };
    sleep_ms(30n);
    let _ = outer.cancel();
    let _ = (await outer) catch (e) {
        baml.panics.Cancelled => -1,
    };
    sleep_ms(150n);
    log.entries.join(",")
}

test "a root task is not cancelled with its spawner" {
    assert.equal(root_survives_its_spawner(), "rooted done")
}
