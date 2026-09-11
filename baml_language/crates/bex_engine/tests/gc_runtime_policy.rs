#![cfg(feature = "gc_policy_experiments")]

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as Ext, FunctionCallContextBuilder};
use bex_heap::{CollectionLevel, gc_experiment::GcExperimentConfig};
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Node { value int }
function Tiny(n: int) -> Node { Node { value: n } }
function Read(node: Node) -> int { node.value }
function Churn(n: int) -> int {
    let i = 0;
    let total = 0;
    while (i < n) { let node = Node { value: i }; total = total + node.value; i = i + 1; }
    total
}
function Compute(n: int) -> int {
    let i = 0;
    while (i < n) { i = i + 1; }
    i
}
function ChurnAndWait() -> int {
    let n = Churn(50000);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(30000n));
    n
}
function Empty() -> int[] { [] }
function Grow(values: int[], n: int) -> int {
    let i = 0;
    while (i < n) { values.push(i); i = i + 1; }
    values.length()
}
function Size(values: int[]) -> int { values.length() }
function Text(text: string) -> string { text }
function TextSize(text: string) -> int { text.length() }
function Async(n: int) -> int {
    let values = Empty();
    Grow(values, n);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1n));
    Size(values)
}
"#;

fn engine(young_budget: Option<usize>, full_budget_floor: usize) -> Arc<BexEngine> {
    engine_with_basis(young_budget, full_budget_floor, false)
}

