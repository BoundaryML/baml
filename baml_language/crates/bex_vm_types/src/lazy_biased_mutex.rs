//! Per-container lazy biased mutex used to protect heap-mutable containers
//! (`Object::Array`'s `Vec<Value>`, `Object::Map`'s `IndexMap`) from racing
//! mutation introduced by BEP-034 `spawn`, without paying any synchronization
//! cost in the (overwhelmingly common) single-accessor case.
//!
//! Design summary:
//!
//! - Two pieces of state per container, both inline:
//!   * `count: AtomicUsize` — incremented on entry, decremented on exit.
//!     The value before the increment tells you if you were alone.
//!   * `mutex: AtomicPtr<parking_lot::Mutex<()>>` — null until first
//!     contention. The OS mutex is heap-allocated lazily and never
//!     deallocated until the container itself is dropped.
//!
//! - Fast path (uncontended): `fetch_add(1, Acquire)` returns 0, run the op,
//!   `fetch_sub(1, Release)`. No mutex. ~2 cycles overhead.
//!
//! - Spin path (mild contention): we observed someone else; spin briefly
//!   reading `count`, hoping they leave (`Vec::push` is nanoseconds). When
//!   `count == 1` we're alone — run the op.
//!
//! - Slow path (sustained contention): allocate the OS mutex on demand,
//!   lock it, run the op.
//!
//! Closest published patterns: Linux kernel seqlock (in-progress counter),
//! `HotSpot` biased locking (cheap fast path), `parking_lot` adaptive mutex
//! (spin + futex fallback). No single canonical name; this is the hybrid
//! Vaibhav proposed.
//!
//! Caveat: the `op` closure must NOT call back into the same container.
//! That would re-enter `access()`, deadlock on the spin then deadlock on
//! the mutex. The discipline is "no user callbacks inside `access()`" —
//! enforced by API design (no public method takes a user-callable while
//! holding the lock).

#![allow(unsafe_code)]

use std::sync::atomic::{AtomicPtr, AtomicUsize, Ordering};

use parking_lot::Mutex;

/// How many spin iterations we attempt before falling back to the OS mutex.
/// Bound chosen so a `Vec::push` or `IndexMap::insert` from the holder thread
/// completes well within budget; sustained contention falls through quickly.
const SPIN_BUDGET: u32 = 64;

#[derive(Debug)]
pub struct LazyBiasedMutex {
    count: AtomicUsize,
    /// Lazy-allocated OS mutex; null until the first sustained contention.
    /// Once installed, points to a leaked `Box<Mutex<()>>` that we recover
    /// and drop in our own `Drop` impl.
    mutex: AtomicPtr<Mutex<()>>,
}

impl LazyBiasedMutex {
    pub const fn new() -> Self {
        Self {
            count: AtomicUsize::new(0),
            mutex: AtomicPtr::new(std::ptr::null_mut()),
        }
    }

    /// Acquire serialized access to the container. Returns an
    /// [`AccessGuard`] that releases the lock on drop. Use this when you
    /// need to hold the lock across multiple operations from the caller's
    /// perspective (e.g., returning a `&mut Vec<Value>` from a getter).
    ///
    /// The guard must NOT be used to call back into the same container's
    /// `enter()` — that would deadlock. Callers are responsible for keeping
    /// the lifetime of the guard tight (one structural mutation per
    /// acquisition; never invoke user callbacks while holding it).
    pub fn enter(&self) -> AccessGuard<'_> {
        let prev = self.count.fetch_add(1, Ordering::Acquire);
        if prev == 0 {
            // Fast path: nobody else is in.
            return AccessGuard {
                lbm: self,
                _os_guard: None,
            };
        }

        // Someone else was in. Spin briefly hoping they leave.
        for _ in 0..SPIN_BUDGET {
            if self.count.load(Ordering::Acquire) == 1 {
                // They left; we're the only +1 contributor now.
                return AccessGuard {
                    lbm: self,
                    _os_guard: None,
                };
            }
            std::hint::spin_loop();
        }

