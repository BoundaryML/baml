use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as Ext, FunctionCallContextBuilder};
use bex_heap::CollectionLevel;
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
    let n = Churn(1000000);
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
function At(values: int[], i: int) -> int { values[i] }
function Text(text: string) -> string { text }
function TextSize(text: string) -> int { text.length() }
function Async(n: int) -> int {
    let values = Empty();
    Grow(values, n);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1n));
    Size(values)
}
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            baml_db::testing::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
        )
        .unwrap(),
    )
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
async fn short_calls_service_pressure_on_next_entry() {
    let engine = engine();
    for n in 0..30_000 {
        let result = call(&engine, "Tiny", vec![Ext::Int(n)], true).await;
        let Ext::Instance { fields, .. } = result else {
            panic!("expected copied instance")
        };
        assert_eq!(fields["value"], Ext::Int(n));
    }
    assert!(engine.heap().gc_budget().full_collections >= 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn long_compute_collects_before_returning() {
    let engine = engine();
    let n = 1_000_000;
    assert_eq!(
        call(&engine, "Churn", vec![Ext::Int(n)], true).await,
        Ext::Int(n * (n - 1) / 2)
    );
    assert!(engine.heap().gc_budget().full_collections > 1);
    engine.shutdown().await;
}

#[tokio::test]
async fn existing_old_array_growth_is_charged_and_survives() {
    let engine = engine();
    let values = call(&engine, "Empty", vec![], false).await;
    engine.collect_garbage(CollectionLevel::Major).await;
    let before = engine.heap().gc_budget();
    assert_eq!(
        call(
            &engine,
            "Grow",
            vec![values.clone(), Ext::Int(2_100_000)],
            true
        )
        .await,
        Ext::Int(2_100_000)
    );
    let after = engine.heap().gc_budget();
    assert!(after.full_collections > before.full_collections);
    assert_eq!(
        call(&engine, "Size", vec![values.clone()], true).await,
        Ext::Int(2_100_000)
    );
    for i in [0, 12345, 2_099_999] {
        assert_eq!(
            call(&engine, "At", vec![values.clone(), Ext::Int(i)], true).await,
            Ext::Int(i)
        );
    }
    engine.shutdown().await;
}

#[tokio::test]
async fn large_payloads_collect_even_with_few_objects() {
    let engine = engine();
    let mut kept = std::collections::VecDeque::new();
    for _ in 0..20 {
        kept.push_back(
            call(
                &engine,
                "Text",
                vec![Ext::String("x".repeat(4 * 1024 * 1024).into())],
                false,
            )
            .await,
        );
        if kept.len() > 2 {
            kept.pop_front();
        }
    }
    assert!(engine.heap().gc_budget().full_collections >= 2);
    engine.collect_garbage(CollectionLevel::Major).await;
    for value in kept {
        assert_eq!(
            call(&engine, "TextSize", vec![value], true).await,
            Ext::Int(4 * 1024 * 1024)
        );
    }
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_callers_share_pressure_and_preserve_parked_roots() {
    let engine = engine();
    let mut tasks = Vec::new();
    for _ in 0..4 {
        let engine = engine.clone();
        tasks.push(tokio::spawn(async move {
            for _ in 0..8 {
                assert_eq!(
                    call(&engine, "Async", vec![Ext::Int(128_000)], true).await,
                    Ext::Int(128_000)
                );
            }
        }));
    }
    tokio::time::timeout(std::time::Duration::from_secs(120), async {
        for task in tasks {
            task.await.unwrap();
        }
    })
    .await
    .expect("pressure collection must not deadlock parked callers");
    assert!(engine.heap().gc_budget().full_collections >= 1);
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn explicit_collection_parks_compute_without_allocation_pressure() {
    let engine = engine();
    let running_engine = engine.clone();
    let task = tokio::spawn(async move {
        call(&running_engine, "Compute", vec![Ext::Int(10_000_000)], true).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    tokio::time::timeout(
        std::time::Duration::from_secs(30),
        engine.collect_garbage(CollectionLevel::Major),
    )
    .await
    .expect("explicit GC must park a VM even when allocation pressure is absent");
    assert!(
        !task.is_finished(),
        "GC must complete before the compute loop finishes"
    );
    assert_eq!(task.await.unwrap(), Ext::Int(10_000_000));
    engine.shutdown().await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn cancellation_after_pressure_does_not_latch_the_checker() {
    let engine = engine();
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
    tokio::time::timeout(std::time::Duration::from_secs(30), async {
        while engine.heap().gc_budget().full_collections < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(1)).await;
        }
    })
    .await
    .expect("call should collect while allocating before its cancellable sys-op");
    token.cancel();
    let result = tokio::time::timeout(std::time::Duration::from_secs(30), task)
        .await
        .unwrap()
        .unwrap();
    assert!(bex_engine::is_cancelled_engine_error(&result.unwrap_err()));
    let before = engine.heap().gc_budget().full_collections;
    assert_eq!(
        call(&engine, "Churn", vec![Ext::Int(2_100_000)], true).await,
        Ext::Int(2_100_000 * 2_099_999 / 2)
    );
    assert!(engine.heap().gc_budget().full_collections > before);
    engine.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn completed_bridge_call_leaves_debt_for_next_entry_and_preserves_its_handle() {
    let engine = engine();
    let bytes = 40 * 1024 * 1024;
    let retained = call(
        &engine,
        "Text",
        vec![Ext::String("x".repeat(bytes).into())],
        false,
    )
    .await;
    assert_eq!(engine.heap().gc_budget().full_collections, 0);
    assert!(
        engine.heap().should_gc(),
        "completion must leave debt unpaid"
    );
    assert_eq!(
        call(&engine, "TextSize", vec![retained], true).await,
        Ext::Int(i64::try_from(bytes).unwrap())
    );
    assert_eq!(engine.heap().gc_budget().full_collections, 1);
    engine.shutdown().await;
}
