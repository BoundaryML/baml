use std::{cell::RefCell, collections::BTreeMap, time::Duration};

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::TestStatus;

use super::{RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap};

pub(super) struct PrettyTestExecutionStatusRenderer {
    multi_progress: MultiProgress,
    // Map of tests (by key) to their individual progress bars.
    test_bars: RefCell<BTreeMap<(String, String), ProgressBar>>,
    // A summary bar for overall status counts.
    summary_bar: ProgressBar,
}


impl PrettyTestExecutionStatusRenderer {
    /// Create a new renderer.
    pub fn new() -> Self {
        let multi_progress = MultiProgress::new();

        // Create a dedicated summary bar and add it FIRST.
        let summary_bar = multi_progress.add(ProgressBar::new(0));
        summary_bar.set_style(
            ProgressStyle::default_spinner()
                .template("{msg}")
                .unwrap(),
        );
        summary_bar.set_message("Summary: 0 failures, 0 passes, 0 running, 0 pending, 0 done");

        Self {
            multi_progress,
            test_bars: RefCell::new(BTreeMap::new()),
            summary_bar,
        }
    }

    pub fn print_final_results(&self, test_status_map: &TestExecutionStatusMap) {
        // Print header.
        println!();
        println!("INFO: Test results:");
        println!("---------------------------------------------------------");
    
        // Counters for summary.
        let mut passed_count = 0;
        let mut failed_count = 0;
        let mut aborted_count = 0;
        let mut needs_eval_count = 0;
        let mut cancelled_count = 0;
        let mut skipped_count = 0;
    
        // Iterate through tests.
        for ((func, test), status) in test_status_map {
            let target = format!("{}::{}", func, test);
            match status {
                TestExecutionStatus::Finished(Ok(response), duration) => {
                    let time_str = format_duration(duration);
                    match response.status() {
                        TestStatus::Pass => {
                            passed_count += 1;
                            println!(
                                "Test {:<50} \x1b[32mPASSED\x1b[0m {}",
                                target, time_str
                            );
                        }
                        TestStatus::Fail(details) => {
                            failed_count += 1;
                            println!(
                                "Test {:<50} \x1b[31mFAILED\x1b[0m {}",
                                target, time_str
                            );
                            println!("  Details: {}", details);
                        }
                        TestStatus::NeedsHumanEval(details) => {
                            needs_eval_count += 1;
                            println!(
                                "Test {:<50} \x1b[33mNEEDS EVAL\x1b[0m {}",
                                target, time_str
                            );
                            println!("  Details: {:?}", details);
                        }
                    }
                }
                TestExecutionStatus::Finished(Err(details), duration) => {
                    aborted_count += 1;
                    let time_str = format_duration(&duration);
                    println!(
                        "Test {:<50} \x1b[31mERROR\x1b[0m {}",
                        target, time_str
                    );
                    println!("  Details: {}", details);
                }
                TestExecutionStatus::Pending => {
                    cancelled_count += 1;
                    println!(
                        "Test {:<50} \x1b[36mCANCELLED\x1b[0m",
                        target
                    );
                }
                TestExecutionStatus::Running => {
                    cancelled_count += 1;
                    println!(
                        "Test {:<50} \x1b[32mCANCELLED\x1b[0m",
                        target
                    );
                }
                TestExecutionStatus::Excluded => {
                    skipped_count += 1;
                    println!(
                        "Test {:<50} \x1b[33mSKIPPED\x1b[0m",
                        target
                    );
                }
            }
        }
    
        println!("---------------------------------------------------------");
    
        // Summary: total tests, passed, failed, needs eval, not run.
        let total = test_status_map.len();
        println!(
            "INFO: Test run completed, {} tests run: {} passed, {} failed, {} needing human eval, {} aborted, {} cancelled, {} skipped",
            total, passed_count, failed_count, needs_eval_count, aborted_count, cancelled_count, skipped_count
        );
        println!();
    }
    
}

/// Helper to format a Duration as a string (e.g. "(in 0.32s)").
fn format_duration(duration: &Duration) -> String {
    let secs = duration.as_secs_f64();
    format!("(in {:.2}s)", secs)
}

impl RenderTestExecutionStatus for PrettyTestExecutionStatusRenderer {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap) {
        // Define a spinner style for individual test bars.
        let spinner_style = ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap();

