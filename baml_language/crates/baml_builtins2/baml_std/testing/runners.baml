/// Creates a test runner that runs the body `n` times and passes when at
/// least `m` of those runs passed.
///
/// Every one of the `n` runs happens, even once `m` passes are in hand.
///
/// # Parameters
///
/// - `n`: how many times to run the body. Must be positive.
/// - `m`: how many of those runs must pass. Must be in `[0, n]`.
///
/// # Panics
///
/// - `baml.panics.UserPanic` if `n <= 0`, or `m` is outside `[0, n]`.
function Quorum(n: int, m: int) -> TestRunner {
    if (n <= 0) {
        baml.sys.panic("testing.Quorum requires n > 0")
    } else if (m < 0 || m > n) {
        baml.sys.panic("testing.Quorum requires 0 <= m <= n")
    } else {
        null
    }

    return (run_once) -> TestReportThunk {
        () -> TestReport {
            let runs: RunReport[] = [];
            let passes = 0;
            for (let i = 0; i < n; i += 1) {
                let report = run_once();
                for (let run in report.runs) {
                    runs.push(run)
                }
                if (report.outcome == "pass") {
                    passes += 1
                } else {
                    null
                }
            }
            let outcome: Outcome = if (passes >= m) {
                "pass"
            } else {
                "fail"
            };
            TestReport { outcome: outcome, runs: runs }
        }
    };
}

/// Creates a test runner that retries a test up to `max_attempts` times.
/// Tests fail if there are no passes after `max_attempts` attempts.
function Retry(max_attempts: int) -> TestRunner {
    if (max_attempts <= 0) {
        baml.sys.panic("testing.Retry requires max_attempts > 0")
    } else {
        null
    }

    return (run_once) -> TestReportThunk {
        () -> TestReport {
            let runs: RunReport[] = [];
            let passed = false;
            let attempts = 0;
            while (attempts < max_attempts && !passed) {
                let report = run_once();
                attempts += 1;
                for (let run in report.runs) {
                    runs.push(run)
                }
                if (report.outcome == "pass") {
                    passed = true
                } else {
                    null
                }
            }
            let outcome: Outcome = if (passed) {
                "pass"
            } else {
                "fail"
            };
            TestReport { outcome: outcome, runs: runs }
        }
    };
}

/// Creates a test runner that requires a pass rate above `threshold` to pass.
function PassRate(threshold: float) -> TestSetRunner {
    if (threshold < 0.0 || threshold > 1.0) {
        baml.sys.panic("testing.PassRate requires 0.0 <= threshold <= 1.0")
    } else {
        null
    }

    return (children) -> TestSetReport {
        // Runner middleware runs outside the CLI's shared pool (the closure
        // cannot capture it); a local fallback pool still bounds the fan-out.
        let report = root._run_children_parallel(children, root._test_pool(0));
        let outcome: Outcome = if (report.total == 0) {
            "pass"
        } else if ((report.passed + 0.0) / (report.total + 0.0) >= threshold) {
            "pass"
        } else {
            "fail"
        };
        TestSetReport {
            outcome: outcome,
            passed: report.passed,
            failed: report.failed,
            total: report.total,
            failed_names: report.failed_names,
            results: report.results,
            result_names: report.result_names,
        }
    };
}

/// Creates a testset runner that runs children one at a time, in
/// registration order, instead of in parallel.
function Sequential() -> TestSetRunner {
    return (children) -> TestSetReport {
        root._run_children_sequential(children, false)
    };
}

/// Creates a testset runner that runs children in registration order and
/// abandons the testset at the first child that does not pass.
///
/// The children after it are never run, so they appear in no report.
function FailFast() -> TestSetRunner {
    return (children) -> TestSetReport {
        root._run_children_sequential(children, true)
    };
}

/// (internal)
function _run_children_sequential(children: TestSetChild[], fail_fast: bool) -> TestSetReport {
    let named_results: _NamedChildReport[] = [];
    for (let child in children) {
        let report = child.run();
        named_results.push(_NamedChildReport { name: child.name, report: report });
        let outcome = match (report) {
            let r: TestReport => r.outcome,
            let r: TestSetReport => r.outcome,
        };
        if (fail_fast && outcome != "pass") {
            return root._aggregate_reports(named_results);
        } else {
            null
        }
    }
    root._aggregate_reports(named_results)
}
