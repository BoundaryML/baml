use anyhow::Result;
use baml_lib::internal_baml_core::ir::TestCaseWalker;
use std::{
    borrow::BorrowMut,
    collections::HashMap,
    sync::{Arc, Mutex},
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
    // Function, Test, Impl
    test_state: HashMap<TestSpecification, TestState>,
}

impl TestRunState {
    pub fn update(&mut self, function_name: &str, test_name: &str, state: TestState) {
        self.test_state.insert(
            TestSpecification {
                function: function_name.into(),
                test: test_name.into(),
            },
            state,
        );
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct TestSpecification {
    function: String,
    test: String,
}

pub enum TestState {
    Queued,
    Running,
    Cancelled,
    // bool is true if the test passed
    UnableToRun(String),
    Finished(TestResponse),
}

impl<'runtime> TestCommand<'runtime> {
    pub fn new<'a>(runtime: &'a Arc<BamlRuntime>, test_filter: &FilterArgs) -> TestCommand<'a> {
        let mut count = 0;
        let selected_tests = runtime
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

        TestCommand {
            total_tests: count,
            selected_tests,
        }
    }

    fn run_state(&self) -> TestRunState {
        TestRunState {
            test_state: self
                .selected_tests
                .iter()
                .map(|test| {
                    let function = test.function().name().into();
                    let test = test.test_case().name.clone();
                    (TestSpecification { function, test }, TestState::Queued)
                })
                .collect(),
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
}
