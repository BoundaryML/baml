// root.TestCollector — collector for tests and testsets.
// The runtime creates a TestCollector, calls $init_test(collector), then reads
// collector.tests and collector.testsets.

class TestRegistration {
  name string
  body TestBody
  runner TestRunner?
}

class TestSetRegistration {
  name string
  collector TestSetBody
  runner TestSetRunner?
}

class TestCollector {
  prefix string
  tests TestRegistration[]
  testsets TestSetRegistration[]

  function new(prefix: string) -> TestCollector {
    TestCollector { prefix: prefix, tests: [], testsets: [] }
  }

  function register_test(self, name: string, body: TestBody, runner: TestRunner?) -> void {
    let full_name = if (self.prefix == "") { name } else { self.prefix + "/" + name }
    let count = 0
    let hash_prefix = full_name + "#"
    for (let t in self.tests) {
      if (t.name == full_name || t.name.starts_with(hash_prefix)) { count += 1 } else { null }
    }
    let final_name = if (count > 0) { full_name + "#" + baml.unstable.string(count + 1) } else { full_name }
    let _ = self.tests.push(TestRegistration { name: final_name, body: body, runner: runner })
  }

  function register_test_set(self, name: string, collector: TestSetBody, runner: TestSetRunner?) -> void {
    let full_name = if (self.prefix == "") { name } else { self.prefix + "/" + name }
    let count = 0
    let hash_prefix = full_name + "#"
    for (let ts in self.testsets) {
      if (ts.name == full_name || ts.name.starts_with(hash_prefix)) { count += 1 } else { null }
    }
    let final_name = if (count > 0) { full_name + "#" + baml.unstable.string(count + 1) } else { full_name }
    let _ = self.testsets.push(TestSetRegistration { name: final_name, collector: collector, runner: runner })
  }

  function find_testset(self, name: string) -> TestSetRegistration {
    for (let ts in self.testsets) {
      if (ts.name == name) {
        return ts
      }
    }
    throw ("TestSet not found: " + name)
  }

  function find_test(self, name: string) -> TestRegistration {
    for (let t in self.tests) {
      if (t.name == name) {
        return t
      }
    }
    throw ("Test not found: " + name)
  }
}

// Bridge function for the runtime to invoke a testset collector closure.
// call_function is name-based, so we need this thin wrapper to call closures.
function $invoke_collector(collector: TestSetBody, c: TestCollector) -> void {
  collector(c)
}

// --- Serialization wire types ---

class SerializedTest {
  type string    // "test" | "lazyTestSet"
  name string
}

class SerializedTestSet {
  name string
  items SerializedTestDef[]
  loadingTimeMs int
}

type SerializedTestDef = SerializedTest | SerializedTestSet

// --- TestRegistry: wraps a TestCollector with lazy expansion state ---

class TestRegistry {
  collector TestCollector
  expansions map<string, TestRegistry>
  loading_time_ms int

  function new(collector: TestCollector) -> TestRegistry {
    TestRegistry {
      collector: collector,
      expansions: {},
      loading_time_ms: 0,
    }
  }

  // Serialize registry to pure-data list for the UI.
  // Checks expansions map to determine lazy vs expanded for each testset.
  //
  // Tests are serialized before testsets by design — tests are leaf items,
  // testsets are containers.
  function serialize(self) -> SerializedTestDef[] {
    let items: SerializedTestDef[] = []
    for (let t in self.collector.tests) {
      items.push(SerializedTest { type: "test", name: t.name })
    }
    for (let ts in self.collector.testsets) {
      let expanded = self.expansions.get(ts.name)
      match (expanded) {
        let sub: TestRegistry => {
          items.push(SerializedTestSet {
            name: ts.name,
            items: sub.serialize(),
            loadingTimeMs: sub.loading_time_ms,
          })
        }
        null => {
          items.push(SerializedTest { type: "lazyTestSet", name: ts.name })
        }
      }
    }
    return items
  }

