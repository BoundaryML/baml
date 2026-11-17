//! Per-container spin-lock used to protect heap-mutable containers
//! (`Object::Array`'s `Vec<Value>`, `Object::Map`'s `IndexMap`,
//! `Object::Uint8Array`'s `Vec<u8>`) from racing mutation introduced by
//! BEP-034 `spawn`.
//!
//! # What the lock guarantees
//!
//! A thread is inside a container's critical section if and only if it won
//! the `compare_exchange(UNLOCKED → LOCKED)` with `Acquire` ordering, and it
//! leaves by storing `UNLOCKED` with `Release` ordering when its
//! [`AccessGuard`] drops. So at most one thread is ever inside, and everything
//! the previous holder wrote to the container happens-before everything the
//! next holder reads. That is the whole memory-safety argument for
//! `LockedContainer`: its `UnsafeCell` is only dereferenced while a guard is
//! held (or under the GC's stop-the-world barrier, which parks every mutator
//! first). How a waiter waits (below) never touches the container, so it
//! cannot weaken this.
//!
//! The lock is not reentrant. A thread that takes a container's lock while it
//! already holds it spins forever, so no code path may hold two guards on one
//! container (the builtin glue snapshots operands that may alias; see
//! `reads_multiple_containers` in `baml_builtins2_codegen`).
//!
//! # Why a spin-lock and not an OS mutex
//!
//! Every critical section is a single container operation (`Vec::push`, an
//! index load, an `IndexMap` probe): no user code, no I/O, no awaits. BAML
//! tasks only yield at `await` points and at safepoints between instructions,
//! never inside one of these Rust functions, so a holder always makes progress
//! and releases the lock within one operation. Parking a thread in the kernel
//! costs more than the whole critical section; measured on the contended
//! workloads below, parking waiters was no faster than the backoff here.
//!
//! - Uncontended acquire: one CAS. Release: one `store(Release)`.
//! - Containers that escape analysis proves thread-local skip the lock
//!   entirely (`LockedContainer::lock_thread_local`).
//!
//! # Why waiters back off for so long
//!
//! When several tasks hammer one container (four tasks pushing onto one
//! array), what decides throughput is how often the lock, and the container's
//! cache lines with it, move between cores. A waiter that looks again as soon
//! as it can wins the lock in the holder's short gap between two operations,
//! so the lock changes cores on nearly every operation: four tasks pushing
//! onto one array ran almost 4x slower than one task doing all the pushes
//! (and about 8x slower than Python, whose GIL serializes them instead).
//!
//! So a waiter that finds the lock taken first waits about a microsecond
//! before it looks again, and doubles that wait on each miss up to a cap. The
//! holder, which comes back to the lock tens of nanoseconds after releasing
//! it, then usually takes it again for its next operation while its caches are
//! still warm, the way a GIL would let it; the lock changes cores once per
//! batch, not once per operation. The same four tasks then run close to the
//! speed of one task doing all the pushes.
//!
//! The cost is fairness and latency on a collision: a waiter can lose several
//! rounds to the holder, and one that collides with a single short operation
//! still waits the first microsecond. It still gets in, because a holder
//! releases between operations and a waiter's looks land in those gaps.

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicU8, Ordering};

const UNLOCKED: u8 = 0;
const LOCKED: u8 = 1;

/// Spin hints a waiter waits before it first looks at a lock it found taken.
/// One `spin_loop` hint (`isb` on Apple silicon) takes about 8 ns on an M5
/// Max, so this is about 1 µs there; x86's `pause` takes from a few ns to
/// about 40 ns depending on the core.
///
/// Measured on four tasks sharing one array (M5 Max): a first wait of 8 hints
/// ran pushes at 1.5x the time of one uncontended task, 32 hints at 1.25x and
/// 128 hints at 1.14x, with the same ordering for one writer and four readers.
const FIRST_WAIT_SPINS: u32 = 128;

/// The longest wait between two looks: each miss doubles the wait up to this,
/// about 8 µs on an M5 Max.
const MAX_WAIT_SPINS: u32 = 1024;

/// Looks with a spinning wait before each, after which a waiter yields its
/// time slice before each look instead, so a holder the OS descheduled can
/// run. About 70 µs of spinning on an M5 Max.
const SPINNING_LOOKS: u32 = 10;

