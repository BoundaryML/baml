//! Implements heap access coordination.
//!
//! Each heap should have a corresponding [`HeapPermitManager`].
//! These ensure that we have only one of:
//! - A single exclusive heap access [`HeapGuard`], or
//! - Any number of non-exclusive tracked active heap permits [`ActiveHeapPermit`].

use ::bex_vm_types::{HeapPtr, RootHaver};
use ::core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use ::std::{
    collections::HashMap,
    sync::{Arc, Weak},
};
use ::tokio::sync::Mutex;

/// The lesser of [`u32::MAX`] and [`tokio::sync::Semaphore::MAX_PERMITS`] (depends on compilation target pointer width).
const MAX_PERMITS: u32 = {
    #[cfg(target_pointer_width = "64")]
    {
        u32::MAX
    }
    #[cfg(any(target_pointer_width = "16", target_pointer_width = "32"))]
    #[expect(
        clippy::cast_possible_truncation,
        reason = "This is only on a 32-bit or less target"
    )]
    {
        tokio::sync::Semaphore::MAX_PERMITS as u32
    }
};

pub trait HeapPermit<T: RootHaver> {
    /// Get a reference to the root holder (for example, the active VM)
    ///
    /// Callers can also use [`Deref`] which will return the same value.
    fn holder(&self) -> &T;
    /// Get a mutable reference to the root holder (for example, the active VM)
    ///
    /// Callers can also use [`DerefMut`] which will return the same value.
    fn holder_mut(&mut self) -> &mut T;
    /// Get a type-erased [`PermitProof`] tied to this active permit's lifetime.
    ///
    /// This lets the GC-exclusion proof flow through APIs (e.g. the sys-op
    /// dispatch glue) that cannot name the concrete `T`. The returned proof
    /// is `Copy`, `Send`, and `Sync` and carries no runtime data — the
    /// guarantee comes purely from the lifetime, which cannot outlive `self`.
    fn proof(&self) -> PermitProof<'_>;
}

/// An active heap permit.
///
/// Provides non-exclusive access to the heap for the contained [`RootHaver`].
/// The holder should call [`ActiveHeapPermit::renew`] at safepoints to allow
/// GC and other exclusive access operations to run.
///
/// `ActiveHeapPermit<T>: Send` iff `T: Send` (which is always true, since
/// [`RootHaver: Send`](RootHaver)) and `ActiveHeapPermit<T>: Sync` iff
/// `T: Sync` — so holders that are not `Sync` can still be parked here, but
/// their `&T` views are never observable from more than one thread at a time.
pub struct ActiveHeapPermit<T: RootHaver> {
    state: InactiveHeapPermit<T>,
    _permit: tokio::sync::OwnedSemaphorePermit,
    /// Ties the auto `Send`/`Sync` of `ActiveHeapPermit` to `T`.
    ///
    /// Without this marker, every field of this struct is unconditionally
    /// `Sync` (notably because [`PermitCell<T>`] has a manual unconditional
    /// `unsafe impl Sync` — which is itself load-bearing, so that
    /// `Weak<PermitCell<dyn RootHaver>>` can live in the manager's shared
    /// `Mutex<Vec<…>>`). That would let the compiler auto-derive
    /// `ActiveHeapPermit<T>: Sync` even when `T: !Sync`, and two threads
    /// sharing `&ActiveHeapPermit<T>` could each call [`Self::holder`] to
    /// observe `&T` at the same time — UB when `T: !Sync`.
    _marker: PhantomData<T>,
}
impl<T: RootHaver> ActiveHeapPermit<T> {
    /// Releases the permit.
    /// This allows the GC and other exclusive access operations to run.
    ///
    /// The caller can run [`InactiveHeapPermit::acquire`] to get a new active permit.
    #[inline]
    pub fn release(self) -> InactiveHeapPermit<T> {
        self.state
    }
    /// Shorthand for [`ActiveHeapPermit::release`] followed by [`InactiveHeapPermit::acquire`].
    #[inline]
    pub async fn renew(self) -> Self {
        self.release().acquire().await
    }
}
impl<T: RootHaver> HeapPermit<T> for ActiveHeapPermit<T> {
    fn holder(&self) -> &T {
        // SAFETY: we have a permit to access the heap so we can access the root holder.
        unsafe { self.state.holder() }
    }
    fn holder_mut(&mut self) -> &mut T {
        // SAFETY: we have a permit to access the heap so we can access the root holder.
        unsafe { self.state.holder_mut() }
    }
    fn proof(&self) -> PermitProof<'_> {
        PermitProof {
            _marker: PhantomData,
        }
    }
}

