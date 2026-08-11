//! The CCT aggregation engine (observability design §5): consumes raw ring
//! records for one producer engine and maintains calling-context-tree
//! counters in RAM. Target-neutral — no fs, no threads; the consumer (or
//! the wasm cooperative drain) feeds it drained byte ranges.
//!
//! Ordering is causal, not timestamp-sorted (§5.2): one logical thread's
//! records can arrive via several rings (task migration at await points),
//! so records whose dependencies haven't arrived yet are deferred (bounded)
//! and retried at range boundaries in per-thread timestamp order; a
//! deferral surviving [`DEFER_MAX_SWEEPS`] sweeps synthesizes its missing
//! dependency as the unattributable node and degrades the partition — a
//! wedge is structurally impossible.
//!
//! Hot-path budget (§5.11): the common case — records of a live thread
//! arriving in order — takes ONE thread-slab lookup (cached), one intern
//! map probe, SoA counter bumps, and a recent-ring slot write. No per-call
//! map maintenance: open-call queries on cold paths scan the (≤256-frame)
//! stack instead.

use rustc_hash::FxHashMap;

use super::nodes::{Nodes, hist_bucket};
use super::recent::{RECENT_RING_SLOTS, RecentRing};
use super::spawn::SpawnEdges;
use crate::prof::record::{FunctionEndStatus, RawRecord, ThreadEndStatus};

/// §5.2: a deferral surviving this many sweeps triggers synthesized
/// recovery instead of waiting forever.
pub const DEFER_MAX_SWEEPS: u32 = 1024;

/// An owned copy of a ring record held by the defer machinery (≤ ~54 B;
/// the hot loop on one ring never defers).
#[derive(Debug, Clone)]
enum OwnedRecord {
    Call {
        thread_id: u64,
        call_id: u64,
        parent_call_id: u64,
        function_id: u32,
        ts_ticks: u64,
    },
    End {
        thread_id: u64,
        call_id: u64,
        status: FunctionEndStatus,
        ts_ticks: u64,
    },
    StartThread {
        thread_id: u64,
        parent_thread_id: u64,
        parent_call_id: u64,
        ts_ticks: u64,
    },
    EndThread {
        thread_id: u64,
        status: ThreadEndStatus,
        ts_ticks: u64,
    },
    Suspend {
        thread_id: u64,
        suspend_seq: u32,
        ts_ticks: u64,
    },
    Resume {
        thread_id: u64,
        suspend_seq: u32,
        suspend_ts_ticks: u64,
        ts_ticks: u64,
    },
    LlmMeta {
        thread_id: u64,
        call_id: u64,
        model_id: u32,
        tokens_in: u32,
        tokens_out: u32,
        provider_err: bool,
        parse_err: bool,
        ts_ticks: u64,
    },
}

/// What a deferred record waits for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum WaitKey {
    /// A `(thread, call)` that must be open before the record can apply.
    Call(u64, u64),
    /// A thread whose `StartThread` hasn't arrived.
    Thread(u64),
}

#[derive(Debug, Clone, Copy)]
struct ActiveCall {
    call_id: u64,
    parent_call_id: u64,
    node: u32,
    start_ns: u64,
}

#[derive(Debug, Clone, Copy)]
struct SuspendState {
    /// Read by the P3 window-close attribution (await spanning a window
    /// edge splits at the suspend timestamp).
    #[expect(dead_code, reason = "P3 window-close attribution reads these")]
    seq: u32,
    #[expect(dead_code, reason = "P3 window-close attribution reads these")]
    ts_ns: u64,
}

struct ThreadState {
    thread_id: u64,
    partition: u32,
    /// Partition-local thread index (§5.8 recent-ring slots).
    partition_thread_idx: u32,
    /// Where this thread's root calls intern: the partition pseudo-root
    /// for root threads, the spawning call's node for spawned threads
    /// (§5.5 — equivalent spawns share one child subtree).
    spawn_ctx_node: u32,
    stack: Vec<ActiveCall>,
    last_charge_ns: u64,
    suspended: Option<SuspendState>,
    /// Highest resume seq applied — a late-arriving suspend for an
    /// already-resumed park is dropped (counted), not double-charged.
    last_resume_seq: u32,
    /// Set once the thread's first root call interned its spawn edge.
    entry_edge: Option<u32>,
    /// Pending spawn-edge intern (child entry fn unknown until the first
    /// root call).
    pending_edge_ctx: Option<u32>,
    /// An `EndThread` waiting for quiescence (§5.2 lifecycle deferral).
    pending_end: Option<(ThreadEndStatus, u64)>,
    alive: bool,
}

struct Partition {
    /// The per-partition pseudo-node (§5.1).
    root_pseudo: u32,
    recent: RecentRing,
    thread_count: u32,
    degraded: bool,
    live_threads: u32,
}

/// Per-(node, model) LLM counters (§5.4), flushed as kind-10 rows in P3.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct LlmCounters {
    pub llm_calls: u64,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub provider_errs: u64,
    pub parse_errs: u64,
}

/// Diagnostics: every bound has a counter — bounded never means silent.
#[derive(Debug, Default, Clone, Copy)]
pub struct CctDiagnostics {
    pub records: u64,
    pub deferred: u64,
    pub replayed: u64,
    pub synthesized_parents: u64,
    pub reorder_clamped: u64,
    pub clock_anomalies: u64,
    pub evicted_calls: u64,
    pub late_suspends_dropped: u64,
    pub folded_frames: u64,
    pub degraded_partitions: u32,
    /// Records dropped by this engine's producers under the structural-
    /// exhaustion policy. Nonzero means population totals are lower
    /// bounds from the first drop onward.
    pub shed_records: u64,
    /// Drained ranges whose decode failed mid-range (rest of the range
    /// discarded, live partitions degraded).
    pub corrupt_ranges: u64,
    /// Histogram increments lost to bucket saturation (a bucket held at
    /// u32::MAX): distribution buckets are lower bounds from the first
    /// drop onward. Counts/times remain exact (u64 in memory).
    pub hist_saturated_drops: u64,
    /// Window-flush rows whose count delta exceeded the u32 wire width
    /// and was clamped. Physically implausible at the 250 ms cadence —
    /// counted so "implausible" is checked, not assumed.
    pub wire_clamped: u64,
}

