//! Reused storage for captures. A capture takes one storage from its
//! runtime's pool, and the storage goes back, emptied, when the capture or
//! its snapshot is dropped.
#[cfg(any(test, feature = "stats"))]
use std::sync::atomic::Ordering;
use std::{
    mem::ManuallyDrop,
    sync::{Arc, Mutex, Weak},
};

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
    max: usize,
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
    pub fn new(max: usize, limits: Limits) -> Self {
        Self::with_config(max, limits, PoolConfig::default())
    }
    pub fn with_config(max: usize, limits: Limits, config: PoolConfig) -> Self {
        assert!(max > 0);
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
            max,
            limits,
            config,
            #[cfg(any(test, feature = "stats"))]
            counters: Arc::default(),
        }))
    }
    /// Admission can wait for owners held elsewhere, but growth never waits for
    /// the idle budget. Publish private records before retrying an exhausted pool.
    pub fn try_acquire(&self) -> Option<Builder> {
        let pool = &self.0;
        let mut state = pool.state.try_lock().ok()?;
        let storage = if let Some(idle) = state.free.pop() {
            state.idle_bytes -= idle.bytes;
            idle.storage
        } else {
            if state.allocated == pool.max {
                return None;
            }
            state.allocated += 1;
            state.misses += 1;
            #[cfg(any(test, feature = "stats"))]
            pool.counters.add(std::mem::size_of::<Storage>());
            Box::new(Storage {
                home: Arc::downgrade(pool),
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
/// idle budget allows.
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