/// A type-erased proof that an [`ActiveHeapPermit`] is held in the current
/// scope (for at least lifetime `'a`).
///
/// Constructed via [`ActiveHeapPermit::proof`]. Carries no runtime data — the
/// GC-exclusion guarantee comes from the lifetime, which is bound by the
/// originating permit's borrow.
#[derive(Clone, Copy)]
pub struct PermitProof<'a> {
    _marker: PhantomData<&'a ()>,
}

impl<T: RootHaver> Deref for ActiveHeapPermit<T> {
    type Target = T;
    #[inline]
    fn deref(&self) -> &Self::Target {
        self.holder()
    }
}

impl<T: RootHaver> DerefMut for ActiveHeapPermit<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.holder_mut()
    }
}

pub struct InactiveHeapPermit<T: RootHaver> {
    active: Arc<tokio::sync::Semaphore>,
    /// This should be the only strong reference, so when it is dropped the [`HeapPermitManager`]'s weak reference
    /// will let it know the permit is no longer needed.
    holder: Arc<PermitCell<T>>,
}
impl<T: RootHaver> InactiveHeapPermit<T> {
    /// Wait for a permit to become available (i.e. as soon as there is no GC or other exclusive access operation running),
    /// and return a [`ActiveHeapPermit`] that can be used to access the heap with the permit.
    pub async fn acquire(self) -> ActiveHeapPermit<T> {
        let permit = Arc::clone(&self.active)
            .acquire_owned()
            .await
            .unwrap_or_else(|_| unreachable!("Semaphore should never be closed"));
        ActiveHeapPermit {
            state: self,
            _permit: permit,
            _marker: PhantomData,
        }
    }
    /// ## Safety
    ///
    /// The caller should not access the heap or use the heap roots unless the permit is active.
    /// They may access other data on the root holder, but not the heap or heap roots.
    pub unsafe fn holder(&self) -> &T {
        let ptr = self.holder.get();
        // SAFETY: caller upholds the fn-level contract — the permit is active,
        // so no other thread (GC or mutator) is concurrently reading/writing
        // through this `PermitCell`.
        unsafe { &*ptr }
    }

    /// ## Safety
    ///
    /// The caller should not access the heap or use the heap roots unless the permit is active.
    /// They may access other data on the root holder, but not the heap or heap roots.
    pub unsafe fn holder_mut(&mut self) -> &mut T {
        let ptr = self.holder.get();
        // SAFETY: caller upholds the fn-level contract — the permit is active
        // and `&mut self` proves exclusive access on this thread.
        unsafe { &mut *ptr }
    }
}

/// For use when multiple threads need to share a single permit.
///
/// Ensures only one thread can use the permit at a time,
/// and yields to exclusive access whenever the permit is released.
pub struct SharedHeapPermit<T: RootHaver> {
    inner: Mutex<InactiveHeapPermit<T>>,
}
impl<T: RootHaver> SharedHeapPermit<T> {
    pub fn new(inner: InactiveHeapPermit<T>) -> Self {
        Self {
            inner: Mutex::new(inner),
        }
    }
    pub async fn acquire(&self) -> SharedHeapPermitGuard<'_, T> {
        let state = self.inner.lock().await;
        let permit = Arc::clone(&state.active)
            .acquire_owned()
            .await
            .unwrap_or_else(|_| unreachable!("Semaphore should never be closed"));
        SharedHeapPermitGuard {
            state,
            _permit: permit,
            _marker: PhantomData,
        }
    }
}