/// The per-producer-engine CCT state.
pub struct CctEngine {
    nodes: Nodes,
    /// Thread slab + id index; `thread_cache` short-circuits the index for
    /// the common run of records from one thread.
    threads: Vec<ThreadState>,
    thread_index: FxHashMap<u64, u32>,
    thread_cache: (u64, u32),
    partitions: Vec<Partition>,
    /// §3.1 OnError pending count (root-level errored closes since the
    /// last [`CctEngine::take_errored_roots`]).
    errored_roots: u64,
    /// §3.1 OnLatencyMs: closes with duration above this fire a trigger
    /// (0 disables; default 30 s — the sysop default).
    latency_threshold_ns: u64,
    latency_triggers: u64,
    /// Deferred records with what they wait for (§5.2). A flat list:
    /// defers are rare and retries re-apply everything in causal order, so
    /// key-indexed storage would only add allocation churn.
    pending: Vec<(WaitKey, OwnedRecord, u32)>,
    spawn: SpawnEdges,
    llm: FxHashMap<(u32, u32), LlmCounters>,
    /// Current flush window epoch (§6.3); bumped by `take_window`.
    epoch: u32,
    diagnostics: CctDiagnostics,
    /// Max event ts seen — the §5.3 drained-event watermark.
    max_seen_ns: u64,
    /// Age carried through a retry re-defer (§5.2 timeout clock).
    current_retry_age: u32,
    /// §3 N4: live `$id` override annotations for OPEN calls (bounded).
    id_overrides: FxHashMap<(u64, u64), [u8; 16]>,
    /// Did any record apply since the last retry? (Skip no-op retries.)
    applied_since_retry: bool,
    /// Birth thread per `nodes.unflushed_births` entry born via
    /// `apply_call` — parallel ONLY within one window (roots/synthetic
    /// births interleave; matched by node id at flush, not position).
    birth_threads: Vec<u64>,
    /// LLM counter shadows for kind-10 delta rows.
    llm_flushed: FxHashMap<(u32, u32), LlmCounters>,
    /// Interned model names by id (from 0x09 births); index = id - 1.
    model_names: Vec<String>,
    /// Names not yet flushed as kind-11 rows.
    unflushed_models: Vec<u32>,
}

const NO_THREAD: u32 = u32::MAX;

impl Default for CctEngine {
    fn default() -> Self {
        Self::new(0)
    }
}

impl CctEngine {
    #[must_use]
    pub fn new(function_count: u32) -> CctEngine {
        CctEngine {
            nodes: Nodes::with_function_capacity(function_count),
            threads: Vec::new(),
            thread_index: FxHashMap::default(),
            thread_cache: (u64::MAX, NO_THREAD),
            partitions: Vec::new(),
            errored_roots: 0,
            latency_threshold_ns: 30_000_000_000,
            latency_triggers: 0,
            pending: Vec::new(),
            spawn: SpawnEdges::default(),
            llm: FxHashMap::default(),
            epoch: 0,
            diagnostics: CctDiagnostics::default(),
            max_seen_ns: 0,
            current_retry_age: 0,
            id_overrides: FxHashMap::default(),
            applied_since_retry: false,
            birth_threads: Vec::new(),
            llm_flushed: FxHashMap::default(),
            model_names: Vec::new(),
            unflushed_models: Vec::new(),
        }
    }

    #[must_use]
    pub fn diagnostics(&self) -> CctDiagnostics {
        let mut d = self.diagnostics;
        d.folded_frames = self.nodes.folded_frames;
        d.degraded_partitions =
            u32::try_from(self.partitions.iter().filter(|p| p.degraded).count()).unwrap_or(0);
        d
    }

    #[must_use]
    pub fn nodes(&self) -> &Nodes {
        &self.nodes
    }

    #[must_use]
    pub fn spawn_edges(&self) -> &SpawnEdges {
        &self.spawn
    }

    #[must_use]
    pub fn llm_counters(&self) -> &FxHashMap<(u32, u32), LlmCounters> {
        &self.llm
    }

    /// §3 N4: the live `$id` override for an open call, if any.
    #[must_use]
    pub fn id_override(&self, thread_id: u64, call_id: u64) -> Option<[u8; 16]> {
        self.id_overrides.get(&(thread_id, call_id)).copied()
    }

    #[must_use]
    pub fn recent_ring(&self, partition: u32) -> Option<&RecentRing> {
        self.partitions.get(partition as usize).map(|p| &p.recent)
    }

    /// Number of partition slots (§5.7; includes freed slots — callers
    /// probe each with [`CctEngine::recent_ring`]).
    #[must_use]
    pub fn partition_count(&self) -> u32 {
        u32::try_from(self.partitions.len()).unwrap_or(u32::MAX)
    }

    /// Drain the §3.1 OnError pending count (root-level errored closes).
    pub fn take_errored_roots(&mut self) -> u64 {
        std::mem::take(&mut self.errored_roots)
    }

    /// Drain the §3.1 OnLatency pending count.
    pub fn take_latency_triggers(&mut self) -> u64 {
        std::mem::take(&mut self.latency_triggers)
    }

    /// Set the §3.1 OnLatencyMs threshold (0 disables).
    pub fn set_latency_threshold_ns(&mut self, ns: u64) {
        self.latency_threshold_ns = ns;
    }

    #[inline]
    fn thread_slot(&mut self, thread_id: u64) -> u32 {
        if self.thread_cache.0 == thread_id {
            return self.thread_cache.1;
        }
        let slot = self
            .thread_index
            .get(&thread_id)
            .copied()
            .unwrap_or(NO_THREAD);
        if slot != NO_THREAD {
            self.thread_cache = (thread_id, slot);
        }
        slot
    }

    /// Consume one drained range of raw records. `to_ns` converts raw
    /// ticks (the consumer passes its `TickConverter::to_ns`).
    pub fn consume(&mut self, bytes: &[u8], to_ns: &mut impl FnMut(u64) -> u64) {
        for rec in crate::prof::record::iter(bytes) {
            match rec {
                Ok(raw) => {
                    self.diagnostics.records += 1;
                    self.apply_raw(&raw, to_ns);
                }
                Err(_) => {
                    // The consumer reports/aborts the range; aggregation
                    // marks everything live degraded (§5.2 resync), counts
                    // the loss so it persists (markers, boundary
                    // diagnostics), and keeps going.
                    self.diagnostics.corrupt_ranges += 1;
                    self.note_corrupt_range();
                    break;
                }
            }
        }
        // Range boundary: dependencies that arrived in this range unblock
        // earlier cross-ring deferrals. Retrying here (never mid-range)
        // preserves intra-ring record order; skipped when nothing applied
        // since the last retry (a fully-deferred range can't unblock
        // anything).
        if !self.pending.is_empty() && self.applied_since_retry {
            self.retry_pending(to_ns);
        }
    }

    /// §5.2: a structural loss happened (corrupt range, shed range). All
    /// live partitions coarsen visibly; open defers resolve via timeout.
    pub fn note_corrupt_range(&mut self) {
        for partition in &mut self.partitions {
            partition.degraded = true;
        }
    }

    /// Structural-exhaustion shed: `count` records this engine's producers
    /// dropped under the policy. Attribution after a drop is unknowable,
    /// so every live partition coarsens visibly, exactly like a corrupt
    /// range — and the count lands in [`CctDiagnostics::shed_records`] so
    /// completion evidence can state the loss.
    pub fn note_structural_shed(&mut self, count: u64) {
        self.diagnostics.shed_records += count;
        self.note_corrupt_range();
    }

