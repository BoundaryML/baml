//! Aggregated spawn edges and bounded instance preservation.

use rustc_hash::FxHashMap;

use super::nodes::NodeId;
use crate::prof::record::ThreadEndStatus;

pub type SpawnEdgeId = u32;

pub const FIRST_INSTANCE_LIMIT: usize = 64;
pub const EXCEPTIONAL_INSTANCE_LIMIT: usize = 256;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpawnCounters {
    pub spawned: u64,
    pub live: u64,
    pub completed: u64,
    pub errored: u64,
    pub cancelled: u64,
    pub running_ns: u64,
    pub awaiting_ns: u64,
}

impl SpawnCounters {
    pub(crate) fn is_zero(self) -> bool {
        self == Self::default()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpawnEdgeIdentity {
    pub spawn_context: NodeId,
    pub child_entry_function: u32,
    /// The first calling-context node entered by the child. Persisted in
    /// `spawn_edge` rows so readers can jump from an aggregate edge into the
    /// shared child subtree without reconstructing thread instances.
    pub child_root_node: NodeId,
    pub partition: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnInstance {
    pub thread_id: u64,
    pub name: Option<String>,
    pub start_ns: u64,
    pub end_ns: Option<u64>,
    pub status: Option<ThreadEndStatus>,
    pub edge_id: SpawnEdgeId,
    pub exceptional: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpawnEdgeSnapshot {
    pub edge_id: SpawnEdgeId,
    pub identity: SpawnEdgeIdentity,
    pub counters: SpawnCounters,
}

#[derive(Debug, Default)]
pub(crate) struct SpawnStore {
    intern: FxHashMap<(NodeId, u32), SpawnEdgeId>,
    identities: Vec<SpawnEdgeIdentity>,
    totals: Vec<SpawnCounters>,
    window: Vec<SpawnCounters>,
    dirty: Vec<bool>,
    dirty_edges: Vec<SpawnEdgeId>,
    instances: Vec<SpawnInstance>,
    first_instances: FxHashMap<u32, usize>,
    exceptional_instances: FxHashMap<u32, usize>,
    pub(crate) instances_dropped: u64,
}

impl SpawnStore {
    pub(crate) fn begin(
        &mut self,
        spawn_context: NodeId,
        child_entry_function: u32,
        child_root_node: NodeId,
        partition: u32,
        thread_id: u64,
        name: Option<String>,
        start_ns: u64,
    ) -> SpawnEdgeId {
        let edge = if let Some(&edge) = self.intern.get(&(spawn_context, child_entry_function)) {
            edge
        } else {
            let edge = u32::try_from(self.identities.len()).unwrap_or(u32::MAX);
            self.intern
                .insert((spawn_context, child_entry_function), edge);
            self.identities.push(SpawnEdgeIdentity {
                spawn_context,
                child_entry_function,
                child_root_node,
                partition,
            });
            self.totals.push(SpawnCounters::default());
            self.window.push(SpawnCounters::default());
            self.dirty.push(false);
            edge
        };
        self.bump(edge, |counter| {
            counter.spawned = counter.spawned.saturating_add(1);
            counter.live = counter.live.saturating_add(1);
        });

        let count = self.first_instances.entry(partition).or_default();
        if *count < FIRST_INSTANCE_LIMIT {
            *count += 1;
            self.instances.push(SpawnInstance {
                thread_id,
                name,
                start_ns,
                end_ns: None,
                status: None,
                edge_id: edge,
                exceptional: false,
            });
        }
        edge
    }

    pub(crate) fn add_time(&mut self, edge: SpawnEdgeId, elapsed: u64, awaiting: bool) {
        if elapsed == 0 {
            return;
        }
        self.bump(edge, |counter| {
            if awaiting {
                counter.awaiting_ns = counter.awaiting_ns.saturating_add(elapsed);
            } else {
                counter.running_ns = counter.running_ns.saturating_add(elapsed);
            }
        });
    }

    pub(crate) fn finish(
        &mut self,
        edge: SpawnEdgeId,
        partition: u32,
        thread_id: u64,
        name: Option<String>,
        start_ns: u64,
        end_ns: u64,
        status: ThreadEndStatus,
    ) {
        self.bump(edge, |counter| {
            counter.live = counter.live.saturating_sub(1);
            match status {
                ThreadEndStatus::Completed => {
                    counter.completed = counter.completed.saturating_add(1);
                }
                ThreadEndStatus::Cancelled => {
                    counter.cancelled = counter.cancelled.saturating_add(1);
                }
                ThreadEndStatus::Errored => {
                    counter.errored = counter.errored.saturating_add(1);
                }
            }
        });

        if let Some(instance) = self
            .instances
            .iter_mut()
            .find(|instance| instance.thread_id == thread_id && instance.end_ns.is_none())
        {
            instance.end_ns = Some(end_ns);
            instance.status = Some(status);
            instance.exceptional = status != ThreadEndStatus::Completed;
            return;
        }
        if status == ThreadEndStatus::Completed {
            self.instances_dropped = self.instances_dropped.saturating_add(1);
            return;
        }
        let count = self.exceptional_instances.entry(partition).or_default();
        if *count < EXCEPTIONAL_INSTANCE_LIMIT {
            *count += 1;
            self.instances.push(SpawnInstance {
                thread_id,
                name,
                start_ns,
                end_ns: Some(end_ns),
                status: Some(status),
                edge_id: edge,
                exceptional: true,
            });
        } else {
            self.instances_dropped = self.instances_dropped.saturating_add(1);
        }
    }

    fn bump(&mut self, edge: SpawnEdgeId, update: impl Fn(&mut SpawnCounters)) {
        update(&mut self.totals[edge as usize]);
        update(&mut self.window[edge as usize]);
        if !self.dirty[edge as usize] {
            self.dirty[edge as usize] = true;
            self.dirty_edges.push(edge);
        }
    }

    pub(crate) fn take_window(&mut self) -> Vec<super::delta::SpawnDelta> {
        let mut rows = Vec::with_capacity(self.dirty_edges.len());
        for edge in self.dirty_edges.drain(..) {
            self.dirty[edge as usize] = false;
            let counters = std::mem::take(&mut self.window[edge as usize]);
            if !counters.is_zero() {
                rows.push(super::delta::SpawnDelta {
                    edge_id: edge,
                    counters,
                });
            }
        }
        rows
    }

    pub(crate) fn snapshots(&self) -> Vec<SpawnEdgeSnapshot> {
        self.identities
            .iter()
            .copied()
            .enumerate()
            .map(|(edge_id, identity)| SpawnEdgeSnapshot {
                edge_id: u32::try_from(edge_id).unwrap_or(u32::MAX),
                identity,
                counters: self.totals[edge_id],
            })
            .collect()
    }

    pub(crate) fn instances(&self) -> Vec<SpawnInstance> {
        self.instances.clone()
    }

    /// Drops bounded per-instance state for a completed boundary partition.
    /// Aggregate edge identities remain session-epoch scoped and therefore
    /// are retained until epoch rotation.
    pub(crate) fn seal_partition(&mut self, partition: u32) {
        self.intern.retain(|_, edge| {
            self.identities
                .get(*edge as usize)
                .is_none_or(|identity| identity.partition != partition)
        });
        for (index, identity) in self.identities.iter().enumerate() {
            if identity.partition != partition {
                continue;
            }
            self.totals[index] = SpawnCounters::default();
            self.window[index] = SpawnCounters::default();
            self.dirty[index] = false;
        }
        self.dirty_edges.retain(|edge| {
            self.identities
                .get(*edge as usize)
                .is_none_or(|identity| identity.partition != partition)
        });
        self.instances.retain(|instance| {
            self.identities
                .get(instance.edge_id as usize)
                .is_none_or(|identity| identity.partition != partition)
        });
        self.first_instances.remove(&partition);
        self.exceptional_instances.remove(&partition);
    }
}
