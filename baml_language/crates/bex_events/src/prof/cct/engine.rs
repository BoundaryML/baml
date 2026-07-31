//! Causal, target-neutral calling-context aggregation.

use std::collections::{HashSet, VecDeque};
use std::ops::{Index, IndexMut};

use rustc_hash::FxHashMap;

use super::{
    delta::{LlmCounters, LlmDelta, WindowDelta},
    nodes::{NODE_FLAG_RECURSION_FOLD, NodeId, NodeSnapshot, NodeStore},
    spawn::{SpawnEdgeSnapshot, SpawnInstance, SpawnStore},
    stacks::{ActiveCall, CallKey, RecentCall, RecentCalls, Suspend, ThreadState},
};
use crate::{
    ids::FunctionId,
    prof::{
        clock::TickConverter,
        record::{FunctionEndStatus, RawRecord, SuspendReason, ThreadEndStatus},
    },
};

pub const DEFAULT_WINDOW_NS: u64 = 250_000_000;
pub const DEFER_MAX_SWEEPS: u32 = 1024;
const RECURSION_FOLD_DEPTH: usize = 512;
const RECURSION_SCAN: usize = 8;
const RESUMED_SEQ_MEMORY: usize = 256;

/// Compact flags carried by `LlmCallMeta`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LlmMetaFlags(pub u8);

impl LlmMetaFlags {
    pub const PROVIDER_ERROR: u8 = 1 << 0;
    pub const PARSE_ERROR: u8 = 1 << 1;
    pub const RETRY: u8 = 1 << 2;

    #[must_use]
    pub fn provider_error(self) -> bool {
        self.0 & Self::PROVIDER_ERROR != 0
    }

    #[must_use]
    pub fn parse_error(self) -> bool {
        self.0 & Self::PARSE_ERROR != 0
    }

    #[must_use]
    pub fn retry(self) -> bool {
        self.0 & Self::RETRY != 0
    }
}

/// Owned, nanosecond-normalized input accepted by [`EngineCct`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CctEvent {
    CallFunction {
        flags: u8,
        thread_id: u64,
        call_id: u64,
        parent_call_id: u64,
        function_id: FunctionId,
        timestamp_ns: u64,
    },
    EndFunction {
        status: FunctionEndStatus,
        thread_id: u64,
        call_id: u64,
        timestamp_ns: u64,
    },
    StartThread {
        flags: u8,
        thread_id: u64,
        parent_thread_id: u64,
        parent_call_id: u64,
        timestamp_ns: u64,
        name: Option<String>,
    },
    EndThread {
        status: ThreadEndStatus,
        thread_id: u64,
        timestamp_ns: u64,
    },
    SetFunctionId {
        thread_id: u64,
        call_id: u64,
        id: [u8; 16],
        timestamp_ns: u64,
    },
    SuspendThread {
        reason: SuspendReason,
        thread_id: u64,
        suspend_seq: u64,
        timestamp_ns: u64,
    },
    ResumeThread {
        thread_id: u64,
        suspend_seq: u64,
        suspend_timestamp_ns: u64,
        timestamp_ns: u64,
    },
    LlmCallMeta {
        thread_id: u64,
        call_id: u64,
        model_id: u32,
        tokens_in: u32,
        tokens_out: u32,
        flags: LlmMetaFlags,
        timestamp_ns: u64,
    },
}

