use std::{collections::BTreeMap, time::Instant};

use anyhow::Result;
use baml_types::BamlValue;
use futures::future::{join, join_all};
use internal_baml_core::ir::repr::IntermediateRepr;

use crate::{BamlRuntime, TestResponse};

#[allow(async_fn_in_trait)]
pub trait TestExecutor {
    async fn run_all_tests(&self) -> Result<()>;
}

/// Test status.
///
/// c.f. github workflow statuses:
/// Can be one of: completed, action_required, cancelled, failure, neutral, skipped, stale, success, timed_out, in_progress, queued, requested, waiting, pending
#[derive(Debug)]
pub enum TestStatus {
    Pending,
    Running,
    Finished(Result<TestResponse>, std::time::Duration),
}

impl TestExecutor for BamlRuntime {
    async fn run_all_tests(&self) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let (futs, mut test_status_map): (Vec<_>, BTreeMap<_, _>) = self
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
                        let _ = tx.send((function_name, test_name, TestStatus::Running));
                        let (result, _) = self
                            .run_test(function_name, test_name, &ctx_manager, Some(|_| {}))
                            .await;
                        let duration = start_instant.elapsed();
                        let _ = tx.send((
                            function_name,
                            test_name,
                            TestStatus::Finished(result, duration),
                        ));
                    },
                    ((function_name, test_name), TestStatus::Pending),
                )
            })
            .unzip();

        join(join_all(futs.into_iter()), async {
            while let Some((function_name, test_name, status)) = rx.recv().await {
                if matches!(status, TestStatus::Finished(_, _)) {
                    println!(
                        "result: {}::{} finished in {:?}",
                        function_name, test_name, status
                    );
                }
                test_status_map.insert((function_name, test_name), status);

                let total_count = test_status_map.len();
                let finished_count = test_status_map
                    .values()
                    .filter(|status| matches!(status, TestStatus::Finished(_, _)))
                    .count();

                println!("finished: {} of {}", finished_count, total_count);
            }
        })
        .await;

        println!("done");

        Ok(())
    }
}