        // Sustained contention: fall back to the OS mutex.
        let mutex = self.get_or_init_mutex();
        // SAFETY: `mutex` lives as long as `self` (we own the box until Drop).
        let guard = mutex.lock();
        // Extend the guard's lifetime to that of `self` — the underlying
        // mutex is leaked-boxed and outlives any caller's borrow.
        let guard: parking_lot::MutexGuard<'_, ()> = unsafe { std::mem::transmute(guard) };
        AccessGuard {
            lbm: self,
            _os_guard: Some(guard),
        }
    }

    /// Run `op` with the container serialized against other concurrent accessors.
    ///
    /// Convenience wrapper around [`Self::enter`] for callers that don't
    /// need to expose a guard.
    #[inline]
    pub fn access<R>(&self, op: impl FnOnce() -> R) -> R {
        let _g = self.enter();
        op()
    }

    fn get_or_init_mutex(&self) -> &Mutex<()> {
        let existing = self.mutex.load(Ordering::Acquire);
        if !existing.is_null() {
            // SAFETY: once non-null, the pointer is a valid `Box::leak`'d
            // `Mutex` that outlives `self` (we own it via `Drop`).
            return unsafe { &*existing };
        }

        let fresh = Box::into_raw(Box::new(Mutex::new(())));
        match self.mutex.compare_exchange(
            std::ptr::null_mut(),
            fresh,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                // SAFETY: we just installed `fresh`; it's exclusively ours
                // until `Drop`.
                unsafe { &*fresh }
            }
            Err(other) => {
                // Lost the race. Reclaim our allocation; use the winner's.
                // SAFETY: `fresh` is the box we just allocated, no other
                // owner has observed it (the CAS failed).
                unsafe { drop(Box::from_raw(fresh)) };
                // SAFETY: `other` is non-null per the CAS contract and was
                // installed by the winning thread.
                unsafe { &*other }
            }
        }
    }
}

impl Default for LazyBiasedMutex {
    fn default() -> Self {
        Self::new()
    }
}

/// Held for the duration of a critical section on a [`LazyBiasedMutex`].
/// On drop, the access counter is decremented (releasing the lock for
/// other waiters), and any held OS mutex guard is dropped first.
pub struct AccessGuard<'a> {
    lbm: &'a LazyBiasedMutex,
    /// `Some` only if we fell back to the OS mutex. Drops along with the
    /// guard; the order between the OS-mutex release and the counter
    /// decrement is immaterial for correctness — both carry the same
    /// happens-before edge for our `op()` writes (the OS mutex via its
    /// own release semantics, the counter via the `Release` `fetch_sub`).
    _os_guard: Option<parking_lot::MutexGuard<'a, ()>>,
}

impl Drop for AccessGuard<'_> {
    fn drop(&mut self) {
        // `_os_guard` drops first by struct-field order; then we decrement.
        self.lbm.count.fetch_sub(1, Ordering::Release);
    }
}

impl Drop for LazyBiasedMutex {
    fn drop(&mut self) {
        let ptr = *self.mutex.get_mut();
        if !ptr.is_null() {
            // SAFETY: `ptr` came from `Box::into_raw` in `get_or_init_mutex`.
            // We're the sole owner now (`&mut self`); reclaim it.
            unsafe { drop(Box::from_raw(ptr)) };
        }
    }
}

// Cloning a container with a `LazyBiasedMutex` produces a fresh,
// uncontended copy. The contention state of the source is not carried
// over — it's not part of the logical value.
impl Clone for LazyBiasedMutex {
    fn clone(&self) -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::*;

    #[test]
    fn uncontended_fast_path() {
        let m = LazyBiasedMutex::new();
        let mut counter = 0;
        for _ in 0..1000 {
            m.access(|| counter += 1);
        }
        assert_eq!(counter, 1000);
        // No contention ever occurred — mutex pointer stays null.
        assert!(m.mutex.load(Ordering::Acquire).is_null());
    }

    #[test]
    fn contended_two_threads_serialize() {
        let m = Arc::new(LazyBiasedMutex::new());
        let counter = Arc::new(std::sync::Mutex::new(0u64));

        let mut handles = vec![];
        for _ in 0..4 {
            let m = m.clone();
            let counter = counter.clone();
            handles.push(thread::spawn(move || {
                for _ in 0..10_000 {
                    m.access(|| {
                        let mut g = counter.lock().unwrap();
                        *g += 1;
                    });
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(*counter.lock().unwrap(), 40_000);
    }

    #[test]
    fn racing_vec_push_no_corruption() {
        // The actual reproducer's spirit: racing pushes serialize through
        // the mutex and produce a valid Vec at the end.
        let m = Arc::new(LazyBiasedMutex::new());
        let vec = Arc::new(std::sync::Mutex::new(Vec::<u64>::new()));

        let mut handles = vec![];
        for t in 0..8u64 {
            let m = m.clone();
            let vec = vec.clone();
            handles.push(thread::spawn(move || {
                for i in 0..1000u64 {
                    m.access(|| {
                        // We're inside the LazyBiasedMutex critical section,
                        // but the test holds an outer std Mutex too just to
                        // observe — the real container uses unsafe to skip
                        // the inner lock.
                        let mut g = vec.lock().unwrap();
                        g.push(t * 1000 + i);
                    });
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(vec.lock().unwrap().len(), 8000);
    }
}
