// root.TestCollector — collector for tests and testsets.
// The runtime creates a TestCollector, calls $init_test(collector), then reads
// collector.tests and collector.testsets.

class TestRegistration {
    name: string,
    body: TestBody,
    runner: TestRunner?,
}

class TestSetRegistration {
    name: string,
    collector: TestSetBody,
    runner: TestSetRunner?,
}

/// Registration failures are structural discovery errors, not failed tests.
/// Callers must let this typed error escape instead of converting it into a
/// synthetic "(failed to expand)" leaf.
class InvalidTestName {
    message: string,
}

class TestCollector {
    prefix: string,
    tests: TestRegistration[],
    testsets: TestSetRegistration[],

    function new(prefix: string) -> TestCollector {
        TestCollector { prefix: prefix, tests: [], testsets: [] }
    }

    function register_test(self, name: string, body: TestBody, runner: TestRunner?) -> void {
        if (name.includes("::")) {
            throw InvalidTestName { message: "test name may not contain reserved separator `::`: " + name }
        } else {
            null
        }
        let full_name = if (self.prefix == "") {
            name
        } else {
            self.prefix + "::" + name
        };
        let count = 0;
        let hash_prefix = full_name + "#";
        for (let t in self.tests) {
            if (t.name == full_name || t.name.starts_with(hash_prefix)) {
                count += 1
            } else {
                null
            }
        }
        let final_name = if (count > 0) {
            full_name + "#" + string.from(count + 1)
        } else {
            full_name
        };
        let _ = self.tests.push(TestRegistration { name: final_name, body: body, runner: runner });
    }

    function register_test_set(
        self,
        name: string,
        collector: TestSetBody,
        runner: TestSetRunner?,
    ) -> void {
        if (name.includes("::")) {
            throw InvalidTestName { message: "testset name may not contain reserved separator `::`: " + name }
        } else {
            null
        }
        let full_name = if (self.prefix == "") {
            name
        } else {
            self.prefix + "::" + name
        };
        let count = 0;
        let hash_prefix = full_name + "#";
        for (let ts in self.testsets) {
            if (ts.name == full_name || ts.name.starts_with(hash_prefix)) {
                count += 1
            } else {
                null
            }
        }
        let final_name = if (count > 0) {
            full_name + "#" + string.from(count + 1)
        } else {
            full_name
        };
        let _ = self.testsets.push(
            TestSetRegistration { name: final_name, collector: collector, runner: runner },
        );
    }

    /// Compiler-only top-level registration entry points. The owner is derived
    /// from the declaring file (`root` plus its BAML namespaces); nested items use
    /// the ordinary methods above and inherit this canonical prefix.
    function register_test_at(
        self,
        owner: string,
        name: string,
        body: TestBody,
        runner: TestRunner?,
    ) -> void {
        let scoped = TestCollector { prefix: owner, tests: self.tests, testsets: self.testsets };
        scoped.register_test(name, body, runner)
    }

    function register_test_set_at(
        self,
        owner: string,
        name: string,
        collector: TestSetBody,
        runner: TestSetRunner?,
    ) -> void {
        let scoped = TestCollector { prefix: owner, tests: self.tests, testsets: self.testsets };
        scoped.register_test_set(name, collector, runner)
    }

    function _find_testset(self, name: string) -> TestSetRegistration {
        for (let ts in self.testsets) {
            if (ts.name == name) {
                return ts;
            }
        }
        throw ("TestSet not found: " + name)
    }

