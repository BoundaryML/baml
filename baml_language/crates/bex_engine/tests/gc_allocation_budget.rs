use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue as Ext, FunctionCallContextBuilder};
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
    let n = Churn(3000000);
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
function SpawnTexts(texts: string[]) -> baml.future.Future<int, never> {
    spawn { texts.length() + texts[0].length() + texts[texts.length() - 1].length() }
}
function JoinText(job: baml.future.Future<int, never>) -> int { await job }
function Bytes(seed: uint8array, n: int) -> int {
    let i = 0;
    let total = 0;
    while (i < n) { let copy = seed.reverse(); total = total + copy.length(); i = i + 1; }
    total
}
function Concat(chunk: string, n: int) -> int {
    let text = "";
    let i = 0;
    while (i < n) { text = text + chunk; i = i + 1; }
    text.length()
}
function ConcatAndRead(chunk: string, n: int) -> int {
    let text = "";
    let i = 0;
    let found = 0;
    while (i < n) {
        text = text + chunk;
        if (text.includes("y")) { found = found + 1; }
        i = i + 1;
    }
    text.length() + found
}
function CsvGrow(cell: string, n: int) -> int {
    let w = baml.csv.buffer();
    let i = 0;
    while (i < n) { w.write_record([cell, cell]); i = i + 1; }
    w.records_written()
}
function Async(n: int) -> int {
    let values = Empty();
    Grow(values, n);
    Churn(n);
    baml.sys.sleep(baml.time.Duration.from_milliseconds(1n));
    Size(values)
}
"#;

fn engine() -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            baml_test_support::compile_source(SOURCE),
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
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
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

// Import spends the budget after the entry check. A short spawn must service
// that debt before returning, and preserve both the captured strings and the
// future handle when the child has not yet acquired its first heap permit.
#[tokio::test(start_paused = true)]
async fn spawn_services_pressure_after_rooting_child_and_future() {
    let engine = engine();
    let count = engine.heap().gc_budget().full_budget_bytes
        / std::mem::size_of::<bex_vm_types::Object>()
        + 1;
    let texts = Ext::Array {
        element_type: bex_engine::WireTy::String,
        items: vec![Ext::String("x".into()); count],
    };
    let job = call(&engine, "SpawnTexts", vec![texts], false).await;
    assert_eq!(engine.heap().gc_budget().full_collections, 1);
    assert_eq!(
        call(&engine, "JoinText", vec![job], true).await,
        Ext::Int(i64::try_from(count + 2).unwrap())
    );
    engine.shutdown().await;
}

#[tokio::test]
async fn long_compute_collects_before_returning() {
    let engine = engine();
    let n = 5_000_000;
    assert_eq!(
        call(&engine, "Churn", vec![Ext::Int(n)], true).await,
        Ext::Int(n * (n - 1) / 2)
    );
    assert!(engine.heap().gc_budget().full_collections > 1);
    engine.shutdown().await;
}

