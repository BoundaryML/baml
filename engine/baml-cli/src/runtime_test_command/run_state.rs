use anyhow::Result;
use baml_lib::internal_baml_core::ir::TestCaseWalker;
use baml_types::BamlValue;
use colored::*;
use indexmap::{IndexMap, IndexSet};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use std::{sync::Arc, time::Duration};
use tokio::{
    sync::{Mutex, MutexGuard, Semaphore},
    task,
};

use baml_runtime::{BamlRuntime, InternalRuntimeInterface, RuntimeContextManager, TestResponse};

use super::filter::FilterArgs;

pub(super) struct TestCommand {
    runtime: Arc<Mutex<BamlRuntime>>,
    filter: FilterArgs,
}

#[derive(Clone)]
struct TestRunBar {
    p: MultiProgress,
    primary_bar: ProgressBar,
    bars: Vec<ProgressBar>,
    max_bars: usize,
}

impl TestRunBar {
    fn new(num_tasks: u64, num_bars: usize) -> TestRunBar {
        let p = MultiProgress::new();
        let primary_bar = p.add(
            ProgressBar::new(num_tasks).with_style(
                ProgressStyle::with_template(
                    "{elapsed} {spinner} {pos} of {len} tests finished: {msg}",
                )
                .unwrap()
                .tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈"),
            ),
        );
        primary_bar.enable_steady_tick(Duration::from_millis(250));

        let bars = (0..num_bars)
            .map(|_| {
                let b = p.add(
                    ProgressBar::new_spinner()
                        .with_style(ProgressStyle::default_spinner().tick_chars("⠁⠂⠄⡀⢀⠠⠐⠈")),
                );
                b.enable_steady_tick(Duration::from_millis(250));
                b
            })
            .collect::<Vec<_>>();

        TestRunBar {
            p,
            max_bars: num_bars,
            primary_bar,
            bars,
        }
    }

    fn update_bars(&self, active_tests: &IndexSet<(String, String)>) {
        for (idx, (func_name, test_name)) in active_tests.iter().take(self.max_bars).enumerate() {
            let bar = self.bars.get(idx).unwrap();
            bar.set_message(format!("Running {}::{}", func_name, test_name));
        }

        // for all non-active bars, hide them
        for idx in active_tests.len()..self.bars.len() {
            let bar = self.bars.get(idx).unwrap();
            bar.set_message("");
        }
    }

    pub fn println<I: AsRef<str>>(&self, msg: I) {
        self.primary_bar.println(msg)
    }

    fn start_test(
        &self,
        _function_name: &str,
        _test_name: &str,
        active_tests: &IndexSet<(String, String)>,
    ) {
        self.update_bars(active_tests);
    }

    fn end_test(
        &self,
        _function_name: &str,
        _test_name: &str,
        active_tests: &IndexSet<(String, String)>,
    ) {
        self.primary_bar.inc(1);
        self.update_bars(active_tests);
    }
}

pub(super) struct TestRunState {
    // Test state for each test
    // <Function, <Test, State>>
    test_state: IndexMap<String, IndexMap<String, TestState>>,
    active_tests: IndexSet<(String, String)>,
}

impl std::fmt::Display for TestRunState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut total = 0;
        let mut passed = 0;
        let mut failed = 0;
        let mut unable_to_run = 0;

        writeln!(f, "\n\n=== Test Results ===")?;
        for (func_name, tests) in &self.test_state {
            total += tests.len();

            writeln!(
                f,
                "{} {}",
                func_name.bold().cyan(),
                format!("({} Tests)", tests.len()).dimmed()
            )?;
            for (test_name, state) in tests {
                match state {
                    TestState::Finished(r) => match r.status() {
                        baml_runtime::TestStatus::Pass => passed += 1,
                        baml_runtime::TestStatus::Fail(_) => failed += 1,
                    },
                    TestState::UnableToRun(_) => unable_to_run += 1,
                    _ => {}
                }
                writeln!(f, "  {state} {}", test_name, state = state.colored_symbol())?;
            }
        }

        write!(
            f,
            "Total: {}, Passed: {}, Failed: {}",
            total, passed, failed
        )?;

        if unable_to_run > 0 {
            write!(f, ", Unable to run: {}", unable_to_run)?;
        }

        Ok(())
    }
}

impl TestRunState {
    fn update(&mut self, function_name: &str, test_name: &str, state: TestState, bar: &TestRunBar) {
        match &state {
            TestState::Queued => {}
            TestState::Running => {
                self.active_tests
                    .insert((function_name.into(), test_name.into()));
                bar.start_test(function_name, test_name, &self.active_tests);
            }
            TestState::Finished(_) | TestState::UnableToRun(_) => {
                self.active_tests
                    .shift_remove(&(function_name.into(), test_name.into()));
                bar.end_test(function_name, test_name, &self.active_tests);
            }
        };

        self.test_state
            .get_mut(function_name)
            .unwrap()
            .insert(test_name.into(), state);
    }
}

pub enum TestState {
    Queued,
    Running,
    // bool is true if the test passed
    UnableToRun(String),
    Finished(TestResponse),
}

impl std::fmt::Display for TestState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TestState::Queued => write!(f, "Queued"),
            TestState::Running => write!(f, "Running"),
            TestState::UnableToRun(e) => write!(f, "{}", e.red()),
            TestState::Finished(r) => r.fmt(f),
        }
    }
}

impl TestState {
    fn _symbol(&self) -> &str {
        match self {
            TestState::Queued => "○",
            TestState::Running => "▶",
            TestState::UnableToRun(_) => "⚠",
            TestState::Finished(r) => match r.status() {
                baml_runtime::TestStatus::Pass => "✔",
                baml_runtime::TestStatus::Fail(_) => "✖",
            },
        }
    }

