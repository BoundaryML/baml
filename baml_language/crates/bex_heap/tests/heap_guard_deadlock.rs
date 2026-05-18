//! Regression test for the GC-park / new-permit AB-BA deadlock in
//! `HeapPermitManager`. See `bex_heap/src/heap_guard.rs:294`.
//!
//! With the buggy ordering (mutex acquired before `acquire_many`), this
//! test times out. With the fix (drain semaphore before taking mutex), it
//! passes in milliseconds.

use std::{sync::Arc, time::Duration};

use bex_heap::HeapPermitManager;

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn request_park_must_not_block_new_permit() {
    let mgr = Arc::new(HeapPermitManager::new());

    // Thread A: simulate a VM in the middle of executing a `spawn` opcode —
    // it is still holding its active heap permit.
    let inactive_a = mgr.new_permit(()).await;
    let active_a = inactive_a.acquire().await;

    // Thread B: the GC. Calls request_park. With the buggy ordering it
    // takes the `holders` mutex and then awaits `acquire_many` forever
    // (because A still holds a permit). The mutex stays held the whole
    // time.
    let mgr_b = Arc::clone(&mgr);
    let park = tokio::spawn(async move {
        let _guard = mgr_b.request_park().await;
    });

    // Give B time to enter request_park and grab the holders mutex.
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Thread C: another VM hitting a `spawn` opcode — it tries to allocate
    // a new permit. With the buggy ordering this blocks on the holders
    // mutex (held by B) forever. With the fix this returns immediately
    // because B is waiting on the semaphore, not on the mutex.
    let mgr_c = Arc::clone(&mgr);
    let new_permit = tokio::spawn(async move {
        let _ = mgr_c.new_permit(()).await;
    });

    let result = tokio::time::timeout(Duration::from_secs(1), new_permit).await;

    // Drop A's permit so B (and the runtime) can shut down cleanly even on
    // the buggy path — otherwise the test process would leak the park task.
    drop(active_a);
    let _ = park.await;

    assert!(
        result.is_ok(),
        "new_permit deadlocked waiting on the holders mutex while \
         request_park was holding it across acquire_many — see \
         baml_language/crates/bex_heap/src/heap_guard.rs:294"
    );
}