/// Growing an array that already survived a collection spends the budget by
/// the capacity it gains: 2.1 million pushes reach a 4 Mi-element buffer, which
/// is 32 MiB, the whole initial allowance.
#[tokio::test(start_paused = true)]
async fn existing_old_array_growth_spends_the_budget() {
    let engine = engine();
    let values = call(&engine, "Empty", vec![], false).await;
    engine.collect_garbage().await;
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
    assert!(
        after.full_collections > before.full_collections || engine.heap().should_gc(),
        "the growth was neither collected during the call nor left due: {after:?}"
    );
    engine.collect_garbage().await;
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

/// Twenty 4 MiB strings, keeping two, is 80 MiB of payload behind a few dozen
/// objects: the payload alone must bring the heap to collect, and the kept
/// strings must survive those collections intact.
#[tokio::test(start_paused = true)]
async fn large_payloads_trigger_gc_with_few_objects() {
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
    let collections = engine.heap().gc_budget().full_collections;
    assert!(
        collections >= 2,
        "{collections} collections for 80 MiB of payload against a 32 MiB budget"
    );
    engine.collect_garbage().await;
    for value in kept {
        assert_eq!(
            call(&engine, "TextSize", vec![value], true).await,
            Ext::Int(4 * 1024 * 1024)
        );
    }
    engine.shutdown().await;
}

/// A loop that makes and drops one large buffer per iteration collects
/// during the call rather than waiting for it to end. The first collection
/// comes after four 8 MiB iterations spend the 32 MiB budget; it finds the
/// seed and the current copy live, 16 MiB, so the budget becomes 64 MiB and
/// the remaining 224 MiB of churn brings at least two more.
#[tokio::test(start_paused = true)]
async fn large_buffers_collect_during_the_call() {
    let engine = engine();
    engine.collect_garbage().await;
    let before = engine.heap().gc_budget().full_collections;
    let size = 8 * 1024 * 1024;
    assert_eq!(
        call(
            &engine,
            "Bytes",
            vec![Ext::Uint8Array(vec![1; size]), Ext::Int(32)],
            true
        )
        .await,
        Ext::Int(32 * i64::try_from(size).unwrap())
    );
    let collections = engine.heap().gc_budget().full_collections - before;
    assert!(
        collections >= 3,
        "{collections} collections while 256 MiB of buffers came and went"
    );
    engine.shutdown().await;
}

/// A concatenation defers its copy until the text is read, and reading it
/// allocates the whole text at once. A loop that appends a 1 MiB chunk and
/// reads the result each time materializes 1 + 2 + ... + 64 MiB, about 2 GiB,
/// so it must collect during the call: the buffers reading allocates count
/// like any other allocation.
#[tokio::test(start_paused = true)]
async fn reading_concatenations_collects_during_the_call() {
    let engine = engine();
    engine.collect_garbage().await;
    let before = engine.heap().gc_budget().full_collections;
    let chunk = 1024 * 1024;
    assert_eq!(
        call(
            &engine,
            "ConcatAndRead",
            vec![Ext::String("x".repeat(chunk).into()), Ext::Int(64)],
            true
        )
        .await,
        Ext::Int(64 * i64::try_from(chunk).unwrap())
    );
    let collections = engine.heap().gc_budget().full_collections - before;
    assert!(
        collections >= 4,
        "{collections} collections while 64 growing strings were built and read"
    );
    engine.shutdown().await;
}

/// The same loop without reading allocates one small node per step: every
/// node shares the one chunk, so the call holds about 1 MiB of text however
/// long the result is. It is charged that, not the length of every
/// intermediate result (64 MiB at the end, about 2 GiB in all), so it needs
/// no collection.
#[tokio::test(start_paused = true)]
async fn unread_concatenation_is_charged_its_nodes() {
    let engine = engine();
    engine.collect_garbage().await;
    let before = engine.heap().gc_budget().full_collections;
    let chunk = 1024 * 1024;
    assert_eq!(
        call(
            &engine,
            "Concat",
            vec![Ext::String("x".repeat(chunk).into()), Ext::Int(64)],
            true
        )
        .await,
        Ext::Int(64 * i64::try_from(chunk).unwrap())
    );
    let collections = engine.heap().gc_budget().full_collections - before;
    assert_eq!(
        collections, 0,
        "an unread rope of 64 shared chunks brought {collections} collections"
    );
    engine.shutdown().await;
}

/// A `@@rust_type` payload that grows behind its lock spends the budget as it
/// grows: a CSV buffer writer accumulating 256 MiB of text in one call must
/// bring collections during the call, though the call allocates almost no
/// heap objects (one two-element record per iteration) and never reads the
/// text back. The first collection comes at 32 MiB; it finds the buffer live
/// and sets the budget to four times that, so the rest brings at least one
/// more.
#[tokio::test(start_paused = true)]
async fn payload_growth_behind_a_lock_collects_during_the_call() {
    let engine = engine();
    engine.collect_garbage().await;
    let before = engine.heap().gc_budget().full_collections;
    let cell = 1024 * 1024;
    assert_eq!(
        call(
            &engine,
            "CsvGrow",
            vec![Ext::String("x".repeat(cell).into()), Ext::Int(128)],
            true,
        )
        .await,
        Ext::Int(128)
    );
    let collections = engine.heap().gc_budget().full_collections - before;
    assert!(
        collections >= 2,
        "{collections} collections while a buffer writer grew to 256 MiB"
    );
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
    tokio::time::timeout(std::time::Duration::from_secs(30), engine.collect_garbage())
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
        call(&engine, "Churn", vec![Ext::Int(5_000_000)], true).await,
        Ext::Int(5_000_000 * 4_999_999 / 2)
    );
    assert!(engine.heap().gc_budget().full_collections > before);
    engine.shutdown().await;
}

