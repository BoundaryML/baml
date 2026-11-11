use std::cell::UnsafeCell;

use crate::{AllocDebt, lazy_biased_mutex::LazyBiasedMutex};

#[cfg(debug_assertions)]
mod lock_stats {
    use std::sync::{
        Once,
        atomic::{AtomicU64, Ordering::Relaxed},
    };
    static LOCKS: AtomicU64 = AtomicU64::new(0);
    static LOCAL: AtomicU64 = AtomicU64::new(0);
    static INIT: Once = Once::new();
    pub(super) fn count(local: bool) {
        INIT.call_once(|| {
            #[cfg(not(target_family = "wasm"))]
            if std::env::var("BAML_LOCK_STATS").as_deref() == Ok("1") {
                unsafe extern "C" {
                    fn atexit(callback: extern "C" fn()) -> std::ffi::c_int;
                }
                #[allow(clippy::print_stderr, reason = "opt-in process-exit lock statistics")]
                extern "C" fn report() {
                    eprintln!(
                        "locks={} lock_free={}",
                        LOCKS.load(Relaxed),
                        LOCAL.load(Relaxed)
                    );
                }
                // SAFETY: report only reads process-lifetime atomics.
                unsafe {
                    atexit(report);
                }
            }
        });
        (if local { &LOCAL } else { &LOCKS }).fetch_add(1, Relaxed);
    }
}

/// Heap-mutable structural container. Pairs a dynamic backing store with a
/// [`LazyBiasedMutex`] so cross-fiber `spawn`-racing mutations don't corrupt
/// internal container state such as a `Vec`'s `(ptr, len, cap)` triple or an
/// `IndexMap`'s hash table.
///
/// # Soundness
///
/// The inner value is wrapped in [`UnsafeCell`] so that both
/// [`Self::lock`] and [`Self::lock_mut`] can take `&self`. Without this,
/// the mutator-side `BexVm::as_array_mut` / `as_map_mut` would have to call
/// `get_object_mut`, which fabricates `&'static mut Object` for slots
/// that — by design — are shared across `spawn` fibers, violating
/// Rust's aliasing rules even though the [`LazyBiasedMutex`] provides
/// actual mutual exclusion at the memory level.
///
/// All mutator access to `data` happens through the lock guards, which is the
/// only place we materialize shared or mutable references to the backing store.
#[derive(Debug)]
pub struct LockedContainer<T> {
    mutex: LazyBiasedMutex,
    data: UnsafeCell<T>,
}

// SAFETY: cross-thread access is serialized by `mutex`. The `UnsafeCell` is
// necessary so callers can take the lock via `&self` (the only sound option
// when the container is reachable through aliased `&Object` from the shared
// heap). `T: Send` is required because the protected backing store can move
// between threads behind the lock.
unsafe impl<T: Send> Sync for LockedContainer<T> {}

impl<T> LockedContainer<T> {
    pub fn new(data: T) -> Self {
        Self {
            mutex: LazyBiasedMutex::new(),
            data: UnsafeCell::new(data),
        }
    }

