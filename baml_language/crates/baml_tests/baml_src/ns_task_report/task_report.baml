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
    let names: string[] = match (task.stack_trace()) {
        let trace: baml.errors.StackTrace => {
            trace.frames.map((frame) -> string {
                frame.function_name
            })
        },
        null => [],
    };
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
    report_sleep(50);
    let report = baml.future._task_report(x._id());
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
    report_sleep(50);
    let report = baml.future._task_report(parent._id());
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

test "the report is empty for a settled future" {
    let done = spawn { 1 };
    let _ = await done;
    assert.equal(baml.future._task_report(done._id()), "")
}
