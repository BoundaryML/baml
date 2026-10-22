class InitState {
    events: string[],
}

// Source order differs from dependency order, including a read through a call.
const INIT_SECOND = InitRecord("second", INIT_FIRST + 1);
const INIT_FIRST = InitRecord("first", 41);
const INIT_STATE = InitState { events: [] };
const INIT_INDIRECT = InitReadSecond();

function InitRecord(label: string, value: int) -> int {
    INIT_STATE.events.push(label);
    value
}

function InitReadSecond() -> int {
    INIT_SECOND
}

function InitAllocate() -> Point[] {
    let points: Point[] = [];
    let i = 0;
    while (i < 96) {
        points.push(Point { x: i });
        i += 1;
    }
    points
}

const INIT_POINTS = InitAllocate();
const INIT_ALIAS = INIT_POINTS;
const INIT_MAPPED = INIT_POINTS.map((p: Point) -> int { p.x + INIT_SECOND });
const INIT_CLOSURE = {
    let captured = [Point { x: INIT_POINTS[95].x }];
    (value: int) -> int { captured[0].x + value + INIT_SECOND }
};

function InitThrow(n: int) -> int throws string {
    if (n == 0) {
        throw "initializer-error";
    }
    InitThrow(n - 1) + 1
}

// Keep the handler in the initializer itself so its trace contains the helper.
const INIT_ERROR = InitThrow(3) catch (error, context) {
    _ => `${error}|${context.error}|${Frames(context)}`,
};

interface InitSource {
    function value(self) -> int throws never;
}

class InitReadsLate {
    implements InitSource {
        function value(self) -> int throws never { INIT_LATE }
    }
}

function InitReadPending(source: InitSource) -> int {
    source.value()
}

// The harness substitutes this literal in the startup-failure packs: initializers
// cannot call sysops such as env.get before the engine exists.
const INIT_FAILURE = "none";
const INIT_GATE = {
    let value = INIT_INDIRECT;
    if (INIT_FAILURE == "throw") {
        InitThrow(3)
    } else if (INIT_FAILURE == "panic") {
        INIT_POINTS[1000].x
    } else if (INIT_FAILURE == "pending") {
        InitReadPending(InitReadsLate {})
    } else {
        InitRecord("gate", value)
    }
};
const INIT_AFTER = InitRecord("after", INIT_GATE + 1);
const INIT_LATE = 99;

function Initializers() -> void {
    baml.io.println(`${INIT_FIRST}:${INIT_SECOND}:${INIT_INDIRECT}:${INIT_AFTER}`);
    baml.io.println(INIT_STATE.events.join(","));
}

function InitializerClosures() -> void {
    INIT_ALIAS[0].x = 700;
    let churn = InitAllocate();
    baml.io.println(`${INIT_POINTS[0].x}:${INIT_ALIAS[95].x}:${churn[95].x}`);
    baml.io.println(`${INIT_MAPPED[0]}:${INIT_MAPPED[95]}:${INIT_CLOSURE(5)}`);
}

function InitializerErrors() -> void {
    baml.io.println(INIT_ERROR);
}

// These entries must never run: startup fails before INIT_AFTER is stored.
function InitializerUncaught() -> void {
    baml.io.println(`unreachable:${INIT_AFTER}`);
}

function InitializerPanic() -> void {
    baml.io.println(`unreachable:${INIT_AFTER}`);
}

function InitializerPending() -> void {
    baml.io.println(`unreachable:${INIT_AFTER}`);
}

function InitTestName() -> string {
    if (baml.env.get("BAML_RUST_DIFF_INIT_FAILURE") == "registration") {
        let _ = InitThrow(2);
    }
    "initialized globals and captured closures"
}

test "initializer " + InitTestName() {
    assert.equal(INIT_SECOND, 42);
    assert.equal(INIT_CLOSURE(5), 142);
}

function InitializerTestSummary(registry: testing.TestRegistry) -> string {
    let outcomes: string[] = [];
    for (let entry in registry.collector.tests) {
        let report = registry.run_test(entry.name);
        outcomes.push(`${entry.name}:${report.outcome}:${report.runs.length()}`);
    }
    outcomes.join(",")
}
