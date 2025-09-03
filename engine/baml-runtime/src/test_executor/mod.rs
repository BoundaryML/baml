use colored::*;
mod output_github;
mod output_junit;
mod output_pretty;
mod test_execution_args;

use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    ops::Deref,
    sync::Arc,
    time::Instant,
};

use anyhow::Result;
use baml_types::BamlValue;
use futures::{future::join_all, join};
use internal_baml_core::ir::repr::IntermediateRepr;
use regex::Regex;
pub use test_execution_args::TestFilter;
use tokio::sync::{Mutex, MutexGuard};

use crate::{BamlRuntime, TestResponse, TestStatus, TripWire};

pub enum TestRunStatus {
    /// No tests were selected.
    NoTests,
    /// All tests passed.
    Passed,
    /// Some tests need human evaluation.
    NeedsEval,
    /// Some tests failed or aborted.
    Failed(usize),
    /// The tests were cancelled.
    Cancelled,
}

#[allow(async_fn_in_trait)]
pub trait TestExecutor {
    fn cli_list_tests(&self, args: &TestFilter) -> Result<()>;
    async fn cli_run_tests(
        self: std::sync::Arc<Self>,
        args: &TestFilter,
        max_concurrency: usize,
        output_format: &crate::cli::testing::OutputFormat,
        junit_path: Option<&String>,
        env_vars: &HashMap<String, String>,
    ) -> TestRunStatus;
}

/// Test status.
///
/// c.f. github workflow statuses:
/// Can be one of: completed, action_required, cancelled, failure, neutral, skipped, stale, success, timed_out, in_progress, queued, requested, waiting, pending
#[derive(Debug)]
pub enum TestExecutionStatus {
    Pending,
    Running,
    Finished(Result<TestResponse>, std::time::Duration),
    /// We say "excluded" instead of "skipped" as inspired by cargo, and for consistency with --exclude.
    /// cargo test makes an explicit distinction between "marked with #[ignore]" and "excluded by cargo test flags"
    Excluded,
}

impl TestExecutionStatus {
    pub fn is_failed(&self) -> bool {
        match self {
            TestExecutionStatus::Finished(Err(_), _) => true,
            TestExecutionStatus::Finished(Ok(t), _) => matches!(t.status(), TestStatus::Fail(_)),
            _ => false,
        }
    }
}

type TestExecutionStatusMap = BTreeMap<(String, String), TestExecutionStatus>;

pub(super) trait RenderTestExecutionStatus {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap);

    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        selected_tests: &BTreeMap<(String, String), String>,
    );
}

struct AggregateRenderer {
    renderers: Vec<Box<dyn RenderTestExecutionStatus>>,
}

impl AggregateRenderer {
    fn new(output_format: &crate::cli::testing::OutputFormat, junit_path: Option<&String>) -> Self {
        let mut renderers: Vec<Box<dyn RenderTestExecutionStatus>> = match output_format {
            crate::cli::testing::OutputFormat::Pretty => vec![Box::new(
                output_pretty::PrettyTestExecutionStatusRenderer::new(),
            )],
            crate::cli::testing::OutputFormat::Github => vec![Box::new(
                output_github::GithubTestExecutionStatusRenderer::new(),
            )],
        };

        if let Some(junit_path) = junit_path {
            renderers.push(Box::new(output_junit::JUnitXMLRenderer::new(
                junit_path.as_str(),
            )));
        }

        Self { renderers }
    }
}

impl RenderTestExecutionStatus for AggregateRenderer {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap) {
        for renderer in self.renderers.iter() {
            renderer.render_progress(test_status_map);
        }
    }

    fn render_final(
        &self,
        test_status_map: &TestExecutionStatusMap,
        selected_tests: &BTreeMap<(String, String), String>,
    ) {
        for renderer in self.renderers.iter() {
            renderer.render_final(test_status_map, selected_tests);
        }
    }
}

async fn file_reader(path: String) -> Result<Vec<u8>> {
    let file_path = async_std::path::PathBuf::from(&path);
    let file_content = async_std::fs::read(file_path).await?;
    Ok(file_content)
}