    /// Direct dispatch: the hot variants (Call/End) run inline against the
    /// thread slab; everything else goes through the cold `apply` path.
    fn apply_raw(&mut self, raw: &RawRecord<'_>, to_ns: &mut impl FnMut(u64) -> u64) {
        match *raw {
            RawRecord::CallFunction {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ts_ticks,
                ..
            } => self.apply_call(
                thread_id.0,
                call_id.0,
                parent_call_id.0,
                function_id.0,
                ts_ticks,
                to_ns,
            ),
            RawRecord::EndFunction {
                status,
                thread_id,
                call_id,
                ts_ticks,
            } => self.apply_end(thread_id.0, call_id.0, status, ts_ticks, to_ns),
            RawRecord::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
                ..
            } => self.apply_cold(
                OwnedRecord::StartThread {
                    thread_id: thread_id.0,
                    parent_thread_id: parent_thread_id.0,
                    parent_call_id: parent_call_id.0,
                    ts_ticks,
                },
                to_ns,
            ),
            RawRecord::EndThread {
                status,
                thread_id,
                ts_ticks,
            } => self.apply_cold(
                OwnedRecord::EndThread {
                    thread_id: thread_id.0,
                    status,
                    ts_ticks,
                },
                to_ns,
            ),
            RawRecord::SuspendThread {
                reason: _,
                thread_id,
                suspend_seq,
                ts_ticks,
            } => self.apply_cold(
                OwnedRecord::Suspend {
                    thread_id: thread_id.0,
                    suspend_seq,
                    ts_ticks,
                },
                to_ns,
            ),
            RawRecord::ResumeThread {
                thread_id,
                suspend_seq,
                suspend_ts_ticks,
                ts_ticks,
                ..
            } => self.apply_cold(
                OwnedRecord::Resume {
                    thread_id: thread_id.0,
                    suspend_seq,
                    suspend_ts_ticks,
                    ts_ticks,
                },
                to_ns,
            ),
            RawRecord::LlmCallMeta {
                flags,
                thread_id,
                call_id,
                model_id,
                tokens_in,
                tokens_out,
                ts_ticks,
            } => self.apply_cold(
                OwnedRecord::LlmMeta {
                    thread_id: thread_id.0,
                    call_id: call_id.0,
                    model_id,
                    tokens_in,
                    tokens_out,
                    provider_err: flags & crate::prof::record::LLM_META_FLAG_PROVIDER_ERROR != 0,
                    parse_err: flags & crate::prof::record::LLM_META_FLAG_PARSE_ERROR != 0,
                    ts_ticks,
                },
                to_ns,
            ),
            RawRecord::ModelBirth { model_id, name, .. } => {
                // Once per model per engine (cold): keep the name for
                // kind-11 flushes and query resolution.
                if model_id >= 1 {
                    let idx = (model_id - 1) as usize;
                    if self.model_names.len() <= idx {
                        self.model_names.resize(idx + 1, String::new());
                    }
                    if self.model_names[idx].is_empty() {
                        self.model_names[idx] = String::from_utf8_lossy(name).into_owned();
                        self.unflushed_models.push(model_id);
                    }
                }
            }
            // `$id` overrides are recent-ring annotations only (§3, OQ7);
            // durable fidelity lives in dumps/full trace. Bounded: entries
            // die with their call's close (or thread finalize).
            RawRecord::SetFunctionId {
                thread_id,
                call_id,
                id,
                ts_ticks: _,
            } => {
                let slot = self.thread_slot(thread_id.0);
                if slot != NO_THREAD
                    && self.threads[slot as usize]
                        .stack
                        .iter()
                        .any(|c| c.call_id == call_id.0)
                    && self.id_overrides.len() < 16 * 1024
                {
                    self.id_overrides.insert((thread_id.0, call_id.0), id);
                }
            }
        }
    }

    /// §5.3 charge-to-current against a resolved thread.
    #[inline]
    fn charge_slot(
        nodes: &mut Nodes,
        diagnostics: &mut CctDiagnostics,
        epoch: u32,
        thread: &mut ThreadState,
        now_ns: u64,
    ) {
        if now_ns < thread.last_charge_ns {
            // Cross-ring drain latency at a window edge; never negative-
            // charge (§5.3 review fix — distinct from clock anomalies).
            diagnostics.reorder_clamped += 1;
            return;
        }
        let elapsed = now_ns - thread.last_charge_ns;
        thread.last_charge_ns = now_ns;
        if elapsed == 0 {
            return;
        }
        let target = thread
            .stack
            .last()
            .map_or(thread.spawn_ctx_node, |c| c.node) as usize;
        if thread.suspended.is_some() {
            nodes.await_ns[target] += elapsed;
        } else {
            nodes.self_ns[target] += elapsed;
        }
        nodes.dirty_epoch[target] = epoch;
    }

    #[inline]
    fn apply_call(
        &mut self,
        thread_id: u64,
        call_id: u64,
        parent_call_id: u64,
        function_id: u32,
        ts_ticks: u64,
        to_ns: &mut impl FnMut(u64) -> u64,
    ) {
        let slot = self.thread_slot(thread_id);
        if slot == NO_THREAD {
            self.defer(
                WaitKey::Thread(thread_id),
                OwnedRecord::Call {
                    thread_id,
                    call_id,
                    parent_call_id,
                    function_id,
                    ts_ticks,
                },
            );
            return;
        }
        let ts_ns = to_ns(ts_ticks);
        self.max_seen_ns = self.max_seen_ns.max(ts_ns);
        let thread = &mut self.threads[slot as usize];
        Self::charge_slot(
            &mut self.nodes,
            &mut self.diagnostics,
            self.epoch,
            thread,
            ts_ns,
        );
        let parent_node = if parent_call_id == 0 {
            thread.spawn_ctx_node
        } else if let Some(top) = thread.stack.last()
            && top.call_id == parent_call_id
        {
            top.node
        } else if let Some(frame) = thread
            .stack
            .iter()
            .rev()
            .find(|c| c.call_id == parent_call_id)
        {
            // Deep-in-stack parent (a sibling closed out of order): the
            // stack is ≤ MAX_FRAMES, and this path is cold.
            frame.node
        } else {
            // Parent push hasn't arrived (cross-ring migration): it lands
            // by the next range/sweep (§5.2); defer and retry.
            self.defer(
                WaitKey::Call(thread_id, parent_call_id),
                OwnedRecord::Call {
                    thread_id,
                    call_id,
                    parent_call_id,
                    function_id,
                    ts_ticks,
                },
            );
            return;
        };
        let pre_len = self.nodes.len();
        let node = self.nodes.intern(parent_node, function_id);
        if self.nodes.len() > pre_len {
            // New context born on this thread (kind-2 birth column).
            self.birth_threads.push(thread_id);
        }
        self.nodes.enters[node as usize] += 1;
        self.nodes.dirty_epoch[node as usize] = self.epoch;
        let thread = &mut self.threads[slot as usize];
        thread.stack.push(ActiveCall {
            call_id,
            parent_call_id,
            node,
            start_ns: ts_ns,
        });
        self.applied_since_retry = true;
        // First root call of a spawned thread completes the §5.5 spawn-
        // edge intern (child entry function now known).
        if parent_call_id == 0
            && let Some(ctx) = self.threads[slot as usize].pending_edge_ctx.take()
        {
            let edge = self.spawn.intern(ctx, function_id, node);
            self.spawn.on_spawn(edge, thread_id, ts_ns);
            self.threads[slot as usize].entry_edge = Some(edge);
        }
    }

    #[inline]
    fn apply_end(
        &mut self,
        thread_id: u64,
        call_id: u64,
        status: FunctionEndStatus,
        ts_ticks: u64,
        to_ns: &mut impl FnMut(u64) -> u64,
    ) {
        let slot = self.thread_slot(thread_id);
        if slot == NO_THREAD {
            self.defer(
                WaitKey::Thread(thread_id),
                OwnedRecord::End {
                    thread_id,
                    call_id,
                    status,
                    ts_ticks,
                },
            );
            return;
        }
        let ts_ns = to_ns(ts_ticks);
        self.max_seen_ns = self.max_seen_ns.max(ts_ns);
        let thread = &mut self.threads[slot as usize];
        // Fast path: stack top; miss walks (cancel drains close innermost-
        // first, §5.2); absent entirely ⇒ defer.
        let idx = if thread.stack.last().is_some_and(|c| c.call_id == call_id) {
            thread.stack.len() - 1
        } else if let Some(pos) = thread.stack.iter().rposition(|c| c.call_id == call_id) {
            pos
        } else {
            self.defer(
                WaitKey::Call(thread_id, call_id),
                OwnedRecord::End {
                    thread_id,
                    call_id,
                    status,
                    ts_ticks,
                },
            );
            return;
        };
        Self::charge_slot(
            &mut self.nodes,
            &mut self.diagnostics,
            self.epoch,
            thread,
            ts_ns,
        );
        let frame = if idx == thread.stack.len() - 1 {
            thread.stack.pop().expect("idx from non-empty stack")
        } else {
            thread.stack.remove(idx)
        };
        let partition = thread.partition;
        let partition_thread_idx = thread.partition_thread_idx;
        let node = frame.node as usize;
        match status {
            FunctionEndStatus::Ok => self.nodes.ends_ok[node] += 1,
            FunctionEndStatus::Errored => {
                self.nodes.ends_err[node] += 1;
                if idx == 0 {
                    // §3.1 OnError: a root-level errored close — the
                    // consumer polls this to fire flight-recorder dumps.
                    self.errored_roots += 1;
                }
            }
            FunctionEndStatus::Cancelled => self.nodes.ends_cancel[node] += 1,
            FunctionEndStatus::Exited => self.nodes.ends_exit[node] += 1,
        }
        let duration = ts_ns.saturating_sub(frame.start_ns);
        if self.latency_threshold_ns != 0 && duration > self.latency_threshold_ns {
            // §3.1 OnLatencyMs — consumer polls and fires a flight dump.
            self.latency_triggers += 1;
        }
        self.nodes.total_ns[node] += duration;
        // Saturating, never wrapping: a bucket held at u32::MAX makes the
        // distribution an explicit lower bound (counted, marked) instead
        // of a silently wrong one.
        let bucket = &mut self.nodes.hist[node][hist_bucket(duration)];
        if *bucket == u32::MAX {
            self.diagnostics.hist_saturated_drops += 1;
        } else {
            *bucket += 1;
        }
        // No dirty write here: charge_slot above already marked this node
        // (it was the stack top) — except the zero-elapsed case, so mark
        // only then.
        if self.nodes.dirty_epoch[node] != self.epoch {
            self.nodes.dirty_epoch[node] = self.epoch;
        }
        self.applied_since_retry = true;
        if !self.id_overrides.is_empty() {
            self.id_overrides.remove(&(thread_id, call_id));
        }
        // §5.8 recent-call ring slot.
        let evicted = self.partitions[partition as usize]
            .recent
            .push(super::recent::RecentCall {
                thread_idx: partition_thread_idx,
                call_id,
                node: frame.node,
                parent_call_id: frame.parent_call_id,
                start_ns: frame.start_ns,
                end_ns: ts_ns,
                status: status as u8,
                dump_ref: 0,
            });
        if evicted {
            self.diagnostics.evicted_calls += 1;
        }
    }

    #[cold]
    fn defer(&mut self, key: WaitKey, record: OwnedRecord) {
        self.diagnostics.deferred += 1;
        let age = self.current_retry_age;
        self.pending.push((key, record, age));
    }

    fn new_thread_state(
        &mut self,
        thread_id: u64,
        partition: u32,
        spawn_ctx_node: u32,
        pending_edge_ctx: Option<u32>,
        last_charge_ns: u64,
    ) -> u32 {
        let part = &mut self.partitions[partition as usize];
        let partition_thread_idx = part.thread_count;
        part.thread_count += 1;
        part.live_threads += 1;
        let slot = u32::try_from(self.threads.len()).expect("thread slab exceeds u32");
        self.threads.push(ThreadState {
            thread_id,
            partition,
            partition_thread_idx,
            spawn_ctx_node,
            stack: Vec::new(),
            last_charge_ns,
            suspended: None,
            last_resume_seq: 0,
            entry_edge: None,
            pending_edge_ctx,
            pending_end: None,
            alive: true,
        });
        self.thread_index.insert(thread_id, slot);
        self.thread_cache = (thread_id, slot);
        self.applied_since_retry = true;
        slot
    }

    fn new_partition(&mut self, degraded: bool) -> u32 {
        let partition = u32::try_from(self.partitions.len()).expect("partitions exceed u32");
        let root_pseudo = self.nodes.partition_root(partition);
        self.nodes.dirty_epoch[root_pseudo as usize] = self.epoch;
        self.partitions.push(Partition {
            root_pseudo,
            recent: RecentRing::new(RECENT_RING_SLOTS),
            thread_count: 0,
            degraded,
            live_threads: 0,
        });
        partition
    }

    #[must_use]
    fn partition_root_node(&self, partition: u32) -> u32 {
        self.partitions[partition as usize].root_pseudo
    }

    /// Cold-path records (thread lifecycle, parks, LLM meta) and every
    /// retried record.
    #[expect(clippy::too_many_lines, reason = "one cohesive causal dispatch")]
    fn apply_cold(&mut self, record: OwnedRecord, to_ns: &mut impl FnMut(u64) -> u64) {
        match record {
            OwnedRecord::Call {
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ts_ticks,
            } => self.apply_call(
                thread_id,
                call_id,
                parent_call_id,
                function_id,
                ts_ticks,
                to_ns,
            ),
            OwnedRecord::End {
                thread_id,
                call_id,
                status,
                ts_ticks,
            } => self.apply_end(thread_id, call_id, status, ts_ticks, to_ns),
            OwnedRecord::StartThread {
                thread_id,
                parent_thread_id,
                parent_call_id,
                ts_ticks,
            } => {
                let ts_ns = to_ns(ts_ticks);
                self.max_seen_ns = self.max_seen_ns.max(ts_ns);
                if self.thread_slot(thread_id) != NO_THREAD {
                    // Duplicate start — producer bug; count, don't wedge.
                    self.diagnostics.clock_anomalies += 1;
                    return;
                }
                if parent_thread_id == 0 {
                    // A root thread founds a new partition (§5.1).
                    let partition = self.new_partition(false);
                    let root = self.partition_root_node(partition);
                    self.new_thread_state(thread_id, partition, root, None, ts_ns);
                    return;
                }
                // Spawned thread: inherit the parent's partition (O(1));
                // the spawning call's node is the child's root context.
                let parent_slot = self.thread_slot(parent_thread_id);
                if parent_slot == NO_THREAD {
                    self.defer(
                        WaitKey::Thread(parent_thread_id),
                        OwnedRecord::StartThread {
                            thread_id,
                            parent_thread_id,
                            parent_call_id,
                            ts_ticks,
                        },
                    );
                    return;
                }
                let parent = &self.threads[parent_slot as usize];
                let partition = parent.partition;
                let fallback_ctx = parent.spawn_ctx_node;
                let parent_alive = parent.alive;
                let spawn_ctx = if parent_call_id == 0 {
                    fallback_ctx
                } else if let Some(frame) = parent
                    .stack
                    .iter()
                    .rev()
                    .find(|c| c.call_id == parent_call_id)
                {
                    frame.node
                } else if parent_alive && self.thread_has_waiters(parent_thread_id) {
                    // The spawning call's push may be cross-ring late.
                    self.defer(
                        WaitKey::Call(parent_thread_id, parent_call_id),
                        OwnedRecord::StartThread {
                            thread_id,
                            parent_thread_id,
                            parent_call_id,
                            ts_ticks,
                        },
                    );
                    return;
                } else {
                    // The spawning call already closed: the child still
                    // belongs to the parent's context.
                    fallback_ctx
                };
                self.new_thread_state(thread_id, partition, spawn_ctx, Some(spawn_ctx), ts_ns);
            }
            OwnedRecord::EndThread {
                thread_id,
                status,
                ts_ticks,
            } => {
                let slot = self.thread_slot(thread_id);
                if slot == NO_THREAD {
                    self.defer(
                        WaitKey::Thread(thread_id),
                        OwnedRecord::EndThread {
                            thread_id,
                            status,
                            ts_ticks,
                        },
                    );
                    return;
                }
                let ts_ns = to_ns(ts_ticks);
                self.max_seen_ns = self.max_seen_ns.max(ts_ns);
                // §5.2: defer finalization while the thread still has open
                // calls or records waiting on it — cross-ring stragglers
                // land by the next sweep.
                if !self.threads[slot as usize].stack.is_empty()
                    || self.thread_has_waiters(thread_id)
                {
                    self.threads[slot as usize].pending_end = Some((status, ts_ns));
                    return;
                }
                self.finalize_thread(slot, status, ts_ns);
            }
            OwnedRecord::Suspend {
                thread_id,
                suspend_seq,
                ts_ticks,
            } => {
                let slot = self.thread_slot(thread_id);
                if slot == NO_THREAD {
                    self.defer(
                        WaitKey::Thread(thread_id),
                        OwnedRecord::Suspend {
                            thread_id,
                            suspend_seq,
                            ts_ticks,
                        },
                    );
                    return;
                }
                let ts_ns = to_ns(ts_ticks);
                self.max_seen_ns = self.max_seen_ns.max(ts_ns);
                let thread = &mut self.threads[slot as usize];
                if suspend_seq <= thread.last_resume_seq {
                    // The self-contained resume already accounted this
                    // park (cross-ring reorder).
                    self.diagnostics.late_suspends_dropped += 1;
                    return;
                }
                Self::charge_slot(
                    &mut self.nodes,
                    &mut self.diagnostics,
                    self.epoch,
                    thread,
                    ts_ns,
                );
                let thread = &mut self.threads[slot as usize];
                thread.suspended = Some(SuspendState {
                    seq: suspend_seq,
                    ts_ns,
                });
                self.applied_since_retry = true;
            }
            OwnedRecord::Resume {
                thread_id,
                suspend_seq,
                suspend_ts_ticks,
                ts_ticks,
            } => {
                let slot = self.thread_slot(thread_id);
                if slot == NO_THREAD {
                    self.defer(
                        WaitKey::Thread(thread_id),
                        OwnedRecord::Resume {
                            thread_id,
                            suspend_seq,
                            suspend_ts_ticks,
                            ts_ticks,
                        },
                    );
                    return;
                }
                let ts_ns = to_ns(ts_ticks);
                let suspend_ns = to_ns(suspend_ts_ticks);
                self.max_seen_ns = self.max_seen_ns.max(ts_ns);
                // Self-contained (§5.3): whether or not the suspend record
                // was seen, the parked window is [suspend_ts, ts]. If the
                // suspend WAS applied, self-time is charged up to
                // suspend_ts and `suspended` is set — the uniform charge
                // below credits the window to await. If not (cross-ring
                // loss), reconstruct: charge self to suspend_ts, then
                // await to ts.
                let thread = &mut self.threads[slot as usize];
                if thread.suspended.is_none() {
                    Self::charge_slot(
                        &mut self.nodes,
                        &mut self.diagnostics,
                        self.epoch,
                        thread,
                        suspend_ns,
                    );
                    let thread = &mut self.threads[slot as usize];
                    thread.suspended = Some(SuspendState {
                        seq: suspend_seq,
                        ts_ns: suspend_ns,
                    });
                }
                let thread = &mut self.threads[slot as usize];
                Self::charge_slot(
                    &mut self.nodes,
                    &mut self.diagnostics,
                    self.epoch,
                    thread,
                    ts_ns,
                );
                let thread = &mut self.threads[slot as usize];
                thread.suspended = None;
                thread.last_resume_seq = thread.last_resume_seq.max(suspend_seq);
                self.applied_since_retry = true;
            }
            OwnedRecord::LlmMeta {
                thread_id,
                call_id,
                model_id,
                tokens_in,
                tokens_out,
                provider_err,
                parse_err,
                ts_ticks,
            } => {
                let slot = self.thread_slot(thread_id);
                if slot == NO_THREAD {
                    self.defer(
                        WaitKey::Thread(thread_id),
                        OwnedRecord::LlmMeta {
                            thread_id,
                            call_id,
                            model_id,
                            tokens_in,
                            tokens_out,
                            provider_err,
                            parse_err,
                            ts_ticks,
                        },
                    );
                    return;
                }
                // The call is usually still open (meta precedes the
                // sysop's EndFunction in program order).
                let node = self.threads[slot as usize]
                    .stack
                    .iter()
                    .rev()
                    .find(|c| c.call_id == call_id)
                    .map(|c| c.node);
                let Some(node) = node else {
                    if self.current_retry_age == 0 {
                        // First sight: the call's push may be cross-ring
                        // late — defer once.
                        self.defer(
                            WaitKey::Call(thread_id, call_id),
                            OwnedRecord::LlmMeta {
                                thread_id,
                                call_id,
                                model_id,
                                tokens_in,
                                tokens_out,
                                provider_err,
                                parse_err,
                                ts_ticks,
                            },
                        );
                    } else {
                        // Retried and the call still isn't open — it
                        // closed before the meta applied. Count rather
                        // than guess an attribution.
                        self.diagnostics.clock_anomalies += 1;
                    }
                    return;
                };
                let counters = self.llm.entry((node, model_id)).or_default();
                counters.llm_calls += 1;
                counters.tokens_in += u64::from(tokens_in);
                counters.tokens_out += u64::from(tokens_out);
                if provider_err {
                    counters.provider_errs += 1;
                }
                if parse_err {
                    counters.parse_errs += 1;
                }
                self.nodes.dirty_epoch[node as usize] = self.epoch;
                self.applied_since_retry = true;
            }
        }
    }

    fn thread_has_waiters(&self, thread_id: u64) -> bool {
        !self.pending.is_empty()
            && self.pending.iter().any(|(key, _, _)| match *key {
                WaitKey::Call(t, _) | WaitKey::Thread(t) => t == thread_id,
            })
    }

    fn finalize_thread(&mut self, slot: u32, status: ThreadEndStatus, ts_ns: u64) {
        {
            let thread = &mut self.threads[slot as usize];
            Self::charge_slot(
                &mut self.nodes,
                &mut self.diagnostics,
                self.epoch,
                thread,
                ts_ns,
            );
            thread.alive = false;
            thread.pending_end = None;
        }
        let thread_id = self.threads[slot as usize].thread_id;
        let partition = self.threads[slot as usize].partition;
        if let Some(edge) = self.threads[slot as usize].entry_edge {
            self.spawn.on_end(edge, thread_id, status, ts_ns);
        }
        self.partitions[partition as usize].live_threads = self.partitions[partition as usize]
            .live_threads
            .saturating_sub(1);
        // The slab entry stays (dead) so late stragglers still resolve the
        // thread; P3's partition free at boundary completion reclaims it.
        self.thread_index.remove(&thread_id);
        if self.thread_cache.0 == thread_id {
            self.thread_cache = (u64::MAX, NO_THREAD);
        }
    }

    /// A sweep tick (§5.2): ages deferrals and retries them. A deferral
    /// surviving [`DEFER_MAX_SWEEPS`] whose dependency no pending record
    /// can provide gets that dependency synthesized (unattributable node /
    /// degraded root partition), then everything retries to fixpoint.
    /// Also finalizes threads whose `EndThread` awaited quiescence.
    pub fn sweep_tick(&mut self, to_ns: &mut impl FnMut(u64) -> u64) {
        for (_, _, age) in &mut self.pending {
            *age = age.saturating_add(1);
        }
        if !self.pending.is_empty() {
            self.retry_pending(to_ns);
        }
        loop {
            // A key is "provided" when a pending record would create it on
            // apply — synthesizing those would fork reality; they resolve
            // through retries once their own dependencies materialize.
            let mut provided_calls: rustc_hash::FxHashSet<(u64, u64)> =
                rustc_hash::FxHashSet::default();
            let mut provided_threads: rustc_hash::FxHashSet<u64> = rustc_hash::FxHashSet::default();
            for (_, record, _) in &self.pending {
                match *record {
                    OwnedRecord::Call {
                        thread_id, call_id, ..
                    } => {
                        provided_calls.insert((thread_id, call_id));
                    }
                    OwnedRecord::StartThread { thread_id, .. } => {
                        provided_threads.insert(thread_id);
                    }
                    _ => {}
                }
            }
            let mut expired: Vec<WaitKey> = self
                .pending
                .iter()
                .filter(|(key, _, age)| {
                    *age >= DEFER_MAX_SWEEPS
                        && match *key {
                            WaitKey::Call(t, c) => !provided_calls.contains(&(t, c)),
                            WaitKey::Thread(t) => !provided_threads.contains(&t),
                        }
                })
                .map(|(key, _, _)| *key)
                .collect();
            expired.dedup();
            if expired.is_empty() {
                break;
            }
            // Deterministic resolution order: threads first, then calls in
            // (thread, call-id) order — call ids are per-thread monotonic,
            // so parents synthesize before their children.
            expired.sort_by_key(|key| match *key {
                WaitKey::Thread(t) => (0u8, t, 0),
                WaitKey::Call(t, c) => (1u8, t, c),
            });
            for key in expired {
                self.synthesize_key(key);
            }
            self.retry_pending(to_ns);
        }
        // Quiescent EndThread finalization (§5.2 lifecycle deferral).
        let ready: Vec<(u32, ThreadEndStatus, u64)> = self
            .threads
            .iter()
            .enumerate()
            .filter_map(|(slot, t)| {
                t.pending_end.and_then(|(status, ts)| {
                    (t.alive && t.stack.is_empty() && !self.thread_has_waiters(t.thread_id))
                        .then_some((u32::try_from(slot).unwrap(), status, ts))
                })
            })
            .collect();
        for (slot, status, ts) in ready {
            self.finalize_thread(slot, status, ts);
        }
    }

    /// Retry every pending record in per-thread timestamp order (one
    /// logical thread's clock is monotonic across rings, so time — never
    /// call-id order — is the causal proxy; a parent's `End` sorts after
    /// its children's records). Runs to fixpoint; re-defers keep their
    /// accumulated age.
    fn retry_pending(&mut self, to_ns: &mut impl FnMut(u64) -> u64) {
        loop {
            let before = self.pending.len();
            if before == 0 {
                break;
            }
            let mut queued = std::mem::take(&mut self.pending);
            queued.sort_by_key(|(_, record, _)| retry_order_key(record));
            for (_, record, age) in queued {
                self.current_retry_age = age;
                self.diagnostics.replayed += 1;
                self.apply_cold(record, to_ns);
            }
            self.current_retry_age = 0;
            if self.pending.len() >= before {
                break;
            }
        }
        self.applied_since_retry = false;
    }

    /// §5.2 synthesized recovery for a dependency nothing pending can
    /// provide: coarsen to the unattributable node / a degraded root
    /// partition. No replay here — the caller retries to fixpoint.
    fn synthesize_key(&mut self, key: WaitKey) {
        self.diagnostics.synthesized_parents += 1;
        match key {
            WaitKey::Thread(thread_id) => {
                if self.thread_slot(thread_id) != NO_THREAD {
                    return;
                }
                let partition = self.new_partition(true);
                let root = self.partition_root_node(partition);
                let last_charge = self.max_seen_ns;
                self.new_thread_state(thread_id, partition, root, None, last_charge);
            }
            WaitKey::Call(thread_id, call_id) => {
                if self.thread_slot(thread_id) == NO_THREAD {
                    self.synthesize_key(WaitKey::Thread(thread_id));
                }
                let slot = self.thread_slot(thread_id);
                if slot == NO_THREAD {
                    return;
                }
                if self.threads[slot as usize]
                    .stack
                    .iter()
                    .any(|c| c.call_id == call_id)
                {
                    return;
                }
                let parent_node = self.threads[slot as usize]
                    .stack
                    .last()
                    .map_or(self.threads[slot as usize].spawn_ctx_node, |c| c.node);
                let partition = self.threads[slot as usize].partition;
                let node = self.nodes.synthesize_unattributable(parent_node);
                self.nodes.enters[node as usize] += 1;
                self.nodes.dirty_epoch[node as usize] = self.epoch;
                self.partitions[partition as usize].degraded = true;
                let start_ns = self.max_seen_ns;
                self.threads[slot as usize].stack.push(ActiveCall {
                    call_id,
                    parent_call_id: 0,
                    node,
                    start_ns,
                });
                self.applied_since_retry = true;
            }
        }
    }

    /// Close the current flush window (§6.3): returns the dirty-node rows
    /// (deltas are the caller's job in P3 — this returns absolute counters
    /// for now, with the dirty set defining "rows this window"). Bumps the
    /// window epoch.
    pub fn take_window(&mut self) -> WindowSnapshot {
        let epoch = self.epoch;
        let mut dirty_nodes = Vec::new();
        for node in 0..self.nodes.len() {
            if self.nodes.dirty_epoch[node] == epoch {
                dirty_nodes.push(u32::try_from(node).unwrap_or(u32::MAX));
            }
        }
        let births = std::mem::take(&mut self.nodes.unflushed_births);
        self.epoch = self.epoch.wrapping_add(1);
        WindowSnapshot {
            epoch,
            dirty_nodes,
            births,
        }
    }
}