    /// Acquire the container's mutex and return a read guard. The lock is
    /// released when the guard is dropped.
    pub fn lock(&self) -> LockedReadGuard<'_, T> {
        #[cfg(debug_assertions)]
        lock_stats::count(false);
        let access = self.mutex.enter();
        // SAFETY: we just acquired the lock; no other thread can hold a
        // `&mut` to `data` (the only place `&mut data` is materialized
        // is `lock_mut`, which also takes the lock). Lifetime is tied
        // to `&self`, which is tied to the access guard.
        let data = unsafe { &*self.data.get() };
        LockedReadGuard {
            data,
            _access: Some(access),
        }
    }

    /// Acquire the container's mutex and return a write guard. The lock
    /// is released when the guard is dropped. Takes `&self` (not
    /// `&mut self`) so callers can lock through a shared reference
    /// obtained from the shared heap (`get_object`, not the unsound
    /// `get_object_mut`).
    ///
    /// Whatever the backing store grows or shrinks by while the guard is
    /// held is charged to `debt` when the guard is dropped. There is no way
    /// to reach the store mutably without naming an account, so growth is
    /// never unaccounted for.
    pub fn lock_mut<'a>(&'a self, debt: &'a AllocDebt) -> LockedWriteGuard<'a, T>
    where
        T: Footprint,
    {
        #[cfg(debug_assertions)]
        lock_stats::count(false);
        let access = self.mutex.enter();
        // SAFETY: the access guard provides mutual exclusion against
        // all other lock holders for this container. The returned
        // `&mut T` lifetime is bounded by the guard's lifetime.
        let data = unsafe { &mut *self.data.get() };
        LockedWriteGuard {
            before: data.footprint(),
            data,
            _access: Some(access),
            debt,
        }
    }

    /// [`Self::lock`] for a container no other thread can reach, which
    /// takes no lock. Debug builds take it anyway, without waiting, and
    /// panic when another holder has it: the container was not thread-local.
    ///
    /// # Safety
    ///
    /// No other thread may be able to reach this container for the guard's
    /// lifetime. Escape analysis proves this for the access sites it marks
    /// thread-local (`baml_compiler2_mir::escape`).
    pub unsafe fn lock_thread_local(&self) -> LockedReadGuard<'_, T> {
        #[cfg(debug_assertions)]
        lock_stats::count(true);
        let access = self.assert_thread_local();
        // SAFETY: no other thread can reach the container (caller contract),
        // and this thread holds no other guard across this access.
        let data = unsafe { &*self.data.get() };
        LockedReadGuard {
            data,
            _access: access,
        }
    }

    /// [`Self::lock_mut`] for a container no other thread can reach; see
    /// [`Self::lock_thread_local`]. Footprint changes are still charged.
    ///
    /// # Safety
    ///
    /// As for [`Self::lock_thread_local`].
    pub unsafe fn lock_mut_thread_local<'a>(
        &'a self,
        debt: &'a AllocDebt,
    ) -> LockedWriteGuard<'a, T>
    where
        T: Footprint,
    {
        #[cfg(debug_assertions)]
        lock_stats::count(true);
        let access = self.assert_thread_local();
        // SAFETY: as for `lock_thread_local`.
        let data = unsafe { &mut *self.data.get() };
        LockedWriteGuard {
            before: data.footprint(),
            data,
            _access: access,
            debt,
        }
    }

    fn assert_thread_local(&self) -> Option<crate::lazy_biased_mutex::AccessGuard<'_>> {
        cfg!(debug_assertions).then(|| {
            self.mutex.try_enter().unwrap_or_else(|| {
                panic!("a container accessed as thread-local is locked by another holder")
            })
        })
    }

    /// Get a reference to the underlying `Vec` WITHOUT acquiring the lock.
    ///
    /// # Safety
    ///
    /// The caller must ensure no other thread is concurrently mutating
    /// this container. Safe contexts:
    ///
    /// - GC traversal while the stop-the-world barrier is engaged
    ///   (all mutator threads are parked).
    /// - Single-threaded engine setup / init.
    /// - Other code that has independently stopped all VM mutators.
    ///
    /// For any path where a `spawn`ed fiber may be running, use
    /// [`Self::lock`] instead.
    #[allow(clippy::missing_safety_doc)]
    pub unsafe fn data_unchecked(&self) -> &T {
        // SAFETY: caller upholds the no-concurrent-writer contract.
        unsafe { &*self.data.get() }
    }

    /// Mutable counterpart of [`Self::data_unchecked`]. Same safety
    /// contract.
    ///
    /// # Safety
    ///
    /// In addition to the no-concurrent-mutator contract, the caller
    /// must hold the only `&mut` to the container (or otherwise
    /// guarantee no other readers).
    #[allow(clippy::missing_safety_doc, clippy::mut_from_ref)]
    pub unsafe fn data_unchecked_mut(&self) -> &mut T {
        // SAFETY: caller upholds the contract.
        unsafe { &mut *self.data.get() }
    }

    /// The backing store, through exclusive access to the container. No lock
    /// is taken: `&mut self` already proves nothing else can reach it.
    pub fn get_mut(&mut self) -> &mut T {
        self.data.get_mut()
    }
}

impl<T> From<T> for LockedContainer<T> {
    fn from(data: T) -> Self {
        Self::new(data)
    }
}

/// The bytes a value's backing storage occupies in its own allocations.
///
/// This is capacity, not length: it is what the allocator handed out, so a
/// container that grew and was then emptied still reports the buffer it holds.
pub trait Footprint {
    fn footprint(&self) -> usize;
}

impl<T> Footprint for Vec<T> {
    fn footprint(&self) -> usize {
        self.capacity().saturating_mul(size_of::<T>())
    }
}

// Cloning takes the lock so a concurrent writer can't tear `data` mid-clone.
// The contention state (the in-flight access counter) is not part of the
// logical value of the source.
impl<T: Clone> Clone for LockedContainer<T> {
    fn clone(&self) -> Self {
        let guard = self.lock();
        Self::new(guard.clone())
    }
}

impl<T> LockedContainer<Vec<T>> {
    /// Locked convenience: number of elements.
    pub fn len(&self) -> usize {
        self.lock().len()
    }

    /// Locked convenience: whether the container is empty.
    pub fn is_empty(&self) -> bool {
        self.lock().is_empty()
    }
}

impl<T: Copy> LockedContainer<Vec<T>> {
    /// Locked convenience: copy the element at `idx`, or `None` if out of bounds.
    pub fn get(&self, idx: usize) -> Option<T> {
        self.lock().get(idx).copied()
    }
}

