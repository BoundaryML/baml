//! Reused storage for captures. A capture takes one storage from its
//! runtime's pool, and the storage goes back, emptied, when the capture or
//! its snapshot is dropped.
//!
//! The pool budgets the structural memory of the captures that are still with
//! the producers: a capture is charged what its storage holds when it is
//! taken and again when it is finished, and admission is refused while the
//! charges reach the budget. The charge is lifted when the processor takes
//! the capture from the producers ([`Snapshot::uncharge`]), or when the
//! storage goes back. Nothing downstream of the producers counts against the
//! budget: that memory is bounded by whatever holds it there. No capture is
//! refused for its size, and growth never waits.
//!
//! The processor, and any other thread that drops a storage, pushes it onto
//! the pool's return stack with one compare-and-swap. A producer takes from a
//! short free list of its own thread, with no lock and no shared write but
//! the counts. When that list is empty the producer refills it from the
//! pool's spare list, which it first tops up by swapping in the whole return
//! stack. Only producers lock the spare list, once per refill of up to
//! [`PoolConfig::max_local_storages`] storages, so the processor's work never
//! makes a producer wait. Storages are reused in the order they came back.
//! The budget's charges, the idle bytes and the counts are atomics.
//!
//! The free lists are capped, each ([`PoolConfig::max_local_storages`],
//! [`PoolConfig::max_local_bytes`]) and all together
//! ([`PoolConfig::max_local_total_bytes`]), so threads that stop capturing,
//! however many, keep little of what other threads could reuse. A thread gives its list back
//! when it serves another pool, and frees it once its pool is gone, the next
//! time it takes or returns any storage. A thread that does neither keeps its
//! list, at most that cap, until it exits.
use std::{
    cell::RefCell,
    collections::VecDeque,
    mem::ManuallyDrop,
    ptr,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicPtr, AtomicUsize, Ordering},
    },
};

use btel_settings::layout::Padded;

#[cfg(any(test, feature = "stats"))]
use crate::arena::Counters;
use crate::{Builder, arena::Meter, graph::Graph, shape::Shape};

/// Capture policies, separate from pooling. Value limits apply to each
/// value/entry arena. None has no policy limit, subject to the checked u32
/// index range. Object traversal is iterative.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub max_values: Option<usize>,
    pub max_objects: Option<usize>,
    pub max_depth: Option<usize>,
    /// The content of one string, bigint (as its encoded limbs) or
    /// `uint8array`. A larger one is captured as truncated, with none of its
    /// content, and a map entry with a larger key is left out. Never absent:
    /// a blob writes these lengths in 32 bits.
    pub max_leaf_bytes: usize,
}
impl Default for Limits {
    /// The limits captures are made with: only
    /// [`MAX_LEAF_BYTES`](btel_settings::snapshot::MAX_LEAF_BYTES).
    fn default() -> Self {
        Self {
            max_values: None,
            max_objects: None,
            max_depth: None,
            max_leaf_bytes: btel_settings::snapshot::MAX_LEAF_BYTES,
        }
    }
}
impl Limits {
    /// Whether a leaf with `bytes` of content is captured.
    pub(crate) fn holds_leaf(&self, bytes: usize) -> bool {
        bytes <= self.max_leaf_bytes
    }
}
#[derive(Clone, Copy, Debug)]
pub struct PoolConfig {
    pub initial_values: usize,
    pub initial_objects: usize,
    pub initial_entries: usize,
    pub initial_bytes: usize,
    pub retention_budget_bytes: usize,
    pub max_retained_allocation_bytes: usize,
    /// Idle storages one thread keeps for itself, by count and by
    /// structural bytes; at least one is always kept.
    pub max_local_storages: usize,
    pub max_local_bytes: usize,
    /// What all threads' free lists may hold together, in structural bytes.
    pub max_local_total_bytes: usize,
}
impl Default for PoolConfig {
    fn default() -> Self {
        use btel_settings::snapshot as settings;
        Self {
            initial_values: settings::INITIAL_VALUES,
            initial_objects: settings::INITIAL_OBJECTS,
            initial_entries: settings::INITIAL_ENTRIES,
            initial_bytes: settings::INITIAL_BYTES,
            retention_budget_bytes: settings::RETENTION_BUDGET_BYTES,
            max_retained_allocation_bytes: settings::MAX_RETAINED_ALLOCATION_BYTES,
            max_local_storages: settings::LOCAL_FREE_STORAGES,
            max_local_bytes: settings::LOCAL_FREE_BYTES,
            max_local_total_bytes: settings::LOCAL_FREE_TOTAL_BYTES,
        }
    }
}
/// Storages across free and checked-out owners. Shared leaf backing is not
/// structural capacity and is never counted.
#[derive(Clone, Copy, Debug, Default)]
pub struct PoolStats {
    pub allocated: usize,
    pub in_use: usize,
    pub allocation_misses: usize,
    /// Structural capacity of the storages waiting to be reused.
    pub idle_bytes: usize,
    /// Structural capacity charged to the budget: the captures still with
    /// the producers.
    pub held_bytes: usize,
    /// The budget those charges are admitted under.
    pub budget_bytes: usize,
    /// The most `held_bytes` has been.
    #[cfg(any(test, feature = "stats"))]
    pub peak_held_bytes: usize,
    /// Storages taken beyond the budget.
    #[cfg(any(test, feature = "stats"))]
    pub overage_acquisitions: usize,
    /// Structural capacity of every storage, idle or checked out.
    #[cfg(any(test, feature = "stats"))]
    pub allocated_bytes: usize,
    /// The most `allocated_bytes` has been, counting an arena's old and new
    /// allocation together while it grows.
    #[cfg(any(test, feature = "stats"))]
    pub peak_accounted_bytes: usize,
    #[cfg(any(test, feature = "stats"))]
    pub growths: usize,
}

