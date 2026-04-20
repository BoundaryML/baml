//! Integration tests for engine-level `EarlyYield` coordination.
//!
//! These tests verify the end-to-end GC-coordination path: the VM runs a
//! long loop, another task calls `collect_garbage`, which sets
//! `park_requested`, the VM observes the flag at its next poll boundary and
//! emits `EarlyYield`, the engine's `gc_safepoint` parks the permit, GC
//! runs, and the VM resumes to completion.
//!
//! All tests use `tokio::test(flavor = "multi_thread")` so the VM (which
//! executes synchronously inside `call_function`) and the GC coordinator run
//! on separate worker threads — a single-threaded runtime would force them
//! to interleave cooperatively, masking coordination bugs.

#![cfg(not(target_arch = "wasm32"))]

mod common;

use std::sync::Arc;

use ::bex_heap::CollectionLevel;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

const LOOP_ITERATIONS: i64 = 50_000;

/// BAML program with a long loop that allocates a small array each
/// iteration. The allocations make GC non-trivial (it has objects to trace
/// through) and the loop is long enough that a concurrent GC request is
/// very likely to land mid-execution.
fn spin_source() -> &'static str {
    r#"
        function spin(n: int) -> int {
            let i = 0;
            while (i < n) {
                let _ = [i, i + 1, i + 2];
                i += 1;
            }
            i
        }
    "#
}

fn make_engine(source: &str) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new(
            compile_for_engine(source),
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    )
}

/// Give the spawned `call_function` task a chance to enter its hot loop
/// before the main task requests GC. `yield_now` is preferable to
/// `sleep`: it's scheduler-driven rather than wall-clock, so it behaves
/// predictably even under heavy CI load.
async fn let_call_start() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

/// Core test: a long-running `call_function` must complete correctly even
/// when `collect_garbage` fires mid-execution. If park coordination is
/// broken, this either deadlocks or returns a corrupted value.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gc_during_long_running_call_completes() {
    let engine = make_engine(spin_source());

    let call_handle = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "spin",
                    vec![BexExternalValue::Int(LOOP_ITERATIONS)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        })
    };

    let_call_start().await;
    engine.collect_garbage(CollectionLevel::Minor).await;

    let result = call_handle.await.expect("call task panicked");
    let value = result.expect("spin() should succeed");
    assert_eq!(
        value,
        BexExternalValue::Int(LOOP_ITERATIONS),
        "spin must return exact iteration count even with concurrent GC"
    );
}

/// Four concurrent calls, a single GC mid-flight. Every call must return
/// the correct value. This stresses the multi-permit parking path —
/// `request_park()` must wait for *every* active permit before GC runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gc_during_concurrent_calls() {
    let engine = make_engine(spin_source());

    let mut handles = Vec::new();
    for _ in 0..4 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            engine
                .call_function(
                    "spin",
                    vec![BexExternalValue::Int(LOOP_ITERATIONS)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        }));
    }

    let_call_start().await;
    let _stats = engine.collect_garbage(CollectionLevel::Minor).await;

    for (i, handle) in handles.into_iter().enumerate() {
        let result = handle.await.expect("task panicked");
        let value = result.unwrap_or_else(|e| panic!("call {i} failed: {e}"));
        assert_eq!(
            value,
            BexExternalValue::Int(LOOP_ITERATIONS),
            "call {i} returned wrong value"
        );
    }
}

/// Multiple sequential GCs during one long call. Regression test against
/// a stuck `park_requested` flag — if the engine forgets to clear it after
/// GC, the second GC would never let the VM resume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn multiple_gcs_during_single_call() {
    let engine = make_engine(spin_source());

    let call_handle = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "spin",
                    vec![BexExternalValue::Int(LOOP_ITERATIONS * 3)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        })
    };

    for _ in 0..3 {
        let_call_start().await;
        engine.collect_garbage(CollectionLevel::Minor).await;
    }

    let value = call_handle
        .await
        .expect("task panicked")
        .expect("spin must succeed despite repeated GC");
    assert_eq!(value, BexExternalValue::Int(LOOP_ITERATIONS * 3));
}

/// A Major collection mid-call must also complete correctly. Major GC
/// walks more of the heap and is the more invasive variant, so it's worth
/// exercising explicitly alongside Minor.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn major_gc_during_long_running_call_completes() {
    let engine = make_engine(spin_source());

    let call_handle = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "spin",
                    vec![BexExternalValue::Int(LOOP_ITERATIONS)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        })
    };

    let_call_start().await;
    engine.collect_garbage(CollectionLevel::Major).await;

    let value = call_handle
        .await
        .expect("task panicked")
        .expect("spin must succeed through a Major GC");
    assert_eq!(value, BexExternalValue::Int(LOOP_ITERATIONS));
}

/// Allocations *after* a mid-call GC must not corrupt allocations made
/// *before* it, and vice versa. This catches the bug where the GC resets
/// the heap-wide TLAB cursor but leaves individual VM TLABs holding stale
/// `alloc_ptr`/`alloc_limit` values — subsequent allocations land in
/// indices that may now be handed out as fresh chunks to other VMs.
///
/// The integer-only Layer C tests don't catch this because corrupted
/// objects don't affect an `i64` return value; this test forces the
/// engine to round-trip object contents through the result.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn objects_allocated_after_mid_call_gc_survive() {
    // Each iteration's array escapes into `last`, preventing the compiler
    // from dead-code-eliminating the loop allocations. This guarantees that
    // both the pre-GC `pre` array and the post-GC allocations (`last` and
    // `post` and the result) actually hit the TLAB.
    let source = r#"
        function alloc_around_gc(n: int) -> int[] {
            let pre = [1, 2, 3, 4];
            let last = [0, 0];
            let i = 0;
            while (i < n) {
                last = [i, i + 1];
                i += 1;
            }
            let post = [10, 20, 30, 40];
            [pre[0], pre[3], post[0], post[3], last[0], last[1]]
        }
    "#;

    let engine = make_engine(source);
    let call_handle = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "alloc_around_gc",
                    vec![BexExternalValue::Int(50_000)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        })
    };

    // Give the call enough wall-clock time to actually allocate a meaningful
    // chunk of its TLAB before GC fires. yield_now isn't sufficient: with a
    // pre-fix VM, we need the call to be deep enough into the loop that the
    // post-GC writes will land in stale TLAB indices (otherwise the call
    // hasn't taken a permit yet and GC just runs on an empty heap).
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    engine.collect_garbage(CollectionLevel::Minor).await;

    let value = call_handle
        .await
        .expect("task panicked")
        .expect("alloc_around_gc must succeed");

    let items = match value {
        BexExternalValue::Array { items, .. } => items,
        other => panic!("expected Array, got {other:?}"),
    };
    let expected: Vec<BexExternalValue> = [1i64, 4, 10, 40, 49_999, 50_000]
        .into_iter()
        .map(BexExternalValue::Int)
        .collect();
    assert_eq!(
        items, expected,
        "post-GC allocations must not corrupt pre-GC ones, and vice versa"
    );
}
