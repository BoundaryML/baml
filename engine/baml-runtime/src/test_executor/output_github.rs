pub(super) struct GithubTestExecutionStatusRenderer;

impl GithubTestExecutionStatusRenderer {
    pub fn new() -> Self {
        Self
    }
}

use std::collections::BTreeMap;

use super::{RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap};
use crate::TestStatus;

impl RenderTestExecutionStatus for GithubTestExecutionStatusRenderer {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap) {}

    #[allow(clippy::print_stdout)]
    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        selected_tests: &BTreeMap<(String, String), String>,
    ) {
        for ((function_name, test_name), status) in test_status_map.iter() {
            match status {
                TestExecutionStatus::Pending => {
                    println!("[internal error] pending: {function_name}::{test_name}");
                }
                TestExecutionStatus::Running => {
                    println!("[internal error] running: {function_name}::{test_name}");
                }
                TestExecutionStatus::Excluded => {
                    println!("[internal error] skipped: {function_name}::{test_name}");
                }
                TestExecutionStatus::Finished(result, duration) => match result {
                    Ok(response) => match response.status() {
                        TestStatus::Fail(reason) => {
                            print!("::group::");
                            println!(
                                "[     FAILED ]: {function_name}::{test_name} in {duration:?}"
                            );
                            println!("{reason:#?}");
                            println!("::endgroup::");
                        }
                        TestStatus::Pass => {
                            println!(
                                "[         ok ]: {function_name}::{test_name} in {duration:?}"
                            );
                        }
                        TestStatus::NeedsHumanEval(reasons) => {
                            print!("::group::");
                            println!(
                                "[ needs-human ]: {function_name}::{test_name} in {duration:?}"
                            );
                            println!("::endgroup::");
                        }
                    },
                    Err(e) => {
                        print!("::group::");
                        println!("error: {function_name}::{test_name} in {duration:?}");
                        println!("{e}");
                        println!("::endgroup::");
                    }
                },
            }
        }
        let excluded = test_status_map
            .iter()
            .filter_map(|((function_name, test_name), status)| {
                if matches!(status, TestExecutionStatus::Excluded) {
                    Some((function_name, test_name))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        if !excluded.is_empty() {
            print!("::group::");
            println!("Excluded: {}", excluded.len());
            for (function_name, test_name) in excluded {
                println!("{function_name}::{test_name}");
            }
            println!("::endgroup::")
        }
    }
}