    fn colored_symbol(&self) -> ColoredString {
        match self {
            TestState::Queued => "○".dimmed(),
            TestState::Running => "▶".dimmed(),
            TestState::UnableToRun(_) => "⚠".red().bold(),
            TestState::Finished(r) => match r.status() {
                baml_runtime::TestStatus::Pass => "✔".green(),
                baml_runtime::TestStatus::Fail(_) => "✖".red(),
            },
        }
    }
}

impl TestCommand {
    pub fn new(runtime: BamlRuntime, filter: FilterArgs) -> TestCommand {
        TestCommand {
            runtime: Arc::from(Mutex::from(runtime)),
            filter,
        }
    }

    async fn run_state(&self, num_bars: usize) -> (TestRunBar, TestRunState) {
        let mut test_state = IndexMap::default();

        let runtime = self.runtime.lock().await;
        let mut num_tests = 0;

        for t in self.selected_tests(&runtime).1 {
            let function = t.function().name().to_string();
            let test = t.test_case().name.clone();
            test_state
                .entry(function)
                .or_insert_with(IndexMap::default)
                .insert(test, TestState::Queued);
            num_tests += 1;
        }

        (
            TestRunBar::new(num_tests, num_bars),
            TestRunState {
                test_state,
                active_tests: IndexSet::new(),
            },
        )
    }

    fn test_handler(
        &self,
        semaphore: Arc<Semaphore>,
        test: &TestCaseWalker,
        state: Arc<Mutex<TestRunState>>,
        ctx: &RuntimeContextManager,
        progress_bar: TestRunBar,
    ) -> task::JoinHandle<()> {
        let function_name = test.function().name().to_string();
        let test_name = test.test_case().name.clone();
        let ctx = ctx.clone();
        let runtime = self.runtime.clone();

        tokio::task::spawn(async move {
            // println!("Starting thread for: {} {}", function_name, test_name);
            let permit = semaphore
                .acquire()
                .await
                .expect("Failed to acquire semaphore permit");

            let result = {
                let rt = runtime.lock().await;
                // println!("Got semaphore: {} {}", function_name, test_name);
                state.lock().await.update(
                    &function_name,
                    &test_name,
                    TestState::Running,
                    &progress_bar,
                );
                // println!("Updated state: {} {}", function_name, test_name);

                let (res, _) = rt
                    .run_test(&function_name, &test_name, &ctx, Some(|_| ()))
                    .await;
                res
            };

            let test_state = match result {
                Ok(r) => TestState::Finished(r),
                Err(e) => TestState::UnableToRun(e.to_string()),
            };

            progress_bar.println(format!(
                "{} {}:{}\n{}",
                test_state.colored_symbol(),
                function_name.bold().cyan(),
                test_name,
                test_state
            ));

            state
                .lock()
                .await
                .update(&function_name, &test_name, test_state, &progress_bar);

            drop(permit);
        })
    }

    pub async fn run_parallel(&self, max_parallel: usize) -> Result<TestRunState> {
        // Start a thread pool with max_parallel threads
        // Each thread will take a test from the queue and run it
        let semaphore = Arc::new(Semaphore::new(max_parallel));

        let (bars, state) = self.run_state(1).await;
        let locked_state = Arc::new(Mutex::new(state));

        let mut handles = vec![];

        let runtime = self.runtime.lock().await;
        let (_, selected_tests) = self.selected_tests(&runtime);

        for test in selected_tests {
            // Assume we have 10 tasks to handle
            let sem_clone = semaphore.clone();

            let state_clone = locked_state.clone();
            let ctx = runtime.create_ctx_manager(
                BamlValue::String("test".to_string()),
                Some(Box::new(|path| {
                    let path = path.to_string();
                    Box::pin(async move { Ok(std::fs::read(path)?) })
                })),
            );
            let handle = self.test_handler(sem_clone, &test, state_clone, &ctx, bars.clone());

            handles.push(handle);
        }
        drop(runtime);

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        match Arc::try_unwrap(locked_state) {
            Ok(state) => Ok(state.into_inner()),
            Err(_) => panic!("Failed to get state.."),
        }
    }

    fn selected_tests<'a>(
        &'a self,
        runtime: &'a MutexGuard<'a, BamlRuntime>,
    ) -> (usize, Vec<TestCaseWalker<'a>>) {
        let mut count = 0;
        let mut selected_tests = runtime
            .internal()
            .ir()
            .walk_tests()
            .filter_map(|test| {
                if self
                    .filter
                    .matches_filters(test.function().name(), &test.test_case().name)
                {
                    count += 1;
                    Some(test)
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();

        // tests should be sorted by function name and then by test name
        selected_tests.sort_by(|a, b| {
            a.function()
                .name()
                .cmp(&b.function().name())
                .then(a.test_case().name.cmp(&b.test_case().name))
        });

        (count, selected_tests)
    }

    pub async fn print_as_list(&self, show_summary: bool) {
        let runtime = self.runtime.lock().await;
        let (total_tests, selected_tests) = self.selected_tests(&runtime);

        let summary = format!(
            "========== {}/{} tests selected ({} deselected) ==========",
            selected_tests.len(),
            total_tests,
            total_tests - selected_tests.len()
        )
        .bold();

        if show_summary {
            println!("{}", summary);
        }

        let mut last_function = None;
        for test in &selected_tests {
            if last_function.as_deref() != Some(test.function().name()) {
                last_function = Some(test.function().name());
                println!("{}", test.function().name().green());
            }
            println!("  {}", test.test_case().name);
        }

        if show_summary && selected_tests.len() > 50 {
            println!("{}", summary);
        }
    }
}
