// root.TestCollector — collector for tests and testsets.
// The runtime creates a TestCollector, calls $init_test(collector), then reads
// collector.tests and collector.testsets.

class TestRegistration {
  name string
  body () -> null throws unknown
  runner TestRunner?
}

class TestSetRegistration {
  name string
  collector (TestCollector) -> null throws unknown
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

  function find_testset(self, name: string) -> TestSetRegistration {
    for (let ts in self.testsets) {
      if (ts.name == name) {
        return ts
      }
    }
    throw ("TestSet not found: " + name)
  }
}

// Bridge function for the runtime to invoke a testset collector closure.
// call_function is name-based, so we need this thin wrapper to call closures.
function $invoke_collector(collector: (TestCollector) -> null, c: TestCollector) -> null {
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
        sub: TestRegistry => {
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
      if (name.startsWith(k + "/")) {
        return sub.run_test(name)
      }
    }
    throw ("Test not found: " + name)
  }

  // Expand a lazy testset by name. Searches recursively through expanded
  // sub-registries so nested testsets can be expanded too.
  function expand_set(self, name: string) -> SerializedTestDef[] {
    // Try root collector first
    for (let ts in self.collector.testsets) {
      if (ts.name == name) {
        let start = baml.sys.now_ms()
        let sub_collector = TestCollector { prefix: name, tests: [], testsets: [] }
        ts.collector(sub_collector)
        let elapsed = baml.sys.now_ms() - start
        let sub_registry = TestRegistry {
          collector: sub_collector,
          expansions: {},
          loading_time_ms: elapsed,
        }
        self.expansions.set(name, sub_registry)
        return self.serialize()
      }
    }
    // Search in expanded sub-registries using segment-aware prefix match
    for (let k in self.expansions.keys()) {
      let sub = self.expansions[k]
      if (name.startsWith(k + "/")) {
        return sub.expand_set(name)
      }
    }
    throw ("TestSet not found for expansion: " + name)
  }
}

// Run a single test with error handling and optional runner middleware.
function run_test(body: () -> null, runner: TestRunner?) -> TestReport {
  let base_run = () -> TestReport {
    let start = baml.sys.now_ms()
    let result = {
      body()
      RunReport { outcome: "pass", duration_ms: 0 }
    } catch_all (e) {
      _ => {
        RunReport { outcome: "fail", duration_ms: 0 }
      }
    }
    TestReport {
      outcome: result.outcome,
      runs: [RunReport { outcome: result.outcome, duration_ms: baml.sys.now_ms() - start }],
    }
  }
  match (runner) {
    null => base_run(),
    r: TestRunner => r(base_run)()
  }
}

// Run all children of a testset with optional runner middleware.
function run_testset(run_children: () -> TestSetReport, runner: TestSetRunner?) -> TestSetReport {
  match (runner) {
    null => run_children(),
    r: TestSetRunner => r(run_children)()
  }
}
