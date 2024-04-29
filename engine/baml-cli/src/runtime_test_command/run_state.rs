use anyhow::Result;
use baml_lib::internal_baml_core::ir::TestCaseWalker;
use colored::*;
use indicatif::{MultiProgress, ProgressBar};
use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{sync::Semaphore, task};

use baml_runtime::{internal::WithInternal, BamlRuntime, RuntimeContext, TestResponse};

use super::filter::FilterArgs;

pub(super) struct TestCommand<'a> {
    // Number of tests total in the runtime
    total_tests: usize,

    // Number of tests selected by the filter
    selected_tests: Vec<TestCaseWalker<'a>>,
}

pub(super) struct TestRunState {
    // Test state for each test
    // <Function, <Test, State>>
    test_state: HashMap<String, HashMap<String, TestState>>,

    progress_bar: MultiProgress,
    bars: HashMap<String, ProgressBar>,
}

impl TestRunState {
    fn update(&mut self, function_name: &str, test_name: &str, state: TestState) {
        let msg: Option<(bool, Option<String>)> = match &state {
            TestState::Queued | TestState::Running => None,
            TestState::UnableToRun(message) => Some((false, Some(message.clone()))),
            TestState::Finished(s) => match s.status() {
                baml_runtime::TestStatus::Pass => Some((true, None)),
                baml_runtime::TestStatus::Fail(err) => match err {
                    baml_runtime::TestFailReason::TestUnspecified(msg) => {
                        Some((false, Some(format!("{:?}", msg))))
                    }
                    baml_runtime::TestFailReason::TestParseFailure(e) => {
                        Some((false, Some(format!("{:?}", e))))
                    }
                    baml_runtime::TestFailReason::TestLLMFailure(llm_fail) => Some((
                        false,
                        Some(match llm_fail.content() {
                            Ok(k) => k.to_string(),
                            Err(e) => format!("{:?}", e),
                        }),
                    )),
                },
            },
        };

        if let Some((passed, msg)) = msg {
            let bar = self.bars.get_mut(function_name).unwrap();
            bar.inc(1);
            let test_line = format!(
                "{} {}:{}",
                if passed { "✔".green() } else { "✖".red() },
                function_name.bold().cyan(),
                test_name,
            );

            if let Some(msg) = msg {
                bar.println(format!(
                    "{test_line}\n{msg}\n\n",
                    msg = msg,
                    test_line = test_line
                ));
            } else {
                bar.println(test_line);
            }
        }

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

impl<'runtime> TestCommand<'runtime> {
    pub fn new<'a>(runtime: &'a Arc<BamlRuntime>, test_filter: &FilterArgs) -> TestCommand<'a> {
        let mut count = 0;
        let mut selected_tests = runtime
            .walk_tests()
            .filter_map(|test| {
                if test_filter.matches_filters(test.function().name(), &test.test_case().name) {
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

        TestCommand {
            total_tests: count,
            selected_tests,
        }
    }

    fn run_state(&self) -> TestRunState {
        let mut test_state = HashMap::new();

        for t in &self.selected_tests {
            let function = t.function().name().to_string();
            let test = t.test_case().name.clone();
            test_state
                .entry(function)
                .or_insert_with(HashMap::new)
                .insert(test, TestState::Queued);
        }

        let p = MultiProgress::new();
        let mut bars = HashMap::new();
        for (func_name, tests) in &test_state {
            let bar = ProgressBar::new(tests.len() as u64)
                .with_prefix(func_name.clone())
                .with_style(
                    indicatif::ProgressStyle::with_template("{spinner} {pos}/{len} {prefix}")
                        .unwrap(),
                );

            let bar = p.add(bar);
            bars.insert(func_name.clone(), bar);
        }

        TestRunState {
            test_state,
            progress_bar: p,
            bars,
        }
    }

    fn test_handler(
        &'runtime self,
        semaphore: Arc<Semaphore>,
        runtime: Arc<BamlRuntime>,
        test: &'runtime TestCaseWalker,
        env_vars: &HashMap<String, String>,
        state: Arc<Mutex<TestRunState>>,
    ) -> task::JoinHandle<()> {
        let function_name = test.function().name().to_string();
        let test_name = test.test_case().name.clone();

        let ctx = RuntimeContext {
            env: env_vars.clone(),
            tags: Default::default(),
        };

        tokio::task::spawn(async move {
            let permit = semaphore
                .acquire()
                .await
                .expect("Failed to acquire semaphore permit");

            state
                .lock()
                .unwrap()
                .update(&function_name, &test_name, TestState::Running);

            let result = runtime.run_test(&function_name, &test_name, &ctx).await;

            let test_state = match result {
                Ok(r) => TestState::Finished(r),
                Err(e) => TestState::UnableToRun(e.to_string()),
            };
            state
                .lock()
                .unwrap()
                .update(&function_name, &test_name, test_state);

            // Permit is automatically dropped and returned to the semaphore here
            drop(permit);
        })
    }

    pub async fn run_parallel(
        &'runtime self,
        max_parallel: usize,
        runtime: Arc<BamlRuntime>,
        env_vars: &HashMap<String, String>,
    ) -> Result<TestRunState> {
        // Start a thread pool with max_parallel threads
        // Each thread will take a test from the queue and run it
        let semaphore = Arc::new(Semaphore::new(max_parallel));

        let locked_state = Arc::new(Mutex::new(self.run_state()));

        let mut handles = vec![];

        for test in self.selected_tests.iter() {
            // Assume we have 10 tasks to handle
            let sem_clone = semaphore.clone();

            let state_clone = locked_state.clone();

            let handle = self.test_handler(sem_clone, runtime.clone(), test, env_vars, state_clone);

            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        match Arc::try_unwrap(locked_state) {
            Ok(state) => Ok(state.into_inner().unwrap()),
            Err(_) => panic!("Failed to get state.."),
        }
    }

    pub fn print_as_list(&self, show_summary: bool) {
        let summary = format!(
            "========== {}/{} tests selected ({} deselected) ==========",
            self.selected_tests.len(),
            self.total_tests,
            self.total_tests - self.selected_tests.len()
        )
        .bold();

        if show_summary {
            println!("{}", summary);
        }

        let mut last_function = None;
        for test in &self.selected_tests {
            if last_function.as_deref() != Some(test.function().name()) {
                last_function = Some(test.function().name());
                println!("{}", test.function().name().green());
            }
            println!("  {}", test.test_case().name);
        }

        if show_summary && self.selected_tests.len() > 50 {
            println!("{}", summary);
        }
    }
}
