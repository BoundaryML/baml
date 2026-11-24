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
//! No lock is taken. The processor and any other thread that drops a storage
//! push it onto the pool's return stack with one compare-and-swap. A producer
//! takes from a free list of its own thread, and when that is empty swaps the
//! whole return stack into it, one atomic instruction. So the processor's
//! work never makes a producer wait, and producers never wait for each other.
//! The budget's charges, the idle bytes and the counts are atomics.
use std::{
    cell::RefCell,
    mem::ManuallyDrop,
    ptr,
    sync::{
        Arc, Weak,
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

/// What both sides change on every take and return, on one line so that
/// either side's turn moves it once.
struct Shared {
    /// Storages returned since a producer last took them all: a stack linked
    /// through [`Storage::next`]. Pushed one at a time, only ever emptied
    /// whole, so it has no ABA problem and no node is read by two threads.
    returned: AtomicPtr<Storage>,
    /// Structural capacity of the storages waiting to be reused, on the
    /// return stack or in a thread's free list.
    idle_bytes: AtomicUsize,
    in_use: AtomicUsize,
    allocated: AtomicUsize,
    misses: AtomicUsize,
}
pub(crate) struct Pool {
    shared: Padded<Shared>,
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
            }),
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
                    pool.take_returned(&mut local.free);
                }
                local.free.pop()
            })
            .ok()
            .flatten();
        let storage = if let Some(storage) = reused {
            // An idle storage is untouched since it was returned, so it
            // measures what it was counted at then.
            shared
                .idle_bytes
                .fetch_sub(storage.capacity_bytes(), Ordering::Relaxed);
            storage
        } else {
            shared.allocated.fetch_add(1, Ordering::Relaxed);
            shared.misses.fetch_add(1, Ordering::Relaxed);
            #[cfg(any(test, feature = "stats"))]
            pool.counters.add(std::mem::size_of::<Storage>());
            Box::new(Storage {
                home: Arc::downgrade(pool),
                next: AtomicPtr::new(ptr::null_mut()),
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
    /// Move every returned storage into `free`.
    #[expect(
        unsafe_code,
        reason = "the return stack links boxed storages by raw pointer"
    )]
    #[expect(clippy::vec_box, reason = "a lease holds the box; storages never move")]
    fn take_returned(&self, free: &mut Vec<Box<Storage>>) {
        let mut node = self
            .shared
            .0
            .returned
            .swap(ptr::null_mut(), Ordering::Acquire);
        while !node.is_null() {
            // SAFETY: every node came from `Box::into_raw` in `push_returned`,
            // and the swap made this thread the stack's only owner.
            let storage = unsafe { Box::from_raw(node) };
            node = storage.next.load(Ordering::Relaxed);
            free.push(storage);
        }
    }
}
impl Drop for Pool {
    fn drop(&mut self) {
        let mut free = Vec::new();
        self.take_returned(&mut free);
        free.into_iter().for_each(discard);
    }
}

/// The storages one thread has taken from a pool's return stack and not yet
/// used. A thread serves one pool at a time; serving another gives these back.
struct Local {
    pool: Weak<Pool>,
    #[expect(clippy::vec_box, reason = "a lease holds the box; storages never move")]
    free: Vec<Box<Storage>>,
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
    /// Return the unused storages to their pool, where they stay counted as
    /// idle, or free them if it is gone.
    fn give_back(&mut self) {
        let free = std::mem::take(&mut self.free);
        match self.pool.upgrade() {
            Some(pool) => free.into_iter().for_each(|s| pool.push_returned(s)),
            None => free.into_iter().for_each(discard),
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
        })
    };
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
        pool.push_returned(storage);
    } else {
        shared.allocated.fetch_sub(1, Ordering::Relaxed);
        discard(storage);
    }
    // Lifted only once the storage is back: a producer that sees the budget
    // open finds this storage returned rather than allocating beside it.
    pool.lift(charged);
}
