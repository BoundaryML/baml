//! §5.5 spawn edges: hybrid aggregation. All equivalent spawns — same
//! `(spawn_ctx_node, child_entry_function)` — share one edge (and, through
//! node interning, one child subtree): 10k identical workers cost one edge
//! row. A bounded instance table preserves the first
//! [`INSTANCES_FIRST`] plus up to [`INSTANCES_EXCEPTIONAL`] exceptional
//! instances; overflow increments `instances_dropped` — aggregates stay
//! lossless, only per-instance identity rows are bounded, explicitly.

use rustc_hash::FxHashMap;

use crate::prof::record::ThreadEndStatus;

/// First-N instances kept per edge.
pub const INSTANCES_FIRST: usize = 64;
/// Additional exceptional (errored/cancelled) instances kept per edge.
pub const INSTANCES_EXCEPTIONAL: usize = 256;

#[derive(Debug, Clone, Copy, Default)]
pub struct EdgeCounters {
    pub spawned: u64,
    pub live: u64,
    pub completed: u64,
    pub errored: u64,
    pub cancelled: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpawnInstance {
    pub thread_id: u64,
    pub start_ns: u64,
    pub end_ns: u64,
    /// `u8::MAX` = still live.
    pub status: u8,
}

/// Edge SoA + intern map + bounded instances.
#[derive(Default)]
pub struct SpawnEdges {
    intern: FxHashMap<(u32, u32), u32>,
    pub parent_node: Vec<u32>,
    pub entry_fn: Vec<u32>,
    pub child_root_node: Vec<u32>,
    pub counters: Vec<EdgeCounters>,
    /// §6.3 kind-3 delta shadows (values at the previous window flush).
    flushed: Vec<EdgeCounters>,
    instances: Vec<Vec<SpawnInstance>>,
    exceptional: Vec<usize>,
    pub instances_dropped: u64,
    /// Open instance start times for final accounting.
    open: FxHashMap<u64, (u32, u64)>,
}

impl SpawnEdges {
    #[must_use]
    pub fn len(&self) -> usize {
        self.parent_node.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.parent_node.is_empty()
    }

    /// Intern the edge for `(spawn_ctx_node, entry_fn)`.
    pub fn intern(&mut self, spawn_ctx_node: u32, entry_fn: u32, child_root_node: u32) -> u32 {
        if let Some(&edge) = self.intern.get(&(spawn_ctx_node, entry_fn)) {
            return edge;
        }
        let edge = u32::try_from(self.parent_node.len()).expect("edge table exceeds u32");
        self.intern.insert((spawn_ctx_node, entry_fn), edge);
        self.parent_node.push(spawn_ctx_node);
        self.entry_fn.push(entry_fn);
        self.child_root_node.push(child_root_node);
        self.counters.push(EdgeCounters::default());
        self.flushed.push(EdgeCounters::default());
        self.instances.push(Vec::new());
        self.exceptional.push(0);
        edge
    }

    pub fn on_spawn(&mut self, edge: u32, thread_id: u64, start_ns: u64) {
        let counters = &mut self.counters[edge as usize];
        counters.spawned += 1;
        counters.live += 1;
        self.open.insert(thread_id, (edge, start_ns));
    }

    pub fn on_end(&mut self, edge: u32, thread_id: u64, status: ThreadEndStatus, end_ns: u64) {
        let counters = &mut self.counters[edge as usize];
        counters.live = counters.live.saturating_sub(1);
        match status {
            ThreadEndStatus::Completed => counters.completed += 1,
            ThreadEndStatus::Errored => counters.errored += 1,
            ThreadEndStatus::Cancelled => counters.cancelled += 1,
        }
        let start_ns = self
            .open
            .remove(&thread_id)
            .map_or(end_ns, |(_, start)| start);
        // Instance preservation (§5.5): first 64, plus up to 256
        // exceptional; the rest are counted drops.
        let exceptional = !matches!(status, ThreadEndStatus::Completed);
        let table = &mut self.instances[edge as usize];
        let keep = table.len() < INSTANCES_FIRST
            || (exceptional && self.exceptional[edge as usize] < INSTANCES_EXCEPTIONAL);
        if keep {
            if exceptional && table.len() >= INSTANCES_FIRST {
                self.exceptional[edge as usize] += 1;
            }
            table.push(SpawnInstance {
                thread_id,
                start_ns,
                end_ns,
                status: status as u8,
            });
        } else {
            self.instances_dropped += 1;
        }
    }