  // Find test by name in root collector, then recurse into expanded sub-registries.
  function run_test(self, name: string) -> TestReport {
    // Check tests in this registry's collector
    for (let t in self.collector.tests) {
      if (t.name == name) {
        return root.run_test(t.body, t.runner)
      }
    }
    // Recurse into expanded sub-registries
    for (let k in self.expansions.keys()) {
      let sub = self.expansions[k]
      // Check if the name could be in this sub-registry (segment-aware prefix match)
      if (name.starts_with(k + "/")) {
        return sub.run_test(name)
      }
    }
    throw ("Test not found: " + name)
  }

  function run_testset(self, name: string) -> TestSetReport {
    for (let ts in self.collector.testsets) {
      if (ts.name == name) {
        return root.run_testset(root.testset_children(self.expand_testset_registry(ts)), ts.runner)
      }
    }
    for (let k in self.expansions.keys()) {
      let sub = self.expansions[k]
      if (name.starts_with(k + "/")) {
        return sub.run_testset(name)
      }
    }
    throw ("TestSet not found: " + name)
  }

  function run_all(self) -> TestSetReport {
    root.run_testset(root.testset_children(self), null)
  }

  function run_selected(self, names: string[]) -> TestSetReport {
    root.run_testset(root.testset_children_selected(self, names), null)
  }

  // Expand a lazy testset by name. Searches recursively through expanded
  // sub-registries so nested testsets can be expanded too.
  function expand_set(self, name: string) -> SerializedTestDef[] {
    // Try root collector first
    for (let ts in self.collector.testsets) {
      if (ts.name == name) {
        self.expand_testset_registry(ts)
        return self.serialize()
      }
    }
    // Search in expanded sub-registries using segment-aware prefix match
    for (let k in self.expansions.keys()) {
      let sub = self.expansions[k]
      if (name.starts_with(k + "/")) {
        return sub.expand_set(name)
      }
    }
    throw ("TestSet not found for expansion: " + name)
  }

  function expand_testset_registry(self, ts: TestSetRegistration) -> TestRegistry {
    match (self.expansions.get(ts.name)) {
      let existing: TestRegistry => existing,
      null => {
        let start = baml.sys.now_ms()
        let sub_collector = TestCollector { prefix: ts.name, tests: [], testsets: [] }
        ts.collector(sub_collector)
        let elapsed = baml.sys.now_ms() - start
        let sub_registry = TestRegistry {
          collector: sub_collector,
          expansions: {},
          loading_time_ms: elapsed,
        }
        self.expansions.set(ts.name, sub_registry)
        sub_registry
      }
    }
  }
}

function testset_children(registry: TestRegistry) -> TestSetChild[] {
  let children: TestSetChild[] = []
  for (let t in registry.collector.tests) {
    children.push(root.test_child(t.name, t.body, t.runner))
  }
  for (let ts in registry.collector.testsets) {
    children.push(root.testset_child(registry, ts))
  }
  children
}

function testset_children_selected(registry: TestRegistry, names: string[]) -> TestSetChild[] {
  let children: TestSetChild[] = []
  for (let t in registry.collector.tests) {
    if (root._name_selected(t.name, names)) {
      children.push(root.test_child(t.name, t.body, t.runner))
    } else {
      null
    }
  }
  for (let ts in registry.collector.testsets) {
    if (root._name_selected(ts.name, names)) {
      children.push(root.testset_child(registry, ts))
    } else if (root._has_selected_descendant(ts.name, names)) {
      children.push(root.testset_child_selected(registry, ts, names))
    } else {
      null
    }
  }
  children
}

function test_child(name: string, body: TestBody, runner: TestRunner?) -> TestSetChild {
  TestSetChild {
    name: name,
    run: () -> ChildReport { root.run_test(body, runner) },
  }
}

function testset_child(registry: TestRegistry, ts: TestSetRegistration) -> TestSetChild {
  TestSetChild {
    name: ts.name,
    run: () -> ChildReport {
      root.run_testset(root.testset_children(registry.expand_testset_registry(ts)), ts.runner) catch_all (e) {
        _ => {
          TestSetReport {
            outcome: "error",
            passed: 0,
            failed: 1,
            total: 1,
            failed_names: [ts.name],
            results: [],
          }
        }
      }
    },
  }
}

