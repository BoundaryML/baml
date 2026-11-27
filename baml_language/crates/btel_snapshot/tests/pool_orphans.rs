//! A thread's free list after its pool is gone. Alone in its binary: it
//! counts every live heap byte.
use std::{
    alloc::{GlobalAlloc, Layout, System},
    sync::{
        atomic::{AtomicIsize, Ordering},
        mpsc,
    },
};

use btel_snapshot::{Limits, PoolConfig, Shaper, Snapshot, SnapshotPool, SnapshotValue};

struct Counting;
static LIVE: AtomicIsize = AtomicIsize::new(0);
#[expect(unsafe_code, reason = "a counting allocator over the system one")]
// SAFETY: every call forwards to `System` unchanged.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        LIVE.fetch_add(layout.size().cast_signed(), Ordering::Relaxed);
        // SAFETY: the caller's contract, forwarded.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        LIVE.fetch_sub(layout.size().cast_signed(), Ordering::Relaxed);
        // SAFETY: the caller's contract, forwarded.
        unsafe { System.dealloc(ptr, layout) }
    }
    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        LIVE.fetch_add(
            new_size.cast_signed() - layout.size().cast_signed(),
            Ordering::Relaxed,
        );
        // SAFETY: the caller's contract, forwarded.
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}
#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn large(pool: &SnapshotPool) -> Snapshot {
    let mut b = pool.try_acquire().unwrap();
    let object = b.bytes(&[7; 120_000]);
    let id = b.leaves().object(object).unwrap();
    b.finish(SnapshotValue::Object(id), &mut Shaper::default())
}
fn scalar(pool: &SnapshotPool) -> Snapshot {
    pool.try_acquire()
        .unwrap()
        .finish(SnapshotValue::Int(1), &mut Shaper::default())
}
fn live() -> isize {
    LIVE.load(Ordering::SeqCst)
}

#[test]
#[cfg_attr(miri, ignore = "captures megabytes")]
fn a_dropped_pools_storages_are_freed_once_the_holding_thread_touches_a_pool() {
    let base = live();
    let pool = SnapshotPool::unbounded(Limits::default());
    let (go, wait) = mpsc::channel::<()>();
    let (done, is_done) = mpsc::channel::<()>();
    let burst = pool.clone();
    let holder = std::thread::spawn(move || {
        let held: Vec<_> = (0..32).map(|_| large(&burst)).collect();
        drop(held);
        drop(scalar(&burst));
        drop(burst);
        done.send(()).unwrap();
        // Parked: its free list outlives the pool, bounded by the local cap.
        wait.recv().unwrap();
        // Returning a capture of any pool frees what the dead pool left.
        let other = SnapshotPool::unbounded(Limits::default());
        let snapshot = scalar(&other);
        drop(snapshot);
        drop(other);
        done.send(()).unwrap();
        wait.recv().unwrap();
    });
    is_done.recv().unwrap();
    drop(pool);
    let parked = live() - base;
    let cap = PoolConfig::default().max_local_bytes.cast_signed();
    // At most the local cap, plus one storage that may cross it.
    assert!(
        parked <= 2 * cap,
        "{parked} bytes live after the pool dropped"
    );
    go.send(()).unwrap();
    is_done.recv().unwrap();
    let touched = live() - base;
    assert!(
        touched < 64 * 1024,
        "{touched} bytes live after the thread touched a pool"
    );
    go.send(()).unwrap();
    holder.join().unwrap();
}
