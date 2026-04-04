// testing.TestCollector — collector for tests and testsets.
// The runtime creates a TestCollector, calls $init_test(collector), then reads
// collector.tests and collector.testsets.

class TestRegistration {
  name string
  body () -> null
  runner TestRunner?
}

class TestSetRegistration {
  name string
  collector (TestCollector) -> null
  runner TestSetRunner?
}

class TestCollector {
  prefix string
  tests TestRegistration[]
  testsets TestSetRegistration[]

  function new(prefix: string) -> TestCollector {
    TestCollector { prefix: prefix, tests: [], testsets: [] }
  }

  function register_test(self, name: string, body: () -> null, runner: TestRunner?) -> null {
    let full_name = if (self.prefix == "") { name } else { self.prefix + "/" + name }
    let count = 0
    let hash_prefix = full_name + "#"
    for (let t in self.tests) {
      if (t.name == full_name || t.name.startsWith(hash_prefix)) { count += 1 } else { null }
    }
    let final_name = if (count > 0) { full_name + "#" + baml.unstable.string(count + 1) } else { full_name }
    let _ = self.tests.push(TestRegistration { name: final_name, body: body, runner: runner })
    null
  }

  function register_test_set(self, name: string, collector: (TestCollector) -> null, runner: TestSetRunner?) -> null {
    let full_name = if (self.prefix == "") { name } else { self.prefix + "/" + name }
    let count = 0
    let hash_prefix = full_name + "#"
    for (let ts in self.testsets) {
      if (ts.name == full_name || ts.name.startsWith(hash_prefix)) { count += 1 } else { null }
    }
    let final_name = if (count > 0) { full_name + "#" + baml.unstable.string(count + 1) } else { full_name }
    let _ = self.testsets.push(TestSetRegistration { name: final_name, collector: collector, runner: runner })
    null
  }
}

// Bridge function for the runtime to invoke a testset collector closure.
// call_function is name-based, so we need this thin wrapper to call closures.
function $invoke_collector(collector: (TestCollector) -> null, c: TestCollector) -> null {
  collector(c)
}

// Run a single test with error handling and optional runner middleware.
function run_test(body: () -> null, runner: TestRunner?) -> TestReport {
  let base_run = () -> TestReport {
    let start = baml.sys.now_ms()
    let result = {
      body()
      RunReport { outcome: "pass", duration_ms: 0, extras: {} }
    } catch_all (e) {
      _ => {
        RunReport { outcome: "fail", duration_ms: 0, extras: {} }
      }
    }
    TestReport {
      outcome: result.outcome,
      runs: [RunReport { outcome: result.outcome, duration_ms: baml.sys.now_ms() - start, extras: {} }],
      extras: {}
    }
  }
  let effective_run = match (runner) {
    null => base_run,
    r: TestRunner => r(base_run)
  }
  effective_run()
}

// Run all children of a testset with optional runner middleware.
function run_testset(run_children: () -> TestSetReport, runner: TestSetRunner?) -> TestSetReport {
  let effective_run = match (runner) {
    null => run_children,
    r: TestSetRunner => r(run_children)
  }
  effective_run()
}