pub struct SharedHeapPermitGuard<'a, T: RootHaver> {
    state: tokio::sync::MutexGuard<'a, InactiveHeapPermit<T>>,
    _permit: tokio::sync::OwnedSemaphorePermit,
    /// Ties the auto `Send`/`Sync` of `SharedHeapPermitGuard` to `T`.
    ///
    /// Without this marker, every field of this struct is unconditionally
    /// `Sync` (notably because [`PermitCell<T>`] has a manual unconditional
    /// `unsafe impl Sync` — which is itself load-bearing, so that
    /// `Weak<PermitCell<dyn RootHaver>>` can live in the manager's shared
    /// `Mutex<Vec<…>>`). That would let the compiler auto-derive
    /// `SharedHeapPermitGuard<T>: Sync` even when `T: !Sync`, and two threads
    /// sharing `&SharedHeapPermitGuard<T>` could each call [`Self::holder`]
    /// to observe `&T` at the same time — UB when `T: !Sync`.
    _marker: PhantomData<T>,
}
impl<'a, T: RootHaver> HeapPermit<T> for SharedHeapPermitGuard<'a, T> {
    fn holder(&self) -> &T {
        // SAFETY: we have a permit to access the heap so we can access the root holder.
        unsafe { self.state.holder() }
    }
    fn holder_mut(&mut self) -> &mut T {
        // SAFETY: we have a permit to access the heap so we can access the root holder.
        unsafe { self.state.holder_mut() }
    }
    fn proof(&self) -> PermitProof<'_> {
        PermitProof {
            _marker: PhantomData,
        }
    }
}
impl<'a, T: RootHaver> Deref for SharedHeapPermitGuard<'a, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.holder()
    }
}
impl<'a, T: RootHaver> DerefMut for SharedHeapPermitGuard<'a, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.holder_mut()
    }
}

/// The central heap coordination system.
///
/// At any given time, there can be either:
/// 1. An exclusive access to the heap (e.g. GC) guarded by a [`HeapGuard`], or
/// 2. Non-exclusive accesses to the heap by permit holders (e.g. VM executor tasks) each guarded by an [`ActiveHeapPermit`].
///
/// There can always be other [`InactiveHeapPermit`]s that have the right to request non-exclusive access to the heap
/// but are not currently holding an active permit.
///
/// Protects the heap during a GC. VM executor tasks should each have an [`ActiveHeapPermit`]/[`InactiveHeapPermit`].
pub struct HeapPermitManager {
    /// Has [`MAX_PERMITS`] semaphore permits.
    ///
    /// This means we can either have:
    /// - A single [`HeapGuard`] holding [`MAX_PERMITS`] semaphore permits, or
    /// - Up to [`MAX_PERMITS`] [`ActiveHeapPermit`]s holding one semaphore permit each.
    active: Arc<tokio::sync::Semaphore>,
    /// Mutex must be held during GC (or other exclusive access operations) to prevent new permits being created during GC.
    holders: tokio::sync::Mutex<Vec<Weak<PermitCell<dyn RootHaver>>>>,
}

