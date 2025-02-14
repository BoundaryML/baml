use std::time::Instant;

use anyhow::Result;
use baml_types::BamlValue;
use futures::future::{join, join_all};
use internal_baml_core::ir::repr::IntermediateRepr;

use crate::BamlRuntime;

#[allow(async_fn_in_trait)]
pub trait TestExecutor {
    async fn run_all_tests(&self) -> Result<()>;
}

impl TestExecutor for BamlRuntime {
    async fn run_all_tests(&self) -> Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();

        let futs = self
            .inner
            .ir
            .walk_tests()
            .take(3)
            .map(|node_pair| {
                let (function_name, test_name) = node_pair.name();

                let tx = tx.clone();

                async move {
                    let ctx_manager =
                        self.create_ctx_manager(BamlValue::String("cli".to_string()), None);

                    let start_instant = Instant::now();
                    let (result, _) = self
                        .run_test(function_name, test_name, &ctx_manager, Some(|_| {}))
                        .await;
                    let duration = start_instant.elapsed();
                    let _ = tx.send((function_name, test_name, result, duration));
                }
            })
            .collect::<Vec<_>>();

        let test_count = futs.len();

        join(join_all(futs.into_iter()), async {
            let mut result_count = 0;

            while let Some((function_name, test_name, result, duration)) = rx.recv().await {
                result_count += 1;
                println!(
                    "result: {}::{} finished in {:?}",
                    function_name, test_name, duration
                );
                println!("finished: {} of {}", result_count, test_count);
            }
        })
        .await;

        println!("done");

        Ok(())
    }
}
