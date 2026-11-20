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
//! parked), or on a heap no manager coordinates at all (a standalone test heap).

use ::bex_vm_types::{PermitProof, RootHaver, RootVisitor};
use ::core::{
    cell::UnsafeCell,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};
use ::std::sync::{
    Arc, Weak,
    atomic::{AtomicIsize, Ordering},
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

/// Dead entries [`HeapPermitManager::holders`] may hold beyond the live ones
/// before a registration drops them.
const DEAD_HOLDER_SLACK: usize = 64;

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
    /// The manager's count of dropped holders, bumped when this permit drops.
    dead_holders: Arc<AtomicIsize>,
}
impl<T: RootHaver> Drop for InactiveHeapPermit<T> {
    fn drop(&mut self) {
        // Counted just before `holder` drops, so the manager's entry for it
        // is dead or about to be. The count only decides when a registration
        // prunes; what is removed is decided by each entry's strong count.
        self.dead_holders.fetch_add(1, Ordering::Relaxed);
    }
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
    /// [`Self::acquire`] without waiting, for a caller that must not suspend
    /// and knows nothing holds the heap exclusively (no collector exists yet,
    /// say). Hands the permit back if a [`HeapGuard`] holds the heap or is
    /// draining it.
    pub fn try_acquire(self) -> Result<ActiveHeapPermit<T>, Self> {
        match Arc::clone(&self.active).try_acquire_owned() {
            Ok(permit) => Ok(ActiveHeapPermit {
                state: self,
                _permit: permit,
                _marker: PhantomData,
            }),
            Err(tokio::sync::TryAcquireError::NoPermits) => Err(self),
            Err(tokio::sync::TryAcquireError::Closed) => {
                unreachable!("Semaphore should never be closed")
            }
        }
    }
    /// ## Safety
    ///
    /// No [`HeapGuard`] may exist while the reference lives: the collector
    /// walks every holder through `&mut`. Holding any active permit of the
    /// same manager guarantees that, whether it is this one's (the permit is
    /// active) or another's (as `FutureManagerGuard` reads its holder). Only
    /// then may the caller touch the holder's heap roots, or the heap.
    pub unsafe fn holder(&self) -> &T {
        let ptr = self.holder.get();
        // SAFETY: caller upholds the fn-level contract — an active permit
        // excludes the collector, and no mutator other than this permit's
        // owner reaches this `PermitCell`.
        unsafe { &*ptr }
    }

