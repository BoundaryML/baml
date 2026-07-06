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
    let final_name = if (count > 0) { full_name + "#" + string.from(count + 1) } else { full_name }
    let _ = self.tests.push(TestRegistration { name: final_name, body: body, runner: runner })
  }

  function register_test_set(self, name: string, collector: TestSetBody, runner: TestSetRunner?) -> void {
    let full_name = if (self.prefix == "") { name } else { self.prefix + "/" + name }
    let count = 0
    let hash_prefix = full_name + "#"
    for (let ts in self.testsets) {
      if (ts.name == full_name || ts.name.starts_with(hash_prefix)) { count += 1 } else { null }
    }
    let final_name = if (count > 0) { full_name + "#" + string.from(count + 1) } else { full_name }
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

  // CLI entry point: glob-filter in BAML, run (honoring testset runners), and
  // flatten into a tolerated-aware flat report the Rust driver prints directly.
  //   * no filters -> run_all (whole tree)
  //   * any filter -> glob the leaf names, then run_selected (parent runners
  //                   still apply to the selected subset)
  function run_filtered(self, include: string[], exclude: string[]) -> FlatTestReport {
    let report = if (include.length() == 0 && exclude.length() == 0) {
      self.run_all()
    } else {
      self.run_selected(root.select_names(self, include, exclude))
    }
    root.flatten_with_tolerated(report)
  }

  // CLI `--list` support: the selected leaf names, without running anything.
  function list_filtered(self, include: string[], exclude: string[]) -> string[] {
    root.select_names(self, include, exclude)
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
    // Select testsets by descendant match only. `names` holds leaf paths, and a
    // testset contributes only "name/..." descendants — never a bare "name" — so
    // an exact ts.name match can happen only when a same-named top-level `test`
    // was selected (tests and testsets dedupe in separate collections, so the
    // names can collide). Honoring that would wrongly run the whole testset.
    if (root._has_selected_descendant(ts.name, names)) {
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
        baml.panics.Panic => root.expansion_error_report(ts.name),
        _ => root.expansion_error_report(ts.name),
      }
    },
  }
}

