// A test body's value is an assertion when it is `false`: `test "x" { 1 == 2 }`
// fails with "test body evaluated to false". Any other value passes, so a
// smoke test that just calls a function (`test "x" { f() }`) still works.
//
// The failing cases are asserted through `expect_fail_with`, which inverts the
// verdict and pins the failure message, so they fail without failing the suite.

/// Passes if and only if the test fails with exactly `message`.
function expect_fail_with(message: string) -> testing.TestRunner {
    (run_once) -> testing.TestReportThunk {
        () -> testing.TestReport {
            let report = run_once();
            let run_message = report.runs[0].message ?? "";
            let outcome: testing.Outcome = if (report.outcome == "fail" && run_message == message) {
                "pass"
            } else {
                "fail"
            };
            testing.TestReport { outcome: outcome, runs: report.runs }
        }
    }
}

function body_value_is_even(n: int) -> bool {
    n % 2 == 0
}

function body_value_int() -> int {
    3
}

test "body evaluating to false fails" with expect_fail_with("test body evaluated to false") {
    1 == 2
}

test "body calling a function that returns false fails" with expect_fail_with("test body evaluated to false") {
    body_value_is_even(3)
}

test "body with statements then a false tail fails" with expect_fail_with("test body evaluated to false") {
    let xs = [1, 2, 3];
    xs.length() == 4
}

test "body evaluating to true passes" {
    1 == 1
}

test "body calling a function that returns true passes" {
    body_value_is_even(4)
}

test "body evaluating to a non-bool value passes" {
    body_value_int()
}

test "body with no tail expression passes" {
    let x = 1;
}

testset "body value inside a testset" {
    test "false tail fails" with expect_fail_with("test body evaluated to false") {
        false
    }

    test "true tail passes" {
        true
    }
}