fn engine_with_basis(
    young_budget: Option<usize>,
    full_budget_floor: usize,
    live_budget_uses_slots: bool,
) -> Arc<BexEngine> {
    let engine = Arc::new(
        BexEngine::new(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    );
    engine.heap().configure_gc_experiment(GcExperimentConfig {
        adaptive: None,
        young_budget,
        full_budget_floor,
        live_multiplier: 1,
        live_budget_uses_slots,
        first_chunk: 32,
        max_chunk: 1024,
        poll_interval: 64,
    });
    engine
}

async fn call(engine: &Arc<BexEngine>, name: &str, args: Vec<Ext>, copy: bool) -> Ext {
    engine
        .call_function(
            name,
            args,
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .suppress_internal_profile()
                .build(),
            copy,
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn short_calls_service_pressure_after_export() {
    let engine = engine(None, 1024);
    for n in 0..100 {
        let result = call(&engine, "Tiny", vec![Ext::Int(n)], true).await;
        let Ext::Instance { fields, .. } = result else {
            panic!("expected copied instance")
        };
        assert_eq!(fields["value"], Ext::Int(n));
        assert_eq!(engine.heap_stats().reserved_slots, 0);
        assert!(engine.heap().should_collect().is_none());
    }
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            >= 100
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn long_compute_collects_before_returning() {
    let engine = engine(None, 64 * 1024);
    let n = 50_000;
    assert_eq!(
        call(&engine, "Churn", vec![Ext::Int(n)], true).await,
        Ext::Int(n * (n - 1) / 2)
    );
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            > 10
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn existing_old_array_growth_is_charged_and_survives() {
    let engine = engine(Some(32 * 1024), 128 * 1024);
    let values = call(&engine, "Empty", vec![], false).await;
    engine.collect_garbage(CollectionLevel::Major).await;
    let before = engine.heap().gc_experiment().unwrap().snapshot();
    assert_eq!(
        call(
            &engine,
            "Grow",
            vec![values.clone(), Ext::Int(50_000)],
            true
        )
        .await,
        Ext::Int(50_000)
    );
    let after = engine.heap().gc_experiment().unwrap().snapshot();
    assert!(
        after.total_charged - before.total_charged >= 50_000 * size_of::<bex_vm_types::Value>()
    );
    assert!(after.collections > before.collections);
    assert_eq!(
        call(&engine, "Size", vec![values], true).await,
        Ext::Int(50_000)
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn large_payloads_collect_even_with_few_objects() {
    let engine = engine_with_basis(None, 4 * 1024 * 1024, true);
    let mut kept = std::collections::VecDeque::new();
    for _ in 0..20 {
        kept.push_back(
            call(
                &engine,
                "Text",
                vec![Ext::String("x".repeat(1024 * 1024).into())],
                false,
            )
            .await,
        );
        if kept.len() > 2 {
            kept.pop_front();
        }
    }
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            >= 4
    );
    engine.collect_garbage(CollectionLevel::Major).await;
    for value in kept {
        assert_eq!(
            call(&engine, "TextSize", vec![value], true).await,
            Ext::Int(1024 * 1024)
        );
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_share_pressure_and_preserve_parked_roots() {
    let engine = engine(Some(16 * 1024), 64 * 1024);
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let engine = engine.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..16 {
                assert_eq!(
                    call(&engine, "Async", vec![Ext::Int(1024)], true).await,
                    Ext::Int(1024)
                );
            }
        }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        for task in tasks {
            task.await.unwrap();
        }
    })
    .await
    .expect("pressure collection must not deadlock parked callers");
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            > 1
    );
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_collection_parks_compute_without_allocation_pressure() {
    let engine = engine(None, usize::MAX);
    let running_engine = engine.clone();
    let task = tokio::spawn(async move {
        call(
            &running_engine,
            "Compute",
            vec![Ext::Int(100_000_000)],
            true,
        )
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(5),
        engine.collect_garbage(CollectionLevel::Major),
    )
    .await
    .expect("explicit GC must park a VM even when allocation pressure is absent");
    assert!(
        !task.is_finished(),
        "GC must complete before the compute loop finishes"
    );
    assert_eq!(task.await.unwrap(), Ext::Int(100_000_000));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancellation_after_pressure_does_not_latch_the_checker() {
    let engine = engine(None, 64 * 1024);
    let token = bex_engine::CancellationToken::new();
    let running_token = token.clone();
    let running_engine = engine.clone();
    let task = tokio::spawn(async move {
        running_engine
            .call_function(
                "ChurnAndWait",
                vec![],
                FunctionCallContextBuilder::new(sys_types::CallId::next())
                    .with_cancel_token(running_token)
                    .suppress_internal_profile()
                    .build(),
                true,
            )
            .await
    });
    // Cancellation is observed at sys-op/await boundaries, not GC EarlyYield.
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            < 5
        {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("call should collect while allocating before its cancellable sys-op");
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    assert!(bex_engine::is_cancelled_engine_error(&result.unwrap_err()));
    let before = engine
        .heap()
        .gc_experiment()
        .unwrap()
        .snapshot()
        .collections;
    assert_eq!(
        call(&engine, "Churn", vec![Ext::Int(50_000)], true).await,
        Ext::Int(50_000 * 49_999 / 2)
    );
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            > before
    );
    engine.shutdown().await;
}

fn adaptive_engine() -> Arc<BexEngine> {
    let engine = Arc::new(
        BexEngine::new(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    );
    engine.heap().configure_gc_experiment(GcExperimentConfig {
        adaptive: Some(bex_heap::gc_adaptive::AdaptiveConfig {
            gen1_budget_floor: 16 * 1024,
            growth_percent: 25,
            gen1_mode: bex_heap::gc_adaptive::Gen1Mode::Minor,
            large_payload_threshold: None,
            survival_backoff: 0,
            survival_early_probe: false,
            gc_time_percent: 5,
        }),
        young_budget: Some(8 * 1024),
        full_budget_floor: 32 * 1024,
        live_multiplier: 2,
        live_budget_uses_slots: false,
        first_chunk: 32,
        max_chunk: 1024,
        poll_interval: 64,
    });
    engine
}

#[tokio::test]
async fn adaptive_policy_uses_all_three_scopes_and_preserves_exported_results() {
    let engine = adaptive_engine();
    let mut kept = Vec::new();
    for n in 0..1500 {
        kept.push(call(&engine, "Tiny", vec![Ext::Int(n)], false).await);
    }
    let snapshot = engine
        .heap()
        .gc_experiment()
        .unwrap()
        .snapshot()
        .adaptive
        .unwrap();
    assert!(snapshot.collections.iter().all(|n| *n > 0), "{snapshot:?}");
    for (n, value) in kept.iter().enumerate().step_by(31) {
        assert_eq!(
            call(&engine, "Read", vec![value.clone()], true).await,
            Ext::Int(i64::try_from(n).unwrap())
        );
    }
    drop(kept);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn adaptive_policy_cooperates_with_many_short_lived_vms() {
    let engine = adaptive_engine();
    let mut tasks = Vec::new();
    for _ in 0..8 {
        let engine = engine.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..16 {
                assert_eq!(
                    call(&engine, "Async", vec![Ext::Int(1024)], true).await,
                    Ext::Int(1024)
                );
            }
        }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        for task in tasks {
            task.await.unwrap();
        }
    })
    .await
    .expect("adaptive collections must not deadlock");
    assert!(
        engine
            .heap()
            .gc_experiment()
            .unwrap()
            .snapshot()
            .collections
            > 1
    );
    engine.shutdown().await;
}
