// Spawn plans and modifiers (BEP-040). `spawn name with m1, m2 { body }` builds
// a `baml.spawn.Plan` from the body and the name, hands it to each modifier in
// turn, and starts the result as a task. A plan is a sealed, reusable recipe; a
// modifier returns a transformed plan; a wrapper a modifier installs receives
// an `Execution`, good for one `run` and only while that wrapper is running.
// Modifiers apply left to right, so a wrapper added later runs outside the ones
// before it.
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
    let names = [named.name() ?? "<none>", bare.name() ?? "<none>"];
    names
}

test "a plan keeps the name it was given" {
    assert.equal(plan_names(), ["fetch", "<none>"])
}

// ─── execution wrappers ──────────────────────────────────────────────────────

class Tag { log: Log, name: string }

implements<T, E> baml.spawn.Modifier<T, E> for Tag {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        plan.wrap((execution: baml.spawn.Execution<T, E>) -> T throws E {
            self.log.entries.push(`${self.name}:before`);
            defer { self.log.entries.push(`${self.name}:after`) }
            execution.run()
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
        plan.wrap((execution: baml.spawn.Execution<int, E>) -> int throws E { execution.run() * 2 })
    }
}

class AddOne {}

implements<E> baml.spawn.Modifier<int, E> for AddOne {
    function apply(self, plan: baml.spawn.Plan<int, E>) -> baml.spawn.Plan<int, E> throws never {
        plan.wrap((execution: baml.spawn.Execution<int, E>) -> int throws E { execution.run() + 1 })
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

// ─── modifiers that change the types ─────────────────────────────────────────

class Fallback<T> { value: T }

implements<T, E> baml.spawn.Modifier<T, E> for Fallback<T> {
    type Error = never
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, never> throws never {
        plan.wrap((execution: baml.spawn.Execution<T, E>) -> T throws never {
            execution.run() catch (e) {
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
        plan.wrap((execution: baml.spawn.Execution<T, E>) -> string throws E {
            let _ = execution.run();
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

// ─── the execution capability ────────────────────────────────────────────────

class RunsTwice {}

implements<T, E> baml.spawn.Modifier<T, E> for RunsTwice {
    function apply(self, plan: baml.spawn.Plan<T, E>) -> baml.spawn.Plan<T, E> throws never {
        plan.wrap((execution: baml.spawn.Execution<T, E>) -> T throws E {
            let _ = execution.run();
            execution.run()
        })
    }
}

function second_run_panics() -> string {
    let runs_twice = RunsTwice {};
    let f = spawn with runs_twice { 1 };
    let outcome = {
        let _ = await f;
        "ran twice"
    } catch (e) {
        baml.panics.UserPanic => e.message,
    };
    outcome
}

test "an execution runs at most once" {
    assert.equal(second_run_panics(), "baml.spawn.Execution.run: already called")
}

class Stash { held: baml.spawn.Execution<int, never>[] }

class Stasher { stash: Stash }

implements baml.spawn.Modifier<int, never> for Stasher {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
        plan.wrap((execution: baml.spawn.Execution<int, never>) -> int throws never {
            self.stash.held.push(execution);
            7
        })
    }
}

// A wrapper may decline to run the rest; keeping the execution for later does
// not keep it alive.
function stashed_execution_cannot_run_later() -> string {
    let stash = Stash { held: [] };
    let stasher = Stasher { stash: stash };
    let v = await spawn with stasher { 1 };
    let outcome = {
        let _ = stash.held[0].run();
        "ran"
    } catch (e) {
        baml.panics.UserPanic => e.message,
    };
    `${v}:${outcome}`
}

test "an execution cannot run after its wrapper returned" {
    assert.equal(
        stashed_execution_cannot_run_later(),
        "7:baml.spawn.Execution.run: the wrapper that received this execution has returned",
    )
}

class ThrowsAfterStash { stash: Stash }

implements baml.spawn.Modifier<int, never> for ThrowsAfterStash {
    type Error = string
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, string> throws never {
        plan.wrap((execution: baml.spawn.Execution<int, never>) -> int throws string {
            self.stash.held.push(execution);
            throw "wrapper failed"
        })
    }
}

// The outer half of the test below: it runs the throwing wrapper, then tries the
// execution that wrapper stashed before it threw.
class ChecksExpiry { stash: Stash }

implements baml.spawn.Modifier<int, string> for ChecksExpiry {
    type Output = string
    type Error = never
    function apply(self, plan: baml.spawn.Plan<int, string>) -> baml.spawn.Plan<string, never> throws never {
        plan.wrap((execution: baml.spawn.Execution<int, string>) -> string throws never {
            let first = {
                let _ = execution.run();
                "ok"
            } catch (e) {
                string => e,
            };
            let second = {
                let _ = self.stash.held[0].run();
                "ran"
            } catch (e) {
                baml.panics.UserPanic => "expired",
            };
            `${first}:${second}`
        })
    }
}

// Leaving a wrapper by a throw ends its execution's extent just as returning does.
function a_throw_closes_the_extent() -> string {
    let stash = Stash { held: [] };
    let throwing = ThrowsAfterStash { stash: stash };
    let checking = ChecksExpiry { stash: stash };
    await spawn with throwing, checking { 1 }
}

test "a throw out of a wrapper ends its execution's extent" {
    assert.equal(a_throw_closes_the_extent(), "wrapper failed:expired")
}

class Retarget { other: baml.spawn.Plan<int, never> }

implements baml.spawn.Modifier<int, never> for Retarget {
    function apply(self, plan: baml.spawn.Plan<int, never>) -> baml.spawn.Plan<int, never> throws never {
        plan.wrap((execution: baml.spawn.Execution<int, never>) -> int throws never {
            let forged = baml.spawn.Execution<int, never> { _plan: self.other._handle, _handle: execution._handle };
            forged.run()
        })
    }
}

// An execution pointed at a plan it was not made for is refused: `other` would
// otherwise run in this task, outside its own admission and cancellation.
function a_retargeted_execution_is_refused() -> string {
    let log = Log { entries: [] };
    let other = baml.spawn.Plan.new(body = () -> int { log.entries.push("other ran"); 99 });
    let retarget = Retarget { other: other };
    let f = spawn with retarget { 1 };
    let outcome = {
        let v = await f;
        `ran:${v}`
    } catch (e) {
        baml.panics.UserPanic => e.message,
    };
    `${outcome}|${log.entries.length()}`
}

test "an execution runs only the plan it was made for" {
    assert.equal(
        a_retargeted_execution_is_refused(),
        "baml.spawn.Execution: this execution belongs to a different plan|0",
    )
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

// ─── plan reuse ──────────────────────────────────────────────────────────────
//
// A plan is a reusable recipe, but the syntax for starting one that already
// exists is not built: BEP-040 leaves the spelling open (`spawn from plan` or
// `baml.spawn.launch(plan)`). The two tests below are the only ones here that
// reach the compiler's internal terminal, because reuse is the one thing the
// `spawn` sugar cannot express -- it builds a fresh plan every time. The
// terminal is internal: user code writes `spawn ... { }`.

function one_plan_two_tasks() -> int[] {
    let counter = Log { entries: [] };
    let plan = baml.spawn.Plan.new(body = () -> int {
        counter.entries.push("x");
        counter.entries.length()
    });
    let first = await baml.spawn.__spawn(plan);
    let second = await baml.spawn.__spawn(plan);
    let results = [first, second];
    results
}

test "a plan started twice yields two independent tasks" {
    assert.equal(one_plan_two_tasks(), [1, 2])
}

function one_limited_plan_started_twice() -> string {
    let limit = limit_of(1);
    let log = Log { entries: [] };
    let plan = baml.spawn.Plan.new(body = () -> int {
        log.entries.push(`active:${limit.active_count()}`);
        sleep_ms(40n);
        1
    }).with(limit);
    let a = baml.spawn.__spawn(plan);
    let b = baml.spawn.__spawn(plan);
    let total = (await a) + (await b);
    // Each body saw exactly one task holding the slot: they did not overlap.
    `${total}|${log.entries.join(",")}`
}

test "two tasks from one limited plan share its limit" {
    assert.equal(one_limited_plan_started_twice(), "2|active:1,active:1")
}
