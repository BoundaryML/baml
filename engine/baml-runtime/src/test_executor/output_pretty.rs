use std::{cell::RefCell, collections::BTreeMap, iter, time::Duration};

use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::TestStatus;

use super::{RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap};
use std::io::Write;

pub(super) struct PrettyTestExecutionStatusRenderer {
    multi_progress: MultiProgress,
    // Map of tests (by key) to their individual progress bars.
    test_bars: RefCell<BTreeMap<(String, String), ProgressBar>>,
    // A summary bar for overall status counts.
    summary_bar: ProgressBar,
}

fn write_indented(
    s: &str,
    indent: usize,
    modifier: impl Fn(&str) -> colored::ColoredString,
    trailing_newline: bool,
) {
    let mut f = std::io::stdout();
    for line in s.lines() {
        let _ = writeln!(&mut f, "{:indent$}{}", "", modifier(line));
    }
    if trailing_newline {
        let _ = writeln!(&mut f);
    }
}

#[derive(Default)]
struct TestCounts {
    passed: usize,
    failed: usize,
    aborted: usize,
    needs_eval: usize,
    running: usize,
    pending: usize,
    skipped: usize,
}

impl TestCounts {
    fn add(&mut self, other: &TestCounts) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.aborted += other.aborted;
        self.needs_eval += other.needs_eval;
        self.skipped += other.skipped;
        self.running += other.running;
        self.pending += other.pending;
    }

    fn total(&self) -> usize {
        self.passed
            + self.failed
            + self.needs_eval
            + self.aborted
            + self.skipped
            + self.running
            + self.pending
    }

    fn cancelled(&self) -> usize {
        self.running + self.pending
    }

    fn done(&self) -> usize {
        self.passed + self.failed + self.needs_eval + self.aborted
    }

    fn progress_summary(&self) -> String {
        // uses emoji to indicate progress
        if self.total() > 0 {
            let mut summary = format!("{}/{} tests - ", self.done(), self.total());
            if self.needs_eval > 0 {
                summary.push_str(&format!("{} 🕵️, ", self.needs_eval));
            }
            if self.passed > 0 {
                summary.push_str(&format!("{} ✅, ", self.passed));
            }
            if self.failed > 0 {
                summary.push_str(&format!("{} ❌, ", self.failed));
            }
            if self.aborted > 0 {
                summary.push_str(&format!("{} 🛑, ", self.aborted));
            }
            if self.skipped > 0 {
                summary.push_str(&format!("{} ⏩, ", self.skipped));
            }
            // if self.running > 0 {
            //     summary.push_str(&format!("{} ▶️, ", self.running));
            // }
            // if self.pending > 0 {
            //     summary.push_str(&format!("{} ⏸️, ", self.pending));
            // }
            summary.pop();
            summary.pop();
            summary
        } else {
            "".to_string()
        }
    }

    fn short_summary(&self, at_end: bool) -> String {
        let total = self.total();
        if total > 0 {
            let mut summary = format!("{} tests (", total);
            if self.needs_eval > 0 {
                summary.push_str(&format!("{} 🕵️, ", self.needs_eval));
            }
            if self.passed > 0 {
                summary.push_str(&format!("{} ✅, ", self.passed));
            }
            if self.failed > 0 {
                summary.push_str(&format!("{} ❌, ", self.failed));
            }
            if self.aborted > 0 {
                summary.push_str(&format!("{} 🛑, ", self.aborted));
            }
            if self.skipped > 0 {
                summary.push_str(&format!("{} ⏩, ", self.skipped));
            }
            if at_end {
                if self.cancelled() > 0 {
                    summary.push_str(&format!("{} cancelled, ", self.cancelled()));
                }
            } else {
                if self.running > 0 {
                    summary.push_str(&format!("{} ▶️, ", self.running));
                }
                if self.pending > 0 {
                    summary.push_str(&format!("{} ⏸️, ", self.pending));
                }
            }
            // Remove the trailing comma and space.
            summary.pop();
            summary.pop();
            summary.push_str(")");
            summary
        } else {
            "".to_string()
        }
    }
}

