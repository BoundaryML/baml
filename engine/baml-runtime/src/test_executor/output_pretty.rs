use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    io::Write,
    iter,
    time::Duration,
};

use colored::Colorize;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use super::{RenderTestExecutionStatus, TestExecutionStatus, TestExecutionStatusMap};
use crate::TestStatus;

pub(super) struct PrettyTestExecutionStatusRenderer {
    multi_progress: MultiProgress,
    // Map of tests (by key) to their individual progress bars.
    test_bars: RefCell<BTreeMap<(String, String), ProgressBar>>,
    // A summary bar for overall status counts.
    summary_bar: ProgressBar,
    // Track tests already printed (to avoid duplicate output)
    printed_tests: RefCell<BTreeSet<(String, String)>>,
}

fn write_indented_string(
    s: &str,
    indent: usize,
    modifier: impl Fn(&str) -> colored::ColoredString,
) -> String {
    let mut output = String::new();
    for line in s.lines() {
        output.push_str(&format!("{:indent$}{}\n", "", modifier(line)));
    }
    output
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
            let mut summary = format!("{total} tests (");
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
            summary.pop();
            summary.pop();
            summary.push(')');
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
            printed_tests: RefCell::new(BTreeSet::new()),
        }
    }

    fn print_test_result(
        &self,
        func: &str,
        test: &str,
        status: &TestExecutionStatus,
        file_name: Option<&String>,
        indent: usize,
    ) -> Option<String> {
        let file_name_string =
            file_name.map(|name| write_indented_string(&format!(" {name}"), 4, |s| s.dimmed()));
        let target = format!("  {func}::{test}");
        let mut output = String::new();

        match status {
            TestExecutionStatus::Finished(Ok(response), duration) => {
                let time_str = format_duration(duration);
                match response.status() {
                    TestStatus::Pass => {
                        output.push_str(&write_indented_string(
                            &format!("{time_str} {:<10} {}\n", "PASSED".green(), target),
                            indent,
                            |s| s.into(),
                        ));
                        if let Some(file_name_str) = &file_name_string {
                            output.push_str(file_name_str);
                        }
                    }
                    TestStatus::Fail(details) => {
                        output.push_str(&write_indented_string(
                            &format!("{time_str} {:<10} {}", "FAILED".red(), target),
                            indent,
                            |s| s.into(),
                        ));
                        if let Some(file_name_str) = &file_name_string {
                            output.push_str(file_name_str);
                        }
                        output.push_str(&write_indented_string(
                            &details.to_string(),
                            indent + 2,
                            |s| s.red().dimmed(),
                        ));
                    }
                    TestStatus::NeedsHumanEval(details) => {
                        output.push_str(&write_indented_string(
                            &format!("{time_str} {:<10} {}", "NEEDS EVAL".yellow(), target),
                            indent,
                            |s| s.into(),
                        ));
                        if let Some(file_name_str) = &file_name_string {
                            output.push_str(file_name_str);
                        }
                        for d in details {
                            output.push_str(&write_indented_string(&d, 4, |s| s.dimmed()));
                        }
                    }
                }
            }
            TestExecutionStatus::Finished(Err(details), duration) => {
                let time_str = format_duration(duration);
                output.push_str(&write_indented_string(
                    &format!("{time_str} {:<10} {}", "ERROR".bright_red(), target),
                    indent,
                    |s| s.into(),
                ));
                if let Some(file_name_str) = &file_name_string {
                    output.push_str(file_name_str);
                }
                output.push_str(&write_indented_string(
                    &details.to_string(),
                    indent + 2,
                    |s| s.red().dimmed(),
                ));
            }
            TestExecutionStatus::Pending => {
                output.push_str(&write_indented_string(
                    &format!("{:<10} {}", "CANCELLED".bright_cyan(), target),
                    indent,
                    |s| s.dimmed(),
                ));
            }
            TestExecutionStatus::Running => {
                output.push_str(&write_indented_string(
                    &format!("{:<10} {}", "CANCELLED".bright_cyan(), target),
                    indent,
                    |s| s.dimmed(),
                ));
                if let Some(file_name_str) = &file_name_string {
                    output.push_str(file_name_str);
                }
            }
            TestExecutionStatus::Excluded => {
                output.push_str(&write_indented_string(
                    &format!("{:<10} {}", "SKIPPED".bright_yellow(), target),
                    indent,
                    |s| s.dimmed(),
                ));
            }
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    #[allow(clippy::print_stdout)]
    pub fn print_final_results(
        &self,
        test_status_map: &TestExecutionStatusMap,
        test_file_map: &BTreeMap<(String, String), String>,
    ) {
        println!();
        println!("INFO: Test results:");
        println!("---------------------------------------------------------");

        let mut grouped: BTreeMap<&str, Vec<(&str, &TestExecutionStatus)>> = BTreeMap::new();
        for ((func, test), status) in test_status_map {
            grouped.entry(func).or_default().push((test, status));
        }
        let mut total_counts = TestCounts::default();

        for (func, tests) in grouped {
            let counts = count_tests(tests.iter().map(|(_, status)| *status));
            if counts.total() == counts.cancelled() {
                println!(
                    "{}",
                    format!(
                        "{} {} ({} cancelled)",
                        "function".blue(),
                        func.blue(),
                        counts.cancelled()
                    )
                    .dimmed()
                );
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
                let file_name = test_file_map.get(&(func.to_string(), test.to_string()));
                if let Some(output) = self.print_test_result(func, test, status, file_name, 2) {
                    print!("{output}"); // Use print! instead of println! to avoid extra newline
                }
            }
        }

        println!("---------------------------------------------------------");
        println!(
            "INFO: Test run completed, {}",
            total_counts.short_summary(true)
        );
        println!();
    }
}

/// Helper to format a Duration as a string (e.g. "0.32s").
fn format_duration(duration: &Duration) -> String {
    let secs = duration.as_secs_f64();
    format!("{secs:.2}s")
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

        // Update the dedicated summary bar.
        self.summary_bar
            .set_message(format!("Summary: {}", counts.progress_summary(),));

        // Update individual test progress bars.
        if running_keys.len() > 5 {
            let individual_keys: Vec<(String, String)> =
                running_keys.iter().take(4).cloned().collect();
            {
                let mut bars = self.test_bars.borrow_mut();
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
                    format!("Running {running_count} more tests... {pending_count} pending")
                } else {
                    format!("Running {running_count} more tests...")
                };
                if !bars.contains_key(&summary_key) {
                    let pb = self.multi_progress.add(ProgressBar::new_spinner());
                    pb.set_style(spinner_style.clone());
                    pb.enable_steady_tick(Duration::from_millis(100));
                    bars.insert(summary_key.clone(), pb);
                }
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

        // --- New code: Immediately print outputs for tests that have finished with errors, failures, or need human evaluation ---
        for ((func, test), status) in test_status_map {
            // Only print if not already printed.
            if self
                .printed_tests
                .borrow()
                .contains(&(func.clone(), test.clone()))
            {
                continue;
            }

            match status {
                TestExecutionStatus::Finished(Ok(response), _) => {
                    match response.status() {
                        TestStatus::Pass => {} // Do not print passes immediately in progress
                        TestStatus::Fail(_) | TestStatus::NeedsHumanEval(_) => {
                            let file_name_option = None; // File name not available here during progress.
                            if let Some(output) =
                                self.print_test_result(func, test, status, file_name_option, 0)
                            {
                                self.multi_progress.println(&output).unwrap();
                            }
                        }
                    }
                }
                TestExecutionStatus::Finished(Err(_), _) => {
                    let file_name_option = None; // File name not available here during progress.
                    if let Some(output) =
                        self.print_test_result(func, test, status, file_name_option, 0)
                    {
                        self.multi_progress.println(&output).unwrap();
                    }
                }
                _ => {}
            }
            if matches!(status, TestExecutionStatus::Finished(_, _)) {
                self.printed_tests
                    .borrow_mut()
                    .insert((func.clone(), test.clone()));
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