/// Idle storages, boxed: a lease holds the box, so a storage never moves.
type Storages = Vec<Box<Storage>>;
/// A storage on the spare list, with what it is counted at while idle: a
/// thread refilling from the list sizes what it takes without reading the
/// storages, which are often cold in its cache.
struct Spare {
    storage: Box<Storage>,
    idle: usize,
}
/// Storages taken off the return stack and not yet handed to a thread, in
/// runs as they were taken: the oldest run first, and in each run the oldest
/// storage last. A run joins or leaves in one step, so the lock is held for
/// at most one thread's share of storages, never for a whole return stack.
#[derive(Default)]
struct Spares {
    runs: VecDeque<Vec<Spare>>,
    len: usize,
}

/// What both sides change on every take and return, on one line so that
/// either side's turn moves it once.
struct Shared {
    /// Storages returned since a producer last took them all: a stack linked
    /// through [`Storage::next`]. Pushed one at a time, only ever emptied
    /// whole, so it has no ABA problem and no node is read by two threads.
    returned: AtomicPtr<Storage>,
    /// Structural capacity of the storages waiting to be reused: on the
    /// return stack, on the spare list, or in a thread's free list.
    idle_bytes: AtomicUsize,
    in_use: AtomicUsize,
    allocated: AtomicUsize,
    misses: AtomicUsize,
    /// How many storages the spare list held at its last change, so that a
    /// producer skips its lock while it is empty. A stale read costs at most
    /// an allocation or a lock taken for nothing.
    spare_len: AtomicUsize,
    /// What the threads' free lists hold together, as charged when each was
    /// filled, against [`PoolConfig::max_local_total_bytes`].
    local_bytes: AtomicUsize,
}
pub(crate) struct Pool {
    shared: Padded<Shared>,
    /// Storages taken off the return stack and not yet handed to a thread.
    /// Locked only by producers refilling their free lists and by threads
    /// giving theirs back, never by the processor.
    spare: Mutex<Spares>,
    /// Structural capacity the captures with the producers may hold.
    budget: usize,
    /// What they hold now: the sum of every storage's `charged`. On a line of
    /// its own: producers charge it while the processor and returning
    /// threads uncharge.
    held: Padded<AtomicUsize>,
    #[cfg(any(test, feature = "stats"))]
    peak_held: AtomicUsize,
    #[cfg(any(test, feature = "stats"))]
    overage_acquisitions: AtomicUsize,
    limits: Limits,
    config: PoolConfig,
    #[cfg(any(test, feature = "stats"))]
    counters: Arc<Counters>,
}
/// Runtime-owned pool. A capture or snapshot that outlives it frees its own
/// storage.
#[derive(Clone)]
pub struct SnapshotPool(Arc<Pool>);