    /// ## Safety
    ///
    /// As for [`Self::holder`].
    pub unsafe fn holder_mut(&mut self) -> &mut T {
        let ptr = self.holder.get();
        // SAFETY: caller upholds the fn-level contract, and `&mut self` proves
        // exclusive access on this thread.
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
    ///
    /// An entry outlives its holder until it is pruned, and keeps the
    /// holder's whole `Arc` allocation (a task's thread, VM included) alive
    /// until then, so entries are pruned at registration as well as by the
    /// collector: see [`Self::register`].
    holders: tokio::sync::Mutex<Vec<Weak<PermitCell<dyn RootHaver>>>>,
    /// Holders dropped and not yet pruned: incremented by each permit's drop,
    /// decreased by each prune by what it removed. Outside the mutex, so a
    /// drop never takes a lock; it may briefly overcount (a permit counted
    /// before its entry is dead) or undercount (a prune subtracting before the
    /// matching increment lands), and readers clamp it to the list's length.
    dead_holders: Arc<AtomicIsize>,
    /// Entries visited by prunes, for the tests' cost checks.
    #[cfg(test)]
    scanned: std::sync::atomic::AtomicUsize,
}

impl HeapPermitManager {
    /// Create a new [`HeapPermitManager`]. Only one should exist per heap.
    #[expect(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            active: Arc::new(tokio::sync::Semaphore::const_new(MAX_PERMITS as usize)),
            holders: tokio::sync::Mutex::new(Vec::new()),
            dead_holders: Arc::new(AtomicIsize::new(0)),
            #[cfg(test)]
            scanned: std::sync::atomic::AtomicUsize::new(0),
        }
    }
    /// Provides a new permit.
    /// If a GC is active, will wait for it to complete.
    pub async fn new_permit<T: RootHaver + 'static>(&self, with_roots: T) -> InactiveHeapPermit<T> {
        let mut guard = self.holders.lock().await;
        let permit = self.register(&mut guard, with_roots);
        drop(guard);
        permit
    }
    /// [`Self::new_permit`] without waiting, for a caller that must not
    /// suspend and knows no collection is in progress (none can be before any
    /// collector exists, say). Hands the holder back if the registry is
    /// contended: a [`HeapGuard`] holds it, or another registration is under
    /// way.
    pub fn try_new_permit<T: RootHaver + 'static>(
        &self,
        with_roots: T,
    ) -> Result<InactiveHeapPermit<T>, T> {
        let Ok(mut guard) = self.holders.try_lock() else {
            return Err(with_roots);
        };
        let permit = self.register(&mut guard, with_roots);
        drop(guard);
        Ok(permit)
    }
    /// Add a holder, under the holders lock the caller already holds.
    ///
    /// First drops the dead entries once they outnumber the live ones by more
    /// than [`DEAD_HOLDER_SLACK`], so at every registration the list holds at
    /// most `live + DEAD_HOLDER_SLACK` dead entries (up to the drop counter's
    /// lag), whatever happened since the last collection. A prune visits
    /// `live + dead < 2 × dead` entries and removes the `dead` ones, each of
    /// which was counted once when its permit dropped, so its cost is
    /// amortized over those drops: a constant per registration, however many
    /// holders stay alive.
    fn register<T: RootHaver + 'static>(
        &self,
        holders: &mut Vec<Weak<PermitCell<dyn RootHaver>>>,
        with_roots: T,
    ) -> InactiveHeapPermit<T> {
        let dead = self.dead_holders(holders);
        if dead > holders.len() - dead + DEAD_HOLDER_SLACK {
            self.prune(holders);
        }
        debug_assert!(holders.len() < MAX_PERMITS as usize);
        let holder = Arc::new(PermitCell::new(with_roots));
        holders.push(Arc::downgrade(&holder) as Weak<PermitCell<dyn RootHaver>>);
        InactiveHeapPermit {
            active: self.active.clone(),
            holder,
            dead_holders: Arc::clone(&self.dead_holders),
        }
    }
    /// The drop counter, clamped to the entries there are.
    fn dead_holders(&self, holders: &[Weak<PermitCell<dyn RootHaver>>]) -> usize {
        usize::try_from(self.dead_holders.load(Ordering::Relaxed))
            .unwrap_or(0)
            .min(holders.len())
    }
    /// Drop every entry whose holder is gone, under the holders lock. Only an
    /// entry with no strong reference left is removed: such an entry can never
    /// be upgraded again, while a live one is how the collector finds that
    /// holder's roots. Order is kept. After a burst, the capacity it needed is
    /// released too.
    fn prune(&self, holders: &mut Vec<Weak<PermitCell<dyn RootHaver>>>) {
        #[cfg(test)]
        self.scanned
            .fetch_add(holders.len(), std::sync::atomic::Ordering::Relaxed);
        let before = holders.len();
        holders.retain(|holder| holder.strong_count() > 0);
        let removed = isize::try_from(before - holders.len()).unwrap_or(isize::MAX);
        self.dead_holders.fetch_sub(removed, Ordering::Relaxed);
        if holders.capacity() > 4 * (holders.len() + DEAD_HOLDER_SLACK) {
            holders.shrink_to(2 * (holders.len() + DEAD_HOLDER_SLACK));
        }
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
        self.prune(&mut guard);
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
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        for permit_holder in self.guard.iter() {
            if let Some(permit_holder) = permit_holder.upgrade() {
                let ptr = permit_holder.get();
                // SAFETY: the guard holds every permit, so no active permit
                // reaches the holder, and an inactive permit's `holder` must
                // not be read while a guard exists (its contract). This is the
                // only reference to the holder for the walk.
                let root_haver = unsafe { &mut *ptr };
                root_haver.visit_roots(visitor);
            }
        }
    }

    fn relocated(&mut self) {
        for permit_holder in self.guard.iter() {
            if let Some(permit_holder) = permit_holder.upgrade() {
                let ptr = permit_holder.get();
                // SAFETY: as in `visit_roots`.
                let root_haver = unsafe { &mut *ptr };
                root_haver.relocated();
            }
        }
    }
}
impl HeapGuard<'_> {
    /// Run `f` over every holder that is still registered: each task and
    /// engine participant not currently executing, since the guard has
    /// stopped every one.
    pub fn for_each_holder(&mut self, mut f: impl FnMut(&mut dyn RootHaver)) {
        for permit_holder in self.guard.iter() {
            if let Some(permit_holder) = permit_holder.upgrade() {
                let ptr = permit_holder.get();
                // SAFETY: as in `visit_roots`.
                f(unsafe { &mut *ptr });
            }
        }
    }

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

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::*;

    /// A holder that names itself, so a walk over the holders can say which
    /// ones it saw.
    struct Probe(usize);
    impl RootHaver for Probe {
        fn visit_roots(&mut self, _: &mut dyn RootVisitor) {}
        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }
    }

    fn register(manager: &HeapPermitManager, id: usize) -> InactiveHeapPermit<Probe> {
        manager
            .try_new_permit(Probe(id))
            .unwrap_or_else(|_| panic!("the registry is uncontended in these tests"))
    }

    fn entries(manager: &HeapPermitManager) -> usize {
        manager.holders.try_lock().expect("uncontended").len()
    }

    fn scanned(manager: &HeapPermitManager) -> usize {
        manager.scanned.load(Ordering::Relaxed)
    }

    /// Every entry that is dead now, read without pruning.
    fn dead_entries(manager: &HeapPermitManager) -> usize {
        let holders = manager.holders.try_lock().expect("uncontended");
        holders.iter().filter(|h| h.strong_count() == 0).count()
    }

    #[test]
    fn finished_holders_do_not_pile_up_between_collections() {
        let manager = HeapPermitManager::new();
        for id in 0..100_000 {
            drop(register(&manager, id));
            // One live holder at most at a time: the list never holds more
            // than the slack's worth of dead entries plus the newest one.
            assert!(entries(&manager) <= DEAD_HOLDER_SLACK + 2, "{id}");
        }
    }

    #[tokio::test]
    async fn live_holders_survive_every_prune_and_are_walked_by_the_collector() {
        let manager = HeapPermitManager::new();
        let mut live = Vec::new();
        for id in 0..10_000 {
            let permit = register(&manager, id);
            // Keep every seventh; the rest finish at once.
            if id % 7 == 0 {
                live.push(permit);
            }
        }
        assert!(scanned(&manager) > 0, "registrations pruned");
        let expected: Vec<usize> = (0..10_000).step_by(7).collect();
        let mut guard = manager.request_park().await;
        let mut seen = Vec::new();
        guard.for_each_holder(|holder| {
            let probe = holder
                .as_any_mut()
                .and_then(|any| any.downcast_mut::<Probe>())
                .expect("every holder is a probe");
            seen.push(probe.0);
        });
        assert_eq!(
            seen, expected,
            "each live holder, once, in registration order"
        );
        assert_eq!(
            guard.num_permits(),
            expected.len(),
            "the collector pruned the rest"
        );
        drop(guard);
        drop(live);
    }

    #[test]
    fn a_burst_then_a_quiet_period_does_not_keep_the_burst_alive() {
        let manager = HeapPermitManager::new();
        // A million tasks alive at once, then all finished.
        let burst: Vec<_> = (0..1_000_000).map(|id| register(&manager, id)).collect();
        assert_eq!(entries(&manager), 1_000_000);
        drop(burst);
        // The quiet period: ten long-lived tasks, then a churn of short ones.
        let quiet: Vec<_> = (0..10).map(|id| register(&manager, id)).collect();
        let capacity = manager.holders.try_lock().expect("uncontended").capacity();
        assert!(
            capacity <= 4 * (10 + DEAD_HOLDER_SLACK),
            "the burst's capacity is released ({capacity})"
        );
        for id in 0..1_000_000 {
            drop(register(&manager, id));
            assert!(dead_entries(&manager) <= 10 + DEAD_HOLDER_SLACK + 1, "{id}");
        }
        drop(quiet);
    }

    #[test]
    fn many_live_holders_and_a_churn_of_short_ones_stay_cheap() {
        let manager = HeapPermitManager::new();
        let live: Vec<_> = (0..10_000).map(|id| register(&manager, id)).collect();
        let before = scanned(&manager);
        let spawns = 200_000;
        for id in 0..spawns {
            drop(register(&manager, id));
        }
        let visited = scanned(&manager) - before;
        assert!(
            visited < 3 * spawns,
            "{visited} entries visited for {spawns} registrations"
        );
        // And the dead never outnumber the live by more than the slack.
        assert!(dead_entries(&manager) <= live.len() + DEAD_HOLDER_SLACK + 1);
        drop(live);
    }

    #[test]
    fn the_drop_count_tracks_what_prunes_remove() {
        let manager = HeapPermitManager::new();
        let kept = register(&manager, 0);
        for id in 1..=100 {
            drop(register(&manager, id));
        }
        // 100 dropped, some of them pruned already: the count is what is left.
        let dead = manager.dead_holders.load(Ordering::Relaxed);
        assert_eq!(usize::try_from(dead).unwrap(), dead_entries(&manager));
        drop(kept);
    }
}
