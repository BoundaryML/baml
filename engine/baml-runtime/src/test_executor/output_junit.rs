use std::{collections::BTreeMap, fs::File, time::Duration};

use junit_report::{Report, TestCase, TestSuite};

use super::{RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap};
use crate::TestStatus;

pub(super) struct JUnitXMLRenderer {
    target_file: String,
}

impl JUnitXMLRenderer {
    pub fn new(target_file: &str) -> Self {
        JUnitXMLRenderer {
            target_file: target_file.to_string(),
        }
    }
}

impl RenderTestExecutionStatus for JUnitXMLRenderer {
    fn render_progress(&self, _test_status_map: &TestExecutionStatusMap) {
        // JUnit XML renderer does not render progress updates.
    }

    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        _selected_tests: &BTreeMap<(String, String), String>, // Not used for JUnit XML output
    ) {
        let mut report = Report::new();

        // Group tests by function (using function name as the testsuite name)
        let mut grouped_tests: BTreeMap<&str, Vec<(&str, &TestExecutionStatus, Option<Duration>)>> =
            BTreeMap::new();
        for ((func, test), status) in test_status_map.iter() {
            let duration = match status {
                TestExecutionStatus::Finished(_, dur) => Some(*dur),
                _ => None,
            };
            grouped_tests
                .entry(func)
                .or_default()
                .push((test, status, duration));
        }

        for (func_name, tests) in grouped_tests.iter() {
            let mut suite = TestSuite::new(func_name);

            for (test_name, status, duration) in tests {
                // Convert Duration to seconds as f64. Default to 0.0 if no duration.
                let duration = duration.unwrap_or(Duration::from_secs(0));
                let nanos = duration.as_nanos();
                let seconds = (nanos / 1_000_000_000) as i64;
                let nanos = (nanos % 1_000_000_000) as i32;

                let duration = junit_report::Duration::new(seconds, nanos);

                match status {
                    TestExecutionStatus::Finished(Ok(response), _) => match response.status() {
                        TestStatus::Pass => {
                            suite.add_testcase(TestCase::success(test_name, duration));
                        }
                        TestStatus::Fail(details) => {
                            suite.add_testcase(TestCase::failure(
                                test_name,
                                duration,
                                "Fail",
                                &details.to_string(),
                            ));
                        }
                        TestStatus::NeedsHumanEval(details) => {
                            suite.add_testcase(TestCase::failure(
                                test_name,
                                duration,
                                "Needs Human Evaluation",
                                &details.join(", "),
                            ));
                        }
                    },
                    TestExecutionStatus::Finished(Err(details), _) => {
                        suite.add_testcase(TestCase::error(
                            test_name,
                            duration,
                            "Error",
                            &details.to_string(),
                        ));
                    }
                    TestExecutionStatus::Excluded => {
                        suite.add_testcase(TestCase::skipped(test_name));
                    }
                    TestExecutionStatus::Pending | TestExecutionStatus::Running => {
                        suite.add_testcase(TestCase::skipped(test_name));
                    }
                }
            }

            report.add_testsuite(suite);
        }

        let file = File::create(&self.target_file).unwrap();
        let _ = report.write_xml(file);
    }
}