/// One capture's memory: what was captured, and how it was shaped.
pub(crate) struct Storage {
    /// The pool this goes back to.
    home: Weak<Pool>,
    /// The next storage on its pool's return stack, while this is on it.
    next: AtomicPtr<Storage>,
    /// What this storage is counted at in its pool's `idle_bytes` while it
    /// waits to be reused.
    idle: usize,
    /// What this storage is charged against its pool's budget: its capacity
    /// as of its last charge, or zero once the charge is lifted.
    charged: AtomicUsize,
    pub(crate) limits: Limits,
    pub(crate) meter: Meter,
    pub(crate) graph: Graph,
    /// Empty until the capture is finished.
    pub(crate) shape: Shape,
}
impl Storage {
    /// Structural capacity: this descriptor and its arenas, whatever they
    /// hold.
    pub(crate) fn capacity_bytes(&self) -> usize {
        std::mem::size_of::<Self>()
            .saturating_add(self.graph.capacity_bytes())
            .saturating_add(self.shape.capacity_bytes())
    }
    /// Charge the pool what this storage holds now, in place of what it was
    /// charged before.
    pub(crate) fn recharge(&self) {
        if let Some(pool) = self.home.upgrade() {
            self.recharge_to(&pool);
        }
    }
    fn recharge_to(&self, pool: &Pool) {
        let now = self.capacity_bytes();
        let before = self.charged.swap(now, Ordering::Relaxed);
        if now >= before {
            let added = now - before;
            #[cfg_attr(not(any(test, feature = "stats")), expect(unused_variables))]
            let held = pool.held.0.fetch_add(added, Ordering::Relaxed);
            #[cfg(any(test, feature = "stats"))]
            pool.peak_held.fetch_max(held + added, Ordering::Relaxed);
        } else {
            pool.held.0.fetch_sub(before - now, Ordering::Relaxed);
        }
    }
    /// Lift the charge. Idempotent: a storage charged nothing stays so.
    pub(crate) fn uncharge(&self) {
        if let Some(pool) = self.home.upgrade() {
            let charged = self.charged.swap(0, Ordering::Relaxed);
            pool.lift(charged);
        }
    }
}

/// Exclusive, pointer-sized hold on a storage, which goes back to its pool
/// when this is dropped.
pub(crate) struct Lease(ManuallyDrop<Box<Storage>>);
impl std::ops::Deref for Lease {
    type Target = Storage;
    fn deref(&self) -> &Storage {
        &self.0
    }
}
impl std::ops::DerefMut for Lease {
    fn deref_mut(&mut self) -> &mut Storage {
        &mut self.0
    }
}
impl Drop for Lease {
    #[expect(unsafe_code, reason = "the one move out of a `ManuallyDrop` field")]
    fn drop(&mut self) {
        // SAFETY: this is the sole owner; ManuallyDrop prevents a second Box drop.
        recycle(unsafe { ManuallyDrop::take(&mut self.0) });
    }
}