        // Compute counts.
        let running_keys: Vec<(String, String)> = test_status_map
            .iter()
            .filter_map(|((func, test), status)| {
                if let TestExecutionStatus::Running = status {
                    Some((func.to_string(), test.to_string()))
                } else {
                    None
                }
            })
            .collect();
        let running_count = running_keys.len();
        let finished_count = test_status_map
        .iter()
        .filter_map(|((_, _), status)| {
            if let TestExecutionStatus::Finished(Ok(res), _) = status {
                Some(res.status())
            } else {
                None
            }
        });

        let done_count = test_status_map
            .iter()
            .filter(|((_, _), status)| matches!(status, TestExecutionStatus::Finished(_, _)))
            .count();

        let pending_count = test_status_map
            .iter()
            .filter(|((_, _), status)| matches!(status, TestExecutionStatus::Pending))
            .count();

        let failures_count = test_status_map
            .iter()
            .filter(|((_, _), status)| {
                if let TestExecutionStatus::Finished(Ok(response), _) = status {
                    matches!(response.status(), TestStatus::Fail(_))
                } else {
                    false
                }
            })
            .count();
        let passes_count = test_status_map
            .iter()
            .filter(|((_, _), status)| {
                if let TestExecutionStatus::Finished(Ok(response), _) = status {
                    matches!(response.status(), TestStatus::Pass)
                } else {
                    false
                }
            })
            .count();
        let needs_human_eval_count = test_status_map
            .iter()
            .filter(|((_, _), status)| {
                if let TestExecutionStatus::Finished(Ok(response), _) = status {
                    matches!(response.status(), TestStatus::NeedsHumanEval(_))
                } else {
                    false
                }
            })
            .count();

        // Update the dedicated summary bar (always at the top).
        self.summary_bar.set_message(format!(
            "Summary: {} failures, {} passes, {} needs-human-eval, {} running, {} pending, {} done",
            failures_count, passes_count, needs_human_eval_count, running_count, pending_count, done_count,
        ));

        // Now update individual test progress bars.
        if running_count > 5 {
            // Show only the first 4 individually.
            let individual_keys: Vec<(String, String)> =
                running_keys.iter().take(4).cloned().collect();
            {
                let mut bars = self.test_bars.borrow_mut();
                // Create or update progress bars for the first 4 tests.
                for key in &individual_keys {
                    if !bars.contains_key(key) {
                        let pb = self.multi_progress.add(ProgressBar::new_spinner());
                        pb.set_style(spinner_style.clone());
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb.set_message(format!("Running {}::{}", key.0, key.1));
                        bars.insert(key.clone(), pb);
                    } else if let Some(pb) = bars.get(key) {
                        pb.set_message(format!("Running {}::{}", key.0, key.1));
                    }
                }
                // Remove any individual bars not among the first 4.
                let keys_to_remove: Vec<(String, String)> = bars
                    .keys()
                    .filter(|k| !individual_keys.contains(k))
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(pb) = bars.remove(&key) {
                        pb.finish_and_clear();
                    }
                }
            }
            // You can optionally create an extra summary for the overflow tests
            // if you still want to indicate "and X more running…"; otherwise, the global summary suffices.
        } else {
            // If 5 or fewer running tests, show them all individually.
            {
                let mut bars = self.test_bars.borrow_mut();
                for key in &running_keys {
                    if !bars.contains_key(key) {
                        let pb = self.multi_progress.add(ProgressBar::new_spinner());
                        pb.set_style(spinner_style.clone());
                        pb.enable_steady_tick(Duration::from_millis(100));
                        pb.set_message(format!("Running {}::{}", key.0, key.1));
                        bars.insert(key.clone(), pb);
                    } else if let Some(pb) = bars.get(key) {
                        pb.set_message(format!("Running {}::{}", key.0, key.1));
                    }
                }
                // Remove any bars not in the current running set.
                let keys_to_remove: Vec<(String, String)> = bars
                    .keys()
                    .filter(|k| !running_keys.contains(k))
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(pb) = bars.remove(&key) {
                        pb.finish_and_clear();
                    }
                }
            }
        }
    }

    fn render_final(&self, test_status_map: &TestExecutionStatusMap) {
        {
            let mut bars = self.test_bars.borrow_mut();
            for (_, pb) in bars.iter_mut() {
                pb.finish_and_clear();
            }
        }
        self.summary_bar.finish_and_clear();
        self.print_final_results(test_status_map);
    }
}
