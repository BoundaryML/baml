//! CCT node storage (observability design §5.1): structure-of-arrays,
//! interned by `(parent_node, function_id)` through one FxHash map — the
//! shape of the measured 22 ns/call prototype. Node ids are dense `u32`s,
//! session-epoch-scoped; each node belongs to exactly one partition
//! (subtrees are disjoint since parent chains root at per-partition
//! pseudo-nodes).

use rustc_hash::FxHashMap;

/// Node flag: this node is a §5.6 recursion-fold back-edge target — path
/// uniqueness beyond the fold depth is coarsened, visibly.
pub const NODE_FLAG_RECURSION_FOLD: u8 = 1 << 0;
/// Node flag: a per-partition root pseudo-node (function_id 0, no parent).
pub const NODE_FLAG_PARTITION_ROOT: u8 = 1 << 1;
/// Node flag: this node was synthesized by defer-timeout resync (§5.2) —
/// attribution beneath it is degraded, visibly.
pub const NODE_FLAG_SYNTHESIZED: u8 = 1 << 2;

/// §5.6: past this stack depth, a new `(parent, f)` first scans nearby
/// ancestors for `f` and reuses on hit (back-edge) instead of minting a new
/// node. Corpus max depth is 14; the scan is cold.
pub const RECURSION_FOLD_DEPTH: u16 = 512;
/// §5.6: how many nearest ancestors the fold scans.
pub const RECURSION_FOLD_SCAN: usize = 8;

/// §6.3 kind-9 histogram: 16 buckets on a ×4 stride from 1 µs.
pub const HIST_BUCKETS: usize = 16;

/// Duration → histogram bucket (×4 stride from 1 µs: bucket 0 < 1 µs,
/// bucket 1 < 4 µs, ... bucket 15 ≥ ~17.9 min).
#[inline]
#[must_use]
pub fn hist_bucket(duration_ns: u64) -> usize {
    let us = duration_ns / 1_000;
    if us == 0 {
        0
    } else {
        (((us.ilog2() / 2) + 1) as usize).min(HIST_BUCKETS - 1)
    }
}

/// The per-engine node table. All columns are parallel to node id.
pub struct Nodes {
    /// Keyed `(parent_node << 32) | function_id`: one u64 FxHash multiply
    /// instead of tuple hashing on the per-call path.
    intern: FxHashMap<u64, u32>,
    // identity (immutable after intern)
    pub parent: Vec<u32>,
    pub function: Vec<u32>,
    pub flags: Vec<u8>,
    pub depth: Vec<u16>,
    pub partition: Vec<u32>,
    // counters
    pub enters: Vec<u64>,
    pub ends_ok: Vec<u64>,
    pub ends_err: Vec<u64>,
    pub ends_cancel: Vec<u64>,
    pub ends_exit: Vec<u64>,
    pub total_ns: Vec<u64>,
    pub self_ns: Vec<u64>,
    pub await_ns: Vec<u64>,
    pub hist: Vec<[u32; HIST_BUCKETS]>,
    // delta bookkeeping (§6.3): nodes touched since the last window flush.
    pub dirty_epoch: Vec<u32>,
    // `last_flushed` shadows (§5.1): the values at the previous window
    // flush; a delta row is (current - shadow), then the shadow catches up.
    pub flushed_enters: Vec<u64>,
    pub flushed_ends_ok: Vec<u64>,
    pub flushed_ends_err: Vec<u64>,
    pub flushed_ends_cancel: Vec<u64>,
    pub flushed_ends_exit: Vec<u64>,
    pub flushed_total_ns: Vec<u64>,
    pub flushed_self_ns: Vec<u64>,
    pub flushed_await_ns: Vec<u64>,
    pub flushed_hist: Vec<[u32; HIST_BUCKETS]>,
    /// Births not yet flushed to the session stream (P3 drains this).
    pub unflushed_births: Vec<u32>,
    /// §5.6 diagnostic: frames folded onto back-edges.
    pub folded_frames: u64,
}