/// Returns a TestCounts struct by iterating over all test statuses.
fn count_tests<'a>(statuses: impl Iterator<Item = &'a TestExecutionStatus>) -> TestCounts {
    let mut counts = TestCounts::default();
    for status in statuses {
        match status {
            TestExecutionStatus::Finished(Ok(response), _) => match response.status() {
                TestStatus::Pass => counts.passed += 1,
                TestStatus::Fail(_) => counts.failed += 1,
                TestStatus::NeedsHumanEval(_) => counts.needs_eval += 1,
            },
            TestExecutionStatus::Finished(Err(_), _) => counts.aborted += 1,
            TestExecutionStatus::Pending => counts.pending += 1,
            TestExecutionStatus::Running => counts.running += 1,
            TestExecutionStatus::Excluded => counts.skipped += 1,
        }
    }
    counts
}

impl PrettyTestExecutionStatusRenderer {
    /// Create a new renderer.
    pub fn new() -> Self {
        let multi_progress = MultiProgress::new();

        // Create a dedicated summary bar and add it FIRST.
        let summary_bar = multi_progress.add(ProgressBar::new(0));
        summary_bar.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        summary_bar.set_message("Summary: 0 failures, 0 passes, 0 running, 0 pending, 0 done");

        Self {
            multi_progress,
            test_bars: RefCell::new(BTreeMap::new()),
            summary_bar,
        }
    }

    pub fn print_final_results(
        &self,
        test_status_map: &TestExecutionStatusMap,
        test_file_map: &BTreeMap<(String, String), String>,
    ) {
        // Print header.
        println!();
        println!("INFO: Test results:");
        println!("---------------------------------------------------------");

        // Group tests by function.
        let mut grouped: BTreeMap<&str, Vec<(&str, &TestExecutionStatus)>> = BTreeMap::new();
        for ((func, test), status) in test_status_map {
            grouped
                .entry(func)
                .or_insert_with(Vec::new)
                .push((test, status));
        }
        let mut total_counts = TestCounts::default();

        // Iterate through each function group.
        for (func, tests) in grouped {
            // Use TestCounts for this group.
            let counts = count_tests(tests.iter().map(|(_, status)| *status));
            if counts.total() == counts.cancelled() {
                println!("{}", format!("{} {} ({} cancelled)", "function".blue(), func.blue(), counts.cancelled()).dimmed());
                continue;
            }

            println!(
                "{} {}\n{}",
                "function".blue().bold(),
                func.blue().bold(),
                counts.short_summary(true)
            );
            total_counts.add(&counts);

            for (test, status) in tests {
                // If available, get the file name for this test.
                let file_name = test_file_map.get(&(func.to_string(), test.to_string())).map(|s| format!(" {}", s));
                let file_name = || {
                    if let Some(file_name) = file_name {
                        write_indented(&file_name, 4, |s| s.dimmed(), false);
                    }
                };
                // Create the test identifier string.
                let target = format!("  {}::{}", func, test);

                match status {
                    TestExecutionStatus::Finished(Ok(response), duration) => {
                        let time_str = format_duration(duration);
                        match response.status() {
                            TestStatus::Pass => {
                                write_indented(&format!("{time_str} {} {}", "PASSED".green(), target), 2, |s| s.into(), false);
                                file_name();
                                println!("");
                            }
                            TestStatus::Fail(details) => {
                                write_indented(&format!("{time_str} {:<20} {}\n", "FAILED".red(), target), 2, |s| s.into(), false);
                                file_name();
                                write_indented(&details.to_string(), 4, |s| s.red().dimmed(), true);
                            }
                            TestStatus::NeedsHumanEval(details) => {
                                write_indented(&format!("{time_str} {} {}", "NEEDS EVAL".yellow(), target), 2, |s| s.into(), false);
                                file_name();
                                for d in details {
                                    write_indented(&d, 4, |s| s.dimmed(), true);
                                }
                            }
                        }
                    }
                    TestExecutionStatus::Finished(Err(details), duration) => {
                        let time_str = format_duration(duration);
                        write_indented(&format!("{time_str} {} {}", "ERROR".bright_red(), target), 2, |s| s.into(), false);
                        file_name();
                        write_indented(&details.to_string(), 4, |s| s.red().dimmed(), true);
                    }
                    TestExecutionStatus::Pending => {
                        write_indented(&format!("{} {:<50}", "CANCELLED".bright_cyan(), target), 2, |s| s.dimmed(), false);
                    }
                    TestExecutionStatus::Running => {
                        write_indented(&format!("{} {:<50}", "CANCELLED".bright_cyan(), target), 2, |s| s.dimmed(), false);
                        file_name();
                    }
                    TestExecutionStatus::Excluded => {
                        write_indented(&format!("{} {:<50}", "SKIPPED".bright_yellow(), target), 2, |s| s.dimmed(), true);
                    }
                }
            }
        }

        println!("---------------------------------------------------------");

        // Summary: total tests, passed, failed, needs eval, not run.
        println!(
            "INFO: Test run completed, {}",
            total_counts.short_summary(true)
        );
        println!();
    }
}