function testset_child_selected(registry: TestRegistry, ts: TestSetRegistration, names: string[]) -> TestSetChild {
  TestSetChild {
    name: ts.name,
    run: () -> ChildReport {
      root.run_testset(root.testset_children_selected(registry.expand_testset_registry(ts), names), ts.runner) catch_all (e) {
        baml.panics.Panic => root.expansion_error_report(ts.name),
        _ => root.expansion_error_report(ts.name),
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

// ---------------------------------------------------------------------------
// CLI runner: BAML-side glob filtering, name collection, and tolerated-aware
// flattening (run_filtered / list_filtered). The Rust driver passes raw
// include/exclude patterns and prints the returned FlatTestReport directly.
// ---------------------------------------------------------------------------

// The report for a testset that errored (e.g. its generator threw or panicked
// during expansion): one error with no per-leaf detail.
function expansion_error_report(name: string) -> TestSetReport {
  TestSetReport {
    outcome: "error",
    passed: 0,
    failed: 1,
    total: 1,
    // Slash-qualified so split_top keeps it under the testset segment (filters
    // can still select/exclude it). The catch in testset_child(_selected) routes
    // runner/execution panics here too — not just expansion failures — so the
    // label stays generic ("testset error") rather than claiming an expansion
    // failure specifically. (collect_subtree_names, which is purely an expansion
    // step, keeps the more precise "failed to expand".)
    failed_names: [name + "/(testset error)"],
    results: [],
  }
}

// A parsed filter pattern: a (function, test) glob pair. "Fn::Test" yields one
// pair; a bare "P" yields ("P", "") and ("", "P") (see parse_filter_patterns).
class FilterPat {
  func_pat string
  test_pat string
}

// A slash path split into its first segment and the remainder.
class NameSplit {
  head string
  tail string
}

// Split a slash path at the first '/'. "suite/x/y" -> ("suite", "x/y").
function split_top(path: string) -> NameSplit {
  if let idx: int = path.index_of("/") {
    NameSplit { head: path.substring(0, idx), tail: path.substring(idx + 1, path.length()) }
  } else {
    NameSplit { head: "", tail: path }
  }
}

// Parse raw "-i"/"-x" patterns into (func, test) glob pairs. "Fn::Test" splits
// at the first "::"; a bare "P" expands to ("P", "") and ("", "P"). Mirrors
// TestFilter::parse_patterns.
function parse_filter_patterns(raw: string[]) -> FilterPat[] {
  let out: FilterPat[] = []
  for (let s in raw) {
    if let idx: int = s.index_of("::") {
      out.push(FilterPat { func_pat: s.substring(0, idx), test_pat: s.substring(idx + 2, s.length()) })
    } else {
      out.push(FilterPat { func_pat: s, test_pat: "" })
      out.push(FilterPat { func_pat: "", test_pat: s })
    }
  }
  out
}

// Glob match where '*' matches any run of characters (including '/'), anchored
// at both ends. Callers pass only non-empty patterns (filter_match handles "").
function glob_match(subject: string, pattern: string) -> bool {
  let parts = pattern.split("*")
  let n = parts.length()
  if (n == 1) {
    return subject == pattern
  }
  let first = parts[0]
  let last = parts[n - 1]
  if (!subject.starts_with(first)) { return false }
  if (!subject.ends_with(last)) { return false }
  let start = first.length()
  let end_limit = subject.length() - last.length()
  if (start > end_limit) { return false }
  let pos = start
  for (let i = 1; i < n - 1; i += 1) {
    let part = parts[i]
    if (part != "") {
      if let idx: int = subject.substring(pos, end_limit).index_of(part) {
        pos = pos + idx + part.length()
      } else {
        return false
      }
    }
  }
  true
}

// Empty pattern matches anything; otherwise glob-match.
function filter_match(expr: string, subject: string) -> bool {
  if (expr == "") {
    true
  } else {
    root.glob_match(subject, expr)
  }
}

function any_pattern_matches(pats: FilterPat[], function_name: string, test_name: string) -> bool {
  for (let p in pats) {
    if (root.filter_match(p.func_pat, function_name) && root.filter_match(p.test_pat, test_name)) {
      return true
    }
  }
  false
}

// Replicates TestFilter::includes: exclude wins; with includes present only
// included leaves are selected; with no includes everything not excluded is.
function leaf_selected(name: string, include: FilterPat[], exclude: FilterPat[]) -> bool {
  let split = root.split_top(name)
  if (root.any_pattern_matches(exclude, split.head, split.tail)) {
    return false
  }
  if (include.length() == 0) {
    return true
  }
  root.any_pattern_matches(include, split.head, split.tail)
}

// Glob-filter the registry's leaf names into the selected set (full slash
// paths). Used by both run_filtered (-> run_selected) and list_filtered.
function select_names(registry: TestRegistry, include: string[], exclude: string[]) -> string[] {
  let inc = root.parse_filter_patterns(include)
  let exc = root.parse_filter_patterns(exclude)
  let out: string[] = []
  for (let name in root.collect_leaf_names(registry)) {
    if (root.leaf_selected(name, inc, exc)) {
      out.push(name);
    }
  }
  out
}

// Fully expand the registry and collect every leaf test's full slash-path name.
function collect_leaf_names(registry: TestRegistry) -> string[] {
  let out: string[] = []
  for (let t in registry.collector.tests) {
    out.push(t.name);
  }
  for (let ts in registry.collector.testsets) {
    for (let name in root.collect_subtree_names(registry, ts)) {
      out.push(name);
    }
  }
  out
}

// Expand one testset to its leaf names, containing failures: a testset whose
// generator throws or panics during expansion is represented by a sentinel leaf
// *under* its own path ("suite/(failed to expand)") so `split_top` keeps it in
// the testset segment and `-i suite::*` / `-x suite::*` select it like a real
// leaf, then `run_selected` runs it and surfaces the error.
function collect_subtree_names(registry: TestRegistry, ts: TestSetRegistration) -> string[] {
  root.collect_leaf_names(registry.expand_testset_registry(ts)) catch_all (e) {
    baml.panics.Panic => [ts.name + "/(failed to expand)"],
    _ => [ts.name + "/(failed to expand)"],
  }
}

// Leaf tallies split into hard failures vs. runner-tolerated failures.
class LeafCounts {
  passed int
  failed int
  tolerated int
  total int
}

// Classify each leaf in a child-report list as passed / hard-failed /
// tolerated. A failing leaf is tolerated iff some enclosing testset's runner
// passed (tolerated_by_parent flows down). Replaces the Rust extract_leaf_summary.
function count_leaves(results: ChildReport[], tolerated_by_parent: bool) -> LeafCounts {
  let passed = 0
  let failed = 0
  let tolerated = 0
  let total = 0
  for (let r in results) {
    let delta = match (r) {
      let tr: TestReport => {
        if (tr.outcome == "pass") {
          LeafCounts { passed: 1, failed: 0, tolerated: 0, total: 1 }
        } else if (tolerated_by_parent) {
          LeafCounts { passed: 0, failed: 0, tolerated: 1, total: 1 }
        } else {
          LeafCounts { passed: 0, failed: 1, tolerated: 0, total: 1 }
        }
      },
      let tsr: TestSetReport => {
        let ft = tolerated_by_parent || (tsr.outcome == "pass")
        if (tsr.results.length() > 0) {
          root.count_leaves(tsr.results, ft)
        } else if (ft) {
          LeafCounts { passed: tsr.passed, failed: 0, tolerated: tsr.failed, total: tsr.total }
        } else {
          LeafCounts { passed: tsr.passed, failed: tsr.failed, tolerated: 0, total: tsr.total }
        }
      },
    }
    passed += delta.passed
    failed += delta.failed
    tolerated += delta.tolerated
    total += delta.total
  }
  LeafCounts { passed: passed, failed: failed, tolerated: tolerated, total: total }
}

// Flatten a run_all()/run_selected() TestSetReport into the CLI flat report.
// Aggregate outcome is preserved (honoring testset runners); counts split hard
// vs tolerated failures; failed_names are the report's (hard) failed names; and
// failure messages are gathered from every failing run.
function flatten_with_tolerated(report: TestSetReport) -> FlatTestReport {
  let top_tolerated = report.outcome == "pass"
  let counts = if (report.results.length() > 0) {
    root.count_leaves(report.results, top_tolerated)
  } else if (top_tolerated) {
    LeafCounts { passed: report.passed, failed: 0, tolerated: report.failed, total: report.total }
  } else {
    LeafCounts { passed: report.passed, failed: report.failed, tolerated: 0, total: report.total }
  }
  let messages: string[] = []
  root.collect_leaf_messages(report.results, messages)
  FlatTestReport {
    outcome: report.outcome,
    passed: counts.passed,
    failed: counts.failed,
    tolerated: counts.tolerated,
    total: counts.total,
    failed_names: report.failed_names,
    messages: messages,
  }
}

// Gather failure messages from every failing run of every leaf, recursing into
// nested testsets.
function collect_leaf_messages(results: ChildReport[], out: string[]) -> void {
  for (let r in results) {
    match (r) {
      let tr: TestReport => {
        for (let run in tr.runs) {
          if (run.outcome != "pass") {
            if let m: string = run.message {
              out.push(m);
            }
          }
        }
      },
      let tsr: TestSetReport => {
        root.collect_leaf_messages(tsr.results, out);
      },
    }
  }
}