impl SnapshotPool {
    /// A pool admitting captures while those with the producers hold less
    /// than `budget_bytes` of structural capacity.
    pub fn new(budget_bytes: usize, limits: Limits) -> Self {
        Self::with_config(budget_bytes, limits, PoolConfig::default())
    }
    /// A pool with no budget: the caller bounds admission itself, as one
    /// taken for a single capture does.
    pub fn unbounded(limits: Limits) -> Self {
        Self::new(usize::MAX, limits)
    }
    pub fn with_config(budget_bytes: usize, limits: Limits, config: PoolConfig) -> Self {
        assert!(budget_bytes > 0);
        assert!(
            [limits.max_values, limits.max_objects]
                .into_iter()
                .flatten()
                .chain([limits.max_leaf_bytes])
                .all(|n| n < u32::MAX as usize)
        );
        Self(Arc::new(Pool {
            shared: Padded(Shared {
                returned: AtomicPtr::new(ptr::null_mut()),
                idle_bytes: AtomicUsize::new(0),
                in_use: AtomicUsize::new(0),
                allocated: AtomicUsize::new(0),
                misses: AtomicUsize::new(0),
                spare_len: AtomicUsize::new(0),
                local_bytes: AtomicUsize::new(0),
            }),
            spare: Mutex::new(Spares::default()),
            budget: budget_bytes,
            held: Padded(AtomicUsize::new(0)),
            #[cfg(any(test, feature = "stats"))]
            peak_held: AtomicUsize::new(0),
            #[cfg(any(test, feature = "stats"))]
            overage_acquisitions: AtomicUsize::new(0),
            limits,
            config,
            #[cfg(any(test, feature = "stats"))]
            counters: Arc::default(),
        }))
    }
    /// `None` only while the captures with the producers hold the budget, so
    /// a capture is never refused for its own size. Never waits: admission
    /// can be refused until the processor lifts charges, but taking and
    /// growing a storage wait for nothing. Publish private records before
    /// retrying an exhausted pool.
    pub fn try_acquire(&self) -> Option<Builder> {
        let pool = &self.0;
        // Acquire pairs with the release in `lift`: a producer that sees a
        // returned storage's charge lifted finds that storage returned.
        if pool.held.0.load(Ordering::Acquire) >= pool.budget {
            return None;
        }
        Some(self.take())
    }
    /// Take storage whatever the budget says: the producers' backstop, after
    /// probing for the budget to open ([`btel_settings::overage`]). Never
    /// waits.
    pub fn acquire_beyond_budget(&self) -> Builder {
        #[cfg(any(test, feature = "stats"))]
        self.0.overage_acquisitions.fetch_add(1, Ordering::Relaxed);
        self.take()
    }
    /// What the captures with the producers hold beyond the budget.
    pub fn overage_bytes(&self) -> usize {
        self.0
            .held
            .0
            .load(Ordering::Relaxed)
            .saturating_sub(self.0.budget)
    }
    fn take(&self) -> Builder {
        let pool = &self.0;
        let shared = &pool.shared.0;
        // Unavailable only while this thread's locals are being destroyed;
        // a fresh storage serves then.
        let reused = LOCAL
            .try_with(|local| {
                let mut local = local.try_borrow_mut().ok()?;
                local.serve(pool);
                if local.free.is_empty() {
                    let Local { free, charged, .. } = &mut *local;
                    pool.refill(free, charged);
                }
                local.free.pop()
            })
            .ok()
            .flatten();
        let storage = if let Some(storage) = reused {
            shared.idle_bytes.fetch_sub(storage.idle, Ordering::Relaxed);
            storage
        } else {
            shared.allocated.fetch_add(1, Ordering::Relaxed);
            shared.misses.fetch_add(1, Ordering::Relaxed);
            #[cfg(any(test, feature = "stats"))]
            pool.counters.add(std::mem::size_of::<Storage>());
            Box::new(Storage {
                home: Arc::downgrade(pool),
                next: AtomicPtr::new(ptr::null_mut()),
                idle: 0,
                charged: AtomicUsize::new(0),
                limits: pool.limits,
                meter: Meter {
                    #[cfg(any(test, feature = "stats"))]
                    counters: Arc::clone(&pool.counters),
                },
                graph: Graph::default(),
                shape: Shape::default(),
            })
        };
        shared.in_use.fetch_add(1, Ordering::Relaxed);
        // The lease owns cleanup even if growth fails partway through.
        let mut lease = Lease(ManuallyDrop::new(storage));
        let Storage { graph, meter, .. } = &mut *lease;
        graph.values.reserve(pool.config.initial_values, meter);
        graph.objects.reserve(pool.config.initial_objects, meter);
        graph.entries.reserve(pool.config.initial_entries, meter);
        graph.bytes.reserve(pool.config.initial_bytes, meter);
        // Charged for what it starts with; finishing charges what it grew to.
        lease.recharge_to(pool);
        Builder::new(lease)
    }
    /// What the threads' free lists are charged together.
    #[cfg(test)]
    pub(crate) fn local_bytes(&self) -> usize {
        self.0.shared.0.local_bytes.load(Ordering::Relaxed)
    }
    /// Each count is read on its own, so one taken while other threads take
    /// and return storages need not add up with the others.
    pub fn stats(&self) -> PoolStats {
        let s = &self.0.shared.0;
        PoolStats {
            allocated: s.allocated.load(Ordering::Relaxed),
            in_use: s.in_use.load(Ordering::Relaxed),
            allocation_misses: s.misses.load(Ordering::Relaxed),
            idle_bytes: s.idle_bytes.load(Ordering::Relaxed),
            held_bytes: self.0.held.0.load(Ordering::Relaxed),
            budget_bytes: self.0.budget,
            #[cfg(any(test, feature = "stats"))]
            peak_held_bytes: self.0.peak_held.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            overage_acquisitions: self.0.overage_acquisitions.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            allocated_bytes: self.0.counters.bytes.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            peak_accounted_bytes: self.0.counters.peak.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            growths: self.0.counters.growths.load(Ordering::Relaxed),
        }
    }
}

