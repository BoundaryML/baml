// Where a stuck task is: `Future.stack_trace()`, and the report a timed-out
// test or an interrupted run prints, which names deadlocks.
//
// Each deadlock is built on purpose: every task takes its first key and
// waits until the others have theirs before asking for the next, so the
// cycle forms every time. Cancelling one task then breaks it.

class ReportLog {
    entries: string[],
}

function report_sleep(ms: int) -> void {
    baml.sys.sleep(baml.time.Duration.from_milliseconds(ms))
}

/// Waits until `log` has `count` entries.
function report_wait_for(log: ReportLog, count: int) -> void {
    while (log.entries.length() < count) {
        report_sleep(5);
    }
}

/// The function names on the stack of the task that settles `task`.
function report_stack_names<T, E>(task: baml.future.Future<T, E>) -> string[] {
    match (task.stack_trace()) {
        let trace: baml.errors.StackTrace => {
            trace.frames.map((frame) -> string {
                frame.function_name
            })
        },
        null => [],
    }
}

/// The report for `future_id` once it names `count` deadlocks, asking again
/// for up to a second: a task may not have reached its wait yet.
function report_with_deadlocks(future_id: int, count: int) -> string {
    let report = baml.future._task_report(future_id);
    let tries = 0;
    while (report.split("deadlock: task ").length() - 1 < count && tries < 200) {
        report_sleep(5);
        report = baml.future._task_report(future_id);
        tries += 1;
    }
    report
}

// ─── stack_trace ─────────────────────────────────────────────────────────────

function report_parked_here(started: ReportLog) -> void {
    started.entries.push("started");
    report_sleep(60000);
}

/// The function names on a running task's stack, then whether it has a stack
/// once settled.
function report_stack_of_running() -> string[] {
    let started = ReportLog { entries: [] };
    let task = spawn { report_parked_here(started) };
    report_wait_for(started, 1);
    // `started` is pushed just before the sleep, so the task may be caught
    // between the two: ask again for up to a second.
    let names = report_stack_names(task);
    let tries = 0;
    while (!names.includes("user.task_report.report_sleep") && tries < 200) {
        report_sleep(5);
        names = report_stack_names(task);
        tries += 1;
    }
    let _ = task.cancel();
    let _ = await baml.future.all_settled([task]);
    let settled = if (task.stack_trace() == null) {
        "null once settled"
    } else {
        "still a stack"
    };
    [names[names.length() - 2], names[names.length() - 1], settled]
}

test "stack_trace shows where a running task is, and null once it settles" {
    assert.equal(
        report_stack_of_running(),
        [
            "user.task_report.report_parked_here",
            "user.task_report.report_sleep",
            "null once settled",
        ],
    )
}

// ─── deadlocks ───────────────────────────────────────────────────────────────

/// Takes `first`, waits until every task has its first key, then takes
/// `second`.
function report_cross(first: ReportLog, second: ReportLog, started: ReportLog) -> void {
    baml.sync.atomic(first, () -> void {
        started.entries.push("in");
        report_wait_for(started, 2);
        baml.sync.atomic(second, () -> void {});
    })
}

/// The report for one of two tasks that each hold the key the other wants.
function report_key_deadlock() -> string {
    let a = ReportLog { entries: [] };
    let b = ReportLog { entries: [] };
    let started = ReportLog { entries: [] };
    let x = spawn { report_cross(a, b, started) };
    let y = spawn { report_cross(b, a, started) };
    report_wait_for(started, 2);
    let report = report_with_deadlocks(x._id(), 1);
    let _ = x.cancel();
    let _ = y.cancel();
    let _ = await baml.future.all_settled([x, y]);
    report
}

test "the report names a deadlock through two keys" {
    let report = report_key_deadlock();
    assert.equal(report.includes("2 tasks still running:"), true);
    assert.equal(report.includes("in user.task_report.report_cross"), true);
    assert.equal(report.includes("deadlock: task "), true);
    assert.equal(report.includes(", which waits for a key held by task "), true)
}

/// Takes `first`, waits until `tasks` tasks have their first key, then
/// takes every key in `then` together.
function report_cross_all(
    first: ReportLog,
    then: reflect.AnyClass[],
    started: ReportLog,
    tasks: int,
) -> void {
    baml.sync.atomic(first, () -> void {
        started.entries.push("in");
        report_wait_for(started, tasks);
        baml.sync.atomic(then, () -> void {});
    })
}

/// The report for two deadlocks that share tasks: `p` waits for keys held by
/// `q` and `r`, which each wait for the key `s` holds, which waits for `p`.
function report_overlapping_deadlocks() -> string {
    let kp = ReportLog { entries: [] };
    let kq = ReportLog { entries: [] };
    let kr = ReportLog { entries: [] };
    let ks = ReportLog { entries: [] };
    let started = ReportLog { entries: [] };
    let p = spawn { report_cross_all(kp, [kq, kr], started, 4) };
    let q = spawn { report_cross_all(kq, [ks], started, 4) };
    let r = spawn { report_cross_all(kr, [ks], started, 4) };
    let s = spawn { report_cross_all(ks, [kp], started, 4) };
    report_wait_for(started, 4);
    let report = report_with_deadlocks(p._id(), 2);
    let _ = p.cancel();
    let _ = q.cancel();
    let _ = r.cancel();
    let _ = s.cancel();
    let _ = await baml.future.all_settled([p, q, r, s]);
    report
}

test "the report names both deadlocks when two share tasks" {
    let report = report_overlapping_deadlocks();
    assert.equal(report.split("deadlock: task ").length() - 1, 2)
}

/// Takes `key`.
function report_take(key: ReportLog) -> void {
    baml.sync.atomic(key, () -> void {});
}

/// The report for a task that awaits a child needing the key it holds: a
/// spawned child is a different task, so it does not re-enter.
function report_await_deadlock() -> string {
    let key = ReportLog { entries: [] };
    let started = ReportLog { entries: [] };
    let parent = spawn {
        baml.sync.atomic(key, () -> void {
            let child = spawn { report_take(key) };
            started.entries.push("spawned");
            await child;
        })
    };
    report_wait_for(started, 1);
    let report = report_with_deadlocks(parent._id(), 1);
    let _ = parent.cancel();
    let _ = await baml.future.all_settled([parent]);
    report
}

test "the report names a deadlock through a key and an await" {
    let report = report_await_deadlock();
    assert.equal(report.includes("awaits task "), true);
    assert.equal(report.includes("waits for a key held by task "), true);
    assert.equal(report.includes("deadlock: task "), true)
}

/// The same deadlock, awaited through `all_settled`: its collector waits for
/// the one child it has left, and the report follows that wait too.
function report_all_settled_deadlock() -> string {
    let key = ReportLog { entries: [] };
    let started = ReportLog { entries: [] };
    let parent = spawn {
        baml.sync.atomic(key, () -> void {
            let child = spawn { report_take(key) };
            started.entries.push("spawned");
            let _ = await baml.future.all_settled([child]);
        })
    };
    report_wait_for(started, 1);
    let report = report_with_deadlocks(parent._id(), 1);
    let _ = parent.cancel();
    let _ = await baml.future.all_settled([parent]);
    report
}

test "the report names a deadlock through all_settled" {
    let report = report_all_settled_deadlock();
    assert.equal(report.includes("waits for a key held by task "), true);
    assert.equal(report.includes("deadlock: task "), true)
}

test "the report is empty for a settled future" {
    let done = spawn { 1 };
    let _ = await done;
    assert.equal(baml.future._task_report(done._id()), "")
}