#[tokio::test(start_paused = true)]
async fn completed_bridge_call_leaves_debt_for_next_entry_and_preserves_its_handle() {
    let engine = engine();
    let mut n = 0;
    let retained = loop {
        let value = call(&engine, "Tiny", vec![Ext::Int(n)], false).await;
        if engine.heap().should_gc() {
            break value;
        }
        n += 1;
        assert!(
            n < 30_000,
            "short-call reservations must eventually exhaust the budget"
        );
    };
    assert_eq!(engine.heap().gc_budget().full_collections, 0);
    assert!(
        engine.heap().should_gc(),
        "completion must leave debt unpaid"
    );
    assert_eq!(
        call(&engine, "Read", vec![retained], true).await,
        Ext::Int(n)
    );
    assert_eq!(engine.heap().gc_budget().full_collections, 1);
    engine.shutdown().await;
}

fn engine_with_gc(growth: &str, min_growth: usize) -> Arc<BexEngine> {
    let gc = bex_engine::GcConfig {
        growth: Some(bex_engine::gc_settings::Growth::parse(growth).unwrap()),
        min_growth: Some(min_growth),
        // Names nothing sets, so the test cannot pick up the environment.
        growth_env_var: "BEX_ENGINE_TEST_UNSET_GC_GROWTH".into(),
        min_growth_env_var: "BEX_ENGINE_TEST_UNSET_GC_MIN_GROWTH".into(),
    };
    let engine = BexEngine::new_with_config(
        baml_test_support::compile_source(SOURCE),
        Arc::new(sys_native::SysOps::native()),
        vec![],
        bex_engine::EngineConfig {
            gc,
            ..Default::default()
        },
    )
    .unwrap();
    let settings = engine.heap().gc_settings();
    assert_eq!(settings.growth().to_string(), growth);
    assert_eq!(settings.min_growth(), min_growth);
    assert_eq!(engine.heap().gc_budget().full_budget_bytes, min_growth);
    Arc::new(engine)
}

/// Collections during 256 MiB of buffer churn with 8 MiB live.
async fn collections_during_churn(engine: &Arc<BexEngine>) -> usize {
    engine.collect_garbage().await;
    let before = engine.heap().gc_budget().full_collections;
    let size = 8 * 1024 * 1024;
    call(
        engine,
        "Bytes",
        vec![Ext::Uint8Array(vec![1; size]), Ext::Int(32)],
        true,
    )
    .await;
    engine.heap().gc_budget().full_collections - before
}

/// The same program collects more often under a lower growth and less often
/// under a higher one; a large `min_growth` holds collections off entirely.
#[tokio::test(start_paused = true)]
async fn growth_and_min_growth_set_how_often_the_same_program_collects() {
    const MIB: usize = 1024 * 1024;
    let mut counts = Vec::new();
    for growth in ["0.25", "4", "16"] {
        let engine = engine_with_gc(growth, MIB);
        counts.push(collections_during_churn(&engine).await);
        engine.shutdown().await;
    }
    let [low, default, high] = counts[..] else {
        unreachable!()
    };
    assert!(
        low > default && default > high,
        "collections at growth 0.25 / 4 / 16: {low} / {default} / {high}"
    );
    assert!(low >= 16, "growth 0.25 collects about every copy: {low}");

    let engine = engine_with_gc("0.25", 1024 * MIB);
    assert_eq!(
        collections_during_churn(&engine).await,
        0,
        "256 MiB of churn stays under a 1 GiB min_growth"
    );
    engine.shutdown().await;
}