#[derive(Debug)]
pub struct LazyBiasedMutex {
    state: AtomicU8,
}

impl LazyBiasedMutex {
    pub const fn new() -> Self {
        Self {
            state: AtomicU8::new(UNLOCKED),
        }
    }

    /// Acquire the lock, returning a guard that releases it on drop.
    ///
    /// The acquire CAS provides the happens-before edge with the previous
    /// holder's release.
    pub fn enter(&self) -> AccessGuard<'_> {
        // Fast path: try the CAS once before any waiting. On the
        // uncontended case (the dominant one) this is the entire cost.
        if self
            .state
            .compare_exchange_weak(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            return AccessGuard { lbm: self };
        }
        self.enter_slow()
    }

    /// Take the lock if nobody holds it, without waiting.
    pub fn try_enter(&self) -> Option<AccessGuard<'_>> {
        // `then`, not `then_some`: the guard must only exist once the CAS
        // succeeded. Building it eagerly and dropping it on failure would
        // release the lock out from under its actual holder.
        self.state
            .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
            .then(|| AccessGuard { lbm: self })
    }

    /// Slow path, separated so the fast path inlines cleanly: wait, look,
    /// and try to take the lock, with the waits growing as described in the
    /// module docs.
    #[cold]
    #[inline(never)]
    fn enter_slow(&self) -> AccessGuard<'_> {
        let mut wait = FIRST_WAIT_SPINS;
        let mut looks: u32 = 0;
        loop {
            if looks < SPINNING_LOOKS {
                for _ in 0..wait {
                    std::hint::spin_loop();
                }
                wait = (wait * 2).min(MAX_WAIT_SPINS);
                looks += 1;
            } else {
                std::thread::yield_now();
            }
            // Read before the CAS: a waiter that only reads keeps its copy
            // of the lock's cache line shared instead of pulling it away
            // from the holder.
            if self.state.load(Ordering::Relaxed) == UNLOCKED
                && self
                    .state
                    .compare_exchange(UNLOCKED, LOCKED, Ordering::Acquire, Ordering::Relaxed)
                    .is_ok()
            {
                return AccessGuard { lbm: self };
            }
        }
    }

    /// Run `op` while holding the lock. Convenience wrapper around
    /// [`Self::enter`].
    #[inline]
    pub fn access<R>(&self, op: impl FnOnce() -> R) -> R {
        let _g = self.enter();
        op()
    }
}

impl Default for LazyBiasedMutex {
    fn default() -> Self {
        Self::new()
    }
}

// Cloning produces a fresh, unlocked instance — the lock state is not
// part of the logical value of whatever container owns it.
impl Clone for LazyBiasedMutex {
    fn clone(&self) -> Self {
        Self::new()
    }
}

/// Held while a thread is inside the critical section. The Drop impl
/// releases the lock via a `store(Release)`, establishing a
/// happens-before edge with the next acquirer's CAS-Acquire.
pub struct AccessGuard<'a> {
    lbm: &'a LazyBiasedMutex,
}

