mod output_github;
mod output_pretty;
mod test_execution_args;

pub use test_execution_args::TestFilter;

use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Deref,
    time::Instant,
};

use anyhow::Result;
use baml_types::BamlValue;
use futures::future::join_all;
use futures::join;
use internal_baml_core::ir::repr::IntermediateRepr;
use regex::Regex;
use tokio::sync::{Mutex, MutexGuard};

use crate::{BamlRuntime, TestResponse, TestStatus};

#[allow(async_fn_in_trait)]
pub trait TestExecutor {
    fn cli_list_tests(&self, args: &TestFilter) -> Result<()>;
    async fn cli_run_tests(&self) -> Result<()>;
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
    /// cargo test makes an expplicit distinction between "marked with #[ignore]" and "excluded by cargo test flags"
    Excluded,
}

type TestExecutionStatusMap<'a, 'b> = BTreeMap<(&'a str, &'b str), TestExecutionStatus>;

pub(super) trait RenderTestExecutionStatus {
    fn render_progress(&self, test_status_map: &TestExecutionStatusMap);

    fn render_final(&self, test_status_map: &TestExecutionStatusMap);
}

impl TestExecutor for BamlRuntime {
    fn cli_list_tests(&self, args: &TestFilter) -> Result<()> {
        let func_test_pairs = self
            .inner
            .ir
            .walk_tests()
            .filter_map(|node_pair| {
                let (function_name, test_name) = node_pair.name();
                if args.includes(function_name, test_name) {
                    Some((function_name, test_name))
                } else {
                    None
                }
            })
            .collect::<BTreeSet<_>>();

        for (function_name, test_name) in func_test_pairs {
            println!("{}::{}", function_name, test_name);
        }

        Ok(())
    }

    async fn cli_run_tests(&self) -> Result<()> {
        let output_renderer = output_github::GithubTestExecutionStatusRenderer {};

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let (futs, test_status_map): (Vec<_>, BTreeMap<_, _>) = self
            .inner
            .ir
            .walk_tests()
            .take(3)
            .map(|node_pair| {
                let (function_name, test_name) = node_pair.name();

                let tx = tx.clone();

                (
                    async move {
                        let ctx_manager =
                            self.create_ctx_manager(BamlValue::String("cli".to_string()), None);

                        let start_instant = Instant::now();
                        let _ = tx.send((function_name, test_name, TestExecutionStatus::Running));
                        let (result, _) = self
                            .run_test(function_name, test_name, &ctx_manager, Some(|_| {}))
                            .await;
                        let duration = start_instant.elapsed();
                        let _ = tx.send((
                            function_name,
                            test_name,
                            TestExecutionStatus::Finished(result, duration),
                        ));
                    },
                    ((function_name, test_name), TestExecutionStatus::Pending),
                )
            })
            .unzip();

        let test_status_locked = Mutex::new(test_status_map);

        join!(
            join_all(futs.into_iter()),
            async {
                while let Some((function_name, test_name, status)) = rx.recv().await {
                    let mut test_status_map = test_status_locked.lock().await;

                    test_status_map.insert((function_name, test_name), status);
                    output_renderer.render_progress(test_status_map.deref());

                    let total_count = test_status_map.len();
                    let finished_count = test_status_map
                        .values()
                        .filter(|status| matches!(status, TestExecutionStatus::Finished(_, _)))
                        .count();

                    println!("finished: {} of {}", finished_count, total_count);

                    if finished_count == total_count {
                        break;
                    }
                }
            },
            async {
                loop {
                    {
                        let test_status_map = test_status_locked.lock().await;
                        let finished_count = test_status_map
                            .values()
                            .filter(|status| matches!(status, TestExecutionStatus::Finished(_, _)))
                            .count();
                        let total_count = test_status_map.len();

                        if finished_count == total_count {
                            break;
                        }

                        output_renderer.render_progress(test_status_map.deref());
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                }
            }
        );

        let test_status_map = test_status_locked.into_inner();

        output_renderer.render_final(&test_status_map);
        println!("done");

        Ok(())
    }
}