impl CctEngine {
    /// Close the flush window producing §6.3 DELTA rows: for every node
    /// dirty this window, `current − last_flushed` (then the shadow
    /// catches up). Hist rows only for nodes with ≥1 close this window;
    /// idle nodes cost zero rows — the growth law in action.
    pub fn flush_window(&mut self) -> WindowFlush {
        use super::blocks::{CctDeltaRow, CctHistRow, LlmDeltaRow, NodeBirthRow, SpawnEdgeRow};
        let epoch = self.epoch;
        let nodes = &mut self.nodes;

        // Births first (§6.3 ordering contract: a birth precedes the first
        // delta row referencing the node).
        let mut birth_rows: Vec<NodeBirthRow> = Vec::new();
        {
            // apply_call births carry their thread; roots/synthetic don't.
            // unflushed_births is in-order; birth_threads only covers the
            // apply_call subset, so match by walking both.
            let mut thread_iter = self.birth_threads.iter();
            for &node in &nodes.unflushed_births {
                let is_pool_birth = nodes.function[node as usize] != 0;
                let logical_thread_id = if is_pool_birth {
                    thread_iter.next().copied().unwrap_or(0)
                } else {
                    0
                };
                birth_rows.push(NodeBirthRow {
                    node_id: node,
                    parent_node_id: nodes.parent[node as usize],
                    function_id: nodes.function[node as usize],
                    logical_thread_id,
                    partition_id: nodes.partition[node as usize],
                });
            }
            nodes.unflushed_births.clear();
            self.birth_threads.clear();
        }

        let mut delta_rows: Vec<CctDeltaRow> = Vec::new();
        let mut hist_rows: Vec<CctHistRow> = Vec::new();
        for node in 0..nodes.len() {
            if nodes.dirty_epoch[node] != epoch {
                continue;
            }
            let enters = nodes.enters[node] - nodes.flushed_enters[node];
            let ends_ok = nodes.ends_ok[node] - nodes.flushed_ends_ok[node];
            let ends_err = nodes.ends_err[node] - nodes.flushed_ends_err[node];
            let ends_cancel = nodes.ends_cancel[node] - nodes.flushed_ends_cancel[node];
            let ends_exit = nodes.ends_exit[node] - nodes.flushed_ends_exit[node];
            let total_ns = nodes.total_ns[node] - nodes.flushed_total_ns[node];
            let self_ns = nodes.self_ns[node] - nodes.flushed_self_ns[node];
            let await_ns = nodes.await_ns[node] - nodes.flushed_await_ns[node];
            if enters | ends_ok | ends_err | ends_cancel | ends_exit | total_ns | self_ns | await_ns
                != 0
            {
                let clamp = |v: u64, clamped: &mut u64| {
                    if v > u64::from(u32::MAX) {
                        *clamped += 1;
                    }
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "explicitly clamped and counted just above"
                    )]
                    {
                        v.min(u64::from(u32::MAX)) as u32
                    }
                };
                let clamped = &mut self.diagnostics.wire_clamped;
                delta_rows.push(CctDeltaRow {
                    node_id: u32::try_from(node).unwrap_or(u32::MAX),
                    enters: clamp(enters, clamped),
                    ends_ok: clamp(ends_ok, clamped),
                    ends_err: clamp(ends_err, clamped),
                    ends_cancel: clamp(ends_cancel, clamped),
                    ends_exit: clamp(ends_exit, clamped),
                    total_ns,
                    self_ns,
                    await_ns,
                });
                nodes.flushed_enters[node] = nodes.enters[node];
                nodes.flushed_ends_ok[node] = nodes.ends_ok[node];
                nodes.flushed_ends_err[node] = nodes.ends_err[node];
                nodes.flushed_ends_cancel[node] = nodes.ends_cancel[node];
                nodes.flushed_ends_exit[node] = nodes.ends_exit[node];
                nodes.flushed_total_ns[node] = nodes.total_ns[node];
                nodes.flushed_self_ns[node] = nodes.self_ns[node];
                nodes.flushed_await_ns[node] = nodes.await_ns[node];
            }
            // Hist row only when the node CLOSED a call this window (§6.3
            // kind 9: an open call has no closes ⇒ no row).
            let mut buckets = [0u32; super::nodes::HIST_BUCKETS];
            let mut any = false;
            for (b, bucket) in buckets.iter_mut().enumerate() {
                // Saturating: a bucket held at u32::MAX yields delta 0 in
                // later windows instead of underflowing the shadow.
                let delta = nodes.hist[node][b].saturating_sub(nodes.flushed_hist[node][b]);
                *bucket = delta;
                any |= delta != 0;
            }
            if any {
                hist_rows.push(CctHistRow {
                    node_id: u32::try_from(node).unwrap_or(u32::MAX),
                    buckets,
                });
                nodes.flushed_hist[node] = nodes.hist[node];
            }
        }

        // LLM deltas: the map is small (distinct (node, model)); diff vs
        // shadow map.
        let mut llm_rows: Vec<LlmDeltaRow> = Vec::new();
        for (&(node, model_id), counters) in &self.llm {
            let shadow = self.llm_flushed.entry((node, model_id)).or_default();
            if *counters != *shadow {
                let clamp = |v: u64, clamped: &mut u64| {
                    if v > u64::from(u32::MAX) {
                        *clamped += 1;
                    }
                    #[expect(
                        clippy::cast_possible_truncation,
                        reason = "explicitly clamped and counted just above"
                    )]
                    {
                        v.min(u64::from(u32::MAX)) as u32
                    }
                };
                let clamped = &mut self.diagnostics.wire_clamped;
                llm_rows.push(LlmDeltaRow {
                    node_id: node,
                    llm_calls_delta: clamp(counters.llm_calls - shadow.llm_calls, clamped),
                    tokens_in_delta: counters.tokens_in - shadow.tokens_in,
                    tokens_out_delta: counters.tokens_out - shadow.tokens_out,
                    provider_errs_delta: clamp(
                        counters.provider_errs - shadow.provider_errs,
                        clamped,
                    ),
                    parse_errs_delta: clamp(counters.parse_errs - shadow.parse_errs, clamped),
                    model_id,
                });
                *shadow = *counters;
            }
        }

        // Spawn-edge deltas via the edge shadow columns.
        let spawn_rows: Vec<SpawnEdgeRow> = self.spawn.flush_deltas();

        // kind-11 model births (once per newly interned model).
        let model_rows: Vec<super::blocks::ModelBirthRow> = self
            .unflushed_models
            .drain(..)
            .map(|model_id| super::blocks::ModelBirthRow {
                model_id,
                name: self.model_names[(model_id - 1) as usize].clone(),
            })
            .collect();

        self.epoch = self.epoch.wrapping_add(1);
        WindowFlush {
            epoch,
            birth_rows,
            delta_rows,
            hist_rows,
            llm_rows,
            spawn_rows,
            model_rows,
        }
    }

    /// The partition a logical thread belongs to (root threads found their
    /// own partition; §5.1). Resolves finished threads too — binding can
    /// legitimately happen after a short run completed (cold slab scan).
    #[must_use]
    pub fn partition_of_thread(&self, thread_id: u64) -> Option<u32> {
        self.thread_index
            .get(&thread_id)
            .map(|&slot| self.threads[slot as usize].partition)
            .or_else(|| {
                self.threads
                    .iter()
                    .find(|t| t.thread_id == thread_id)
                    .map(|t| t.partition)
            })
    }

    /// §5.7 partition free at boundary completion: recent ring, dead
    /// thread stacks, and per-partition tables drop; the node rows stay
    /// (session-scoped) but the partition stops accumulating per-boundary
    /// state. Server memory stays O(live boundaries).
    pub fn free_partition(&mut self, partition: u32) {
        if let Some(part) = self.partitions.get_mut(partition as usize) {
            part.recent = RecentRing::new(2);
        }
        for thread in &mut self.threads {
            if thread.partition == partition && !thread.alive {
                thread.stack = Vec::new();
            }
        }
    }

    /// The §5.3 drained-event watermark: max event timestamp seen.
    #[must_use]
    pub fn max_seen_ns(&self) -> u64 {
        self.max_seen_ns
    }

    /// §6.1 session-epoch rotation: fresh node table (ids restart), live
    /// thread stacks re-interned by path so open calls keep aggregating.
    /// Callers write the carry-over checkpoint BEFORE rotating. Calls that
    /// span the epoch close with their `ends` in the later epoch (enters
    /// stayed in the earlier one) — visible in cross-epoch folds by
    /// design, never silent.
    pub fn rotate_epoch(&mut self, function_count_hint: u32) {
        let old_nodes = std::mem::replace(
            &mut self.nodes,
            Nodes::with_function_capacity(function_count_hint),
        );
        // Re-mint partition pseudo-roots in partition-id order (ids are
        // dense, so root order matches).
        for (partition_id, partition) in self.partitions.iter_mut().enumerate() {
            partition.root_pseudo = 0; // placeholder; fixed below
            let _ = partition_id;
        }
        for partition_id in 0..self.partitions.len() {
            let root = self
                .nodes
                .partition_root(u32::try_from(partition_id).unwrap_or(u32::MAX));
            self.partitions[partition_id].root_pseudo = root;
            self.nodes.dirty_epoch[root as usize] = self.epoch;
        }
        // Path re-intern helper: rebuild one old node's context in the new
        // table by walking its ancestor chain.
        fn reintern(
            old_nodes: &Nodes,
            new_nodes: &mut Nodes,
            partitions_roots: &[u32],
            old_node: u32,
            cache: &mut FxHashMap<u32, u32>,
        ) -> u32 {
            if let Some(&new) = cache.get(&old_node) {
                return new;
            }
            let i = old_node as usize;
            let parent = old_nodes.parent[i];
            let new = if parent == u32::MAX {
                // A partition pseudo-root.
                partitions_roots[old_nodes.partition[i] as usize]
            } else {
                let new_parent = reintern(old_nodes, new_nodes, partitions_roots, parent, cache);
                new_nodes.intern(new_parent, old_nodes.function[i])
            };
            cache.insert(old_node, new);
            new
        }
        let roots: Vec<u32> = self.partitions.iter().map(|p| p.root_pseudo).collect();
        let mut cache: FxHashMap<u32, u32> = FxHashMap::default();
        for thread in &mut self.threads {
            if !thread.alive {
                continue;
            }
            thread.spawn_ctx_node = reintern(
                &old_nodes,
                &mut self.nodes,
                &roots,
                thread.spawn_ctx_node,
                &mut cache,
            );
            for frame in &mut thread.stack {
                frame.node = reintern(&old_nodes, &mut self.nodes, &roots, frame.node, &mut cache);
            }
        }
        // Spawn edges: remap node references, carry totals, and set the
        // delta shadows to current so no delta re-emits.
        self.spawn.remap_after_epoch(|old_node| {
            reintern(&old_nodes, &mut self.nodes, &roots, old_node, &mut cache)
        });
        // LLM totals: remap keys, shadows catch up (no delta re-emission).
        let old_llm = std::mem::take(&mut self.llm);
        self.llm_flushed.clear();
        for ((node, model), counters) in old_llm {
            let new_node = reintern(&old_nodes, &mut self.nodes, &roots, node, &mut cache);
            let entry = self.llm.entry((new_node, model)).or_default();
            entry.llm_calls += counters.llm_calls;
            entry.tokens_in += counters.tokens_in;
            entry.tokens_out += counters.tokens_out;
            entry.provider_errs += counters.provider_errs;
            entry.parse_errs += counters.parse_errs;
            self.llm_flushed.insert((new_node, model), *entry);
        }
        // Model names persist (ids are engine-scoped, not epoch-scoped);
        // re-announce them in the new epoch's stream.
        self.unflushed_models = (1..=u32::try_from(self.model_names.len()).unwrap_or(0)).collect();
        // New-table counters start clean: suppress delta noise for the
        // re-interned rows by making shadows equal current (all zero) —
        // they already are in a fresh table.
        self.birth_threads.clear();
    }

    /// Interned model name for an id (from 0x09 births).
    #[must_use]
    pub fn model_name(&self, model_id: u32) -> Option<&str> {
        if model_id == 0 {
            return None;
        }
        self.model_names
            .get((model_id - 1) as usize)
            .map(String::as_str)
            .filter(|name| !name.is_empty())
    }
}