fn file_reader_pinned(
    path: &str,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, anyhow::Error>> + Send>> {
    Box::pin(file_reader(path.to_string()))
}

impl TestExecutor for BamlRuntime {
    fn cli_list_tests(&self, args: &TestFilter) -> Result<()> {
        let func_test_pairs = {
            let ir = &self.inner.ir;
            // Regular LLM function tests
            let from_fn_tests = ir.walk_function_test_pairs().filter_map(|node_pair| {
                let (function_name, test_name) = node_pair.name();
                if args.includes(function_name, test_name) {
                    Some((function_name.to_string(), test_name.to_string()))
                } else {
                    None
                }
            });

            // Expr function tests (pretending as functions)
            let expr_fns = internal_baml_core::ir::ExprFnAsFunctionWalker::new(ir);
            let expr_fn_tests_owned: Vec<(String, String)> = expr_fns
                .walk_functions()
                .flat_map(|f| {
                    f.walk_tests()
                        .filter_map(|node_pair| {
                            let (function_name, test_name) = node_pair.name();
                            if args.includes(function_name, test_name) {
                                Some((function_name.to_string(), test_name.to_string()))
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            from_fn_tests
                .chain(expr_fn_tests_owned)
                .collect::<BTreeSet<_>>()
        };

        println!("Found {} tests", func_test_pairs.len());

        if !func_test_pairs.is_empty() {
            println!("{}", "--------------------------------".dimmed());
            for (function_name, test_name) in func_test_pairs {
                println!("{function_name}::{test_name}");
            }
            println!("{}", "--------------------------------".dimmed());

            println!(
                "{}",
                "To run these tests, rerun without the --list arg:".blue()
            );
            println!("{}", "baml-cli test [args]".blue());
        }

        Ok(())
    }

    async fn cli_run_tests(
        self: std::sync::Arc<Self>,
        args: &TestFilter,
        max_concurrency: usize,
        output_format: &crate::cli::testing::OutputFormat,
        junit_path: Option<&String>,
        env_vars: &HashMap<String, String>,
    ) -> TestRunStatus {
        let renderer = AggregateRenderer::new(output_format, junit_path);
        let selected_tests = {
            let ir = &self.inner.ir;
            // Regular LLM function tests
            let from_fn_tests = ir.walk_function_test_pairs().filter_map(|node_pair| {
                let (function_name, test_name) = node_pair.name();
                if args.includes(function_name, test_name) {
                    node_pair.span().map(|s| {
                        (
                            (function_name.to_string(), test_name.to_string()),
                            format!("{}:{}", s.file.path(), s.line_and_column().0 .0 + 1),
                        )
                    })
                } else {
                    None
                }
            });

            // Expr function tests (pretending as functions)
            let expr_fns = internal_baml_core::ir::ExprFnAsFunctionWalker::new(ir);
            let expr_fn_tests_owned: Vec<((String, String), String)> = expr_fns
                .walk_functions()
                .flat_map(|f| {
                    f.walk_tests()
                        .filter_map(|node_pair| {
                            let (function_name, test_name) = node_pair.name();
                            if args.includes(function_name, test_name) {
                                node_pair.span().map(|s| {
                                    (
                                        (function_name.to_string(), test_name.to_string()),
                                        format!(
                                            "{}:{}",
                                            s.file.path(),
                                            s.line_and_column().0 .0 + 1
                                        ),
                                    )
                                })
                            } else {
                                None
                            }
                        })
                        .collect::<Vec<_>>()
                })
                .collect();

            from_fn_tests
                .chain(expr_fn_tests_owned.into_iter())
                .collect::<BTreeMap<_, _>>()
        };

        if selected_tests.is_empty() {
            println!("No tests selected");
            return TestRunStatus::NoTests;
        }

        let semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_concurrency));

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        // Build futures and initial test status map.
        let (futs, test_status_map): (Vec<_>, BTreeMap<_, _>) = selected_tests
            .iter()
            .map(|((fn_name, tt_name), _)| {
                let semaphore = semaphore.clone();
                let tx = tx.clone();
                // Clone the Arc pointer for self here.
                let runtime = self.clone();
                let function_name = fn_name.to_string();
                let test_name = tt_name.to_string();
                let env_vars = env_vars.clone();
                let fut = tokio::spawn(async move {
                    let _permit = semaphore.acquire().await.unwrap();
                    let ctx_manager = runtime.create_ctx_manager(
                        BamlValue::String("cli".to_string()),
                        Some(Box::new(file_reader_pinned)),
                    );

                    let start_instant = Instant::now();
                    let _ = tx.send((
                        function_name.clone(),
                        test_name.clone(),
                        TestExecutionStatus::Running,
                    ));
                    let on_tick = if false { Some(|| {}) } else { None };
                    let on_event = if false { Some(|_| {}) } else { None };
                    let (result, _) = runtime
                        .run_test(
                            &function_name,
                            &test_name,
                            &ctx_manager,
                            on_event,
                            None,
                            env_vars,
                            TripWire::new(None), // No tripwire for test executor,
                            on_tick,
                        )
                        .await;
                    let duration = start_instant.elapsed();
                    let _ = tx.send((
                        function_name,
                        test_name,
                        TestExecutionStatus::Finished(result, duration),
                    ));
                });
                (
                    fut,
                    (
                        (fn_name.to_string(), tt_name.to_string()),
                        TestExecutionStatus::Pending,
                    ),
                )
            })
            .unzip();

        let test_status_locked = Mutex::new(test_status_map);

        let tests_future = async {
            join!(
                join_all(futs.into_iter()),
                async {
                    while let Some((function_name, test_name, status)) = rx.recv().await {
                        let mut status_map = test_status_locked.lock().await;
                        status_map
                            .insert((function_name.to_string(), test_name.to_string()), status);
                        renderer.render_progress(status_map.deref());

                        let total_count = status_map.len();
                        let finished_count = status_map
                            .values()
                            .filter(|status| matches!(status, TestExecutionStatus::Finished(_, _)))
                            .count();

                        if finished_count == total_count {
                            break;
                        }
                    }
                },
                async {
                    loop {
                        {
                            let status_map = test_status_locked.lock().await;
                            let finished_count = status_map
                                .values()
                                .filter(|status| {
                                    matches!(status, TestExecutionStatus::Finished(_, _))
                                })
                                .count();
                            let total_count = status_map.len();

                            if finished_count == total_count {
                                break;
                            }
                            renderer.render_progress(status_map.deref());
                        }
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    }
                }
            );
        };

        let ctrl_c_future = async {
            tokio::signal::ctrl_c()
                .await
                .expect("Failed to listen for Ctrl+C");
            println!("\nCtrl+C received. Cancelling remaining tests...");
            Ok::<(), anyhow::Error>(())
        };

        let res = tokio::select! {
            _ = tests_future => { Ok(())},
            _ = ctrl_c_future => { Err(1) },
        };

        let final_status = test_status_locked.into_inner();
        renderer.render_final(&final_status, &selected_tests);

        match res {
            Ok(_) => {
                let failed_count = final_status
                    .values()
                    .filter(|status| status.is_failed())
                    .count();
                if failed_count > 0 {
                    TestRunStatus::Failed(failed_count)
                } else {
                    TestRunStatus::Passed
                }
            }
            Err(_) => TestRunStatus::Cancelled,
        }
    }
}