impl CctEvent {
    #[must_use]
    pub fn from_raw(raw: &RawRecord<'_>, conv: &TickConverter) -> Self {
        match *raw {
            RawRecord::CallFunction {
                flags,
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ts_ticks,
                ..
            } => Self::CallFunction {
                flags,
                thread_id: thread_id.0,
                call_id: call_id.0,
                parent_call_id: parent_call_id.0,
                function_id,
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::EndFunction {
                status,
                thread_id,
                call_id,
                ts_ticks,
            } => Self::EndFunction {
                status,
                thread_id: thread_id.0,
                call_id: call_id.0,
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::StartThread {
                flags,
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                name,
            } => Self::StartThread {
                flags,
                thread_id: thread_id.0,
                parent_thread_id: parent_thread_id.0,
                parent_call_id: parent_call_id.0,
                timestamp_ns: conv.to_ns(ts_ticks),
                name: (!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()),
            },
            RawRecord::EndThread {
                status,
                thread_id,
                ts_ticks,
            } => Self::EndThread {
                status,
                thread_id: thread_id.0,
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::SetFunctionId {
                thread_id,
                call_id,
                id,
                ts_ticks,
            } => Self::SetFunctionId {
                thread_id: thread_id.0,
                call_id: call_id.0,
                id,
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::SuspendThread {
                reason,
                thread_id,
                suspend_seq,
                ts_ticks,
            } => Self::SuspendThread {
                reason,
                thread_id: thread_id.0,
                suspend_seq: u64::from(suspend_seq),
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::ResumeThread {
                thread_id,
                suspend_seq,
                suspend_ts_ticks,
                ts_ticks,
            } => Self::ResumeThread {
                thread_id: thread_id.0,
                suspend_seq: u64::from(suspend_seq),
                suspend_timestamp_ns: conv.to_ns(suspend_ts_ticks),
                timestamp_ns: conv.to_ns(ts_ticks),
            },
            RawRecord::LlmCallMeta {
                thread_id,
                call_id,
                model_id,
                tokens_in,
                tokens_out,
                flags,
                ts_ticks,
            } => Self::LlmCallMeta {
                thread_id: thread_id.0,
                call_id: call_id.0,
                model_id,
                tokens_in,
                tokens_out,
                flags: LlmMetaFlags(flags),
                timestamp_ns: conv.to_ns(ts_ticks),
            },
        }
    }

    fn timestamp_ns(&self) -> u64 {
        match self {
            Self::CallFunction { timestamp_ns, .. }
            | Self::EndFunction { timestamp_ns, .. }
            | Self::StartThread { timestamp_ns, .. }
            | Self::EndThread { timestamp_ns, .. }
            | Self::SetFunctionId { timestamp_ns, .. }
            | Self::SuspendThread { timestamp_ns, .. }
            | Self::ResumeThread { timestamp_ns, .. }
            | Self::LlmCallMeta { timestamp_ns, .. } => *timestamp_ns,
        }
    }

    fn thread_id(&self) -> u64 {
        match self {
            Self::CallFunction { thread_id, .. }
            | Self::EndFunction { thread_id, .. }
            | Self::StartThread { thread_id, .. }
            | Self::EndThread { thread_id, .. }
            | Self::SetFunctionId { thread_id, .. }
            | Self::SuspendThread { thread_id, .. }
            | Self::ResumeThread { thread_id, .. }
            | Self::LlmCallMeta { thread_id, .. } => *thread_id,
        }
    }

    fn is_end_thread(&self) -> bool {
        matches!(self, Self::EndThread { .. })
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CctHealth {
    pub deferred_records: u64,
    pub resync_records: u64,
    pub corrupt_ranges: u64,
    pub degraded_partitions: u64,
    pub reorder_clamped: u64,
    pub clock_anomalies: u64,
    pub folded_frames: u64,
    pub evicted_calls: u64,
    pub instances_dropped: u64,
    pub shed_ranges: u64,
    pub shed_events: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LlmSnapshot {
    pub node_id: NodeId,
    pub model_id: u32,
    pub counters: LlmCounters,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CctSnapshot {
    pub nodes: Vec<NodeSnapshot>,
    pub llm: Vec<LlmSnapshot>,
    pub spawn_edges: Vec<SpawnEdgeSnapshot>,
    pub spawn_instances: Vec<SpawnInstance>,
    pub recent_calls: Vec<RecentCall>,
    pub open_calls: usize,
    pub live_threads: usize,
    pub health: CctHealth,
}

#[derive(Debug, Default)]
struct PartitionState {
    recent: RecentCalls,
}

/// Partition ids are private, monotonically allocated engine-local indices.
#[derive(Debug, Default)]
struct PartitionStore {
    /// Partition id represented by `states[0]`. Completed boundary ids are
    /// never reused, so indexing from zero would retain a permanently growing
    /// prefix of empty slots even when every partition had been released.
    base: u32,
    states: Vec<Option<PartitionState>>,
}

impl PartitionStore {
    #[inline]
    fn is_empty(&self) -> bool {
        self.states.is_empty()
    }

    #[inline]
    fn offset(&self, partition: u32) -> Option<usize> {
        let offset = partition.checked_sub(self.base)?;
        usize::try_from(offset)
            .ok()
            .filter(|&offset| offset < self.states.len())
    }

    #[inline]
    fn contains_key(&self, partition: &u32) -> bool {
        self.get(partition).is_some()
    }

    #[inline]
    fn get(&self, partition: &u32) -> Option<&PartitionState> {
        self.states
            .get(self.offset(*partition)?)
            .and_then(Option::as_ref)
    }

    #[inline]
    fn get_mut(&mut self, partition: &u32) -> Option<&mut PartitionState> {
        let offset = self.offset(*partition)?;
        self.states.get_mut(offset).and_then(Option::as_mut)
    }

    fn insert(&mut self, partition: u32, state: PartitionState) -> Option<PartitionState> {
        if self.states.is_empty() {
            self.base = partition;
            self.states.push(Some(state));
            return None;
        }
        let index = usize::try_from(
            partition
                .checked_sub(self.base)
                .expect("partition ids are inserted monotonically"),
        )
        .expect("partition offset fits usize");
        if self.states.len() <= index {
            self.states.resize_with(index + 1, || None);
        }
        self.states[index].replace(state)
    }

    fn remove(&mut self, partition: &u32) -> Option<PartitionState> {
        let offset = self.offset(*partition)?;
        let removed = self.states.get_mut(offset).and_then(Option::take)?;

        let Some(first_live) = self.states.iter().position(Option::is_some) else {
            self.states.clear();
            return Some(removed);
        };
        let after_last_live = self
            .states
            .iter()
            .rposition(Option::is_some)
            .expect("first live slot implies last live slot")
            + 1;
        self.states.truncate(after_last_live);
        if first_live != 0 {
            drop(self.states.drain(..first_live));
            self.base = self
                .base
                .checked_add(u32::try_from(first_live).expect("partition offset fits u32"))
                .expect("live partition id fits u32");
        }
        Some(removed)
    }

    fn values(&self) -> impl Iterator<Item = &PartitionState> {
        self.states.iter().filter_map(Option::as_ref)
    }
}

impl Index<&u32> for PartitionStore {
    type Output = PartitionState;

    #[inline]
    fn index(&self, partition: &u32) -> &Self::Output {
        self.get(partition).expect("partition exists")
    }
}

impl IndexMut<&u32> for PartitionStore {
    #[inline]
    fn index_mut(&mut self, partition: &u32) -> &mut Self::Output {
        self.get_mut(partition).expect("partition exists")
    }
}

#[derive(Clone, Copy, Debug)]
enum Missing {
    Thread,
    ParentThread,
    Call,
    Quiescent,
}

#[derive(Clone, Debug)]
struct Deferred {
    event: CctEvent,
    born_sweep: u32,
    not_before_sweep: u32,
    _missing: Missing,
}

const MAX_DENSE_THREAD_ID: u64 = 4096;

/// Engine-local thread ids are allocated densely from one. Keep the normal
/// lookup as a checked vector access while retaining a hash fallback for
/// synthetic, malformed, or exceptionally large ids.
#[derive(Debug, Default)]
struct ThreadStore {
    dense: Vec<Option<ThreadState>>,
    sparse: FxHashMap<u64, ThreadState>,
    len: usize,
}

impl ThreadStore {
    #[inline]
    fn dense_index(thread_id: u64) -> Option<usize> {
        (thread_id <= MAX_DENSE_THREAD_ID).then_some(thread_id as usize)
    }

    #[inline]
    fn contains_key(&self, thread_id: &u64) -> bool {
        self.get(thread_id).is_some()
    }

    #[inline]
    fn get(&self, thread_id: &u64) -> Option<&ThreadState> {
        if let Some(index) = Self::dense_index(*thread_id) {
            self.dense.get(index).and_then(Option::as_ref)
        } else {
            self.sparse.get(thread_id)
        }
    }

    #[inline]
    fn get_mut(&mut self, thread_id: &u64) -> Option<&mut ThreadState> {
        if let Some(index) = Self::dense_index(*thread_id) {
            self.dense.get_mut(index).and_then(Option::as_mut)
        } else {
            self.sparse.get_mut(thread_id)
        }
    }

    fn insert(&mut self, thread_id: u64, state: ThreadState) -> Option<ThreadState> {
        let previous = if let Some(index) = Self::dense_index(thread_id) {
            if self.dense.len() <= index {
                self.dense.resize_with(index + 1, || None);
            }
            self.dense[index].replace(state)
        } else {
            self.sparse.insert(thread_id, state)
        };
        if previous.is_none() {
            self.len = self.len.saturating_add(1);
        }
        previous
    }

    fn remove(&mut self, thread_id: &u64) -> Option<ThreadState> {
        let removed = if let Some(index) = Self::dense_index(*thread_id) {
            self.dense.get_mut(index).and_then(Option::take)
        } else {
            self.sparse.remove(thread_id)
        };
        if removed.is_some() {
            self.len = self.len.saturating_sub(1);
        }
        removed
    }

    fn iter(&self) -> impl Iterator<Item = (u64, &ThreadState)> {
        self.dense
            .iter()
            .enumerate()
            .filter_map(|(thread_id, state)| state.as_ref().map(|state| (thread_id as u64, state)))
            .chain(
                self.sparse
                    .iter()
                    .map(|(&thread_id, state)| (thread_id, state)),
            )
    }

    fn keys(&self) -> impl Iterator<Item = u64> + '_ {
        self.iter().map(|(thread_id, _)| thread_id)
    }

    fn values(&self) -> impl Iterator<Item = &ThreadState> {
        self.iter().map(|(_, state)| state)
    }

    #[inline]
    fn len(&self) -> usize {
        self.len
    }
}

impl Index<&u64> for ThreadStore {
    type Output = ThreadState;

    #[inline]
    fn index(&self, thread_id: &u64) -> &Self::Output {
        self.get(thread_id).expect("thread id exists")
    }
}

impl IndexMut<&u64> for ThreadStore {
    #[inline]
    fn index_mut(&mut self, thread_id: &u64) -> &mut Self::Output {
        self.get_mut(thread_id).expect("thread id exists")
    }
}

/// One engine's complete in-RAM aggregation state.
#[derive(Debug)]
pub struct EngineCct {
    nodes: NodeStore,
    threads: ThreadStore,
    partitions: PartitionStore,
    next_partition: u32,
    spawn: SpawnStore,
    llm_totals: FxHashMap<(NodeId, u32), LlmCounters>,
    llm_window: FxHashMap<(NodeId, u32), LlmCounters>,
    deferred: Vec<Deferred>,
    sweep: u32,
    degraded: HashSet<u32>,
    health: CctHealth,
    window_ns: u64,
    window_start_ns: Option<u64>,
    window_end_ns: u64,
    pending_windows: VecDeque<WindowDelta>,
}

impl Default for EngineCct {
    fn default() -> Self {
        Self::new(DEFAULT_WINDOW_NS)
    }
}

impl EngineCct {
    #[must_use]
    pub fn new(window_ns: u64) -> Self {
        Self {
            nodes: NodeStore::default(),
            threads: ThreadStore::default(),
            partitions: PartitionStore::default(),
            next_partition: 1,
            spawn: SpawnStore::default(),
            llm_totals: FxHashMap::default(),
            llm_window: FxHashMap::default(),
            deferred: Vec::new(),
            sweep: 0,
            degraded: HashSet::new(),
            health: CctHealth::default(),
            window_ns: window_ns.max(1),
            window_start_ns: None,
            window_end_ns: 0,
            pending_windows: VecDeque::new(),
        }
    }

    #[inline]
    pub fn ingest_raw(&mut self, raw: &RawRecord<'_>, conv: &TickConverter) {
        let timestamp_ns = conv.to_ns(match raw {
            RawRecord::CallFunction { ts_ticks, .. }
            | RawRecord::EndFunction { ts_ticks, .. }
            | RawRecord::StartThread { ts_ticks, .. }
            | RawRecord::EndThread { ts_ticks, .. }
            | RawRecord::SetFunctionId { ts_ticks, .. }
            | RawRecord::SuspendThread { ts_ticks, .. }
            | RawRecord::ResumeThread { ts_ticks, .. }
            | RawRecord::LlmCallMeta { ts_ticks, .. } => *ts_ticks,
        });
        self.observe_timestamp(timestamp_ns);

        // EndThread is a quiescence barrier. It must wait one sweep even when
        // it arrives after the visible stack emptied because records from a
        // migrated OS-thread ring may still be pending.
        if matches!(raw, RawRecord::EndThread { .. }) {
            self.defer(CctEvent::from_raw(raw, conv), Missing::Quiescent);
            return;
        }

        // Keep the overwhelmingly common Call/End path borrowed and scalar.
        // Construct the owned enum only when causal deferral actually needs
        // to retain the record across a drain sweep.
        let result = match *raw {
            RawRecord::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ..
            } => self.process_call(
                thread_id.0,
                call_id.0,
                parent_call_id.0,
                function_id,
                timestamp_ns,
                false,
            ),
            RawRecord::EndFunction {
                status,
                thread_id,
                call_id,
                ..
            } => self.process_end(thread_id.0, call_id.0, status, timestamp_ns, false),
            RawRecord::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                name,
                ..
            } => self.process_start_thread(
                thread_id.0,
                parent_thread_id.0,
                parent_call_id.0,
                timestamp_ns,
                (!name.is_empty()).then(|| String::from_utf8_lossy(name).into_owned()),
                false,
            ),
            RawRecord::SetFunctionId {
                thread_id,
                call_id,
                id,
                ..
            } => self.process_set_id(thread_id.0, call_id.0, id, timestamp_ns, false),
            RawRecord::SuspendThread {
                thread_id,
                suspend_seq,
                ..
            } => self.process_suspend(thread_id.0, u64::from(suspend_seq), timestamp_ns, false),
            RawRecord::ResumeThread {
                thread_id,
                suspend_seq,
                suspend_ts_ticks,
                ..
            } => self.process_resume(
                thread_id.0,
                u64::from(suspend_seq),
                conv.to_ns(suspend_ts_ticks),
                timestamp_ns,
                false,
            ),
            RawRecord::LlmCallMeta {
                thread_id,
                call_id,
                model_id,
                tokens_in,
                tokens_out,
                flags,
                ..
            } => self.process_llm(
                thread_id.0,
                call_id.0,
                model_id,
                tokens_in,
                tokens_out,
                LlmMetaFlags(flags),
                timestamp_ns,
                false,
            ),
            RawRecord::EndThread { .. } => unreachable!("handled above"),
        };
        if let Err(missing) = result {
            self.defer(CctEvent::from_raw(raw, conv), missing);
        } else if !self.deferred.is_empty() {
            self.replay_ready();
        }
    }

    pub fn ingest(&mut self, event: CctEvent) {
        self.observe_timestamp(event.timestamp_ns());
        if event.is_end_thread() {
            self.defer(event, Missing::Quiescent);
            return;
        }
        if let Err(missing) = self.try_process(&event, false) {
            self.defer(event, missing);
        } else if !self.deferred.is_empty() {
            self.replay_ready();
        }
    }

    /// Marks the causal sweep boundary. Deferrals age only here, never per
    /// event, matching cross-ring migration semantics.
    pub fn finish_sweep(&mut self) {
        self.sweep = self.sweep.wrapping_add(1);
        if self.deferred.is_empty() {
            self.health.deferred_records = 0;
            return;
        }
        self.replay_ready();

        let mut keep = Vec::with_capacity(self.deferred.len());
        let deferred = std::mem::take(&mut self.deferred);
        for item in deferred {
            if self.sweep.wrapping_sub(item.born_sweep) >= DEFER_MAX_SWEEPS {
                self.health.resync_records = self.health.resync_records.saturating_add(1);
                let partition = self.partition_for_event(&item.event);
                if let Some(partition) = partition {
                    self.mark_degraded(partition);
                }
                let _ = self.try_process(&item.event, true);
            } else {
                keep.push(item);
            }
        }
        self.deferred = keep;
        self.replay_ready();
        self.health.deferred_records = u64::try_from(self.deferred.len()).unwrap_or(u64::MAX);
    }

    /// A corrupt raw range has no trustworthy tail framing. All currently
    /// live partitions are visibly degraded, but aggregation remains usable.
    pub fn mark_corrupt_range(&mut self) {
        self.health.corrupt_ranges = self.health.corrupt_ranges.saturating_add(1);
        self.resync_all_live_threads();
    }

    /// Last-resort overload handling. Structural records were dropped before
    /// they reached the consumer; every live thread is closed into its
    /// unattributable recovery context and the loss remains query-visible.
    pub fn mark_shed_range(&mut self, events: u64) {
        self.health.shed_ranges = self.health.shed_ranges.saturating_add(1);
        self.health.shed_events = self.health.shed_events.saturating_add(events);
        self.resync_all_live_threads();
    }

    fn resync_all_live_threads(&mut self) {
        let threads = self.threads.keys().collect::<Vec<_>>();
        for thread_id in threads {
            self.resync_thread_after_loss(thread_id);
        }
    }

    /// Closes all elapsed windows. Only explicitly suspended threads advance
    /// to a consumer clock boundary; running threads charge through their
    /// drained-event watermark, avoiding drain-latency attribution.
    pub fn close_windows_through(&mut self, timestamp_ns: u64) {
        if self.window_start_ns.is_none() {
            let start = timestamp_ns - timestamp_ns % self.window_ns;
            self.window_start_ns = Some(start);
            self.window_end_ns = start.saturating_add(self.window_ns);
        }
        while timestamp_ns >= self.window_end_ns {
            let start = self.window_start_ns.expect("initialized");
            let end = self.window_end_ns;
            if end == start {
                break;
            }
            let suspended = self
                .threads
                .iter()
                .filter_map(|(thread_id, thread)| thread.suspended.map(|_| thread_id))
                .collect::<Vec<_>>();
            for thread_id in suspended {
                self.charge_thread(thread_id, end);
            }
            self.close_one_window(start, end);
            self.window_start_ns = Some(end);
            self.window_end_ns = end.saturating_add(self.window_ns);
        }
    }

    /// The event hot path almost never crosses a 250 ms window boundary.
    /// Keep that overwhelmingly common check scalar and leave the allocation-
    /// capable rollover work out of line.
    #[inline]
    fn observe_timestamp(&mut self, timestamp_ns: u64) {
        if timestamp_ns < self.window_end_ns {
            return;
        }
        let Some(start_ns) = self.window_start_ns else {
            let start_ns = timestamp_ns - timestamp_ns % self.window_ns;
            self.window_start_ns = Some(start_ns);
            self.window_end_ns = start_ns.saturating_add(self.window_ns);
            return;
        };
        debug_assert_eq!(self.window_end_ns, start_ns.saturating_add(self.window_ns));
        self.close_windows_through(timestamp_ns);
    }

    /// Closes the trailing partial window at an engine/boundary durability
    /// barrier. Periodic drains keep fixed-width windows; terminal snapshots
    /// must not strand the final sub-window merely because it is shorter.
    pub fn close_final_window_through(&mut self, timestamp_ns: u64) {
        self.close_windows_through(timestamp_ns);
        let Some(start) = self.window_start_ns else {
            return;
        };
        if timestamp_ns <= start {
            return;
        }
        let suspended = self
            .threads
            .iter()
            .filter_map(|(thread_id, thread)| thread.suspended.map(|_| thread_id))
            .collect::<Vec<_>>();
        for thread_id in suspended {
            self.charge_thread(thread_id, timestamp_ns);
        }
        self.close_one_window(start, timestamp_ns);
        self.window_start_ns = Some(timestamp_ns);
        self.window_end_ns = timestamp_ns.saturating_add(self.window_ns);
    }

    fn close_one_window(&mut self, start_ns: u64, end_ns: u64) {
        let (nodes, histograms) = self.nodes.take_window();
        let mut llm = self
            .llm_window
            .drain()
            .filter_map(|((node_id, model_id), counters)| {
                (!counters.is_zero()).then_some(LlmDelta {
                    node_id,
                    model_id,
                    counters,
                })
            })
            .collect::<Vec<_>>();
        llm.sort_by_key(|row| (row.node_id, row.model_id));
        let spawn = self.spawn.take_window();
        let window = WindowDelta {
            start_ns,
            end_ns,
            nodes,
            histograms,
            llm,
            spawn,
        };
        if !window.is_empty() {
            self.pending_windows.push_back(window);
        }
    }

    pub fn take_windows(&mut self) -> Vec<WindowDelta> {
        self.pending_windows.drain(..).collect()
    }

    /// Returns true only at a safe session-epoch boundary.
    ///
    /// Epoch rotation re-densifies every CCT identity, so it must never occur
    /// while a logical thread, boundary partition, causal deferral, or
    /// unpersisted window can still reference an old node id. Completed
    /// boundary partitions are released after their snapshot is durable,
    /// which makes the common request-to-request server gap a safe rotation
    /// point without retaining the prior epoch's node table.
    #[must_use]
    pub fn can_rotate_epoch(&self) -> bool {
        self.threads.len() == 0
            && self.partitions.is_empty()
            && self.deferred.is_empty()
            && self.pending_windows.is_empty()
    }

    /// Returns the session partition currently owned by a live logical
    /// thread. Hosts bind while the root `StartThread` is live; completion
    /// keeps the returned partition id rather than attempting to infer it
    /// after `EndThread` has retired the thread.
    #[must_use]
    pub fn partition_for_thread(&self, thread_id: u64) -> Option<u32> {
        self.threads.get(&thread_id).map(|thread| thread.partition)
    }

    /// Returns one exact recently completed call without allocating a full
    /// snapshot. Consumers use this immediately after an `EndFunction` record
    /// to evaluate cold-path capture triggers.
    #[must_use]
    pub fn recent_call(&self, thread_id: u64, call_id: u64) -> Option<RecentCall> {
        let partition = self.partition_for_thread(thread_id)?;
        self.recent_call_in_partition(partition, thread_id, call_id)
    }

    /// Partition-qualified variant that remains usable after `EndThread` retires
    /// the live thread state.
    #[must_use]
    pub fn recent_call_in_partition(
        &self,
        partition: u32,
        thread_id: u64,
        call_id: u64,
    ) -> Option<RecentCall> {
        self.partitions
            .get(&partition)?
            .recent
            .find(CallKey { thread_id, call_id })
            .cloned()
    }

    /// Returns a final partition-only snapshot once all its threads settle.
    #[must_use]
    pub fn partition_snapshot(&self, partition: u32) -> Option<CctSnapshot> {
        if !self.partitions.contains_key(&partition)
            || self
                .threads
                .values()
                .any(|thread| thread.partition == partition)
        {
            return None;
        }

        let node_ids = self
            .nodes
            .snapshots()
            .into_iter()
            .filter(|node| node.identity.partition == partition)
            .map(|node| node.node_id)
            .collect::<HashSet<_>>();
        let nodes = self
            .nodes
            .snapshots()
            .into_iter()
            .filter(|node| node.identity.partition == partition)
            .collect::<Vec<_>>();
        let llm = self
            .llm_totals
            .iter()
            .filter_map(|(&(node_id, model_id), &counters)| {
                node_ids.contains(&node_id).then_some(LlmSnapshot {
                    node_id,
                    model_id,
                    counters,
                })
            })
            .collect::<Vec<_>>();
        let edge_ids = self
            .spawn
            .snapshots()
            .into_iter()
            .filter(|edge| edge.identity.partition == partition)
            .map(|edge| edge.edge_id)
            .collect::<HashSet<_>>();
        let spawn_edges = self
            .spawn
            .snapshots()
            .into_iter()
            .filter(|edge| edge.identity.partition == partition)
            .collect::<Vec<_>>();
        let spawn_instances = self
            .spawn
            .instances()
            .into_iter()
            .filter(|instance| edge_ids.contains(&instance.edge_id))
            .collect::<Vec<_>>();
        let partition_state = self.partitions.get(&partition)?;
        let recent_calls = partition_state.recent.snapshot();
        let degraded = self.degraded.contains(&partition);
        let mut health = self.health;
        health.degraded_partitions = u64::from(degraded);
        health.evicted_calls = partition_state.recent.evicted_calls;

        Some(CctSnapshot {
            nodes,
            llm,
            spawn_edges,
            spawn_instances,
            recent_calls,
            open_calls: 0,
            live_threads: 0,
            health,
        })
    }

    /// Releases the mutable, boundary-scoped portion of a durable partition.
    /// Identity rows remain allocated until session epoch rotation because
    /// their numeric ids must not be reused.
    pub fn release_partition(&mut self, partition: u32) -> bool {
        if self
            .threads
            .values()
            .any(|thread| thread.partition == partition)
        {
            return false;
        }
        let node_ids = self
            .nodes
            .snapshots()
            .into_iter()
            .filter(|node| node.identity.partition == partition)
            .map(|node| node.node_id)
            .collect::<HashSet<_>>();
        if self.partitions.remove(&partition).is_none() {
            return false;
        }
        self.degraded.remove(&partition);
        self.llm_totals
            .retain(|(node_id, _), _| !node_ids.contains(node_id));
        self.llm_window
            .retain(|(node_id, _), _| !node_ids.contains(node_id));
        self.spawn.seal_partition(partition);
        self.nodes.seal_partition(partition);
        self.health.degraded_partitions = u64::try_from(self.degraded.len()).unwrap_or(u64::MAX);
        true
    }

    /// Convenience for non-durable/cooperative consumers.
    pub fn seal_partition(&mut self, partition: u32) -> Option<CctSnapshot> {
        let snapshot = self.partition_snapshot(partition)?;
        self.release_partition(partition).then_some(snapshot)
    }

    #[must_use]
    pub fn snapshot(&self) -> CctSnapshot {
        let mut llm = self
            .llm_totals
            .iter()
            .map(|(&(node_id, model_id), &counters)| LlmSnapshot {
                node_id,
                model_id,
                counters,
            })
            .collect::<Vec<_>>();
        llm.sort_by_key(|row| (row.node_id, row.model_id));
        let mut recent_calls = self
            .partitions
            .values()
            .flat_map(|partition| partition.recent.snapshot())
            .collect::<Vec<_>>();
        recent_calls.sort_by_key(|call| (call.end_ns, call.thread_id, call.call_id));
        let mut health = self.health;
        health.deferred_records = u64::try_from(self.deferred.len()).unwrap_or(u64::MAX);
        health.degraded_partitions = u64::try_from(self.degraded.len()).unwrap_or(u64::MAX);
        health.evicted_calls = self
            .partitions
            .values()
            .map(|partition| partition.recent.evicted_calls)
            .fold(0u64, u64::saturating_add);
        health.instances_dropped = self.spawn.instances_dropped;
        CctSnapshot {
            nodes: self.nodes.snapshots(),
            llm,
            spawn_edges: self.spawn.snapshots(),
            spawn_instances: self.spawn.instances(),
            recent_calls,
            open_calls: self.threads.values().map(|thread| thread.stack.len()).sum(),
            live_threads: self.threads.len(),
            health,
        }
    }

    fn defer(&mut self, event: CctEvent, missing: Missing) {
        let not_before_sweep = if matches!(missing, Missing::Quiescent) {
            self.sweep.wrapping_add(1)
        } else {
            self.sweep
        };
        self.deferred.push(Deferred {
            event,
            born_sweep: self.sweep,
            not_before_sweep,
            _missing: missing,
        });
        self.health.deferred_records = u64::try_from(self.deferred.len()).unwrap_or(u64::MAX);
    }

    fn replay_ready(&mut self) {
        // Dependency arrivals can unlock chains. EndThread is deliberately
        // processed last so pending call/thread records get first refusal.
        for _ in 0..16 {
            let mut work = std::mem::take(&mut self.deferred);
            if work.is_empty() {
                break;
            }
            work.sort_by_key(|item| item.event.is_end_thread());
            let before = work.len();
            for mut item in work {
                if self.sweep < item.not_before_sweep {
                    self.deferred.push(item);
                    continue;
                }
                match self.try_process(&item.event, false) {
                    Ok(()) => {}
                    Err(missing) => {
                        item._missing = missing;
                        self.deferred.push(item);
                    }
                }
            }
            if self.deferred.len() == before {
                break;
            }
        }
        self.health.deferred_records = u64::try_from(self.deferred.len()).unwrap_or(u64::MAX);
    }

    fn try_process(&mut self, event: &CctEvent, force: bool) -> Result<(), Missing> {
        match event {
            CctEvent::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                timestamp_ns,
                name,
                ..
            } => self.process_start_thread(
                *thread_id,
                *parent_thread_id,
                *parent_call_id,
                *timestamp_ns,
                name.clone(),
                force,
            ),
            CctEvent::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                timestamp_ns,
                ..
            } => self.process_call(
                *thread_id,
                *call_id,
                *parent_call_id,
                *function_id,
                *timestamp_ns,
                force,
            ),
            CctEvent::EndFunction {
                status,
                thread_id,
                call_id,
                timestamp_ns,
            } => self.process_end(*thread_id, *call_id, *status, *timestamp_ns, force),
            CctEvent::EndThread {
                status,
                thread_id,
                timestamp_ns,
            } => self.process_end_thread(*thread_id, *status, *timestamp_ns, force),
            CctEvent::SetFunctionId {
                thread_id,
                call_id,
                id,
                timestamp_ns,
            } => self.process_set_id(*thread_id, *call_id, *id, *timestamp_ns, force),
            CctEvent::SuspendThread {
                thread_id,
                suspend_seq,
                timestamp_ns,
                ..
            } => self.process_suspend(*thread_id, *suspend_seq, *timestamp_ns, force),
            CctEvent::ResumeThread {
                thread_id,
                suspend_seq,
                suspend_timestamp_ns,
                timestamp_ns,
            } => self.process_resume(
                *thread_id,
                *suspend_seq,
                *suspend_timestamp_ns,
                *timestamp_ns,
                force,
            ),
            CctEvent::LlmCallMeta {
                thread_id,
                call_id,
                model_id,
                tokens_in,
                tokens_out,
                flags,
                timestamp_ns,
            } => self.process_llm(
                *thread_id,
                *call_id,
                *model_id,
                *tokens_in,
                *tokens_out,
                *flags,
                *timestamp_ns,
                force,
            ),
        }
    }

    fn process_start_thread(
        &mut self,
        thread_id: u64,
        parent_thread_id: u64,
        parent_call_id: u64,
        timestamp_ns: u64,
        name: Option<String>,
        force: bool,
    ) -> Result<(), Missing> {
        if self.threads.contains_key(&thread_id) {
            return Ok(());
        }
        if parent_thread_id == 0 {
            self.start_root_thread(thread_id, timestamp_ns, name, false);
            return Ok(());
        }
        let Some(parent) = self.threads.get(&parent_thread_id) else {
            if force {
                self.start_root_thread(thread_id, timestamp_ns, name, true);
                return Ok(());
            }
            return Err(Missing::ParentThread);
        };
        let parent_partition = parent.partition;
        let parent_root = parent.root_node;
        let spawn_ctx_node = if parent_call_id == 0 {
            parent.target_node()
        } else if let Some(active) = parent
            .stack
            .iter()
            .rev()
            .find(|active| active.key.call_id == parent_call_id)
        {
            active.node
        } else if let Some(recent) = self.partitions[&parent_partition].recent.find(CallKey {
            thread_id: parent_thread_id,
            call_id: parent_call_id,
        }) {
            recent.node_id
        } else if force {
            self.nodes
                .unattributable(parent_root, parent_partition, parent_thread_id)
        } else {
            return Err(Missing::Call);
        };
        self.threads.insert(
            thread_id,
            ThreadState {
                partition: parent_partition,
                root_node: parent_root,
                stack: Vec::new(),
                intern_cache: None,
                last_charge_ns: timestamp_ns,
                watermark_ns: timestamp_ns,
                suspended: None,
                resumed_recent: VecDeque::new(),
                spawn_ctx_node,
                entry_edge: None,
                started_ns: timestamp_ns,
                name,
                is_spawned: true,
            },
        );
        Ok(())
    }

    fn start_root_thread(
        &mut self,
        thread_id: u64,
        timestamp_ns: u64,
        name: Option<String>,
        degraded: bool,
    ) {
        let partition = self.next_partition;
        self.next_partition = self.next_partition.saturating_add(1);
        let root_node = self.nodes.new_partition_root(partition, thread_id);
        self.partitions.insert(
            partition,
            PartitionState {
                recent: RecentCalls::default(),
            },
        );
        self.threads.insert(
            thread_id,
            ThreadState {
                partition,
                root_node,
                stack: Vec::new(),
                intern_cache: None,
                last_charge_ns: timestamp_ns,
                watermark_ns: timestamp_ns,
                suspended: None,
                resumed_recent: VecDeque::new(),
                spawn_ctx_node: root_node,
                entry_edge: None,
                started_ns: timestamp_ns,
                name,
                is_spawned: false,
            },
        );
        if degraded {
            self.mark_degraded(partition);
        }
    }

    fn ensure_recovery_thread(&mut self, thread_id: u64, timestamp_ns: u64) {
        if !self.threads.contains_key(&thread_id) {
            self.start_root_thread(thread_id, timestamp_ns, None, true);
        }
    }

    fn process_call(
        &mut self,
        thread_id: u64,
        call_id: u64,
        parent_call_id: u64,
        function_id: FunctionId,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        if force && !self.threads.contains_key(&thread_id) {
            self.ensure_recovery_thread(thread_id, timestamp_ns);
        }
        let Some(thread) = self.threads.get_mut(&thread_id) else {
            return Err(Missing::Thread);
        };
        let partition = thread.partition;
        let root_node = thread.root_node;
        let spawn_ctx = thread.spawn_ctx_node;
        let is_spawned = thread.is_spawned;
        let first_call = thread.stack.is_empty() && thread.entry_edge.is_none();
        let parent_node = if parent_call_id == 0 {
            if is_spawned && first_call {
                spawn_ctx
            } else {
                root_node
            }
        } else if let Some(active) = thread
            .stack
            .iter()
            .rev()
            .find(|active| active.key.call_id == parent_call_id)
        {
            active.node
        } else if let Some(recent) = self.partitions[&partition].recent.find(CallKey {
            thread_id,
            call_id: parent_call_id,
        }) {
            recent.node_id
        } else if force {
            self.nodes.unattributable(root_node, partition, thread_id)
        } else {
            return Err(Missing::Call);
        };

        charge_state(
            thread,
            &mut self.nodes,
            &mut self.spawn,
            &mut self.health,
            timestamp_ns,
        );
        let folded = {
            if thread.stack.len() >= RECURSION_FOLD_DEPTH {
                thread
                    .stack
                    .iter()
                    .rev()
                    .take(RECURSION_SCAN)
                    .find(|active| self.nodes.identity(active.node).function_id == function_id)
                    .map(|active| active.node)
            } else {
                None
            }
        };
        let node = folded.unwrap_or_else(|| {
            if let Some((cached_parent, cached_function, cached_node)) = thread.intern_cache {
                if (cached_parent, cached_function) == (parent_node, function_id.0) {
                    return cached_node;
                }
            }
            let node = self
                .nodes
                .intern_node(parent_node, function_id, partition, thread_id);
            thread.intern_cache = Some((parent_node, function_id.0, node));
            node
        });
        if folded.is_some() {
            self.nodes.flag(node, NODE_FLAG_RECURSION_FOLD);
            self.health.folded_frames = self.health.folded_frames.saturating_add(1);
        }

        let edge = if is_spawned && first_call {
            Some(self.spawn.begin(
                spawn_ctx,
                function_id.0,
                node,
                partition,
                thread_id,
                thread.name.clone(),
                thread.started_ns,
            ))
        } else {
            None
        };
        let key = CallKey { thread_id, call_id };
        thread.stack.push(ActiveCall {
            key,
            node,
            start_ns: timestamp_ns,
            parent_call_id,
        });
        if let Some(edge) = edge {
            thread.entry_edge = Some(edge);
        }
        self.nodes.enter(node);
        Ok(())
    }

    fn process_end(
        &mut self,
        thread_id: u64,
        call_id: u64,
        status: FunctionEndStatus,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        let key = CallKey { thread_id, call_id };
        if force && !self.threads.contains_key(&thread_id) {
            self.ensure_recovery_thread(thread_id, timestamp_ns);
        }
        let Some(thread) = self.threads.get_mut(&thread_id) else {
            return Err(Missing::Thread);
        };
        let is_top = thread.stack.last().is_some_and(|call| call.key == key);
        let (active, partition) = if is_top {
            charge_state(
                thread,
                &mut self.nodes,
                &mut self.spawn,
                &mut self.health,
                timestamp_ns,
            );
            (
                thread.stack.pop().expect("top call checked above"),
                thread.partition,
            )
        } else if let Some(position) = thread.stack.iter().rposition(|call| call.key == key) {
            charge_state(
                thread,
                &mut self.nodes,
                &mut self.spawn,
                &mut self.health,
                timestamp_ns,
            );
            (thread.stack.remove(position), thread.partition)
        } else {
            if self.partitions[&thread.partition]
                .recent
                .find(key)
                .is_some()
            {
                return Ok(());
            }
            if !force {
                return Err(Missing::Call);
            }
            let _ = thread;
            self.process_call(thread_id, call_id, 0, FunctionId(0), timestamp_ns, true)?;
            let thread = self.threads.get_mut(&thread_id).expect("ensured");
            charge_state(
                thread,
                &mut self.nodes,
                &mut self.spawn,
                &mut self.health,
                timestamp_ns,
            );
            let position = thread
                .stack
                .iter()
                .rposition(|call| call.key == key)
                .expect("forced process_call inserts the missing active call");
            (thread.stack.remove(position), thread.partition)
        };
        let duration = timestamp_ns.saturating_sub(active.start_ns);
        self.nodes.close(active.node, status, duration);
        self.partitions
            .get_mut(&partition)
            .expect("thread partition exists")
            .recent
            .push(RecentCall {
                thread_id,
                call_id,
                node_id: active.node,
                parent_call_id: active.parent_call_id,
                start_ns: active.start_ns,
                end_ns: timestamp_ns,
                status,
                dump_ref: 0,
            });
        Ok(())
    }

    fn process_end_thread(
        &mut self,
        thread_id: u64,
        status: ThreadEndStatus,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        if !self.threads.contains_key(&thread_id) {
            if !force {
                return Err(Missing::Thread);
            }
            self.ensure_recovery_thread(thread_id, timestamp_ns);
        }
        let has_pending = self
            .deferred
            .iter()
            .any(|item| item.event.thread_id() == thread_id && !item.event.is_end_thread());
        if !force && (has_pending || !self.threads[&thread_id].stack.is_empty()) {
            return Err(Missing::Quiescent);
        }
        if force {
            let open = self.threads[&thread_id]
                .stack
                .iter()
                .rev()
                .map(|active| active.key.call_id)
                .collect::<Vec<_>>();
            let close_status = match status {
                ThreadEndStatus::Completed => FunctionEndStatus::Ok,
                ThreadEndStatus::Cancelled => FunctionEndStatus::Cancelled,
                ThreadEndStatus::Errored => FunctionEndStatus::Errored,
            };
            for call_id in open {
                let _ = self.process_end(thread_id, call_id, close_status, timestamp_ns, true);
            }
        }
        self.charge_thread(thread_id, timestamp_ns);
        let thread = self.threads.remove(&thread_id).expect("ensured");
        if let Some(edge) = thread.entry_edge {
            self.spawn.finish(
                edge,
                thread.partition,
                thread_id,
                thread.name,
                thread.started_ns,
                timestamp_ns,
                status,
            );
        }
        Ok(())
    }

    fn process_set_id(
        &mut self,
        thread_id: u64,
        call_id: u64,
        _id: [u8; 16],
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        let key = CallKey { thread_id, call_id };
        if !self.threads.contains_key(&thread_id) {
            if !force {
                return Err(Missing::Thread);
            }
            return Ok(());
        }
        self.charge_thread(thread_id, timestamp_ns);
        if self
            .threads
            .get(&thread_id)
            .expect("checked above")
            .stack
            .iter()
            .rev()
            .any(|active| active.key == key)
        {
            Ok(())
        } else {
            let partition = self.threads[&thread_id].partition;
            if self.partitions[&partition].recent.find(key).is_some() {
                Ok(())
            } else if force {
                Ok(())
            } else {
                Err(Missing::Call)
            }
        }
    }

    fn process_suspend(
        &mut self,
        thread_id: u64,
        suspend_seq: u64,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        if !self.threads.contains_key(&thread_id) {
            if !force {
                return Err(Missing::Thread);
            }
            self.ensure_recovery_thread(thread_id, timestamp_ns);
        }
        if self.threads[&thread_id]
            .resumed_recent
            .contains(&suspend_seq)
        {
            return Ok(());
        }
        self.charge_thread(thread_id, timestamp_ns);
        self.threads.get_mut(&thread_id).expect("ensured").suspended =
            Some(Suspend { seq: suspend_seq });
        Ok(())
    }

    fn process_resume(
        &mut self,
        thread_id: u64,
        suspend_seq: u64,
        suspend_timestamp_ns: u64,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        if !self.threads.contains_key(&thread_id) {
            if !force {
                return Err(Missing::Thread);
            }
            self.ensure_recovery_thread(thread_id, suspend_timestamp_ns);
        }
        let matching = self.threads[&thread_id]
            .suspended
            .is_some_and(|suspend| suspend.seq == suspend_seq);
        if matching {
            self.charge_thread(thread_id, timestamp_ns);
            self.threads.get_mut(&thread_id).expect("ensured").suspended = None;
        } else {
            // Resume is self-contained. Avoid double credit if later events
            // already moved this thread through part of the interval.
            let credit_start = self.threads[&thread_id]
                .last_charge_ns
                .max(suspend_timestamp_ns)
                .min(timestamp_ns);
            self.charge_thread(thread_id, credit_start);
            let elapsed = timestamp_ns.saturating_sub(credit_start);
            let (node, edge) = {
                let thread = &self.threads[&thread_id];
                (thread.target_node(), thread.entry_edge)
            };
            self.nodes.add_await(node, elapsed);
            if let Some(edge) = edge {
                self.spawn.add_time(edge, elapsed, true);
            }
            let thread = self.threads.get_mut(&thread_id).expect("ensured");
            thread.last_charge_ns = thread.last_charge_ns.max(timestamp_ns);
            thread.watermark_ns = thread.watermark_ns.max(timestamp_ns);
            thread.suspended = None;
        }
        let resumed = &mut self
            .threads
            .get_mut(&thread_id)
            .expect("ensured")
            .resumed_recent;
        if !resumed.contains(&suspend_seq) {
            if resumed.len() == RESUMED_SEQ_MEMORY {
                resumed.pop_front();
            }
            resumed.push_back(suspend_seq);
        }
        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "mirrors the compact raw LlmCallMeta record"
    )]
    fn process_llm(
        &mut self,
        thread_id: u64,
        call_id: u64,
        model_id: u32,
        tokens_in: u32,
        tokens_out: u32,
        flags: LlmMetaFlags,
        timestamp_ns: u64,
        force: bool,
    ) -> Result<(), Missing> {
        if !self.threads.contains_key(&thread_id) {
            if !force {
                return Err(Missing::Thread);
            }
            self.ensure_recovery_thread(thread_id, timestamp_ns);
        }
        self.charge_thread(thread_id, timestamp_ns);
        let key = CallKey { thread_id, call_id };
        let partition = self.threads[&thread_id].partition;
        let node = if let Some(active) = self.threads[&thread_id]
            .stack
            .iter()
            .rev()
            .find(|active| active.key == key)
        {
            active.node
        } else if let Some(recent) = self.partitions[&partition].recent.find(key) {
            recent.node_id
        } else if force {
            let thread = &self.threads[&thread_id];
            self.nodes
                .unattributable(thread.root_node, thread.partition, thread_id)
        } else {
            return Err(Missing::Call);
        };
        self.llm_totals
            .entry((node, model_id))
            .or_default()
            .add_meta(
                tokens_in,
                tokens_out,
                flags.provider_error(),
                flags.parse_error(),
                flags.retry(),
            );
        self.llm_window
            .entry((node, model_id))
            .or_default()
            .add_meta(
                tokens_in,
                tokens_out,
                flags.provider_error(),
                flags.parse_error(),
                flags.retry(),
            );
        Ok(())
    }

    fn charge_thread(&mut self, thread_id: u64, timestamp_ns: u64) {
        let Some(thread) = self.threads.get_mut(&thread_id) else {
            return;
        };
        charge_state(
            thread,
            &mut self.nodes,
            &mut self.spawn,
            &mut self.health,
            timestamp_ns,
        );
    }

    fn resync_thread_after_loss(&mut self, thread_id: u64) {
        let Some(thread) = self.threads.get(&thread_id) else {
            return;
        };
        let partition = thread.partition;
        let root_node = thread.root_node;
        let timestamp_ns = thread.watermark_ns;
        self.mark_degraded(partition);
        self.charge_thread(thread_id, timestamp_ns);
        let unattributable = self.nodes.unattributable(root_node, partition, thread_id);
        let open = std::mem::take(
            &mut self
                .threads
                .get_mut(&thread_id)
                .expect("checked above")
                .stack,
        );
        for active in open.into_iter().rev() {
            self.nodes.enter(unattributable);
            self.nodes.close(
                unattributable,
                FunctionEndStatus::Cancelled,
                timestamp_ns.saturating_sub(active.start_ns),
            );
            self.partitions
                .get_mut(&partition)
                .expect("thread partition exists")
                .recent
                .push(RecentCall {
                    thread_id: active.key.thread_id,
                    call_id: active.key.call_id,
                    node_id: unattributable,
                    parent_call_id: active.parent_call_id,
                    start_ns: active.start_ns,
                    end_ns: timestamp_ns,
                    status: FunctionEndStatus::Cancelled,
                    dump_ref: 0,
                });
        }
        let thread = self.threads.get_mut(&thread_id).expect("exists");
        thread.root_node = unattributable;
        thread.spawn_ctx_node = unattributable;
        thread.suspended = None;
    }

    fn partition_for_event(&self, event: &CctEvent) -> Option<u32> {
        let thread_id = event.thread_id();
        self.threads.get(&thread_id).map(|thread| thread.partition)
    }

    fn mark_degraded(&mut self, partition: u32) {
        self.degraded.insert(partition);
        self.health.degraded_partitions = u64::try_from(self.degraded.len()).unwrap_or(u64::MAX);
    }
}

#[inline]
fn charge_state(
    thread: &mut ThreadState,
    nodes: &mut NodeStore,
    spawn: &mut SpawnStore,
    health: &mut CctHealth,
    timestamp_ns: u64,
) {
    thread.watermark_ns = thread.watermark_ns.max(timestamp_ns);
    if timestamp_ns < thread.last_charge_ns {
        health.reorder_clamped = health.reorder_clamped.saturating_add(1);
        return;
    }
    let elapsed = timestamp_ns - thread.last_charge_ns;
    let awaiting = thread.suspended.is_some();
    let node = thread.target_node();
    let edge = thread.entry_edge;
    thread.last_charge_ns = timestamp_ns;
    if awaiting {
        nodes.add_await(node, elapsed);
    } else {
        nodes.add_self(node, elapsed);
    }
    if let Some(edge) = edge {
        spawn.add_time(edge, elapsed, awaiting);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn start(thread_id: u64, timestamp_ns: u64) -> CctEvent {
        CctEvent::StartThread {
            flags: 0,
            thread_id,
            parent_thread_id: 0,
            parent_call_id: 0,
            timestamp_ns,
            name: None,
        }
    }

    fn call(
        thread_id: u64,
        call_id: u64,
        parent_call_id: u64,
        function_id: u32,
        timestamp_ns: u64,
    ) -> CctEvent {
        CctEvent::CallFunction {
            flags: 0,
            thread_id,
            call_id,
            parent_call_id,
            function_id: FunctionId(function_id),
            timestamp_ns,
        }
    }

    fn end(thread_id: u64, call_id: u64, timestamp_ns: u64) -> CctEvent {
        CctEvent::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id,
            call_id,
            timestamp_ns,
        }
    }

    fn node(snapshot: &CctSnapshot, function_id: u32) -> &NodeSnapshot {
        snapshot
            .nodes
            .iter()
            .find(|node| node.identity.function_id.0 == function_id)
            .expect("function node")
    }

    #[test]
    fn aggregates_counts_histogram_self_and_await_time() {
        let mut cct = EngineCct::new(1_000);
        for event in [
            start(1, 0),
            call(1, 1, 0, 10, 10),
            call(1, 2, 1, 20, 20),
            end(1, 2, 50),
            CctEvent::SuspendThread {
                reason: SuspendReason::Await,
                thread_id: 1,
                suspend_seq: 7,
                timestamp_ns: 60,
            },
            CctEvent::ResumeThread {
                thread_id: 1,
                suspend_seq: 7,
                suspend_timestamp_ns: 60,
                timestamp_ns: 160,
            },
            end(1, 1, 200),
        ] {
            cct.ingest(event);
        }
        let snapshot = cct.snapshot();
        let parent = node(&snapshot, 10);
        assert_eq!(parent.counters.enters, 1);
        assert_eq!(parent.counters.ends_ok, 1);
        assert_eq!(parent.counters.total_ns, 190);
        assert_eq!(parent.counters.self_ns, 60);
        assert_eq!(parent.counters.await_ns, 100);
        assert_eq!(parent.histogram.iter().sum::<u32>(), 1);
        let child = node(&snapshot, 20);
        assert_eq!(child.counters.total_ns, 30);
        assert_eq!(child.counters.self_ns, 30);
        assert_eq!(snapshot.recent_calls.len(), 2);
    }

    #[test]
    fn child_and_end_defer_until_their_parent_arrives() {
        let mut cct = EngineCct::default();
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 2, 1, 20, 20));
        cct.ingest(end(1, 2, 30));
        assert_eq!(cct.snapshot().health.deferred_records, 2);
        cct.ingest(call(1, 1, 0, 10, 10));
        cct.ingest(end(1, 1, 40));
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.health.deferred_records, 0);
        assert_eq!(node(&snapshot, 10).counters.ends_ok, 1);
        assert_eq!(node(&snapshot, 20).counters.ends_ok, 1);
    }