function testset_child_selected(registry: TestRegistry, ts: TestSetRegistration, names: string[]) -> TestSetChild {
  TestSetChild {
    name: ts.name,
    run: () -> ChildReport {
      root.run_testset(root.testset_children_selected(registry.expand_testset_registry(ts), names), ts.runner) catch_all (e) {
        _ => {
          TestSetReport {
            outcome: "error",
            passed: 0,
            failed: 1,
            total: 1,
            failed_names: [ts.name],
            results: [],
          }
        }
      }
    },
  }
}

function _name_selected(name: string, names: string[]) -> bool {
  for (let selected in names) {
    if (selected == name) {
      return true
    } else {
      null
    }
  }
  false
}

function _has_selected_descendant(prefix: string, names: string[]) -> bool {
  let descendant_prefix = prefix + "/"
  for (let selected in names) {
    if (selected.starts_with(descendant_prefix)) {
      return true
    } else {
      null
    }
  }
  false
}

function aggregate_reports(named_results: NamedChildReport[]) -> TestSetReport {
  let passed = 0
  let failed = 0
  let has_error = false
  let failed_names: string[] = []
  let results: ChildReport[] = []
  for (let named_result in named_results) {
    let result = named_result.report
    results.push(result)
    let outcome = match (result) {
      let report: TestReport => report.outcome,
      let report: TestSetReport => report.outcome,
    }
    if (outcome == "pass") {
      passed += 1
    } else {
      failed += 1
      let child_failed_names = match (result) {
        let report: TestReport => [named_result.name],
        let report: TestSetReport => if (report.failed_names.length() > 0) {
          report.failed_names
        } else {
          [named_result.name]
        },
      }
      for (let name in child_failed_names) {
        failed_names.push(name)
      }
      if (outcome == "error") {
        has_error = true
      } else {
        null
      }
    }
  }
  let outcome: Outcome = if (has_error) { "error" } else if (failed == 0) { "pass" } else { "fail" }
  TestSetReport {
    outcome: outcome,
    passed: passed,
    failed: failed,
    total: results.length(),
    failed_names: failed_names,
    results: results,
  }
}

function run_children_parallel(children: TestSetChild[]) -> TestSetReport {
  let futures: baml.future.Future<NamedChildReport, null>[] = []
  for (let child in children) {
    futures.push(spawn {
      NamedChildReport { name: child.name, report: child.run() }
    })
  }
  let named_results = (await baml.future.all_complete(futures)) catch_all (e) {
    _ => baml.sys.panic("unexpected test child failure")
  }
  root.aggregate_reports(named_results)
}

// Run a single test with error handling and optional runner middleware.
function run_test(body: TestBody, runner: TestRunner?) -> TestReport {
  let base_run: TestReportThunk = () -> TestReport {
    let start = baml.sys.now_ms()
    let result = {
      body()
      RunReport { outcome: "pass", duration_ms: 0, message: null }
    } catch_all (e) {
      let p: baml.panics.UserPanic => {
        RunReport { outcome: "fail", duration_ms: 0, message: p.message }
      },
      let p: baml.panics.AssertionFailed => {
        RunReport { outcome: "fail", duration_ms: 0, message: p.message }
      },
      baml.panics.Panic => {
        RunReport { outcome: "fail", duration_ms: 0, message: null }
      },
      _ => {
        RunReport { outcome: "fail", duration_ms: 0, message: null }
      }
    }
    TestReport {
      outcome: result.outcome,
      runs: [RunReport { outcome: result.outcome, duration_ms: baml.sys.now_ms() - start, message: result.message }],
    }
  }
  match (runner) {
    null => base_run(),
    let r: TestRunner => {
      let wrapped: TestReportThunk = r(base_run)
      wrapped()
    }
  }
}

// Run all children of a testset with optional runner middleware.
function run_testset(children: TestSetChild[], runner: TestSetRunner?) -> TestSetReport {
  match (runner) {
    null => root.run_children_parallel(children),
    let r: TestSetRunner => r(children),
  }
}