    #[must_use]
    pub fn instances(&self, edge: u32) -> &[SpawnInstance] {
        &self.instances[edge as usize]
    }

    /// §6.1 epoch rotation: remap node references into the new table,
    /// carry totals, and align delta shadows so nothing re-emits.
    pub fn remap_after_epoch(&mut self, mut remap: impl FnMut(u32) -> u32) {
        self.intern.clear();
        for edge in 0..self.parent_node.len() {
            self.parent_node[edge] = remap(self.parent_node[edge]);
            self.child_root_node[edge] = remap(self.child_root_node[edge]);
            self.intern.insert(
                (self.parent_node[edge], self.entry_fn[edge]),
                u32::try_from(edge).unwrap_or(u32::MAX),
            );
            self.flushed[edge] = self.counters[edge];
        }
    }

    /// §6.3 kind-3 delta rows for edges that changed since the previous
    /// flush; shadows catch up. `running_ns_delta`/`awaiting_ns_delta` are
    /// 0 in v1: per-edge time is derivable from the shared child subtree's
    /// nodes (the edge duplicates it only for read convenience) — deferred,
    /// documented, never silently wrong.
    pub fn flush_deltas(&mut self) -> Vec<super::blocks::SpawnEdgeRow> {
        let mut rows = Vec::new();
        for edge in 0..self.counters.len() {
            let cur = self.counters[edge];
            let old = self.flushed[edge];
            let changed = cur.spawned != old.spawned
                || cur.completed != old.completed
                || cur.errored != old.errored
                || cur.cancelled != old.cancelled;
            if changed {
                #[expect(
                    clippy::cast_possible_truncation,
                    reason = "per-window count deltas fit u32 by cadence"
                )]
                rows.push(super::blocks::SpawnEdgeRow {
                    edge_id: u32::try_from(edge).unwrap_or(u32::MAX),
                    parent_node: self.parent_node[edge],
                    entry_fn: self.entry_fn[edge],
                    child_root_node: self.child_root_node[edge],
                    spawn_delta: (cur.spawned - old.spawned).min(u64::from(u32::MAX)) as u32,
                    completed_delta: (cur.completed - old.completed).min(u64::from(u32::MAX))
                        as u32,
                    errored_delta: (cur.errored - old.errored).min(u64::from(u32::MAX)) as u32,
                    cancelled_delta: (cur.cancelled - old.cancelled).min(u64::from(u32::MAX))
                        as u32,
                    running_ns_delta: 0,
                    awaiting_ns_delta: 0,
                });
                self.flushed[edge] = cur;
            }
        }
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equivalent_spawns_share_one_edge_and_instances_bound() {
        let mut edges = SpawnEdges::default();
        let edge = edges.intern(7, 100, 42);
        assert_eq!(edges.intern(7, 100, 42), edge, "one edge for 10k workers");
        for tid in 0..(INSTANCES_FIRST as u64 + 10) {
            edges.on_spawn(edge, tid, tid);
            edges.on_end(edge, tid, ThreadEndStatus::Completed, tid + 5);
        }
        assert_eq!(edges.counters[0].spawned, INSTANCES_FIRST as u64 + 10);
        assert_eq!(edges.counters[0].completed, INSTANCES_FIRST as u64 + 10);
        assert_eq!(edges.counters[0].live, 0);
        assert_eq!(edges.instances(edge).len(), INSTANCES_FIRST);
        assert_eq!(
            edges.instances_dropped, 10,
            "overflow counted, never silent"
        );

        // Exceptional instances keep landing past the first-N bound.
        let tid = 9999;
        edges.on_spawn(edge, tid, 1);
        edges.on_end(edge, tid, ThreadEndStatus::Errored, 2);
        assert_eq!(edges.instances(edge).len(), INSTANCES_FIRST + 1);
        assert_eq!(edges.counters[0].errored, 1);
    }
}