impl<T: Clone> LockedContainer<Vec<T>> {
    /// Locked convenience: snapshot the underlying `Vec<T>`.
    pub fn to_vec(&self) -> Vec<T> {
        self.lock().clone()
    }
}

/// Read guard for a [`LockedContainer`]. Holds the container's
/// [`LazyBiasedMutex`] for the duration of the guard's lifetime.
pub struct LockedReadGuard<'a, T> {
    data: &'a T,
    /// `None` for a thread-local access, which takes no lock.
    _access: Option<crate::lazy_biased_mutex::AccessGuard<'a>>,
}

impl<T> std::ops::Deref for LockedReadGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

/// Write guard for a [`LockedContainer`]. Holds the container's
/// [`LazyBiasedMutex`] for the duration of the guard's lifetime, and charges
/// the change in the store's footprint to an [`AllocDebt`] when dropped.
pub struct LockedWriteGuard<'a, T: Footprint> {
    data: &'a mut T,
    /// `None` for a thread-local access, which takes no lock.
    _access: Option<crate::lazy_biased_mutex::AccessGuard<'a>>,
    debt: &'a AllocDebt,
    /// The store's footprint when the guard was taken.
    before: usize,
}

impl<T: Footprint> Drop for LockedWriteGuard<'_, T> {
    fn drop(&mut self) {
        // Still under the container's lock, so the footprint cannot change
        // between measuring it and charging it. Growth is positive; a store
        // replaced by a smaller one, or shrunk, is a release.
        let after = self.data.footprint();
        let before = self.before;
        let delta = if after >= before {
            isize::try_from(after - before).unwrap_or(isize::MAX)
        } else {
            isize::try_from(before - after).map_or(isize::MIN, |shrink| -shrink)
        };
        self.debt.add(delta);
    }
}

impl<T: Footprint> std::ops::Deref for LockedWriteGuard<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.data
    }
}

impl<T: Footprint> std::ops::DerefMut for LockedWriteGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        self.data
    }
}

/// Heap-mutable byte-array container: a `Vec<u8>` behind a
/// [`LockedContainer`].
pub type Uint8ArrayContainer = LockedContainer<Vec<u8>>;
pub type Uint8ArrayReadGuard<'a> = LockedReadGuard<'a, Vec<u8>>;
pub type Uint8ArrayWriteGuard<'a> = LockedWriteGuard<'a, Vec<u8>>;

#[cfg(test)]
mod guard_tests {
    use super::*;
    use crate::Value;

    /// A write guard charges what the store's footprint changed by while it
    /// was held: growth is charged, a shrink is credited, and an in-place
    /// write charges nothing.
    #[test]
    fn write_guards_charge_the_change_in_footprint() {
        let debt = AllocDebt::new();
        let array: LockedContainer<Vec<Value>> = LockedContainer::new(vec![Value::int(1); 4]);

        array.lock_mut(&debt)[..].fill(Value::int(0));
        assert_eq!(debt.balance(), 0, "an in-place write costs nothing");

        {
            let mut guard = array.lock_mut(&debt);
            guard.extend((0..100).map(Value::int));
        }
        let grown = debt.balance();
        assert!(
            grown >= 96 * size_of::<Value>().cast_signed(),
            "{grown} bytes charged for growing from 4 to at least 104 elements"
        );

        array.lock_mut(&debt).clear();
        assert_eq!(debt.balance(), grown, "clearing keeps the capacity");

        // The initial four elements were never charged through a guard, so
        // replacing the store credits them too: a guard reports the change
        // it saw, not the history of the account.
        *array.lock_mut(&debt) = Vec::new();
        assert_eq!(
            debt.balance(),
            -(4 * size_of::<Value>().cast_signed()),
            "replacing the store credits its whole capacity"
        );
    }

    /// Thread-local guards take no lock, still charge footprint, and in debug
    /// builds catch a container someone else holds: the classifier was wrong.
    #[test]
    fn thread_local_guards_skip_the_lock_and_still_charge() {
        let debt = AllocDebt::new();
        let array: LockedContainer<Vec<Value>> = LockedContainer::new(vec![Value::int(1)]);
        // SAFETY: nothing else can reach `array`.
        unsafe { array.lock_mut_thread_local(&debt) }.extend((0..100).map(Value::int));
        assert!(
            debt.balance() > 0,
            "growth through a thread-local guard is charged"
        );
        // SAFETY: as above.
        assert_eq!(unsafe { array.lock_thread_local() }.len(), 101);

        if cfg!(debug_assertions) {
            let held = array.lock();
            let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                // SAFETY: deliberately violated; the debug check must panic
                // before the store is read.
                let _ = unsafe { array.lock_thread_local() }.len();
            }));
            drop(held);
            assert!(
                caught.is_err(),
                "a held lock must fail a thread-local access"
            );
        }
    }
}