/// One flush window's §6.3 delta rows, ready for block encoding.
#[derive(Debug, Default)]
pub struct WindowFlush {
    pub epoch: u32,
    pub birth_rows: Vec<super::blocks::NodeBirthRow>,
    pub delta_rows: Vec<super::blocks::CctDeltaRow>,
    pub hist_rows: Vec<super::blocks::CctHistRow>,
    pub llm_rows: Vec<super::blocks::LlmDeltaRow>,
    pub spawn_rows: Vec<super::blocks::SpawnEdgeRow>,
    pub model_rows: Vec<super::blocks::ModelBirthRow>,
}

/// Causal-order proxy for retrying deferred records: per-thread timestamp
/// order, kind rank tiebreaking equal timestamps.
fn retry_order_key(record: &OwnedRecord) -> (u64, u64, u8) {
    match *record {
        OwnedRecord::StartThread {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 0),
        OwnedRecord::Call {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 1),
        OwnedRecord::Suspend {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 2),
        OwnedRecord::Resume {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 3),
        OwnedRecord::LlmMeta {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 4),
        OwnedRecord::End {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 5),
        OwnedRecord::EndThread {
            thread_id,
            ts_ticks,
            ..
        } => (thread_id, ts_ticks, 6),
    }
}

/// One flush window's dirty set (P3 turns this into `cct_delta` /
/// `cct_hist` / `llm_delta` blocks).
#[derive(Debug)]
pub struct WindowSnapshot {
    pub epoch: u32,
    pub dirty_nodes: Vec<u32>,
    pub births: Vec<u32>,
}
