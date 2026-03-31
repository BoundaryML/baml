// Report types for test execution results.

type Outcome = "pass" | "fail" | "error"

class RunReport {
  outcome Outcome
  duration_ms int
  extras map<string, unknown>
}

class TestReport {
  outcome Outcome
  runs RunReport[]
  extras map<string, unknown>
}

class TestSetReport {
  outcome Outcome
  passed int
  failed int
  total int
  results ChildReport[]
  extras map<string, unknown>
}

type ChildReport = TestReport | TestSetReport

// Runner type aliases — lambda transformers.
// TestRunner wraps "run test once" into a new execution lambda.
type TestRunner = (() -> TestReport) -> () -> TestReport

// TestSetRunner wraps "run all children" into a new execution lambda.
type TestSetRunner = (() -> TestSetReport) -> () -> TestSetReport
