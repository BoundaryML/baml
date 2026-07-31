//! Dense, integer-keyed calling-context nodes and counter storage.

use rustc_hash::FxHashMap;

use crate::ids::FunctionId;

pub type NodeId = u32;
pub type Histogram = [u32; 16];

pub const NODE_FLAG_PARTITION_ROOT: u8 = 1 << 0;
pub const NODE_FLAG_UNATTRIBUTABLE: u8 = 1 << 1;
pub const NODE_FLAG_RECURSION_FOLD: u8 = 1 << 2;
const INTERN_CACHE_SLOTS: usize = 4096;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeCounters {
    pub enters: u64,
    pub ends_ok: u64,
    pub ends_err: u64,
    pub ends_cancel: u64,
    pub ends_exit: u64,
    pub total_ns: u64,
    pub self_ns: u64,
    pub await_ns: u64,
}

impl NodeCounters {
    pub(crate) fn is_zero(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NodeIdentity {
    pub parent: NodeId,
    pub function_id: FunctionId,
    pub first_thread_id: u64,
    pub partition: u32,
    pub flags: u8,
    pub depth: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NodeSnapshot {
    pub node_id: NodeId,
    pub identity: NodeIdentity,
    pub counters: NodeCounters,
    pub histogram: Histogram,
}

#[derive(Debug)]
pub(crate) struct NodeStore {
    identity: Vec<NodeIdentity>,
    totals: Vec<NodeCounters>,
    hist_totals: Vec<Histogram>,
    window: Vec<NodeCounters>,
    hist_window: Vec<Histogram>,
    dirty: Vec<bool>,
    dirty_nodes: Vec<NodeId>,
    intern: FxHashMap<u64, NodeId>,
    /// A correctness-neutral direct cache in front of the canonical map.
    /// Function ids are dense in normal runs, making repeated child lookups
    /// a pair of integer comparisons even at the committed 4096-function
    /// benchmark cardinality. Collisions simply fall back to `intern`.
    intern_cache_keys: Vec<u64>,
    intern_cache_nodes: Vec<NodeId>,
}

impl Default for NodeStore {
    fn default() -> Self {
        let mut store = Self {
            identity: Vec::new(),
            totals: Vec::new(),
            hist_totals: Vec::new(),
            window: Vec::new(),
            hist_window: Vec::new(),
            dirty: Vec::new(),
            dirty_nodes: Vec::new(),
            intern: FxHashMap::default(),
            intern_cache_keys: vec![0; INTERN_CACHE_SLOTS],
            intern_cache_nodes: vec![NodeId::MAX; INTERN_CACHE_SLOTS],
        };
        // Node zero is the engine-wide recovery anchor. Per-partition
        // unattributable nodes are children of the partition's unique root.
        store.push_node(NodeIdentity {
            parent: 0,
            function_id: FunctionId(0),
            first_thread_id: 0,
            partition: 0,
            flags: NODE_FLAG_UNATTRIBUTABLE,
            depth: 0,
        });
        store
    }
}

impl NodeStore {
    fn push_node(&mut self, identity: NodeIdentity) -> NodeId {
        let id = u32::try_from(self.identity.len()).unwrap_or(u32::MAX);
        self.identity.push(identity);
        self.totals.push(NodeCounters::default());
        self.hist_totals.push([0; 16]);
        self.window.push(NodeCounters::default());
        self.hist_window.push([0; 16]);
        self.dirty.push(false);
        id
    }

    pub(crate) fn new_partition_root(&mut self, partition: u32, thread_id: u64) -> NodeId {
        self.push_node(NodeIdentity {
            parent: 0,
            function_id: FunctionId(0),
            first_thread_id: thread_id,
            partition,
            flags: NODE_FLAG_PARTITION_ROOT,
            depth: 0,
        })
    }

    pub(crate) fn unattributable(
        &mut self,
        root: NodeId,
        partition: u32,
        thread_id: u64,
    ) -> NodeId {
        let node = self.intern_node(root, FunctionId(0), partition, thread_id);
        self.identity[node as usize].flags |= NODE_FLAG_UNATTRIBUTABLE;
        node
    }

    #[inline]
    pub(crate) fn intern_node(
        &mut self,
        parent: NodeId,
        function_id: FunctionId,
        partition: u32,
        thread_id: u64,
    ) -> NodeId {
        let key = u64::from(parent) << 32 | u64::from(function_id.0);
        let cache_index = (function_id.0 as usize ^ (parent as usize).wrapping_mul(0x9e37_79b1))
            & (INTERN_CACHE_SLOTS - 1);
        let cached = self.intern_cache_nodes[cache_index];
        if cached != NodeId::MAX && self.intern_cache_keys[cache_index] == key {
            return cached;
        }
        if let Some(&node) = self.intern.get(&key) {
            self.intern_cache_keys[cache_index] = key;
            self.intern_cache_nodes[cache_index] = node;
            return node;
        }
        let parent_depth = self.identity[parent as usize].depth;
        let node = self.push_node(NodeIdentity {
            parent,
            function_id,
            first_thread_id: thread_id,
            partition,
            flags: 0,
            depth: parent_depth.saturating_add(1),
        });
        self.intern.insert(key, node);
        self.intern_cache_keys[cache_index] = key;
        self.intern_cache_nodes[cache_index] = node;
        node
    }

    pub(crate) fn flag(&mut self, node: NodeId, flag: u8) {
        self.identity[node as usize].flags |= flag;
    }

    pub(crate) fn identity(&self, node: NodeId) -> NodeIdentity {
        self.identity[node as usize]
    }

    #[inline]
    pub(crate) fn enter(&mut self, node: NodeId) {
        self.totals[node as usize].enters = self.totals[node as usize].enters.saturating_add(1);
        self.window[node as usize].enters = self.window[node as usize].enters.saturating_add(1);
        self.mark_dirty(node);
    }

    #[inline]
    pub(crate) fn add_self(&mut self, node: NodeId, elapsed: u64) {
        if elapsed == 0 {
            return;
        }
        self.totals[node as usize].self_ns =
            self.totals[node as usize].self_ns.saturating_add(elapsed);
        self.window[node as usize].self_ns =
            self.window[node as usize].self_ns.saturating_add(elapsed);
        self.mark_dirty(node);
    }

    #[inline]
    pub(crate) fn add_await(&mut self, node: NodeId, elapsed: u64) {
        if elapsed == 0 {
            return;
        }
        self.totals[node as usize].await_ns =
            self.totals[node as usize].await_ns.saturating_add(elapsed);
        self.window[node as usize].await_ns =
            self.window[node as usize].await_ns.saturating_add(elapsed);
        self.mark_dirty(node);
    }

    #[inline]
    pub(crate) fn close(
        &mut self,
        node: NodeId,
        status: crate::prof::record::FunctionEndStatus,
        duration_ns: u64,
    ) {
        use crate::prof::record::FunctionEndStatus;
        let total = &mut self.totals[node as usize];
        let window = &mut self.window[node as usize];
        match status {
            FunctionEndStatus::Ok => {
                total.ends_ok = total.ends_ok.saturating_add(1);
                window.ends_ok = window.ends_ok.saturating_add(1);
            }
            FunctionEndStatus::Errored => {
                total.ends_err = total.ends_err.saturating_add(1);
                window.ends_err = window.ends_err.saturating_add(1);
            }
            FunctionEndStatus::Cancelled => {
                total.ends_cancel = total.ends_cancel.saturating_add(1);
                window.ends_cancel = window.ends_cancel.saturating_add(1);
            }
            FunctionEndStatus::Exited => {
                total.ends_exit = total.ends_exit.saturating_add(1);
                window.ends_exit = window.ends_exit.saturating_add(1);
            }
        }
        total.total_ns = total.total_ns.saturating_add(duration_ns);
        window.total_ns = window.total_ns.saturating_add(duration_ns);
        let bucket = duration_bucket(duration_ns);
        self.hist_totals[node as usize][bucket] =
            self.hist_totals[node as usize][bucket].saturating_add(1);
        self.hist_window[node as usize][bucket] =
            self.hist_window[node as usize][bucket].saturating_add(1);
        self.mark_dirty(node);
    }

    #[inline]
    fn mark_dirty(&mut self, node: NodeId) {
        let dirty = &mut self.dirty[node as usize];
        if !*dirty {
            *dirty = true;
            self.dirty_nodes.push(node);
        }
    }

    pub(crate) fn take_window(
        &mut self,
    ) -> (
        Vec<super::delta::NodeDelta>,
        Vec<super::delta::NodeHistogramDelta>,
    ) {
        let mut deltas = Vec::with_capacity(self.dirty_nodes.len());
        let mut histograms = Vec::new();
        for node in self.dirty_nodes.drain(..) {
            self.dirty[node as usize] = false;
            let counters = std::mem::take(&mut self.window[node as usize]);
            if !counters.is_zero() {
                deltas.push(super::delta::NodeDelta {
                    node_id: node,
                    counters,
                });
            }
            let buckets = std::mem::take(&mut self.hist_window[node as usize]);
            if buckets.iter().any(|count| *count != 0) {
                histograms.push(super::delta::NodeHistogramDelta {
                    node_id: node,
                    buckets,
                });
            }
        }
        (deltas, histograms)
    }

    pub(crate) fn snapshots(&self) -> Vec<NodeSnapshot> {
        self.identity
            .iter()
            .copied()
            .enumerate()
            .map(|(node_id, identity)| NodeSnapshot {
                node_id: u32::try_from(node_id).unwrap_or(u32::MAX),
                identity,
                counters: self.totals[node_id],
                histogram: self.hist_totals[node_id],
            })
            .collect()
    }

    /// Releases the mutable, boundary-scoped portion of a sealed partition.
    ///
    /// Node identities remain allocated until the session epoch rotates:
    /// their numeric ids are part of the already-persisted session stream and
    /// must never be reused within that epoch. Counters, histograms and intern
    /// keys are no longer needed once the boundary snapshot has been sealed.
    pub(crate) fn seal_partition(&mut self, partition: u32) {
        self.intern.retain(|&key, _| {
            let parent = (key >> 32) as NodeId;
            self.identity
                .get(parent as usize)
                .is_none_or(|identity| identity.partition != partition)
        });
        for entry in &mut self.intern_cache_nodes {
            if *entry != NodeId::MAX && self.identity[*entry as usize].partition == partition {
                *entry = NodeId::MAX;
            }
        }
        for (index, identity) in self.identity.iter().enumerate() {
            if identity.partition != partition {
                continue;
            }
            self.totals[index] = NodeCounters::default();
            self.hist_totals[index] = [0; 16];
            self.window[index] = NodeCounters::default();
            self.hist_window[index] = [0; 16];
            self.dirty[index] = false;
        }
        self.dirty_nodes.retain(|node| {
            self.identity
                .get(*node as usize)
                .is_none_or(|identity| identity.partition != partition)
        });
    }
}

#[inline]
fn duration_bucket(duration_ns: u64) -> usize {
    if duration_ns <= 1_000 {
        return 0;
    }
    let mut micros = duration_ns.saturating_add(999) / 1_000;
    let mut bucket = 0usize;
    while micros > 1 && bucket < 15 {
        micros = micros.saturating_add(3) / 4;
        bucket += 1;
    }
    bucket
}

#[cfg(test)]
mod tests {
    use super::duration_bucket;

    #[test]
    fn duration_histogram_has_quarter_decade_stride_and_saturates() {
        assert_eq!(duration_bucket(0), 0);
        assert_eq!(duration_bucket(1_000), 0);
        assert_eq!(duration_bucket(1_001), 1);
        assert_eq!(duration_bucket(4_000), 1);
        assert_eq!(duration_bucket(4_001), 2);
        assert_eq!(duration_bucket(u64::MAX), 15);
    }
}
