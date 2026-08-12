//! Regression test for the GC-park / new-permit AB-BA deadlock in
//! `HeapPermitManager`. See `bex_heap/src/heap_guard.rs` `request_park`.
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

    // Thread C: another VM wants a fresh permit (e.g. for a spawned
    // child). With the buggy ordering, this hangs on `holders.lock()`
    // because B is holding it across the semaphore await.
    let mgr_c = Arc::clone(&mgr);
    let new_permit_handle = tokio::spawn(async move {
        let _p = mgr_c.new_permit(()).await;
    });

    // Bound the wait — without the fix this never resolves. Generous
    // enough to absorb CI scheduler jitter; the correct path takes
    // microseconds.
    match tokio::time::timeout(Duration::from_secs(2), new_permit_handle).await {
        Ok(Ok(())) => { /* new_permit returned promptly — fix is in place */ }
        Ok(Err(join_err)) => panic!("new_permit task panicked: {join_err}"),
        Err(_) => panic!(
            "new_permit deadlocked waiting on the holders mutex while \
             request_park was holding it across acquire_many — see \
             baml_language/crates/bex_heap/src/heap_guard.rs request_park"
        ),
    }

    // Drop A so the GC park can finally complete (otherwise the test
    // process leaks the parked task).
    drop(active_a);
    park.await
        .expect("park task should complete after permit released");
}
