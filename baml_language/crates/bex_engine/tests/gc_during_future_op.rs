//! H1 regression: garbage collection requested concurrently with VMs that
//! are mid-`ScheduleFuture` / `Await` must not deadlock the engine.
//!
//! Pre-fix the engine acquired a *second* heap-permit-semaphore slot from
//! `FutureManager` while still holding the VM permit, putting two
//! `acquire(1)` requests in tokio's strict-FIFO wait queue behind a GC's
//! `acquire_many(MAX_PERMITS)`. With multiple VMs the queue would wedge.
//!
//! Post-fix the `FutureManager` state lock is a plain `tokio::sync::Mutex`
//! independent of the heap semaphore, and the spawned `run_future` task
//! takes its own one-shot permit only during its brief writeback. GC can
//! run freely whenever the in-VM critical section drops the futures
//! mutex; deadlock is structurally impossible.

mod common;

use std::sync::Arc;

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use common::compile_for_engine;
use sys_native::SysOpsExt;

fn make_engine(source: &str) -> Arc<BexEngine> {
    let snapshot = compile_for_engine(source);
    Arc::new(
        BexEngine::new(
            snapshot,
            Arc::new(sys_native::SysOps::native()),
            None,
            Vec::new(),
        )
        .expect("Failed to create engine"),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn no_deadlock_between_gc_and_concurrent_schedule_future() {
    // Pre-fix this hung once a VM started a `ScheduleFuture` critical section
    // and another VM concurrently entered `request_park`: GC's
    // `acquire_many(MAX_PERMITS)` queued ahead of the VM's second permit
    // request, with the VM still holding its first one.
    //
    // Post-fix: in-VM callers of `FutureManager::acquire(proof)` only take
    // the futures Mutex (no second semaphore slot), and the spawned
    // `run_future` task takes its own one-shot permit only after the long
    // wait. The Await branch also drops the VM permit while awaiting the
    // SetOnce so the spawned task's one-shot permit acquire doesn't queue
    // behind the VM's permit holding.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            baml.sys.sleep(0);
            1
        }
    "#;
    let engine = make_engine(source);

    let stop = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stop_clone = Arc::clone(&stop);
    let engine_for_gc = Arc::clone(&engine);
    let gc_task = tokio::spawn(async move {
        // Periodic GC — gentle enough to let VMs make progress between
        // collections. The point is to provoke the GC-vs-engine ordering,
        // not stress-test heap throughput.
        while !stop_clone.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            engine_for_gc
                .collect_garbage(bex_heap::CollectionLevel::Major)
                .await;
        }
    });

    let mut handles = Vec::new();
    for _ in 0..8 {
        let engine = Arc::clone(&engine);
        handles.push(tokio::spawn(async move {
            engine
                .call_function(
                    "main",
                    vec![],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        }));
    }

    let calls = async {
        for handle in handles {
            let result = handle.await.expect("call task panicked");
            assert_eq!(result.expect("call failed"), BexExternalValue::Int(1));
        }
    };

    tokio::time::timeout(std::time::Duration::from_secs(30), calls)
        .await
        .expect("calls did not complete in time — likely a GC vs. ScheduleFuture deadlock");

    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    gc_task.await.expect("gc task panicked");

    assert_eq!(engine.active_future_count().await, 0);
}

// ============================================================================
// GC during `m.original` SysOp dispatch (VmExecState::DispatchSysOpInline).
//
// The engine allocates a `Future` heap object via `new_future` and stores
// only a local `HeapPtr` to it. Before dereferencing, it releases the VM
// permit, optionally runs `maybe_collect_garbage`, and awaits the future.
// During that release-and-await window the freshly-allocated Future is
// reachable through the `FutureManager` registry (which forwards on GC)
// but the local `future_ptr` is NOT a GC root — so a minor or major GC
// during the window moves the Future and the local pointer goes stale.
// Dereferencing it after the await reads the pre-GC Gen0 bytes (still
// `state = Pending`, since `fulfill_future` only wrote the new location),
// hitting the `unreachable!("future was awaited successfully but isn't
// terminal")` branch in `DispatchSysOpInline`.
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn gc_during_mock_original_does_not_dereference_stale_future() {
    // We need a Minor GC to land *inside* the DispatchSysOpInline window:
    // between `new_future` (Future allocated in Gen0) and the
    // `future_ptr.get()` deref after `future.await?`. The pre-fix code held
    // `future_ptr` only as a Rust local — not a GC root — so a Minor GC
    // there would move the Future out of Gen0 and leave the local stale.
    //
    // Approach: long `baml.sys.sleep` provides plenty of GC window, and a
    // single calibrated background `collect_garbage` call lands inside it.
    // We use the engine's existing `collect_garbage` rather than the
    // internal heuristic because the heuristic resets `allocs_since_gc`
    // at the VM's EarlyYield safepoints, which would fire before
    // DispatchSysOpInline rather than inside it.
    let source = r#"
        function main() -> int throws unknown {
            let m = baml.mock.Mock.new(baml.sys.sleep)
            m.replace((ms: int) -> null throws baml.errors.Io {
                m.original(ms)
            })
            m.scope<int, unknown>(() -> int throws unknown {
                let _ = baml.sys.sleep(200)
                42
            })
        }
    "#;
    let engine = make_engine(source);

    let engine_for_gc = Arc::clone(&engine);
    let gc_task = tokio::spawn(async move {
        // Wait long enough that the VM is parked inside DispatchSysOpInline's
        // `future.await?` — the BAML sleep started ~immediately after the
        // call kicks off, and we want GC to run while the future hasn't
        // resolved yet (so the post-GC local `future_ptr` is stale).
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        engine_for_gc
            .collect_garbage(bex_heap::CollectionLevel::Minor)
            .await;
    });

    let call = engine.call_function(
        "main",
        vec![],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .expect("call did not complete in time");

    gc_task.await.expect("gc task panicked");

    assert_eq!(
        result.expect("call failed"),
        BexExternalValue::Int(42),
        "DispatchSysOpInline dereferenced a stale future_ptr after GC"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn gc_during_await_completes_promptly() {
    // Single call that schedules a future and awaits it; concurrent GC
    // requests must not block that progress. This isolates the `Await`
    // branch's `gc_safepoint` -> `futures.acquire(proof)` window that
    // pre-fix could land behind a queued GC.
    let source = r#"
        function main() -> int {
            baml.sys.sleep(50);
            baml.sys.sleep(50);
            baml.sys.sleep(50);
            42
        }
    "#;
    let engine = make_engine(source);

    let engine_for_gc = Arc::clone(&engine);
    let gc_task = tokio::spawn(async move {
        for _ in 0..4 {
            engine_for_gc
                .collect_garbage(bex_heap::CollectionLevel::Major)
                .await;
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        }
    });

    let call = engine.call_function(
        "main",
        vec![],
        FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
        true,
    );
    let result = tokio::time::timeout(std::time::Duration::from_secs(10), call)
        .await
        .expect("call did not complete — likely a GC vs. Await deadlock");
    assert_eq!(result.expect("call failed"), BexExternalValue::Int(42));

    gc_task.await.expect("gc task panicked");
}
