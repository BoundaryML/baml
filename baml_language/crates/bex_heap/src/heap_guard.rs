//! Implements heap access coordination.
//!
//! Each heap should have a corresponding [`HeapPermitManager`].
//! These ensure that we have only one of:
//! - A single exclusive heap access [`HeapGuard`], or
//! - Any number of non-exclusive tracked active heap permits [`ActiveHeapPermit`].
//!
//! Spawn-safety invariant: an active permit excludes moving/collecting GC, but
//! it does not serialize multiple VM mutators. Heap objects reachable from
//! spawned VMs must use object-level synchronization for mutable state. Raw
//! heap/container access is only sound while holding [`HeapGuard`] (all mutators
//! parked) or during proven single-threaded setup.

use ::bex_vm_types::{HeapPtr, PermitProof, RootHaver};
use ::core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use ::std::{
    collections::HashMap,
    sync::{Arc, Weak},
};

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

/// The existence of a value that implements this trait proves that the heap is currently accessible for non-exclusive access (e.g. by a VM executor task).
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
        // SAFETY: `&self` proves an `ActiveHeapPermit<T>` is held for the
        // returned proof's lifetime, which is the very invariant
        // `PermitProof::new` requires. This is the canonical safe
        // constructor referenced by `PermitProof`'s docs.
        #[allow(
            unsafe_code,
            reason = "this is the canonical safe constructor of PermitProof"
        )]
        unsafe {
            PermitProof::new()
        }
    }
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
        // wants the mutex; we hold the mutex and want its permit.
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
//
// # `Sync` is unconditional even for `T: !Sync` — why this is sound
//
// The unconditional `Sync` impl below is **structurally** load-bearing:
// `Weak<PermitCell<dyn RootHaver>>` lives in
// `HeapPermitManager::holders: Mutex<Vec<Weak<...>>>`, and `Mutex<Vec<Weak<U>>>`
// requires `U: Send + Sync`. Constraining the bound to `T: Sync` would
// reject every `T: !Sync` `RootHaver` (e.g. `BexVm`, which intentionally
// is not `Sync`).
//
// What rescues soundness is the *safe wrappers* that hand out access to
// the inner `T`: `ActiveHeapPermit<T>` and `SharedHeapPermitGuard<'_, T>`
// each carry a `_marker: PhantomData<T>` field that re-ties the wrapper's
// auto-`Send`/`Sync` derivation to `T`. So while `&PermitCell<T>` is
// `Sync` regardless of `T`, the only safe way to project a `&T` out of
// it is through one of those wrappers, and `&Wrapper<T>: Sync` iff
// `T: Sync`. Two threads cannot simultaneously hold `&Wrapper<T>: Sync`
// for `T: !Sync`, so two threads cannot simultaneously call `.holder()`
// to observe `&T`.
//
// **Maintenance hazard**: any future safe API that returns `&T` from a
// `&PermitCell<T>` *without* the `PhantomData<T>` re-tie (or another
// equivalent `T: Sync` requirement on the consumer) silently breaks the
// contract above. If you add such an API, also tighten this `Sync` impl
// to `T: Sync` (and accept that some `RootHaver`s can no longer be
// holders), or rework the holders Mutex's element type.
unsafe impl<T: ?Sized + RootHaver> Send for PermitCell<T> {}
unsafe impl<T: ?Sized + RootHaver> Sync for PermitCell<T> {}
