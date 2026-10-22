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
use std::{
    mem::ManuallyDrop,
    sync::{
        Arc, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
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

/// A storage waiting to be reused, and the capacity it was measured at.
struct Idle {
    storage: Box<Storage>,
    bytes: usize,
}
struct State {
    free: Vec<Idle>,
    allocated: usize,
    in_use: usize,
    misses: usize,
    idle_bytes: usize,
}
pub(crate) struct Pool {
    state: Mutex<State>,
    /// Structural capacity the captures with the producers may hold.
    budget: usize,
    /// What they hold now: the sum of every storage's `charged`. On a line of
    /// its own: producers charge it while the processor and the pool's lock
    /// holders uncharge.
    held: Padded<AtomicUsize>,
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
        let now = self.capacity_bytes();
        let before = self.charged.swap(now, Ordering::Relaxed);
        if let Some(pool) = self.home.upgrade() {
            if now >= before {
                pool.held.0.fetch_add(now - before, Ordering::Relaxed);
            } else {
                pool.held.0.fetch_sub(before - now, Ordering::Relaxed);
            }
        }
    }
    /// Lift the charge. Idempotent: a storage charged nothing stays so.
    pub(crate) fn uncharge(&self) {
        if let Some(pool) = self.home.upgrade() {
            self.uncharge_from(&pool);
        }
    }
    fn uncharge_from(&self, pool: &Pool) {
        let charged = self.charged.swap(0, Ordering::Relaxed);
        if charged != 0 {
            pool.held.0.fetch_sub(charged, Ordering::Relaxed);
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
            state: Mutex::new(State {
                free: Vec::new(),
                allocated: 0,
                in_use: 0,
                misses: 0,
                idle_bytes: 0,
            }),
            budget: budget_bytes,
            held: Padded(AtomicUsize::new(0)),
            limits,
            config,
            #[cfg(any(test, feature = "stats"))]
            counters: Arc::default(),
        }))
    }
    /// `None` while the captures with the producers hold the budget, so a
    /// capture is never refused for its own size; and while another thread
    /// has the pool, which is as brief as taking a storage. Admission can
    /// wait for the processor, but growth never waits for anything. Publish
    /// private records before retrying an exhausted pool.
    pub fn try_acquire(&self) -> Option<Builder> {
        let pool = &self.0;
        if pool.held.0.load(Ordering::Relaxed) >= pool.budget {
            return None;
        }
        let mut state = pool.state.try_lock().ok()?;
        let storage = if let Some(idle) = state.free.pop() {
            state.idle_bytes -= idle.bytes;
            idle.storage
        } else {
            state.allocated += 1;
            state.misses += 1;
            #[cfg(any(test, feature = "stats"))]
            pool.counters.add(std::mem::size_of::<Storage>());
            Box::new(Storage {
                home: Arc::downgrade(pool),
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
        state.in_use += 1;
        drop(state);
        // The lease owns cleanup even if growth fails partway through.
        let mut lease = Lease(ManuallyDrop::new(storage));
        let Storage { graph, meter, .. } = &mut *lease;
        graph.values.reserve(pool.config.initial_values, meter);
        graph.objects.reserve(pool.config.initial_objects, meter);
        graph.entries.reserve(pool.config.initial_entries, meter);
        graph.bytes.reserve(pool.config.initial_bytes, meter);
        // Charged for what it starts with; finishing charges what it grew to.
        lease.recharge();
        Some(Builder::new(lease))
    }
    pub fn stats(&self) -> PoolStats {
        let s = self
            .0
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        PoolStats {
            allocated: s.allocated,
            in_use: s.in_use,
            allocation_misses: s.misses,
            idle_bytes: s.idle_bytes,
            held_bytes: self.0.held.0.load(Ordering::Relaxed),
            budget_bytes: self.0.budget,
            #[cfg(any(test, feature = "stats"))]
            allocated_bytes: self.0.counters.bytes.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            peak_accounted_bytes: self.0.counters.peak.load(Ordering::Relaxed),
            #[cfg(any(test, feature = "stats"))]
            growths: self.0.counters.growths.load(Ordering::Relaxed),
        }
    }
}

/// Empty the storage and hand it back to its pool, which keeps it while its
/// idle budget allows. A storage still charged to the budget is uncharged:
/// it left the producers without reaching the processor.
fn recycle(mut storage: Box<Storage>) {
    storage.graph.clear();
    storage.shape.clear();
    let bytes = storage.capacity_bytes();
    if let Some(pool) = storage.home.upgrade() {
        let mut state = pool
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.in_use -= 1;
        // Lifted under the lock admission takes, with the storage about to
        // join the free list: a producer that sees the budget open finds
        // this storage there rather than allocating beside it.
        storage.uncharge_from(&pool);
        if bytes <= pool.config.max_retained_allocation_bytes
            && bytes
                <= pool
                    .config
                    .retention_budget_bytes
                    .saturating_sub(state.idle_bytes)
        {
            state.idle_bytes += bytes;
            state.free.push(Idle { storage, bytes });
            return;
        }
        state.allocated -= 1;
    }
    #[cfg(any(test, feature = "stats"))]
    storage.meter.counters.remove(bytes);
}
