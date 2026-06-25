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

  // CLI entry point: expand, filter, run, and aggregate into a flat report.
  //
  // Mirrors the two behaviors of the legacy Rust driver:
  //   * no filters  -> honor testset runners via run_all (the aggregate
  //                    outcome can pass even with a failing leaf, e.g.
  //                    PassRate), while reporting leaf-level counts.
  //   * any filter  -> run only the selected leaf tests, each with its own
  //                    test runner but WITHOUT the parent testset runner.
  // Selected leaves run concurrently via spawn/await.
  function run_filtered(self, include: string[], exclude: string[]) -> FlatTestReport {
    let has_filters = include.length() > 0 || exclude.length() > 0
    if (!has_filters) {
      root.flatten_testset_report(self.run_all())
    } else {
      let inc = root.parse_filter_patterns(include)
      let exc = root.parse_filter_patterns(exclude)
      let selected: FlatLeaf[] = []
      for (let leaf in root.collect_leaves(self)) {
        if (root.leaf_selected(leaf.name, inc, exc)) {
          selected.push(leaf);
        }
      }
      root.run_selected_leaves(selected)
    }
  }

  // CLI `--list` support: the selected leaf names, without running anything.
  function list_filtered(self, include: string[], exclude: string[]) -> string[] {
    let inc = root.parse_filter_patterns(include)
    let exc = root.parse_filter_patterns(exclude)
    let names: string[] = []
    for (let leaf in root.collect_leaves(self)) {
      if (root.leaf_selected(leaf.name, inc, exc)) {
        names.push(leaf.name);
      }
    }
    names
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

// The report for a testset that errored (e.g. its generator threw or panicked
// during expansion): one error with no per-leaf detail.
function expansion_error_report(name: string) -> TestSetReport {
  TestSetReport {
    outcome: "error",
    passed: 0,
    failed: 1,
    total: 1,
    failed_names: [name],
    results: [],
  }
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
// Flat report + filtering helpers for the CLI runner (run_filtered / list_filtered)
// ---------------------------------------------------------------------------

// A fully-resolved leaf test: its full slash-path name plus the body and
// test-level runner needed to run it on its own. Parent testset runners are
// intentionally not carried — selected leaves run independently.
class FlatLeaf {
  name string
  body TestBody
  runner TestRunner?
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

// Recursively expand every testset and collect all leaf tests. Leaf names are
// already full slash paths (register_test applies the prefix). Expansion is
// memoized in the registry, so generator bodies run at most once.
function collect_leaves(registry: TestRegistry) -> FlatLeaf[] {
  let out: FlatLeaf[] = []
  for (let t in registry.collector.tests) {
    out.push(FlatLeaf { name: t.name, body: t.body, runner: t.runner })
  }
  for (let ts in registry.collector.testsets) {
    for (let leaf in root.collect_subtree_leaves(registry, ts)) {
      out.push(leaf)
    }
  }
  out
}

// Expand one testset and collect its leaves, containing failures: a testset
// whose generator body throws becomes a single failing leaf named after the
// testset rather than aborting the whole run (mirrors the unfiltered path's
// testset_child catch).
function collect_subtree_leaves(registry: TestRegistry, ts: TestSetRegistration) -> FlatLeaf[] {
  root.collect_leaves(registry.expand_testset_registry(ts)) catch_all (e) {
    baml.panics.Panic => [root.expansion_error_leaf(ts.name)],
    _ => [root.expansion_error_leaf(ts.name)],
  }
}

// A synthetic leaf standing in for a testset that failed to expand. Running it
// re-throws so run_test records a failure. The name is slash-qualified under
// the testset (e.g. "suite/(failed to expand)") so split_top keeps it under the
// testset segment and `-i suite::*` / `-x suite::*` select it like a real leaf.
function expansion_error_leaf(name: string) -> FlatLeaf {
  FlatLeaf {
    name: name + "/(failed to expand)",
    body: () -> void throws unknown { baml.sys.panic("testset failed to expand") },
    runner: null,
  }
}

// Split a slash path at the first '/'. "suite/x/y" -> ("suite", "x/y");
// "solo" -> ("", "solo"). Mirrors the Rust split_top feeding the filter's
// FunctionName::TestName split.
function split_top(path: string) -> NameSplit {
  let idx = path.index_of("/")
  if (idx < 0) {
    NameSplit { head: "", tail: path }
  } else {
    NameSplit { head: path.substring(0, idx), tail: path.substring(idx + 1, path.length()) }
  }
}

// Parse raw "-i"/"-x" patterns into (func, test) glob pairs. "Fn::Test" splits
// at the first "::"; a bare "P" expands to ("P", "") and ("", "P"). Mirrors
// TestFilter::parse_patterns.
function parse_filter_patterns(raw: string[]) -> FilterPat[] {
  let out: FilterPat[] = []
  for (let s in raw) {
    let idx = s.index_of("::")
    if (idx < 0) {
      out.push(FilterPat { func_pat: s, test_pat: "" })
      out.push(FilterPat { func_pat: "", test_pat: s })
    } else {
      out.push(FilterPat { func_pat: s.substring(0, idx), test_pat: s.substring(idx + 2, s.length()) })
    }
  }
  out
}

// Glob match where '*' matches any run of characters (including '/'), anchored
// at both ends. Mirrors TestFilter::filter_expr_match's `^<expr, * -> .*>$`.
// Callers pass only non-empty patterns (filter_match handles the empty case).
function glob_match(subject: string, pattern: string) -> bool {
  let parts = pattern.split("*")
  let n = parts.length()
  if (n == 1) {
    // No wildcard: exact match.
    return subject == pattern
  }
  let first = parts[0]
  let last = parts[n - 1]
  if (!subject.starts_with(first)) { return false }
  if (!subject.ends_with(last)) { return false }
  let start = first.length()
  let end_limit = subject.length() - last.length()
  // Prefix and suffix anchors must not overlap.
  if (start > end_limit) { return false }
  // Each interior segment must appear in order within (start, end_limit].
  let pos = start
  for (let i = 1; i < n - 1; i += 1) {
    let part = parts[i]
    // Empty parts (from consecutive '*') match nothing and are skipped.
    if (part != "") {
      let idx = subject.substring(pos, end_limit).index_of(part)
      if (idx < 0) { return false }
      pos = pos + idx + part.length()
    }
  }
  true
}

// An empty pattern matches anything; otherwise glob-match. Mirrors the
// `filter_expr.is_empty()` short-circuit in TestFilter::filter_expr_match.
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

// Replicates TestFilter::includes: exclude wins; if any include patterns exist
// only included leaves are selected; with no include patterns everything not
// excluded is selected.
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

// Run the given leaves concurrently (spawn/await), each with its own test
// runner, and aggregate into a flat report.
function run_selected_leaves(leaves: FlatLeaf[]) -> FlatTestReport {
  let futures: baml.future.Future<FlatTestResult, null>[] = []
  for (let leaf in leaves) {
    futures.push(spawn {
      root.flatten_leaf_report(leaf.name, root.run_test(leaf.body, leaf.runner))
    })
  }
  let results = (await baml.future.all_complete(futures)) catch_all (e) {
    _ => baml.sys.panic("unexpected test child failure")
  }
  let passed = 0
  let failed = 0
  let has_error = false
  let failed_names: string[] = []
  let messages: string[] = []
  for (let r in results) {
    if (r.outcome == "pass") {
      passed += 1;
    } else {
      failed += 1;
      failed_names.push(r.name);
      for (let m in r.messages) {
        messages.push(m);
      }
      if (r.outcome == "error") {
        has_error = true;
      }
    }
  }
  let outcome: Outcome = if (has_error) { "error" } else if (failed == 0) { "pass" } else { "fail" }
  FlatTestReport {
    outcome: outcome,
    passed: passed,
    failed: failed,
    total: results.length(),
    failed_names: failed_names,
    messages: messages,
    results: results,
  }
}

// Convert a single leaf's TestReport into a flat row. Surfaces every failing
// run's message and sums run durations.
function flatten_leaf_report(name: string, report: TestReport) -> FlatTestResult {
  let messages: string[] = []
  let duration = 0
  for (let run in report.runs) {
    duration += run.duration_ms;
    if (run.outcome != "pass") {
      if let m: string = run.message {
        messages.push(m);
      }
    }
  }
  FlatTestResult {
    name: name,
    outcome: report.outcome,
    messages: messages,
    duration_ms: duration,
    run_count: report.runs.length(),
  }
}

// Flatten a run_all() TestSetReport (unfiltered path). The aggregate outcome is
// preserved so testset runners (e.g. PassRate) are honored, while counts are
// recomputed over leaves and failure messages/names are gathered for display.
function flatten_testset_report(report: TestSetReport) -> FlatTestReport {
  let outcomes: string[] = []
  root.collect_leaf_outcomes(report.results, outcomes)
  let passed = 0
  let failed = 0
  for (let o in outcomes) {
    if (o == "pass") { passed += 1 } else { failed += 1 }
  }
  let messages: string[] = []
  root.collect_leaf_messages(report.results, messages)
  let no_rows: FlatTestResult[] = []
  FlatTestReport {
    outcome: report.outcome,
    passed: passed,
    failed: failed,
    total: outcomes.length(),
    failed_names: report.failed_names,
    messages: messages,
    results: no_rows,
  }
}

// Push "pass"/"fail" for each leaf TestReport, recursing into nested testsets.
function collect_leaf_outcomes(results: ChildReport[], out: string[]) -> void {
  for (let r in results) {
    match (r) {
      let tr: TestReport => {
        out.push(if (tr.outcome == "pass") { "pass" } else { "fail" });
      },
      let tsr: TestSetReport => {
        if (tsr.results.length() == 0) {
          // Opaque/errored testset (e.g. failed expansion) carries no per-leaf
          // detail, so fall back to its own tallies instead of dropping it.
          for (let i = 0; i < tsr.passed; i += 1) { out.push("pass"); }
          for (let i = 0; i < tsr.failed; i += 1) { out.push("fail"); }
        } else {
          root.collect_leaf_outcomes(tsr.results, out);
        }
      },
    }
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