    #[test]
    fn self_contained_resume_is_order_independent() {
        let mut cct = EngineCct::default();
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 1, 0, 10, 10));
        cct.ingest(CctEvent::ResumeThread {
            thread_id: 1,
            suspend_seq: 3,
            suspend_timestamp_ns: 20,
            timestamp_ns: 120,
        });
        cct.ingest(CctEvent::SuspendThread {
            reason: SuspendReason::Await,
            thread_id: 1,
            suspend_seq: 3,
            timestamp_ns: 20,
        });
        cct.ingest(end(1, 1, 150));
        let function = node(&cct.snapshot(), 10).clone();
        assert_eq!(function.counters.await_ns, 100);
        assert_eq!(function.counters.self_ns, 40);
    }

    #[test]
    fn dirty_windows_charge_open_suspends_without_emitting_idle_rows() {
        let mut cct = EngineCct::new(100);
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 1, 0, 10, 10));
        cct.ingest(CctEvent::SuspendThread {
            reason: SuspendReason::Await,
            thread_id: 1,
            suspend_seq: 1,
            timestamp_ns: 20,
        });
        cct.close_windows_through(350);
        let windows = cct.take_windows();
        assert_eq!(windows.len(), 3);
        let await_per_window = windows
            .iter()
            .map(|window| {
                window
                    .nodes
                    .iter()
                    .map(|row| row.counters.await_ns)
                    .sum::<u64>()
            })
            .collect::<Vec<_>>();
        assert_eq!(await_per_window, vec![80, 100, 100]);
        assert!(windows.iter().all(|window| window.histograms.is_empty()));
    }

    #[test]
    fn equivalent_spawn_instances_share_one_edge_and_child_context() {
        let mut cct = EngineCct::default();
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 1, 0, 10, 1));
        for child in [2, 3] {
            cct.ingest(CctEvent::StartThread {
                flags: 0,
                thread_id: child,
                parent_thread_id: 1,
                parent_call_id: 1,
                timestamp_ns: 2,
                name: Some(format!("worker-{child}")),
            });
            cct.ingest(call(child, 1, 0, 20, 3));
            cct.ingest(end(child, 1, 10));
            cct.ingest(CctEvent::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: child,
                timestamp_ns: 11,
            });
        }
        cct.finish_sweep();
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.spawn_edges.len(), 1);
        assert_eq!(snapshot.spawn_edges[0].counters.spawned, 2);
        assert_eq!(snapshot.spawn_edges[0].counters.completed, 2);
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .filter(|node| node.identity.function_id.0 == 20)
                .count(),
            1
        );
        assert_eq!(snapshot.spawn_instances.len(), 2);
    }

    /// C11 acceptance: boundary ids grow for the lifetime of an engine, but
    /// completed partitions must not leave an id-sized prefix of empty slots.
    /// Exercise recent-call and sampled-spawn instance state as well as the
    /// partition slot itself so release covers all boundary-scoped stores.
    #[test]
    fn c11_releasing_10k_boundaries_keeps_partition_state_flat() {
        const PARTITIONS: u64 = 10_000;
        let mut cct = EngineCct::default();
        let mut retained_slot_capacity = None;

        for cycle in 0..PARTITIONS {
            let timestamp = cycle * 10;
            cct.ingest(start(1, timestamp));
            let partition = cct.partition_for_thread(1).expect("root partition");
            cct.ingest(call(1, 1, 0, 10, timestamp + 1));
            cct.ingest(CctEvent::StartThread {
                flags: 0,
                thread_id: 2,
                parent_thread_id: 1,
                parent_call_id: 1,
                timestamp_ns: timestamp + 2,
                name: Some("child".to_owned()),
            });
            cct.ingest(call(2, 1, 0, 20, timestamp + 3));
            cct.ingest(end(2, 1, timestamp + 4));
            cct.ingest(CctEvent::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: 2,
                timestamp_ns: timestamp + 5,
            });
            cct.ingest(end(1, 1, timestamp + 6));
            cct.ingest(CctEvent::EndThread {
                status: ThreadEndStatus::Completed,
                thread_id: 1,
                timestamp_ns: timestamp + 7,
            });
            cct.finish_sweep();

            assert_eq!(cct.threads.len(), 0);
            assert_eq!(cct.partitions.states.len(), 1);
            assert_eq!(
                cct.partitions
                    .values()
                    .map(|state| state.recent.snapshot().len())
                    .sum::<usize>(),
                2
            );
            assert_eq!(cct.spawn.instances().len(), 1);
            let capacity = *retained_slot_capacity.get_or_insert(cct.partitions.states.capacity());
            assert_eq!(cct.partitions.states.capacity(), capacity);

            assert!(cct.release_partition(partition));
            assert!(cct.partitions.states.is_empty());
            assert!(cct.partitions.values().next().is_none());
            assert!(cct.spawn.instances().is_empty());
            assert_eq!(cct.partitions.states.capacity(), capacity);
        }
    }

    #[test]
    fn session_epoch_rotation_requires_a_fully_released_partition() {
        let mut cct = EngineCct::default();
        assert!(cct.can_rotate_epoch());
        cct.ingest(start(1, 0));
        let partition = cct.partition_for_thread(1).unwrap();
        assert!(!cct.can_rotate_epoch());
        cct.ingest(CctEvent::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: 1,
            timestamp_ns: 1,
        });
        cct.finish_sweep();
        assert!(!cct.can_rotate_epoch());
        assert!(cct.release_partition(partition));
        assert!(cct.can_rotate_epoch());
    }

    #[test]
    fn deep_recursion_folds_only_after_depth_512() {
        let mut cct = EngineCct::default();
        cct.ingest(start(1, 0));
        for depth in 1..=514u64 {
            cct.ingest(call(1, depth, depth.saturating_sub(1), 10, depth));
        }
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.health.folded_frames, 2);
        assert!(
            snapshot
                .nodes
                .iter()
                .any(|node| { node.identity.flags & NODE_FLAG_RECURSION_FOLD != 0 })
        );
        assert_eq!(
            snapshot
                .nodes
                .iter()
                .filter(|node| node.identity.function_id.0 == 10)
                .count(),
            512
        );
        assert_eq!(snapshot.open_calls, 514);
    }

    #[test]
    fn llm_metadata_rolls_up_by_node_and_model() {
        let mut cct = EngineCct::new(100);
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 1, 0, 10, 10));
        for flags in [
            LlmMetaFlags(0),
            LlmMetaFlags(LlmMetaFlags::PROVIDER_ERROR | LlmMetaFlags::RETRY),
        ] {
            cct.ingest(CctEvent::LlmCallMeta {
                thread_id: 1,
                call_id: 1,
                model_id: 42,
                tokens_in: 10,
                tokens_out: 20,
                flags,
                timestamp_ns: 20,
            });
        }
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.llm.len(), 1);
        assert_eq!(snapshot.llm[0].counters.calls, 2);
        assert_eq!(snapshot.llm[0].counters.tokens_in, 20);
        assert_eq!(snapshot.llm[0].counters.provider_errs, 1);
        assert_eq!(snapshot.llm[0].counters.retries, 1);
    }

    #[test]
    fn expired_deferral_resyncs_to_unattributable_context() {
        let mut cct = EngineCct::default();
        cct.ingest(call(9, 2, 1, 20, 10));
        for _ in 0..DEFER_MAX_SWEEPS {
            cct.finish_sweep();
        }
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.health.deferred_records, 0);
        assert_eq!(snapshot.health.resync_records, 1);
        assert_eq!(snapshot.health.degraded_partitions, 1);
        assert!(snapshot.nodes.iter().any(|node| {
            node.identity.flags & super::super::nodes::NODE_FLAG_UNATTRIBUTABLE != 0
        }));
    }

    #[test]
    fn corrupt_range_closes_open_frames_and_continues_under_recovery_node() {
        let mut cct = EngineCct::default();
        cct.ingest(start(1, 0));
        cct.ingest(call(1, 1, 0, 10, 10));
        cct.mark_corrupt_range();
        cct.ingest(call(1, 2, 0, 20, 20));
        cct.ingest(end(1, 2, 30));
        let snapshot = cct.snapshot();
        assert_eq!(snapshot.health.corrupt_ranges, 1);
        assert_eq!(snapshot.health.degraded_partitions, 1);
        assert_eq!(snapshot.open_calls, 0);
        let recovery = snapshot
            .nodes
            .iter()
            .find(|node| {
                node.identity.flags & super::super::nodes::NODE_FLAG_UNATTRIBUTABLE != 0
                    && node.identity.partition != 0
            })
            .unwrap();
        assert_eq!(recovery.counters.ends_cancel, 1);
        assert_eq!(node(&snapshot, 20).identity.parent, recovery.node_id);
    }
}
