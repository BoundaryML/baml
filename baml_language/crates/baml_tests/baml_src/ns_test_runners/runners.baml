// The built-in test runners and the `with` clause that attaches them.
//
// Nothing else in the repository uses that clause, so this fixture is the only
// coverage for the whole path: the parser's contextual `with`, the runner
// argument the compiler synthesizes into `register_test`/`register_test_set`,
// and the five runners in `baml_std/testing/runners.baml`.
//
// A runner is only interesting when the thing it wraps FAILS, so the failing
// cases are asserted through `expect_fail` / `expect_set_fail`, which invert
// the verdict. That pins the failure without failing the suite.

/// Inverts a test's verdict: it passes if and only if its body fails.
///
/// Pass `inner` to wrap another runner first — that is how a test asserts the
/// inner runner reported failure, and it exercises runner composition.
function expect_fail(inner: testing.TestRunner? = null) -> testing.TestRunner {
    (run_once) -> testing.TestReportThunk {
        let wrapped = if let runner: testing.TestRunner = inner {
            runner(run_once)
        } else {
            run_once
        };
        () -> testing.TestReport {
            let report = wrapped();
            let outcome: testing.Outcome = if (report.outcome == "pass") {
                "fail"
            } else {
                "pass"
            };
            testing.TestReport { outcome: outcome, runs: report.runs }
        }
    }
}

/// Inverts a testset runner's verdict, and requires that it ran exactly
/// `ran` children.
///
/// The child count is the only way to observe that a runner stopped early, so
/// it is checked here rather than asserted: a mismatch has to surface as a
/// failing testset, because a `TestSetRunner` is `throws never`.
function expect_set_fail(inner: testing.TestSetRunner, ran: int) -> testing.TestSetRunner {
    (children) -> testing.TestSetReport {
        let report = inner(children);
        let outcome: testing.Outcome = if (report.outcome != "pass" && report.results.length() == ran) {
            "pass"
        } else {
            "fail"
        };
        testing.TestSetReport {
            outcome: outcome,
            passed: report.passed,
            failed: report.failed,
            total: report.total,
            failed_names: report.failed_names,
            results: report.results,
            result_names: report.result_names,
        }
    }
}

testset "retry" with testing.Sequential() {
    let attempts = 0;

    test "passes once an attempt succeeds" with testing.Retry(3) {
        attempts += 1;
        assert.is_true(attempts >= 3);
    }

    test "stopped retrying at the first pass" {
        assert.equal(attempts, 3);
    }
}

testset "retry exhausted" with testing.Sequential() {
    let tries = 0;

    test "fails after max_attempts" with expect_fail(inner = testing.Retry(2)) {
        tries += 1;
        assert.is_true(false);
    }

    test "ran exactly max_attempts times" {
        assert.equal(tries, 2);
    }
}

// `Quorum(n, m)` runs the body `n` times and passes when at least `m` of those
// runs passed — n is the run count, m the required passes.
testset "quorum" with testing.Sequential() {
    let runs = 0;

    test "passes when enough runs pass" with testing.Quorum(3, 2) {
        runs += 1;
        // Fails only on the third run, leaving 2 passes out of 3.
        assert.is_true(runs != 3);
    }

    test "ran every run, not just until the quorum was met" {
        assert.equal(runs, 3);
    }
}

testset "quorum shortfall" with testing.Sequential() {
    let runs = 0;

    test "fails when too few runs pass" with expect_fail(inner = testing.Quorum(3, 3)) {
        runs += 1;
        assert.is_true(runs != 2);
    }

    test "still ran the full count" {
        assert.equal(runs, 3);
    }
}

// One failing leaf out of two is exactly the 0.5 threshold, so the testset
// passes and the failure is tolerated rather than propagated.
testset "pass rate tolerates a minority" with testing.PassRate(0.5) {
    test "passes" {
        null
    }

    test "fails but is tolerated" {
        assert.is_true(false);
    }
}

// FailFast must abandon the testset at the first non-passing child, so exactly
// one child report comes back even though two were registered.
testset "fail fast stops at the first failure" with expect_set_fail(testing.FailFast(), 1) {
    test "fails first" {
        assert.is_true(false);
    }

    test "must not run" {
        null
    }
}

// Sequential runs children in registration order; each child records itself and
// the last one checks the sequence.
testset "sequential preserves order" with testing.Sequential() {
    let order: string[] = [];

    test "first" {
        order.push("first");
    }

    test "second" {
        order.push("second");
    }

    test "third sees the two before it" {
        assert.equal(order.join(","), "first,second");
    }
}