/// Helper to format a Duration as a string (e.g. "(in 0.32s)").
fn format_duration(duration: &Duration) -> String {
    let secs = duration.as_secs_f64();
    format!("{:.2}s", secs)
}

impl RenderTestExecutionStatus for PrettyTestExecutionStatusRenderer {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap) {
        // Define a spinner style for individual test bars.
        let spinner_style = ProgressStyle::default_spinner()
            .template("{spinner:.green} {msg}")
            .unwrap();

        // Determine individual running tests for progress bars.
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

        // Use TestCounts to aggregate all statuses.
        let counts = count_tests(test_status_map.values());
        // Compute finished tests as those that are either passed, failed, needs eval, or aborted.

        // Update the dedicated summary bar (always at the top).
        self.summary_bar
            .set_message(format!("Summary: {}", counts.progress_summary(),));

        // Update individual test progress bars.
        if running_keys.len() > 5 {
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
                // Create an extra summary for the overflow tests.
                let summary_key = ("<summary>".to_string(), "<summary>".to_string());
                let running_count = counts.running - 4;
                let pending_count = counts.pending;
                let summary_str = if pending_count > 0 {
                    format!("Running {} more tests... {} pending", running_count, pending_count)
                } else {
                    format!("Running {} more tests...", running_count)
                };
                if !bars.contains_key(&summary_key) {
                    let pb = self.multi_progress.add(ProgressBar::new_spinner());
                    pb.set_style(spinner_style.clone());
                    pb.enable_steady_tick(Duration::from_millis(100));
                    bars.insert(summary_key.clone(), pb);
                };
                bars.get_mut(&summary_key).unwrap().set_message(summary_str);


                // Remove any individual bars not among the first 4.
                let keys_to_remove: Vec<(String, String)> = bars
                    .keys()
                    .filter(|k| !(individual_keys.contains(k) || k.0 == summary_key.0))
                    .cloned()
                    .collect();
                for key in keys_to_remove {
                    if let Some(pb) = bars.remove(&key) {
                        pb.finish_and_clear();
                    }
                }
            }
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

    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        selected_tests: &BTreeMap<(String, String), String>,
    ) {
        {
            let mut bars = self.test_bars.borrow_mut();
            for (_, pb) in bars.iter_mut() {
                pb.finish_and_clear();
            }
        }
        self.summary_bar.finish_and_clear();
        self.print_final_results(test_status_map, selected_tests);
    }
}