impl HeapPermitManager {
    /// Create a new [`HeapPermitManager`]. Only one should exist per heap.
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            active: Arc::new(tokio::sync::Semaphore::const_new(MAX_PERMITS as usize)),
            holders: tokio::sync::Mutex::new(Vec::new()),
        }
    }
    /// Provides a new permit.
    /// If a GC is active, will wait for it to complete.
    pub async fn new_permit<T: RootHaver + 'static>(&self, with_roots: T) -> InactiveHeapPermit<T> {
        let mut guard = self.holders.lock().await;
        debug_assert!(guard.len() < MAX_PERMITS as usize);
        let holder = Arc::new(PermitCell::new(with_roots));
        guard.push(Arc::downgrade(&holder) as Weak<PermitCell<dyn RootHaver>>);
        let permit = InactiveHeapPermit {
            active: self.active.clone(),
            holder,
        };
        drop(guard);
        permit
    }
    pub async fn request_park(&self) -> HeapGuard<'_> {
        // Drain the semaphore BEFORE taking the holders mutex. The semaphore
        // is the stop-the-world barrier: once we hold all MAX_PERMITS, no
        // ActiveHeapPermit::acquire() can complete, so no mutator can run.
        // Taking the mutex first (and then awaiting acquire_many) deadlocks
        // against new_permit(): a VM mid-spawn holds an active permit and
        // wants the mutex; we hold the mutex and want its permit. Latent
        // since #3386 ("New garbage collector"); BEP-034 surfaces it under
        // any spawn-heavy workload.
        let permits = self
            .active
            .acquire_many(MAX_PERMITS)
            .await
            .unwrap_or_else(|_| unreachable!("We do not close the semaphore"));
        let mut guard = self.holders.lock().await;
        guard.retain(|holder| holder.strong_count() > 0);
        HeapGuard {
            guard,
            _permits: permits,
        }
    }
}

/// Guard for exclusive access to the heap. Gotten from [`HeapPermitManager::request_park`].
///
/// When dropped, the guard and permits are returned and exclusive access to the heap is released.
/// Releasing allows the manager to create new permits and for permit holders to access the heap.
pub struct HeapGuard<'a> {
    guard: ::tokio::sync::MutexGuard<'a, Vec<Weak<PermitCell<dyn RootHaver>>>>,
    _permits: ::tokio::sync::SemaphorePermit<'a>,
}
// we are a root haver in the sense that we have access to all root havers
impl RootHaver for HeapGuard<'_> {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        for permit_holder in self.guard.iter() {
            if let Some(permit_holder) = permit_holder.upgrade() {
                let ptr = permit_holder.get();
                // SAFETY: permit holders must be parked during root forwarding
                let root_haver = unsafe { &*ptr };
                root_haver.collect_roots(roots);
            }
        }
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        for permit_holder in self.guard.iter() {
            if let Some(permit_holder) = permit_holder.upgrade() {
                let ptr = permit_holder.get();
                // SAFETY: permit holders must be parked during root forwarding
                let root_haver = unsafe { &mut *ptr };
                root_haver.forward_roots(roots);
            }
        }
    }
}
impl HeapGuard<'_> {
    /// Gets the number of inactive permits when the heap guard was created.
    ///
    /// This means if a permit was dropped after parking, it will still be counted.
    pub fn num_permits(&self) -> usize {
        self.guard.len()
    }
}

/// A cell wrapping a [`RootHaver`] whose access is gated externally by the
/// [`HeapPermitManager`]'s semaphore (for permit holders) and mutex (for the GC).
///
/// `?Sized` so it can hold either a concrete `T: RootHaver` (in the strong `Arc`
/// kept by [`InactiveHeapPermit`]) or `dyn RootHaver` (in the `Weak` references
/// stored in [`HeapPermitManager::holders`]).
#[repr(transparent)]
struct PermitCell<T: ?Sized + RootHaver>(UnsafeCell<T>);

impl<T: RootHaver> PermitCell<T> {
    fn new(value: T) -> Self {
        Self(UnsafeCell::new(value))
    }
}

impl<T: ?Sized + RootHaver> PermitCell<T> {
    fn get(&self) -> *mut T {
        self.0.get()
    }
}

// SAFETY: at most one thread accesses the inner value at any time. Permit holders
// gain access via a semaphore permit; the GC gains access by draining all permits
// while holding the manager mutex. `RootHaver: Send` ensures the inner value is safe
// to move between threads, which is what's actually happening — never true sharing.
unsafe impl<T: ?Sized + RootHaver> Send for PermitCell<T> {}
unsafe impl<T: ?Sized + RootHaver> Sync for PermitCell<T> {}