    function _find_test(self, name: string) -> TestRegistration {
        for (let t in self.tests) {
            if (t.name == name) {
                return t;
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
    type: string, // "test" | "lazyTestSet"
    name: string,
}

class SerializedTestSet {
    name: string,
    items: SerializedTestDef[],
    loadingTimeMs: int,
}

type SerializedTestDef = SerializedTest | SerializedTestSet;

// --- TestRegistry: wraps a TestCollector with lazy expansion state ---

class TestRegistry {
    collector: TestCollector,
    expansions: map<string, TestRegistry>,
    loading_time_ms: int,

    function new(collector: TestCollector) -> TestRegistry {
        TestRegistry { collector: collector, expansions: {}, loading_time_ms: 0 }
    }

    /// Serialize registry to pure-data list for the UI.
    /// Checks expansions map to determine lazy vs expanded for each testset.
    ///
    /// Tests are serialized before testsets by design — tests are leaf items,
    /// testsets are containers.
    function serialize(self) -> SerializedTestDef[] {
        let items: SerializedTestDef[] = [];
        for (let t in self.collector.tests) {
            items.push(SerializedTest { type: "test", name: t.name })
        }
        for (let ts in self.collector.testsets) {
            let expanded = self.expansions.get(ts.name);
            match (expanded) {
                let sub: TestRegistry => {
                    items.push(
                        SerializedTestSet {
                            name: ts.name,
                            items: sub.serialize(),
                            loadingTimeMs: sub.loading_time_ms,
                        },
                    )
                },
                null => {
                    items.push(SerializedTest { type: "lazyTestSet", name: ts.name })
                },
            }
        }
        return items;
    }

    /// Find test by name in root collector, then recurse into expanded sub-registries.
    function run_test(self, name: string) -> TestReport {
        // Check tests in this registry's collector
        for (let t in self.collector.tests) {
            if (t.name == name) {
                return root._run_test(t.body, t.runner);
            }
        }
        // Recurse into expanded sub-registries
        for (let k in self.expansions.keys()) {
            let sub = self.expansions[k];
            // Check if the name could be in this sub-registry (segment-aware prefix match)
            if (name.starts_with(k + "::")) {
                return sub.run_test(name);
            }
        }
        throw ("Test not found: " + name)
    }

    function run_testset(self, name: string) -> TestSetReport {
        for (let ts in self.collector.testsets) {
            if (ts.name == name) {
                let pool = root._test_pool(0);
                return root._run_testset(
                    root._testset_children(self._expand_testset_registry(ts), pool),
                    ts.runner,
                    pool,
                );
            }
        }
        for (let k in self.expansions.keys()) {
            let sub = self.expansions[k];
            if (name.starts_with(k + "::")) {
                return sub.run_testset(name);
            }
        }
        throw ("TestSet not found: " + name)
    }

    function run_all(self) -> TestSetReport {
        self._run_all_pooled(root._test_pool(0))
    }

    function _run_all_pooled(self, pool: _TestPool) -> TestSetReport {
        root._run_testset(root._testset_children(self, pool), null, pool)
    }

    function _run_selected(self, names: string[]) -> TestSetReport {
        self._run_selected_pooled(names, root._test_pool(0))
    }

    function _run_selected_pooled(self, names: string[], pool: _TestPool) -> TestSetReport {
        root._run_testset(root._testset_children_selected(self, names, pool), null, pool)
    }

    /// CLI entry point: glob-filter in BAML, run (honoring testset runners), and
    /// flatten into a tolerated-aware flat report the Rust driver prints directly.
    ///   * no filters -> run_all (whole tree)
    ///   * any filter -> glob the leaf names, then run_selected (parent runners
    ///                   still apply to the selected subset)
    function run_filtered(
        self,
        profile_include: string[],
        profile_exclude: string[],
        cli_include: string[],
        cli_exclude: string[],
        max_concurrency: int,
    ) -> FlatTestReport {
        // Always discover the selected leaf IDs first. Besides supplying canonical
        // console identities, this makes structural registration errors actionable
        // even when the unfiltered execution path preserves run_all semantics.
        let selected_names = root._select_names_layered(self, profile_include, profile_exclude, cli_include, cli_exclude);
        let pool = root._test_pool(max_concurrency);
        let report = if (
            profile_include.length() == 0
                && profile_exclude.length() == 0
                && cli_include.length() == 0
                && cli_exclude.length() == 0
        ) {
            self._run_all_pooled(pool)
        } else {
            self._run_selected_pooled(selected_names, pool)
        };
        root._flatten_with_tolerated(report, selected_names)
    }

    /// CLI `--list` support: the selected leaf names, without running anything.
    function list_filtered(
        self,
        profile_include: string[],
        profile_exclude: string[],
        cli_include: string[],
        cli_exclude: string[],
    ) -> string[] {
        root._select_names_layered(self, profile_include, profile_exclude, cli_include, cli_exclude)
    }

    /// Expand a lazy testset by name. Searches recursively through expanded
    /// sub-registries so nested testsets can be expanded too.
    function expand_set(self, name: string) -> SerializedTestDef[] {
        // Try root collector first
        for (let ts in self.collector.testsets) {
            if (ts.name == name) {
                self._expand_testset_registry(ts);
                return self.serialize();
            }
        }
        // Search in expanded sub-registries using segment-aware prefix match
        for (let k in self.expansions.keys()) {
            let sub = self.expansions[k];
            if (name.starts_with(k + "::")) {
                return sub.expand_set(name);
            }
        }
        throw ("TestSet not found for expansion: " + name)
    }

    function _expand_testset_registry(self, ts: TestSetRegistration) -> TestRegistry {
        match (self.expansions.get(ts.name)) {
            let existing: TestRegistry => existing,
            null => {
                let start = baml.time.Instant.now();
                let sub_collector = TestCollector { prefix: ts.name, tests: [], testsets: [] };
                ts.collector(sub_collector);
                // `loading_time_ms` is an `int`, so narrowing the bigint millisecond
                // count is declared fallible. An expansion would have to run for
                // ~146 million years to leave `int`'s range; there is no honest
                // duration to report if it somehow did.
                let elapsed = start.elapsed().to_milliseconds().to_int() catch (_) {
                    baml.errors.InvalidArgument { message } => {
                        baml.sys.panic("testing: expansion duration: " + message)
                    },
                };
                let sub_registry = TestRegistry { collector: sub_collector, expansions: {}, loading_time_ms: elapsed };
                self.expansions.set(ts.name, sub_registry);
                sub_registry
            },
        }
    }
}

function _testset_children(registry: TestRegistry, pool: _TestPool) -> TestSetChild[] {
    let children: TestSetChild[] = [];
    for (let t in registry.collector.tests) {
        children.push(root._test_child(t.name, t.body, t.runner))
    }
    for (let ts in registry.collector.testsets) {
        children.push(root._testset_child(registry, ts, pool))
    }
    children
}

function _testset_children_selected(
    registry: TestRegistry,
    names: string[],
    pool: _TestPool,
) -> TestSetChild[] {
    let children: TestSetChild[] = [];
    for (let t in registry.collector.tests) {
        if (names.includes(t.name)) {
            children.push(root._test_child(t.name, t.body, t.runner))
        } else {
            null
        }
    }
    for (let ts in registry.collector.testsets) {
        let prefix = `${ts.name}::`;
        // Select testsets by descendant match only. `names` holds leaf paths, and a
        // testset contributes only "name::..." descendants — never a bare "name" — so
        // an exact ts.name match can happen only when a same-named top-level `test`
        // was selected (tests and testsets dedupe in separate collections, so the
        // names can collide). Honoring that would wrongly run the whole testset.
        if (
            names.some((n) -> {
                n.starts_with(prefix)
            })
        ) {
            children.push(root._testset_child_selected(registry, ts, names, pool))
        } else {
            null
        }
    }
    children
}

function _test_child(name: string, body: TestBody, runner: TestRunner?) -> TestSetChild {
    TestSetChild {
        name: name,
        run: () -> ChildReport {
            root._run_test(body, runner)
        },
        is_container: false,
    }
}

function _testset_child(
    registry: TestRegistry,
    ts: TestSetRegistration,
    pool: _TestPool,
) -> TestSetChild {
    TestSetChild {
        name: ts.name,
        is_container: true,
        run: () -> ChildReport {
            root._run_testset(
                root._testset_children(registry._expand_testset_registry(ts), pool),
                ts.runner,
                pool,
            ) catch_all (e) {
                baml.panics.Panic => root._expansion_error_report(ts.name),
                _ => root._expansion_error_report(ts.name),
            }
        },
    }
}

function _testset_child_selected(
    registry: TestRegistry,
    ts: TestSetRegistration,
    names: string[],
    pool: _TestPool,
) -> TestSetChild {
    TestSetChild {
        name: ts.name,
        is_container: true,
        run: () -> ChildReport {
            root._run_testset(
                root._testset_children_selected(registry._expand_testset_registry(ts), names, pool),
                ts.runner,
                pool,
            ) catch_all (e) {
                baml.panics.Panic => root._expansion_error_report(ts.name),
                _ => root._expansion_error_report(ts.name),
            }
        },
    }
}

function _aggregate_reports(named_results: _NamedChildReport[]) -> TestSetReport {
    let passed = 0;
    let failed = 0;
    let has_error = false;
    let failed_names: string[] = [];
    let results: ChildReport[] = [];
    let result_names: string[] = [];
    for (let named_result in named_results) {
        let result = named_result.report;
        results.push(result);
        result_names.push(named_result.name);
        let outcome = match (result) {
            let report: TestReport => report.outcome,
            let report: TestSetReport => report.outcome,
        };
        if (outcome == "pass") {
            passed += 1
        } else {
            failed += 1;
            let child_failed_names = match (result) {
                let report: TestReport => [named_result.name],
                let report: TestSetReport => {
                    if (report.failed_names.length() > 0) {
                        report.failed_names
                    } else {
                        [named_result.name]
                    }
                },
            };
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
    let outcome: Outcome = if (has_error) {
        "error"
    } else if (failed == 0) {
        "pass"
    } else {
        "fail"
    };
    TestSetReport {
        outcome: outcome,
        passed: passed,
        failed: failed,
        total: results.length(),
        failed_names: failed_names,
        results: results,
        result_names: result_names,
    }
}

/// (internal) Global gate on concurrently RUNNING leaf test bodies. A leaf acquires a
/// slot BEFORE its body is spawned, so a gated test costs nothing — no VM
/// thread, no watchdog timer — until a slot frees. One pool is shared by the
/// whole run (threaded down through every nested testset), so the limit is a
/// run-wide invariant rather than a per-testset one.
class _TestPool {
    // Token bucket. `pop` and `push` are single locked array operations, so
    // concurrent dispatchers race safely; an empty `pop` means "no slot free".
    tokens: int[],

    function acquire(self) -> null throws never {
        while (true) {
            if let _slot: int = self.tokens.pop() {
                return null;
            }
            // Saturated: park briefly rather than spin. Only starved
            // dispatchers poll, and there is at most one dispatcher per
            // currently-expanding testset, so the wakeup load is bounded by
            // testset nesting, not by test count.
            baml.sys.sleep(baml.time.Duration.from_milliseconds(10)) catch (e) {
                _ => null
            };
        }
    }

    function release(self) -> null throws never {
        self.tokens.push(1);
        null
    }
}

/// (internal) A pool admitting `max_concurrency` leaves at once. Zero or negative means
/// "no explicit budget": fall back to a bound that keeps a pathological
/// fan-out from creating thousands of live VM threads while staying above any
/// real machine's core count (the CLI always passes an explicit budget).
function _test_pool(max_concurrency: int) -> _TestPool {
    let limit = if (max_concurrency > 0) {
        max_concurrency
    } else {
        64
    };
    let tokens: int[] = [];
    let i = 0;
    while (i < limit) {
        tokens.push(1);
        i = i + 1;
    }
    _TestPool { tokens: tokens }
}

function _run_children_parallel(
    children: TestSetChild[],
    pool: _TestPool,
) -> TestSetReport throws never {
    let futures: baml.future.Future<_NamedChildReport, never>[] = [];
    for (let child in children) {
        if (child.is_container) {
            // Containers only spawn and await; admitting them freely cannot
            // oversubscribe the machine, and gating them could deadlock the
            // slots their own leaves need.
            futures.push(spawn { _NamedChildReport { name: child.name, report: child.run() } })
        } else {
            // Acquire on the dispatcher, before the spawn exists: the whole
            // point is that an inadmissible leaf has no thread yet.
            pool.acquire();
            futures.push(spawn {
                let named = _NamedChildReport { name: child.name, report: child.run() };
                // `run` is total: a leaf is `run_test`, whose runner arm
                // catches middleware panics precisely so this release is
                // reached on every outcome.
                pool.release();
                named
            })
        }
    }
    // No catch: the children's bodies cannot throw, so the futures are
    // `Future<_NamedChildReport, never>` and awaiting them is infallible.
    let named_results = await baml.future.all_complete(futures);
    root._aggregate_reports(named_results)
}

/// (internal) Run a single test with error handling and optional runner middleware.
/// Every test races against a deadline (default 5 minutes; override with the
/// BAML_TEST_TIMEOUT_MS env var): a hung test — a stalled network read, a
/// deadlocked await — becomes a FAIL with a "timed out" message instead of
/// hanging the whole run. `baml.future.race` cancels the losers when one
/// side settles (BEP-034), so a timed-out body is actively cancelled and a
/// finished test's leftover timer doesn't linger.
function _run_test(body: TestBody, runner: TestRunner?) -> TestReport {
    let base_run: TestReportThunk = () -> TestReport {
        let start = baml.time.Instant.now();
        let timeout_ms = match (
            baml.env.get("BAML_TEST_TIMEOUT_MS") catch (e) {
                _ => null
            }
        ) {
            let t: string => {
                int.parse(t) catch (e) {
                    _ => 300000
                }
            },
            _ => 300000,
        };
        let work: baml.future.Future<RunReport, never> = spawn {
            {
                body();
                RunReport { outcome: "pass", duration_ms: 0, message: null }
            } catch_all (e, ctx) {
                let p: baml.panics.UserPanic => {
                    RunReport { outcome: "fail", duration_ms: 0, message: p.message }
                },
                let p: baml.panics.AssertionFailed => {
                    RunReport { outcome: "fail", duration_ms: 0, message: p.message }
                },
                baml.panics.Panic => {
                    RunReport { outcome: "fail", duration_ms: 0, message: ctx.to_string() }
                },
                _ => {
                    RunReport { outcome: "fail", duration_ms: 0, message: ctx.to_string() }
                }
            }
        };
        let timer: baml.future.Future<RunReport, never> = spawn {
            baml.sys.sleep(baml.time.Duration.from_milliseconds(timeout_ms)) catch (e) {
                _ => null
            };
            RunReport {
                outcome: "fail",
                duration_ms: 0,
                message: "test timed out after "
                    + timeout_ms.to_string()
                    + "ms (override with BAML_TEST_TIMEOUT_MS)",
            }
        };
        // Race the body against the deadline; race cancels the loser on settle
        // (BEP-034), so a timed-out body is actively cancelled and a finished
        // test's timer does not linger.
        let result = (await baml.future.race([work, timer])) catch_all (e) {
            _ => {
                RunReport {
                    outcome: "fail",
                    duration_ms: 0,
                    message: "test task failed unexpectedly",
                }
            }
        };
        // `duration_ms` is an `int`; see the note in expand_testset_registry for
        // why narrowing the bigint millisecond count cannot fail here.
        let duration = start.elapsed().to_milliseconds().to_int() catch (_) {
            baml.errors.InvalidArgument { message } => {
                baml.sys.panic("testing: test duration: " + message)
            },
        };
        TestReport {
            outcome: result.outcome,
            runs: [RunReport { outcome: result.outcome, duration_ms: duration, message: result.message }],
        }
    };
    match (runner) {
        null => base_run(),
        let r: TestRunner => {
            // Runner middleware is user code with no totality contract of its
            // own. Catching here is what makes `run_test` total — the leaf
            // dispatcher's pool slot is released after this returns, so a
            // panicking runner must become a failed report, never an escaped
            // throw that strands the slot.
            {
                let wrapped: TestReportThunk = r(base_run);
                wrapped()
            } catch_all (e, ctx) {
                _ => {
                    TestReport {
                        outcome: "fail",
                        runs: [
                            RunReport {
                                outcome: "fail",
                                duration_ms: 0,
                                message: "test runner middleware failed: " + ctx.to_string(),
                            },
                        ],
                    }
                }
            }
        },
    }
}

/// (internal) Run all children of a testset with optional runner middleware.
///
/// A custom `TestSetRunner` receives the children as a plain closure and runs
/// them however it likes, outside the pool's budget — the built-in runners are
/// sequential (inherently capped) or take a local fallback pool (`PassRate`).
function _run_testset(
    children: TestSetChild[],
    runner: TestSetRunner?,
    pool: _TestPool,
) -> TestSetReport {
    match (runner) {
        null => root._run_children_parallel(children, pool),
        let r: TestSetRunner => r(children),
    }
}

// ---------------------------------------------------------------------------
// CLI runner: BAML-side glob filtering, name collection, and tolerated-aware
// flattening (run_filtered / list_filtered). The Rust driver passes raw
// include/exclude patterns and prints the returned FlatTestReport directly.
// ---------------------------------------------------------------------------

/// (internal) The report for a testset that errored (e.g. its generator threw or panicked
/// during expansion): one error with no per-leaf detail.
function _expansion_error_report(name: string) -> TestSetReport {
    TestSetReport {
        outcome: "error",
        passed: 0,
        failed: 1,
        total: 1,
        // Canonically nested under the testset so filters can still select/exclude
        // it. The catch in testset_child(_selected) routes
        // runner/execution panics here too — not just expansion failures — so the
        // label stays generic ("testset error") rather than claiming an expansion
        // failure specifically. (collect_subtree_names, which is purely an expansion
        // step, keeps the more precise "failed to expand".)
        failed_names: [name + "::(testset error)"],
        results: [],
        result_names: [],
    }
}

/// (internal) Match a CLI selector against a canonical test id. Plain selectors are
/// case-sensitive substrings. A selector containing '*' is an anchored glob
/// where '*' matches any run of characters (including '::').
function _glob_match(subject: string, pattern: string) -> bool {
    let parts = pattern.split("*");
    let n = parts.length();
    if (n == 1) {
        return subject.index_of(pattern) != null;
    }
    let first = parts[0];
    let last = parts[n - 1];
    if (!subject.starts_with(first)) {
        return false;
    }
    if (!subject.ends_with(last)) {
        return false;
    }
    let start = first.length();
    let end_limit = subject.length() - last.length();
    if (start > end_limit) {
        return false;
    }
    let pos = start;
    for (let i = 1; i < n - 1; i += 1) {
        let part = parts[i];
        if (part != "") {
            if let idx: int = subject.slice(pos, end_limit).index_of(part) {
                pos = pos + idx + part.length()
            } else {
                return false;
            }
        }
    }
    true
}

function _any_pattern_matches(pats: string[], canonical_id: string) -> bool {
    for (let p in pats) {
        if (root._glob_match(canonical_id, p)) {
            return true;
        }
    }
    false
}

/// (internal) Replicates TestFilter::includes: exclude wins; with includes present only
/// included leaves are selected; with no includes everything not excluded is.
function _leaf_selected(name: string, include: string[], exclude: string[]) -> bool {
    if (root._any_pattern_matches(exclude, name)) {
        return false;
    }
    if (include.length() == 0) {
        return true;
    }
    root._any_pattern_matches(include, name)
}

/// (internal) Apply a profile and an explicit CLI layer independently. This is deliberately
/// not equivalent to concatenating their include arrays: profile AND CLI is the
/// contract, while includes within either layer remain OR.
function _leaf_selected_layered(
    name: string,
    profile_include: string[],
    profile_exclude: string[],
    cli_include: string[],
    cli_exclude: string[],
) -> bool {
    root._leaf_selected(name, profile_include, profile_exclude)
        && root._leaf_selected(name, cli_include, cli_exclude)
}

/// (internal) A trailing-star exclusion that matches the canonical descendant prefix proves
/// that every leaf below a lazy testset is excluded. This intentionally handles
/// only the provable/common case and otherwise expands conservatively.
function _subtree_excluded(prefix: string, excludes: string[]) -> bool {
    let descendant_prefix = prefix + "::";
    for (let pattern in excludes) {
        // A plain substring already present in the descendant prefix will be
        // present in every leaf below it.
        if (pattern.index_of("*") == null && descendant_prefix.index_of(pattern) != null) {
            return true;
        }
        // Matching the descendant prefix with a trailing-star glob proves every
        // possible suffix is excluded. This includes `*` and `root*`.
        if (pattern.ends_with("*") && root._glob_match(descendant_prefix, pattern)) {
            return true;
        }
    }
    false
}

/// (internal) Whether an include selector could match at least one leaf below `prefix`.
/// A plain substring can always occur in a future leaf name, so it expands
/// conservatively. For an explicit glob, the literal portion before the first
/// `*` must be compatible with the canonical descendant prefix.
function _pattern_may_match_descendant(pattern: string, prefix: string) -> bool {
    let descendant_prefix = prefix + "::";
    match (pattern.index_of("*")) {
        null => true,
        let star: int => {
            let literal = pattern.slice(0, star);
            descendant_prefix.starts_with(literal) || literal.starts_with(descendant_prefix)
        },
    }
}

function _subtree_may_be_selected(prefix: string, include: string[], exclude: string[]) -> bool {
    if (root._subtree_excluded(prefix, exclude)) {
        return false;
    }
    if (include.length() == 0) {
        return true;
    }
    for (let pattern in include) {
        if (root._pattern_may_match_descendant(pattern, prefix)) {
            return true;
        }
    }
    false
}

/// (internal) Glob-filter the registry while traversing it. Profile-excluded lazy subtrees
/// are pruned before their executable testset collectors are invoked.
function _select_names_layered(
    registry: TestRegistry,
    profile_include: string[],
    profile_exclude: string[],
    cli_include: string[],
    cli_exclude: string[],
) -> string[] {
    let out: string[] = [];
    for (let t in registry.collector.tests) {
        if (
            root._leaf_selected_layered(
                t.name,
                profile_include,
                profile_exclude,
                cli_include,
                cli_exclude,
            )
        ) {
            out.push(t.name)
        }
    }
    for (let ts in registry.collector.testsets) {
        // Each filter layer narrows independently. If either layer proves the
        // subtree cannot contribute a selected leaf, do not execute its collector.
        if (
            root._subtree_may_be_selected(ts.name, profile_include, profile_exclude)
                && root._subtree_may_be_selected(ts.name, cli_include, cli_exclude)
        ) {
            let names = root._collect_subtree_names_layered(
                registry,
                ts,
                profile_include,
                profile_exclude,
                cli_include,
                cli_exclude,
            );
            for (let name in names) {
                out.push(name)
            }
        }
    }
    out
}

/// (internal) Compatibility helper for internal callers: one ordinary filter layer.
function _select_names(registry: TestRegistry, include: string[], exclude: string[]) -> string[] {
    root._select_names_layered(registry, [], [], include, exclude)
}

/// (internal) Fully expand the registry and collect every leaf test's canonical id.
function _collect_leaf_names(registry: TestRegistry) -> string[] {
    let out: string[] = [];
    for (let t in registry.collector.tests) {
        out.push(t.name);
    }
    for (let ts in registry.collector.testsets) {
        for (let name in root._collect_subtree_names(registry, ts)) {
            out.push(name);
        }
    }
    out
}

/// (internal) Expand one testset to its leaf names, containing failures: a testset whose
/// generator throws or panics during expansion is represented by a sentinel leaf
/// *under* its own path ("root::suite::(failed to expand)") so canonical-id
/// filters select it like a real
/// leaf, then `run_selected` runs it and surfaces the error.
function _collect_subtree_names(registry: TestRegistry, ts: TestSetRegistration) -> string[] {
    root._collect_leaf_names(registry._expand_testset_registry(ts)) catch_all (e) {
        let err: InvalidTestName => throw err,
        baml.panics.Panic => [ts.name + "::(failed to expand)"],
        _ => [ts.name + "::(failed to expand)"],
    }
}

function _collect_subtree_names_layered(
    registry: TestRegistry,
    ts: TestSetRegistration,
    profile_include: string[],
    profile_exclude: string[],
    cli_include: string[],
    cli_exclude: string[],
) -> string[] {
    root._select_names_layered(
        registry._expand_testset_registry(ts),
        profile_include,
        profile_exclude,
        cli_include,
        cli_exclude,
    ) catch_all (e) {
        let err: InvalidTestName => throw err,
        baml.panics.Panic => {
            let sentinel = ts.name + "::(failed to expand)";
            if (
                root._leaf_selected_layered(
                    sentinel,
                    profile_include,
                    profile_exclude,
                    cli_include,
                    cli_exclude,
                )
            ) {
                [sentinel]
            } else {
                []
            }
        },
        _ => {
            let sentinel = ts.name + "::(failed to expand)";
            if (
                root._leaf_selected_layered(
                    sentinel,
                    profile_include,
                    profile_exclude,
                    cli_include,
                    cli_exclude,
                )
            ) {
                [sentinel]
            } else {
                []
            }
        },
    }
}

/// (internal) Leaf tallies split into hard failures vs. runner-tolerated failures.
class _LeafCounts {
    passed: int,
    failed: int,
    tolerated: int,
    total: int,
}

class _LeafIdentities {
    passed: string[],
    failed: string[],
    tolerated: string[],
    // Durations in milliseconds, parallel to the arrays above; `-1` for a
    // leaf with no measured run. Left EMPTY by the fallback classifiers,
    // which have names but no reports.
    passed_ms: int[],
    failed_ms: int[],
    tolerated_ms: int[],
}

/// (internal) Classify each leaf in a child-report list as passed / hard-failed /
/// tolerated. A failing leaf is tolerated iff some enclosing testset's runner
/// passed (tolerated_by_parent flows down). Replaces the Rust extract_leaf_summary.
function _count_leaves(results: ChildReport[], tolerated_by_parent: bool) -> _LeafCounts {
    let passed = 0;
    let failed = 0;
    let tolerated = 0;
    let total = 0;
    for (let r in results) {
        let delta = match (r) {
            let tr: TestReport => {
                if (tr.outcome == "pass") {
                    _LeafCounts { passed: 1, failed: 0, tolerated: 0, total: 1 }
                } else if (tolerated_by_parent) {
                    _LeafCounts { passed: 0, failed: 0, tolerated: 1, total: 1 }
                } else {
                    _LeafCounts { passed: 0, failed: 1, tolerated: 0, total: 1 }
                }
            },
            let tsr: TestSetReport => {
                let ft = tolerated_by_parent || (tsr.outcome == "pass");
                if (tsr.results.length() > 0) {
                    root._count_leaves(tsr.results, ft)
                } else if (ft) {
                    _LeafCounts {
                        passed: tsr.passed,
                        failed: 0,
                        tolerated: tsr.failed,
                        total: tsr.total,
                    }
                } else {
                    _LeafCounts {
                        passed: tsr.passed,
                        failed: tsr.failed,
                        tolerated: 0,
                        total: tsr.total,
                    }
                }
            },
        };
        passed += delta.passed;
        failed += delta.failed;
        tolerated += delta.tolerated;
        total += delta.total
    }
    _LeafCounts { passed: passed, failed: failed, tolerated: tolerated, total: total }
}

/// (internal) Flatten a run_all()/run_selected() TestSetReport into the CLI flat report.
/// Aggregate outcome is preserved (honoring testset runners); counts split hard
/// vs tolerated failures; failed_names are the report's (hard) failed names; and
/// failure messages are gathered from every failing run.
function _flatten_with_tolerated(
    report: TestSetReport,
    selected_names: string[],
) -> FlatTestReport {
    let top_tolerated = report.outcome == "pass";
    let counts = if (report.results.length() > 0) {
        root._count_leaves(report.results, top_tolerated)
    } else if (top_tolerated) {
        _LeafCounts {
            passed: report.passed,
            failed: 0,
            tolerated: report.failed,
            total: report.total,
        }
    } else {
        _LeafCounts {
            passed: report.passed,
            failed: report.failed,
            tolerated: 0,
            total: report.total,
        }
    };
    let messages: string[] = [];
    root._collect_leaf_messages(report.results, messages);
    let all_failed_names: string[] = [];
    root._collect_all_failed_names(report, all_failed_names);
    let executed = _LeafIdentities {
        passed: [],
        failed: [],
        tolerated: [],
        passed_ms: [],
        failed_ms: [],
        tolerated_ms: [],
    };
    root._collect_leaf_identities(report, top_tolerated, selected_names, executed);
    // Built-in reports carry names parallel to their executed child results.
    // Custom runners written before result_names existed may not; fall back to
    // selection/failure correlation only when the executed partition cannot
    // account for the leaf counts.
    let identities = if (
        executed.passed.length() == counts.passed
            && executed.failed.length() == counts.failed
            && executed.tolerated.length() == counts.tolerated
    ) {
        executed
    } else if (selected_names.length() == counts.total) {
        root._classify_selected_names(selected_names, report.failed_names, all_failed_names)
    } else {
        // An older custom runner may execute only part of the selection without
        // returning result_names. In that case the aggregate counts remain valid,
        // but assigning the unexecuted names would fabricate PASS identities.
        _LeafIdentities { passed: [], failed: [], tolerated: [] }
    };
    FlatTestReport {
        outcome: report.outcome,
        passed: counts.passed,
        failed: counts.failed,
        tolerated: counts.tolerated,
        total: counts.total,
        passed_names: identities.passed,
        failed_names: identities.failed,
        tolerated_names: identities.tolerated,
        passed_ms: identities.passed_ms,
        failed_ms: identities.failed_ms,
        tolerated_ms: identities.tolerated_ms,
        messages: messages,
    }
}

function _classify_selected_names(
    selected_names: string[],
    hard_failed_names: string[],
    all_failed_names: string[],
) -> _LeafIdentities {
    let out = _LeafIdentities {
        passed: [],
        failed: [],
        tolerated: [],
        passed_ms: [],
        failed_ms: [],
        tolerated_ms: [],
    };
    for (let name in selected_names) {
        if (root._failure_matches_selected(name, all_failed_names)) {
            if (root._failure_matches_selected(name, hard_failed_names)) {
                out.failed.push(name)
            } else {
                out.tolerated.push(name)
            }
        } else {
            out.passed.push(name)
        }
    }
    out
}

function _name_in(name: string, names: string[]) -> bool {
    for (let candidate in names) {
        if (candidate == name) {
            return true;
        }
    }
    false
}

function _failure_matches_selected(selected: string, failed_names: string[]) -> bool {
    if (root._name_in(selected, failed_names)) {
        return true;
    }
    let sentinel_suffix = "::(failed to expand)";
    if (selected.ends_with(sentinel_suffix)) {
        let prefix = selected.slice(0, selected.length() - sentinel_suffix.length()) + "::";
        for (let failed in failed_names) {
            if (failed.starts_with(prefix)) {
                return true;
            }
        }
    }
    false
}

function _collect_leaf_identities(
    report: TestSetReport,
    tolerated_by_parent: bool,
    selected_names: string[],
    out: _LeafIdentities,
) -> void {
    match (report.result_names) {
        null => null,
        let names: string[] => {
            let limit = if (names.length() < report.results.length()) {
                names.length()
            } else {
                report.results.length()
            };
            for (let i = 0; i < limit; i += 1) {
                let name = names[i];
                match (report.results[i]) {
                    let leaf: TestReport => {
                        let leaf_ms = if (leaf.runs.length() > 0) {
                            leaf.runs[0].duration_ms
                        } else {
                            -1
                        };
                        if (leaf.outcome == "pass") {
                            out.passed.push(name);
                            out.passed_ms.push(leaf_ms)
                        } else if (tolerated_by_parent) {
                            out.tolerated.push(name);
                            out.tolerated_ms.push(leaf_ms)
                        } else {
                            out.failed.push(name);
                            out.failed_ms.push(leaf_ms)
                        }
                    },
                    let nested: TestSetReport => {
                        let nested_tolerated = tolerated_by_parent || (nested.outcome == "pass");
                        if (nested.results.length() > 0) {
                            root._collect_leaf_identities(
                                nested,
                                nested_tolerated,
                                selected_names,
                                out,
                            )
                        } else if (nested.failed > 0) {
                            let sentinel = name + "::(failed to expand)";
                            let identity = if (root._name_in(sentinel, selected_names)) {
                                sentinel
                            } else if (nested.failed_names.length() > 0) {
                                nested.failed_names[0]
                            } else {
                                name + "::(testset error)"
                            };
                            if (nested_tolerated) {
                                out.tolerated.push(identity);
                                out.tolerated_ms.push(-1)
                            } else {
                                out.failed.push(identity);
                                out.failed_ms.push(-1)
                            }
                        } else {
                            null
                        }
                    },
                }
            }
        },
    }
}

/// (internal) Collect failures from every nested report, including failures hidden from a
/// parent aggregate because a testset runner tolerated them.
function _collect_all_failed_names(report: TestSetReport, out: string[]) -> void {
    for (let name in report.failed_names) {
        if (!root._name_in(name, out)) {
            out.push(name)
        }
    }
    for (let child in report.results) {
        match (child) {
            TestReport => null,
            let nested: TestSetReport => root._collect_all_failed_names(nested, out),
        }
    }
}

/// (internal) Gather failure messages from every failing run of every leaf, recursing into
/// nested testsets.
function _collect_leaf_messages(results: ChildReport[], out: string[]) -> void {
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
                root._collect_leaf_messages(tsr.results, out);
            },
        }
    }
}
