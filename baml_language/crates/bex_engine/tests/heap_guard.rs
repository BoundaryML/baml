#![expect(
    unsafe_code,
    reason = "fabricates opaque HeapPtr identities for permit-manager tests"
)]
//! Direct unit tests for [`HeapPermitManager`] and its [`HeapGuard`].
//!
//! These exercise the permit/semaphore coordination primitive in isolation,
//! using a minimal [`RootHaver`] impl — no heap or VM is involved. We care
//! about:
//!
//! - Permit lifecycle: acquire → release → re-acquire.
//! - Multiple permits can coexist while no `HeapGuard` is in flight.
//! - `request_park` blocks while any `ActiveHeapPermit` is outstanding.
//! - `request_park` is mutually exclusive with `new_permit`.
//! - `HeapGuard::collect_roots` / `forward_roots` visit every living permit
//!   holder and skip dropped ones.

use std::{
    collections::HashMap,
    future::Future,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bex_engine::HeapPermitManager;
use bex_vm_types::{HeapPtr, RootHaver};

/// Minimal `RootHaver` fixture: pretends to hold a single `HeapPtr` root that
/// can be observed and rewritten via the `RootHaver` trait.
#[derive(Debug)]
struct TestHolder {
    root: HeapPtr,
    collect_calls: Arc<AtomicUsize>,
    forward_calls: Arc<AtomicUsize>,
}

impl TestHolder {
    fn new(root: HeapPtr) -> Self {
        Self {
            root,
            collect_calls: Arc::new(AtomicUsize::new(0)),
            forward_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl RootHaver for TestHolder {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        self.collect_calls.fetch_add(1, Ordering::Relaxed);
        roots.push(self.root);
    }
    fn forward_roots(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        self.forward_calls.fetch_add(1, Ordering::Relaxed);
        if let Some(&new_ptr) = forwarding.get(&self.root) {
            self.root = new_ptr;
        }
    }
}

/// Fabricate a distinct `HeapPtr` from an integer tag. The manager never
/// dereferences these, so any non-null aligned address is fine.
fn fake_heap_ptr(tag: usize) -> HeapPtr {
    // Align to 8 to satisfy any alignment sniffing a future debug build might do.
    let raw = (tag * 8 + 8) as *mut bex_vm_types::Object;
    // SAFETY: the resulting `HeapPtr` is only used as an opaque identity by the
    // permit manager under test. It is never dereferenced — `TestHolder` only
    // stores and rewrites it, and the tests never traverse through it into heap
    // memory. The manager itself treats roots as opaque `HeapPtr` values.
    #[cfg(feature = "heap_debug")]
    unsafe {
        HeapPtr::from_ptr(raw, 0)
    }
    #[cfg(not(feature = "heap_debug"))]
    unsafe {
        HeapPtr::from_ptr(raw)
    }
}

/// Run `fut` with a short timeout so a hung test fails fast instead of hanging CI.
async fn with_timeout<F: Future>(fut: F) -> F::Output {
    tokio::time::timeout(Duration::from_secs(5), fut)
        .await
        .expect("operation timed out — likely a deadlock in permit coordination")
}

#[tokio::test]
async fn permit_acquire_release_re_acquire_cycle() {
    let mgr = HeapPermitManager::new();
    let inactive = mgr.new_permit(TestHolder::new(fake_heap_ptr(1))).await;

    let active = inactive.acquire().await;
    let inactive_again = active.release();
    // Re-acquiring must succeed without a running GC.
    let active_again = with_timeout(inactive_again.acquire()).await;
    drop(active_again);
}

#[tokio::test]
async fn multiple_active_permits_can_coexist() {
    let mgr = HeapPermitManager::new();

    let p1 = mgr.new_permit(TestHolder::new(fake_heap_ptr(1))).await;
    let p2 = mgr.new_permit(TestHolder::new(fake_heap_ptr(2))).await;
    let p3 = mgr.new_permit(TestHolder::new(fake_heap_ptr(3))).await;

    let a1 = p1.acquire().await;
    let a2 = p2.acquire().await;
    let a3 = with_timeout(p3.acquire()).await;

    drop((a1, a2, a3));
}

#[tokio::test]
async fn request_park_waits_for_all_active_permits() {
    let mgr = Arc::new(HeapPermitManager::new());

    let p1 = mgr.new_permit(TestHolder::new(fake_heap_ptr(1))).await;
    let p2 = mgr.new_permit(TestHolder::new(fake_heap_ptr(2))).await;
    let a1 = p1.acquire().await;
    let a2 = p2.acquire().await;

    let mgr_for_gc = Arc::clone(&mgr);
    let gc_task = tokio::spawn(async move {
        let guard = mgr_for_gc.request_park().await;
        assert_eq!(
            guard.num_permits(),
            2,
            "both live permits should be tracked"
        );
    });

    // Give the GC task a chance to reach `acquire_many` and block.
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !gc_task.is_finished(),
        "request_park must block while active permits exist"
    );

    // Release one — GC should still be blocked on the other. Keep the
    // `InactiveHeapPermit` alive so its weak holder entry stays populated
    // for `HeapGuard::num_permits`.
    let i1 = a1.release();
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !gc_task.is_finished(),
        "request_park must still block while any active permit is outstanding"
    );

    // Release the last — GC can proceed. Again keep the inactive holder alive.
    let i2 = a2.release();
    with_timeout(gc_task).await.expect("GC task panicked");

    // After the guard is dropped the inactive permits can re-acquire.
    drop((i1, i2));
}

#[tokio::test]
async fn new_permit_is_blocked_during_park() {
    let mgr = Arc::new(HeapPermitManager::new());
    let guard = mgr.request_park().await;

    let mgr_for_mutator = Arc::clone(&mgr);
    let new_permit_task = tokio::spawn(async move {
        mgr_for_mutator
            .new_permit(TestHolder::new(fake_heap_ptr(7)))
            .await
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !new_permit_task.is_finished(),
        "new_permit must block on the holders mutex while a HeapGuard is held"
    );

    drop(guard);
    let inactive = with_timeout(new_permit_task)
        .await
        .expect("new_permit task panicked");
    drop(inactive);
}

#[tokio::test]
async fn heap_guard_collects_and_forwards_roots_of_live_holders() {
    let mgr = HeapPermitManager::new();

    let ptr_a = fake_heap_ptr(10);
    let ptr_b = fake_heap_ptr(20);
    let ptr_b_new = fake_heap_ptr(21);

    let p1 = mgr.new_permit(TestHolder::new(ptr_a)).await;
    let p2 = mgr.new_permit(TestHolder::new(ptr_b)).await;

    let mut guard = mgr.request_park().await;
    assert_eq!(guard.num_permits(), 2);

    let mut roots = Vec::new();
    guard.collect_roots(&mut roots);
    assert!(roots.contains(&ptr_a), "root from holder 1 missing");
    assert!(roots.contains(&ptr_b), "root from holder 2 missing");

    let mut forwarding = HashMap::new();
    forwarding.insert(ptr_b, ptr_b_new);
    guard.forward_roots(&forwarding);

    let mut roots_after = Vec::new();
    guard.collect_roots(&mut roots_after);
    assert!(
        roots_after.contains(&ptr_a),
        "ptr_a should be untouched — no forwarding entry"
    );
    assert!(
        roots_after.contains(&ptr_b_new),
        "ptr_b should have been rewritten to ptr_b_new"
    );
    assert!(
        !roots_after.contains(&ptr_b),
        "old ptr_b should no longer appear after forwarding"
    );

    drop(guard);
    drop((p1, p2));
}

#[tokio::test]
async fn unit_permit_participates_in_park_as_no_op() {
    let mgr = Arc::new(HeapPermitManager::new());

    // `()` is `RootHaver` with no roots — equivalent to an external-only caller.
    let inactive = mgr.new_permit(()).await;
    let active = inactive.acquire().await;

    // Another party tries to park; it blocks until we release.
    let gc_mgr = Arc::clone(&mgr);
    let gc_task = tokio::spawn(async move {
        let mut guard = gc_mgr.request_park().await;
        assert_eq!(guard.num_permits(), 1, "() holder should be tracked");

        let mut roots = Vec::new();
        guard.collect_roots(&mut roots);
        assert!(
            roots.is_empty(),
            "()-backed permit must contribute zero roots"
        );

        let forwarding = HashMap::new();
        guard.forward_roots(&forwarding); // must be a no-op with no panic
    });

    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !gc_task.is_finished(),
        "park must block while the ()-backed active permit is held"
    );

    let inactive = active.release();
    with_timeout(gc_task).await.expect("GC task panicked");

    drop(inactive);
}

#[tokio::test]
async fn heap_guard_skips_dropped_holders() {
    let mgr = HeapPermitManager::new();

    let ptr_live = fake_heap_ptr(100);
    let ptr_dead = fake_heap_ptr(200);

    let p_live = mgr.new_permit(TestHolder::new(ptr_live)).await;
    let p_dead = mgr.new_permit(TestHolder::new(ptr_dead)).await;

    // Drop the dead permit before parking — its `Weak` entry in the manager
    // must be cleaned up by `request_park`'s `retain`.
    drop(p_dead);

    let guard = mgr.request_park().await;
    assert_eq!(
        guard.num_permits(),
        1,
        "dropped permits must be pruned during request_park"
    );

    let mut roots = Vec::new();
    guard.collect_roots(&mut roots);
    assert_eq!(roots, vec![ptr_live]);

    drop(guard);
    drop(p_live);
}
