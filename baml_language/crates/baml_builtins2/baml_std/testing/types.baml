// Report types for test execution results.

type Outcome = "pass" | "fail" | "error"

class RunReport {
  outcome Outcome
  duration_ms int
  // TODO: extras map<string, unknown>
  // Commented out: Ty::BuiltinUnknown is not handled by ty_to_field_type in
  // value_encode.rs, so deep-copying a TestReport for FFI panics when it
  // encounters the map's declared value type. Re-enable once the encoder
  // supports unknown (or we replace it with a concrete type like string).
}

class TestReport {
  outcome Outcome
  runs RunReport[]
  // TODO: extras map<string, unknown> — see RunReport comment
}

class TestSetReport {
  outcome Outcome
  passed int
  failed int
  total int
  results ChildReport[]
  // TODO: extras map<string, unknown> — see RunReport comment
}

type ChildReport = TestReport | TestSetReport

// Runner type aliases — lambda transformers.
// TestRunner wraps "run test once" into a new execution lambda.
type TestRunner = (() -> TestReport) -> () -> TestReport

// TestSetRunner wraps "run all children" into a new execution lambda.
type TestSetRunner = (() -> TestSetReport) -> () -> TestSetReport
