// Report types for test execution results.

/// The outcome of a test or test-set run: `"pass"`, `"fail"`, or `"error"`.
type Outcome = "pass" | "fail" | "error"

/// The result of a single test-function execution.
class RunReport {
  outcome Outcome
  duration_ms int
  // TODO: extras map<string, unknown>
  // Commented out: Ty::BuiltinUnknown is not handled by ty_to_field_type in
  // value_encode.rs, so deep-copying a TestReport for FFI panics when it
  // encounters the map's declared value type. Re-enable once the encoder
  // supports unknown (or we replace it with a concrete type like string).
}

/// The aggregated result of all runs for a single `test` block.
class TestReport {
  outcome Outcome
  runs RunReport[]
  // TODO: extras map<string, unknown> — see RunReport comment
}

/// The aggregated result of all tests in a `testset` block.
class TestSetReport {
  outcome Outcome
  passed int
  failed int
  total int
  results ChildReport[]
  // TODO: extras map<string, unknown> — see RunReport comment
}

/// A report for a single child — either a `TestReport` or a `TestSetReport`.
type ChildReport = TestReport | TestSetReport

// Runner type aliases — lambda transformers.
// TestRunner wraps "run test once" into a new execution lambda.
type TestBody = () -> void throws unknown
type TestSetBody = (TestCollector) -> void throws unknown
type TestReportThunk = () -> TestReport throws never
type TestSetReportThunk = () -> TestSetReport throws never
type TestRunner = (TestReportThunk) -> TestReportThunk throws never

// TestSetRunner wraps "run all children" into a new execution lambda.
type TestSetRunner = (TestSetReportThunk) -> TestSetReportThunk throws never