impl Pool {
    /// Lift `charged` bytes from the budget. Release pairs with the acquire
    /// in [`SnapshotPool::try_acquire`].
    fn lift(&self, charged: usize) {
        if charged != 0 {
            self.held.0.fetch_sub(charged, Ordering::Release);
        }
    }
    /// Put an idle storage, already counted in `idle_bytes`, on the return
    /// stack.
    #[expect(
        unsafe_code,
        reason = "the return stack links boxed storages by raw pointer"
    )]
    fn push_returned(&self, storage: Box<Storage>) {
        let returned = &self.shared.0.returned;
        let node = Box::into_raw(storage);
        let mut head = returned.load(Ordering::Relaxed);
        loop {
            // SAFETY: `node` is this thread's until the exchange publishes it.
            unsafe { (*node).next.store(head, Ordering::Relaxed) };
            // Release publishes the storage and its link to the producer that
            // takes the stack; later pushes continue the release sequence.
            match returned.compare_exchange_weak(head, node, Ordering::Release, Ordering::Relaxed) {
                Ok(_) => return,
                Err(now) => head = now,
            }
        }
    }
    /// Take the whole return stack.
    fn take_returned(&self) -> Returned {
        Returned(
            self.shared
                .0
                .returned
                .swap(ptr::null_mut(), Ordering::Acquire),
        )
    }
    /// Fill a thread's empty free list with as many storages as it may
    /// keep, and at least one if there is any, charging what it keeps beyond
    /// the one it takes now. Storages are reused in the order they came
    /// back, as one unbounded list would: a storage that came back long ago
    /// has left the processor's cache, while one it just emptied would have
    /// to be pulled out of it, line by line.
    fn refill(&self, free: &mut Storages, charged: &mut usize) {
        let shared = &self.shared.0;
        self.uncharge_local(charged);
        // Walked before the lock: the stack yields the newest first, so the
        // run ends with the oldest.
        let run: Vec<Spare> = self
            .take_returned()
            .map(|storage| Spare {
                // Read while the walk has the storage's link in cache.
                idle: storage.idle,
                storage,
            })
            .collect();
        if run.is_empty() && shared.spare_len.load(Ordering::Relaxed) == 0 {
            return;
        }
        // Reserve this thread's share of what all free lists may hold.
        let cap = self.config.max_local_total_bytes;
        let mut allowance = 0;
        let _ = shared
            .local_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |held| {
                allowance = self.config.max_local_bytes.min(cap.saturating_sub(held));
                Some(held + allowance)
            });
        let mut kept = 0usize;
        {
            let mut spare = self.lock_spare();
            let Spares { runs, len } = &mut *spare;
            if !run.is_empty() {
                *len += run.len();
                runs.push_back(run);
            }
            while free.len() < self.config.max_local_storages.max(1) {
                let Some(oldest) = runs.front_mut() else {
                    break;
                };
                let Some(&Spare { idle, .. }) = oldest.last() else {
                    runs.pop_front();
                    continue;
                };
                // The first is taken now and charged as in use, not idle.
                if !free.is_empty() {
                    if kept.saturating_add(idle) > allowance {
                        break;
                    }
                    kept += idle;
                }
                free.extend(oldest.pop().map(|spare| spare.storage));
                *len -= 1;
                if oldest.is_empty() {
                    runs.pop_front();
                }
            }
            shared.spare_len.store(*len, Ordering::Relaxed);
        }
        shared
            .local_bytes
            .fetch_sub(allowance - kept, Ordering::Relaxed);
        *charged = kept;
        // The thread takes from the end of its list: the oldest goes last.
        free.reverse();
    }
    /// Lift a free list's charge against what all free lists may hold.
    fn uncharge_local(&self, charged: &mut usize) {
        let charged = std::mem::take(charged);
        if charged != 0 {
            self.shared
                .0
                .local_bytes
                .fetch_sub(charged, Ordering::Relaxed);
        }
    }
    fn lock_spare(&self) -> std::sync::MutexGuard<'_, Spares> {
        self.spare
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        self.take_returned().for_each(discard);
        let spare = self
            .spare
            .get_mut()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        for spare in spare.runs.drain(..).flatten() {
            discard(spare.storage);
        }
    }
}

