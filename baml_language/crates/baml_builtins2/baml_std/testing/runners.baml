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

function PassRate(threshold: float) -> TestSetRunner {
    if (threshold < 0.0 || threshold > 1.0) {
        baml.sys.panic("testing.PassRate requires 0.0 <= threshold <= 1.0")
    } else {
        null
    }

    return (children) -> TestSetReport {
        // Runner middleware runs outside the CLI's shared pool (the closure
        // cannot capture it); a local fallback pool still bounds the fan-out.
        let report = root.run_children_parallel(children, root.test_pool(0));
        let outcome: Outcome = if (report.total == 0) {
            "pass"
        } else if (root._int_to_float(report.passed) / root._int_to_float(report.total) >= threshold) {
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

function Sequential() -> TestSetRunner {
    return (children) -> TestSetReport {
        root.run_children_sequential(children, false)
    };
}

function FailFast() -> TestSetRunner {
    return (children) -> TestSetReport {
        root.run_children_sequential(children, true)
    };
}

function run_children_sequential(children: TestSetChild[], fail_fast: bool) -> TestSetReport {
    let named_results: NamedChildReport[] = [];
    for (let child in children) {
        let report = child.run();
        named_results.push(NamedChildReport { name: child.name, report: report });
        let outcome = match (report) {
            let r: TestReport => r.outcome,
            let r: TestSetReport => r.outcome,
        };
        if (fail_fast && outcome != "pass") {
            return root.aggregate_reports(named_results);
        } else {
            null
        }
    }
    root.aggregate_reports(named_results)
}

function _int_to_float(value: int) -> float {
    float.parse(string.from(value)) catch_all (e) {
        _ => baml.sys.panic("failed to convert int to float")
    }
}