impl Drop for AccessGuard<'_> {
    fn drop(&mut self) {
        self.lbm.state.store(UNLOCKED, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use std::{
        cell::UnsafeCell,
        sync::{
            Arc, Barrier,
            atomic::{AtomicBool, AtomicUsize},
        },
        thread,
        time::Duration,
    };

    use super::*;

    #[test]
    fn uncontended_fast_path() {
        let m = LazyBiasedMutex::new();
        let mut counter = 0;
        for _ in 0..1000 {
            m.access(|| counter += 1);
        }
        assert_eq!(counter, 1000);
        assert_eq!(m.state.load(Ordering::Acquire), UNLOCKED);
    }

    /// A failed `try_enter` must leave the holder's lock taken.
    #[test]
    fn try_enter_fails_while_held() {
        let m = LazyBiasedMutex::new();
        let g = m.enter();
        assert!(m.try_enter().is_none());
        assert_eq!(m.state.load(Ordering::Relaxed), LOCKED);
        assert!(m.try_enter().is_none(), "the holder still has it");
        drop(g);
        assert_eq!(m.state.load(Ordering::Relaxed), UNLOCKED);
        let g = m.try_enter().expect("a released lock can be taken");
        assert!(m.try_enter().is_none());
        assert_eq!(m.state.load(Ordering::Relaxed), LOCKED);
        drop(g);
        assert_eq!(m.state.load(Ordering::Relaxed), UNLOCKED);
    }

    #[test]
    fn clone_is_unlocked() {
        let m = LazyBiasedMutex::new();
        let _g = m.enter();
        let copy = m.clone();
        assert!(copy.try_enter().is_some());
    }

    /// Data the lock protects, deliberately with no synchronization of its
    /// own: if two threads were ever inside the critical section together,
    /// the plain `Vec` would lose writes or corrupt its `(ptr, len, cap)`,
    /// and the `inside` flag would catch the overlap directly.
    struct Unprotected {
        lock: LazyBiasedMutex,
        inside: AtomicBool,
        items: UnsafeCell<Vec<u64>>,
        reads: UnsafeCell<u64>,
    }

    // SAFETY: `items` and `reads` are only touched while `lock` is held.
    unsafe impl Sync for Unprotected {}

    impl Unprotected {
        fn with<R>(&self, op: impl FnOnce(&mut Vec<u64>, &mut u64) -> R) -> R {
            let _g = self.lock.enter();
            assert!(
                !self.inside.swap(true, Ordering::Relaxed),
                "two threads inside the critical section"
            );
            // SAFETY: we hold the lock.
            let r = op(unsafe { &mut *self.items.get() }, unsafe {
                &mut *self.reads.get()
            });
            self.inside.store(false, Ordering::Relaxed);
            r
        }
    }

    /// Stress: writers grow one `Vec` (reallocating it many times) while
    /// readers index into it, all on one lock, the way tasks share an array
    /// under `spawn`. Every push must land exactly once, and every read must
    /// see an initialized element.
    #[test]
    fn racing_writers_and_readers_stay_serialized() {
        const WRITERS: usize = 4;
        const READERS: usize = 4;
        const PER_WRITER: u64 = 20_000;
        const PER_READER: usize = 20_000;

        let shared = Arc::new(Unprotected {
            lock: LazyBiasedMutex::new(),
            inside: AtomicBool::new(false),
            items: UnsafeCell::new(Vec::new()),
            reads: UnsafeCell::new(0),
        });
        let start = Arc::new(Barrier::new(WRITERS + READERS));
        let total = PER_WRITER * WRITERS as u64;

        let mut handles = vec![];
        for t in 0..WRITERS as u64 {
            let shared = Arc::clone(&shared);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                for i in 0..PER_WRITER {
                    shared.with(|items, _| items.push(t * PER_WRITER + i));
                }
            }));
        }
        for r in 0..READERS {
            let shared = Arc::clone(&shared);
            let start = Arc::clone(&start);
            handles.push(thread::spawn(move || {
                start.wait();
                for i in 0..PER_READER {
                    let seen = shared.with(|items, reads| {
                        *reads += 1;
                        (!items.is_empty()).then(|| items[(i * 7 + r) % items.len()])
                    });
                    if let Some(v) = seen {
                        assert!(v < total, "read a value nobody wrote: {v}");
                    }
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        shared.with(|items, reads| {
            assert_eq!(*reads, (READERS * PER_READER) as u64, "lost a read count");
            assert_eq!(items.len() as u64, total, "lost a push");
            items.sort_unstable();
            for (expected, &got) in (0..total).zip(items.iter()) {
                assert_eq!(got, expected, "a push landed twice or not at all");
            }
        });
    }

    /// Waiters that run out of spinning looks and fall back to yielding still
    /// get the lock once a long holder releases it, each exactly once.
    #[test]
    fn waiters_get_in_after_a_long_hold() {
        let m = Arc::new(LazyBiasedMutex::new());
        let entered = Arc::new(AtomicUsize::new(0));
        let g = m.enter();
        let handles: Vec<_> = (0..4)
            .map(|_| {
                let m = Arc::clone(&m);
                let entered = Arc::clone(&entered);
                thread::spawn(move || {
                    m.access(|| entered.fetch_add(1, Ordering::Relaxed));
                })
            })
            .collect();
        // Long enough for every waiter to pass its spinning looks.
        thread::sleep(Duration::from_millis(50));
        assert_eq!(entered.load(Ordering::Relaxed), 0);
        drop(g);
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(entered.load(Ordering::Relaxed), 4);
    }
}
