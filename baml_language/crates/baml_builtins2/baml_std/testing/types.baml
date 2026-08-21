// Report types for test execution results.

/// The outcome of a test or test-set run: `"pass"`, `"fail"`, or `"error"`.
type Outcome = "pass" | "fail" | "error";

/// The result of a single test-function execution.
class RunReport {
    outcome: Outcome,
    duration_ms: int,
    message: string?,
    // TODO: extras map<string, unknown>
    // Commented out: Ty::BuiltinUnknown is not handled by ty_to_field_type in
    // value_encode.rs, so deep-copying a TestReport for FFI panics when it
    // encounters the map's declared value type. Re-enable once the encoder
    // supports unknown (or we replace it with a concrete type like string).
}

/// The aggregated result of all runs for a single `test` block.
class TestReport {
    outcome: Outcome,
    runs: RunReport[],
    // TODO: extras map<string, unknown> — see RunReport comment
}

/// The aggregated result of all tests in a `testset` block.
class TestSetReport {
    outcome: Outcome,
    passed: int,
    failed: int,
    total: int,
    failed_names: string[],
    results: ChildReport[],
    // Parallel to `results` when produced by built-in runners. Optional so
    // existing custom TestSetRunner implementations remain source-compatible.
    result_names: string[]?,
    // TODO: extras map<string, unknown> — see RunReport comment
}

/// A report for a single child — either a `TestReport` or a `TestSetReport`.
type ChildReport = TestReport | TestSetReport;

/// A named child report used internally when aggregating testset children.
class NamedChildReport {
    name: string,
    report: ChildReport,
}

// Runner type aliases — lambda transformers.
// TestRunner wraps "run test once" into a new execution lambda.
type TestBody = () -> void throws unknown;

type TestSetBody = (TestCollector) -> void throws unknown;

type TestReportThunk = () -> TestReport throws never;

type TestSetReportThunk = () -> TestSetReport throws never;

type ChildReportThunk = () -> ChildReport throws never;

type TestRunner = (TestReportThunk) -> TestReportThunk throws never;

/// A named child of a testset. Runners decide when, and whether, to invoke it.
class TestSetChild {
    name: string,
    run: ChildReportThunk,
}

// TestSetRunner receives lazy children and returns the aggregate report.
type TestSetRunner = (TestSetChild[]) -> TestSetReport throws never;

// --- Flat report type for the CLI test runner ---
//
// `TestRegistry.run_filtered` returns this already-aggregated, primitive-only
// shape so the Rust driver prints it directly instead of walking the nested,
// union-wrapped `TestReport | TestSetReport` tree across the FFI boundary.

/// Aggregated, already-flattened report returned to the CLI driver.
///
/// `outcome` is the aggregate verdict (honors testset runners). Counts are
/// leaf-level: `failed` = hard failures, `tolerated` = failing leaves whose
/// enclosing testset runner still passed (e.g. under `PassRate`), `passed` and
/// `total` as usual. `failed_names` are the hard-failure names; `messages` are
/// every failing run's message. The three name arrays partition selected leaf
/// IDs by their actual outcome after applying testset-runner tolerance.
class FlatTestReport {
    outcome: Outcome,
    passed: int,
    failed: int,
    tolerated: int,
    total: int,
    passed_names: string[],
    failed_names: string[],
    tolerated_names: string[],
    /// Per-leaf durations, parallel to the matching name array, when the
    /// executed child partition supplied them; EMPTY when the identities came
    /// from fallback selection/failure correlation (durations unknown there).
    /// `-1` marks an individual leaf with no measured run (an expansion
    /// failure sentinel). Consumers must treat a shorter-than-names array as
    /// "no duration for the tail", never index blindly.
    passed_ms: int[],
    failed_ms: int[],
    tolerated_ms: int[],
    messages: string[],
}