impl Nodes {
    #[must_use]
    pub fn with_function_capacity(function_count: u32) -> Nodes {
        // Dense compile-time ids (§4.6) let us pre-size: typical trees
        // intern about as many nodes as functions at first, growing with
        // unique contexts (corpus p99: 3,537).
        let cap = (function_count as usize).clamp(64, 8192);
        Nodes {
            intern: FxHashMap::with_capacity_and_hasher(cap, rustc_hash::FxBuildHasher),
            parent: Vec::with_capacity(cap),
            function: Vec::with_capacity(cap),
            flags: Vec::with_capacity(cap),
            depth: Vec::with_capacity(cap),
            partition: Vec::with_capacity(cap),
            enters: Vec::with_capacity(cap),
            ends_ok: Vec::with_capacity(cap),
            ends_err: Vec::with_capacity(cap),
            ends_cancel: Vec::with_capacity(cap),
            ends_exit: Vec::with_capacity(cap),
            total_ns: Vec::with_capacity(cap),
            self_ns: Vec::with_capacity(cap),
            await_ns: Vec::with_capacity(cap),
            hist: Vec::with_capacity(cap),
            dirty_epoch: Vec::with_capacity(cap),
            flushed_enters: Vec::with_capacity(cap),
            flushed_ends_ok: Vec::with_capacity(cap),
            flushed_ends_err: Vec::with_capacity(cap),
            flushed_ends_cancel: Vec::with_capacity(cap),
            flushed_ends_exit: Vec::with_capacity(cap),
            flushed_total_ns: Vec::with_capacity(cap),
            flushed_self_ns: Vec::with_capacity(cap),
            flushed_await_ns: Vec::with_capacity(cap),
            flushed_hist: Vec::with_capacity(cap),
            unflushed_births: Vec::new(),
            folded_frames: 0,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parent.is_empty()
    }

    fn push_node(
        &mut self,
        parent: u32,
        function_id: u32,
        depth: u16,
        partition: u32,
        flags: u8,
    ) -> u32 {
        let id = u32::try_from(self.parent.len()).expect("node table exceeds u32");
        self.parent.push(parent);
        self.function.push(function_id);
        self.flags.push(flags);
        self.depth.push(depth);
        self.partition.push(partition);
        self.enters.push(0);
        self.ends_ok.push(0);
        self.ends_err.push(0);
        self.ends_cancel.push(0);
        self.ends_exit.push(0);
        self.total_ns.push(0);
        self.self_ns.push(0);
        self.await_ns.push(0);
        self.hist.push([0; HIST_BUCKETS]);
        self.dirty_epoch.push(u32::MAX);
        self.flushed_enters.push(0);
        self.flushed_ends_ok.push(0);
        self.flushed_ends_err.push(0);
        self.flushed_ends_cancel.push(0);
        self.flushed_ends_exit.push(0);
        self.flushed_total_ns.push(0);
        self.flushed_self_ns.push(0);
        self.flushed_await_ns.push(0);
        self.flushed_hist.push([0; HIST_BUCKETS]);
        self.unflushed_births.push(id);
        id
    }

    /// Mint one per-partition root pseudo-node (§5.1). Not interned: each
    /// partition owns its own root even though all roots are
    /// `(no-parent, fn 0)`.
    pub fn partition_root(&mut self, partition: u32) -> u32 {
        self.push_node(u32::MAX, 0, 0, partition, NODE_FLAG_PARTITION_ROOT)
    }

    /// The hot-path primitive: resolve `(parent, function_id)` to a node,
    /// interning a birth on first sight. `parent_depth` is the parent's
    /// depth (child = +1). Applies the §5.6 recursion fold past
    /// [`RECURSION_FOLD_DEPTH`].
    #[inline]
    pub fn intern(&mut self, parent: u32, function_id: u32) -> u32 {
        let key = (u64::from(parent) << 32) | u64::from(function_id);
        if let Some(&node) = self.intern.get(&key) {
            return node;
        }
        self.intern_slow(parent, function_id)
    }

    #[cold]
    fn intern_slow(&mut self, parent: u32, function_id: u32) -> u32 {
        let parent_depth = self.depth[parent as usize];
        let depth = parent_depth.saturating_add(1);
        if depth > RECURSION_FOLD_DEPTH {
            // §5.6: scan ≤8 nearest ancestors for the same function; reuse
            // on hit as a flagged back-edge. Counts and time stay exact;
            // path uniqueness beyond the fold depth coarsens, visibly.
            let mut ancestor = parent;
            for _ in 0..RECURSION_FOLD_SCAN {
                if self.function[ancestor as usize] == function_id {
                    self.flags[ancestor as usize] |= NODE_FLAG_RECURSION_FOLD;
                    self.folded_frames += 1;
                    // Also map the key so subsequent interns are hot-path.
                    self.intern
                        .insert((u64::from(parent) << 32) | u64::from(function_id), ancestor);
                    return ancestor;
                }
                let up = self.parent[ancestor as usize];
                if up == u32::MAX {
                    break;
                }
                ancestor = up;
            }
        }
        let partition = self.partition[parent as usize];
        let id = self.push_node(parent, function_id, depth, partition, 0);
        self.intern
            .insert((u64::from(parent) << 32) | u64::from(function_id), id);
        id
    }

    /// Mint the §5.2 synthesized unattributable child of `parent` (defer
    /// timeout / corrupt-range resync).
    pub fn synthesize_unattributable(&mut self, parent: u32) -> u32 {
        let key = u64::from(parent) << 32;
        if let Some(&node) = self.intern.get(&key) {
            self.flags[node as usize] |= NODE_FLAG_SYNTHESIZED;
            return node;
        }
        let depth = self.depth[parent as usize].saturating_add(1);
        let partition = self.partition[parent as usize];
        let id = self.push_node(parent, 0, depth, partition, NODE_FLAG_SYNTHESIZED);
        self.intern.insert(key, id);
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn intern_is_stable_and_depth_tracked() {
        let mut nodes = Nodes::with_function_capacity(16);
        let root = nodes.partition_root(1);
        let a = nodes.intern(root, 100);
        let b = nodes.intern(a, 101);
        assert_eq!(nodes.intern(root, 100), a);
        assert_eq!(nodes.intern(a, 101), b);
        assert_eq!(nodes.depth[a as usize], 1);
        assert_eq!(nodes.depth[b as usize], 2);
        assert_eq!(nodes.partition[b as usize], 1);
        assert_eq!(nodes.len(), 3);
        assert_eq!(nodes.unflushed_births, vec![root, a, b]);
    }

    #[test]
    fn recursion_fold_engages_past_threshold_only() {
        let mut nodes = Nodes::with_function_capacity(16);
        let root = nodes.partition_root(1);
        // Alternate two functions down to beyond the fold depth (the deep
        // workload's mutual-recursion shape).
        let mut current = root;
        for level in 0..RECURSION_FOLD_DEPTH as u32 {
            current = nodes.intern(current, 100 + (level % 2));
        }
        assert_eq!(nodes.folded_frames, 0, "no fold below the threshold");
        let before = nodes.len();
        // The next intern crosses the threshold: it must fold onto the
        // matching ancestor (same function two frames up).
        let folded = nodes.intern(current, 100 + (RECURSION_FOLD_DEPTH as u32 % 2));
        assert!(nodes.folded_frames > 0, "fold engages past the threshold");
        assert_eq!(nodes.len(), before, "no new node minted for the back-edge");
        assert_ne!(nodes.flags[folded as usize] & NODE_FLAG_RECURSION_FOLD, 0);
    }

    #[test]
    fn hist_buckets_follow_x4_stride() {
        assert_eq!(hist_bucket(0), 0);
        assert_eq!(hist_bucket(999), 0); // < 1 µs
        assert_eq!(hist_bucket(1_000), 1); // 1 µs
        assert_eq!(hist_bucket(3_999), 1);
        assert_eq!(hist_bucket(4_000), 2); // 4 µs
        assert_eq!(hist_bucket(16_000), 3);
        // ≥ ~17.9 min saturates the last bucket.
        assert_eq!(hist_bucket(2_000_000_000_000), HIST_BUCKETS - 1);
    }
}