/// A taken return stack, owned whole by this thread, yielding its storages.
/// What is not taken from it is freed.
struct Returned(*mut Storage);
impl Iterator for Returned {
    type Item = Box<Storage>;
    #[expect(
        unsafe_code,
        reason = "the return stack links boxed storages by raw pointer"
    )]
    fn next(&mut self) -> Option<Box<Storage>> {
        if self.0.is_null() {
            return None;
        }
        // SAFETY: every node came from `Box::into_raw` in `push_returned`,
        // and taking the stack made this thread its only owner.
        let storage = unsafe { Box::from_raw(self.0) };
        self.0 = storage.next.load(Ordering::Relaxed);
        Some(storage)
    }
}
impl Drop for Returned {
    fn drop(&mut self) {
        self.for_each(discard);
    }
}

/// The storages one thread has taken from a pool and not yet used, at most
/// the pool's local cap. A thread serves one pool at a time; serving another
/// gives these back.
struct Local {
    pool: Weak<Pool>,
    free: Storages,
    /// What `free` is charged against all free lists' cap: what it held
    /// when filled, beyond the storage taken then.
    charged: usize,
}
impl Local {
    /// Hold storages for `pool` from now on.
    fn serve(&mut self, pool: &Arc<Pool>) {
        // The held `Weak` keeps the old pool's address from being reused.
        if !ptr::eq(self.pool.as_ptr(), Arc::as_ptr(pool)) {
            self.give_back();
            self.pool = Arc::downgrade(pool);
        }
    }
    /// Return the unused storages to their pool's spare list, where they stay
    /// counted as idle, or free them if it is gone.
    fn give_back(&mut self) {
        let Some(pool) = self.pool.upgrade() else {
            self.charged = 0;
            self.free.drain(..).for_each(discard);
            return;
        };
        pool.uncharge_local(&mut self.charged);
        if self.free.is_empty() {
            return;
        }
        // Taken from the spare list's front, with the oldest last: back to
        // its front as one run.
        let run: Vec<Spare> = self
            .free
            .drain(..)
            .map(|storage| Spare {
                idle: storage.idle,
                storage,
            })
            .collect();
        let mut spare = pool.lock_spare();
        spare.len += run.len();
        spare.runs.push_front(run);
        pool.shared.0.spare_len.store(spare.len, Ordering::Relaxed);
    }
    /// Free the storages of a pool that is gone.
    fn release_if_orphaned(&mut self) {
        if !self.free.is_empty() && self.pool.strong_count() == 0 {
            self.charged = 0;
            self.free.drain(..).for_each(discard);
        }
    }
}
impl Drop for Local {
    fn drop(&mut self) {
        self.give_back();
    }
}
thread_local! {
    static LOCAL: RefCell<Local> = const {
        RefCell::new(Local {
            pool: Weak::new(),
            free: Vec::new(),
            charged: 0,
        })
    };
}

/// How many storages this thread's free list holds.
#[cfg(test)]
pub(crate) fn local_storages() -> usize {
    LOCAL.with(|local| local.borrow().free.len())
}

/// Free a storage its pool no longer counts.
fn discard(storage: Box<Storage>) {
    #[cfg(any(test, feature = "stats"))]
    storage.meter.counters.remove(storage.capacity_bytes());
    drop(storage);
}

/// Empty the storage and hand it back to its pool, which keeps it while its
/// idle budget allows. A storage still charged to the budget is uncharged:
/// it left the producers without reaching the processor.
fn recycle(mut storage: Box<Storage>) {
    // This thread may hold storages of a pool that has gone since it last
    // took one: free them now rather than when the thread exits.
    let _ = LOCAL.try_with(|local| {
        if let Ok(mut local) = local.try_borrow_mut() {
            local.release_if_orphaned();
        }
    });
    storage.graph.clear();
    storage.shape.clear();
    let bytes = storage.capacity_bytes();
    let Some(pool) = storage.home.upgrade() else {
        discard(storage);
        return;
    };
    let shared = &pool.shared.0;
    let charged = std::mem::take(storage.charged.get_mut());
    shared.in_use.fetch_sub(1, Ordering::Relaxed);
    let retention = pool.config.retention_budget_bytes;
    let kept = bytes <= pool.config.max_retained_allocation_bytes
        && shared
            .idle_bytes
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |idle| {
                (bytes <= retention.saturating_sub(idle)).then(|| idle + bytes)
            })
            .is_ok();
    if kept {
        storage.idle = bytes;
        pool.push_returned(storage);
    } else {
        shared.allocated.fetch_sub(1, Ordering::Relaxed);
        discard(storage);
    }
    // Lifted only once the storage is back: a producer that sees the budget
    // open finds this storage returned rather than allocating beside it.
    pool.lift(charged);
}
