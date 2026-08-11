//! CCT fold over BCCT segments (§9.2): the one reader every view shares.
//!
//! Input: segment bytes (`.bamlseg` streams or a sealed `.bamlcct`
//! snapshot) through a [`SegmentSource`]. Output: [`CctFold`] — merged SoA
//! nodes keyed by (parent, function) path identity, per-window activity
//! bands (§9.4 aggregate tier), spawn edges, and LLM totals.
//!
//! Epoch handling: node ids reset when a session rotates epochs (§6.1).
//! Segments are processed in `session_seg_seq` order; an `EPOCH_CLOSE`
//! marker ends the current id space, and the per-epoch tree is merged into
//! the fold by ancestor path, so cross-epoch nodes with the same calling
//! context unify. Within an epoch, `node_total` checkpoint blocks are the
//! authority (§9.5): totals = last checkpoint + deltas after it.

use rustc_hash::FxHashMap;

use bex_events::prof::cct::blocks::{self};
use bex_events::prof::cct::segment::{self, ScanEnd};

use crate::source::{ByteRange, FileId, Poll, SegmentSource};

pub const HIST_BUCKETS: usize = 16;

/// Merged fold output. SoA; index = fold-global node id; 0 = root.
#[derive(Debug, Default)]
pub struct CctFold {
    pub parent: Vec<u32>,
    pub function: Vec<u32>,
    pub thread: Vec<u64>,
    pub depth: Vec<u32>,
    pub enters: Vec<u64>,
    pub ends_ok: Vec<u64>,
    pub ends_err: Vec<u64>,
    pub ends_cancel: Vec<u64>,
    pub ends_exit: Vec<u64>,
    pub total_ns: Vec<u64>,
    pub self_ns: Vec<u64>,
    pub await_ns: Vec<u64>,
    pub hist: Vec<[u32; HIST_BUCKETS]>,
    /// Children adjacency (built at finish; sorted by total_ns desc).
    pub children: Vec<Vec<u32>>,
    /// §9.4 aggregate tier: (thread, window_first_ts, window_last_ts,
    /// busy_ns, await_ns, dominant_function, errors).
    pub bands: Vec<BandRow>,
    /// Spawn edges: (parent_node fold id, entry_fn, spawns, completed,
    /// errored, cancelled).
    pub spawns: Vec<(u32, u32, u64, u64, u64, u64)>,
    /// Partition each node was born in (u32::MAX = fold root/unknown).
    /// Partition ids are session-scoped; with `partition_binds` they
    /// attribute live-session subtrees to bound boundaries.
    pub partition: Vec<u32>,
    /// PartitionBind rows seen: (partition_id, boundary_id bytes).
    pub partition_binds: Vec<(u32, [u8; 16])>,
    /// LLM totals per (fold node, model id): (llm_calls, tokens_in,
    /// tokens_out, provider_errs, parse_errs).
    pub llm: FxHashMap<(u32, u32), (u64, u64, u64, u64, u64)>,
    /// Model id → interned model name (ModelBirth rows).
    pub models: FxHashMap<u32, String>,
    /// (parent<<32)|function → node id intern (fold construction).
    intern: FxHashMap<u64, u32>,
    /// Scan facts for the §8.4 trust contract.
    pub torn: bool,
    pub sealed: bool,
    pub last_ts_ns: u64,
    pub first_ts_ns: u64,
    /// Declared loss/degradation evidence from kind-12 markers (SHED,
    /// DEGRADED, LOSS, BUDGET_EXHAUSTED, SATURATED): `(marker_kind,
    /// detail)`. Epoch-close markers are structural, not loss, and are
    /// handled separately. Every consumer of this fold must surface
    /// these — declared loss must never disappear in a reader.
    pub loss_markers: Vec<(u8, String)>,
}

#[derive(Debug, Clone, Copy)]
pub struct BandRow {
    pub thread: u64,
    pub first_ts_ns: u64,
    pub last_ts_ns: u64,
    pub busy_ns: u64,
    pub await_ns: u64,
    pub dominant_function: u32,
    pub errors: u64,
}

impl CctFold {
    fn root() -> CctFold {
        let mut fold = CctFold::default();
        fold.push_node(0, 0, 0, 0, u32::MAX);
        fold
    }

    fn push_node(
        &mut self,
        parent: u32,
        function: u32,
        thread: u64,
        depth: u32,
        partition: u32,
    ) -> u32 {
        let id = u32::try_from(self.parent.len()).unwrap_or(u32::MAX);
        self.intern
            .insert(u64::from(parent) << 32 | u64::from(function), id);
        self.parent.push(parent);
        self.function.push(function);
        self.thread.push(thread);
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
        id
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.parent.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() <= 1
    }
}

/// Per-epoch accumulation state (segment-local node ids).
#[derive(Default)]
struct Epoch {
    /// node id → (parent id, function, thread, partition).
    births: FxHashMap<u32, (u32, u32, u64, u32)>,
    /// node id → checkpointed absolute totals (from the last `node_total`).
    checkpoint: FxHashMap<u32, NodeTotals>,
    /// node id → deltas accumulated AFTER the last checkpoint.
    delta: FxHashMap<u32, NodeTotals>,
    hist: FxHashMap<u32, [u32; HIST_BUCKETS]>,
    llm: FxHashMap<(u32, u32), (u64, u64, u64, u64, u64)>,
    /// edge_id → (parent_node, entry_fn, spawns, completed, errored, cancelled).
    spawns: FxHashMap<u32, (u32, u32, u64, u64, u64, u64)>,
}

#[derive(Debug, Default, Clone, Copy)]
struct NodeTotals {
    enters: u64,
    ends_ok: u64,
    ends_err: u64,
    ends_cancel: u64,
    ends_exit: u64,
    total_ns: u64,
    self_ns: u64,
    await_ns: u64,
}

impl NodeTotals {
    fn from_row(r: &blocks::CctDeltaRow) -> NodeTotals {
        NodeTotals {
            enters: u64::from(r.enters),
            ends_ok: u64::from(r.ends_ok),
            ends_err: u64::from(r.ends_err),
            ends_cancel: u64::from(r.ends_cancel),
            ends_exit: u64::from(r.ends_exit),
            total_ns: r.total_ns,
            self_ns: r.self_ns,
            await_ns: r.await_ns,
        }
    }

    fn add(&mut self, o: &NodeTotals) {
        self.enters += o.enters;
        self.ends_ok += o.ends_ok;
        self.ends_err += o.ends_err;
        self.ends_cancel += o.ends_cancel;
        self.ends_exit += o.ends_exit;
        self.total_ns += o.total_ns;
        self.self_ns += o.self_ns;
        self.await_ns += o.await_ns;
    }
}

/// Fold one or more segment files (one session's `cct/` in seq order, or a
/// single sealed `.bamlcct`). Sans-io: missing views surface as
/// `Poll::NeedData`.
pub fn fold_segments(source: &dyn SegmentSource, files: &[FileId]) -> Poll<CctFold> {
    let mut need = Vec::new();
    let mut views = Vec::with_capacity(files.len());
    for &file in files {
        let len = source.committed_len(file);
        if len == 0 {
            continue;
        }
        let range = ByteRange {
            file,
            offset: 0,
            len,
        };
        match source.view(&range) {
            Some(bytes) => views.push(bytes),
            None => need.push(range),
        }
    }
    if !need.is_empty() {
        return Poll::NeedData(need);
    }

    let mut fold = CctFold::root();
    let mut epoch = Epoch::default();
    let mut sealed_all = true;
    for bytes in views {
        let Ok(contents) = segment::scan_segment(bytes) else {
            fold.torn = true;
            continue;
        };
        match contents.end {
            ScanEnd::Sealed => {}
            ScanEnd::ActiveEnd => sealed_all = false,
            ScanEnd::Torn { .. } => {
                fold.torn = true;
                sealed_all = false;
            }
        }
        let mut epoch_closed = false;
        for block in &contents.blocks {
            fold.last_ts_ns = fold.last_ts_ns.max(block.last_ts_ns);
            if fold.first_ts_ns == 0 {
                fold.first_ts_ns = block.first_ts_ns;
            } else if block.first_ts_ns > 0 {
                fold.first_ts_ns = fold.first_ts_ns.min(block.first_ts_ns);
            }
            apply_block(&mut fold, &mut epoch, block, &mut epoch_closed);
        }
        if epoch_closed {
            merge_epoch(&mut fold, std::mem::take(&mut epoch));
        }
    }
    merge_epoch(&mut fold, epoch);
    fold.sealed = sealed_all;
    finish(&mut fold);
    Poll::Ready(fold)
}

fn apply_block(
    fold: &mut CctFold,
    epoch: &mut Epoch,
    block: &segment::Block<'_>,
    epoch_closed: &mut bool,
) {
    let rows = block.row_count as usize;
    match block.kind {
        k if k == segment::BlockKind::NodeBirth as u8 => {
            if let Some(births) = blocks::decode_node_birth(block.payload, rows) {
                for b in births {
                    epoch.births.insert(
                        b.node_id,
                        (
                            b.parent_node_id,
                            b.function_id,
                            b.logical_thread_id,
                            b.partition_id,
                        ),
                    );
                }
            }
        }
        k if k == segment::BlockKind::CctDelta as u8 => {
            if let Some(deltas) = blocks::decode_cct_delta(block.payload, rows) {
                // §9.4 band: one row per (thread × this window).
                let mut per_thread: FxHashMap<u64, (u64, u64, u64, FxHashMap<u32, u64>)> =
                    FxHashMap::default();
                for d in &deltas {
                    epoch
                        .delta
                        .entry(d.node_id)
                        .or_default()
                        .add(&NodeTotals::from_row(d));
                    if let Some(&(_, function, thread, _)) = epoch.births.get(&d.node_id) {
                        if function == 0 {
                            // Partition-root/unattributable nodes are
                            // shared across threads — no lane to charge.
                            continue;
                        }
                        let slot = per_thread.entry(thread).or_default();
                        slot.0 += d.self_ns;
                        slot.1 += d.await_ns;
                        slot.2 += u64::from(d.ends_err);
                        *slot.3.entry(function).or_default() += d.self_ns;
                    }
                }
                for (thread, (busy, awaiting, errors, by_fn)) in per_thread {
                    if busy == 0 && awaiting == 0 && errors == 0 {
                        // Root/bookkeeping nodes produce zero-activity
                        // rows; an empty band is noise, not a lane.
                        continue;
                    }
                    let dominant = by_fn
                        .iter()
                        .max_by_key(|(_, ns)| **ns)
                        .map_or(0, |(f, _)| *f);
                    fold.bands.push(BandRow {
                        thread,
                        first_ts_ns: block.first_ts_ns,
                        last_ts_ns: block.last_ts_ns,
                        busy_ns: busy,
                        await_ns: awaiting,
                        dominant_function: dominant,
                        errors,
                    });
                }
            }
        }
        k if k == segment::BlockKind::NodeTotal as u8 => {
            if let Some(totals) = blocks::decode_cct_delta(block.payload, rows) {
                for t in totals {
                    epoch.checkpoint.insert(t.node_id, NodeTotals::from_row(&t));
                    epoch.delta.remove(&t.node_id);
                }
            }
        }
        k if k == segment::BlockKind::CctHist as u8 => {
            if let Some(hists) = blocks::decode_cct_hist(block.payload, rows) {
                for h in hists {
                    let slot = epoch.hist.entry(h.node_id).or_insert([0; HIST_BUCKETS]);
                    for (a, b) in slot.iter_mut().zip(h.buckets.iter()) {
                        *a = a.saturating_add(*b);
                    }
                }
            }
        }
        k if k == segment::BlockKind::LlmDelta as u8 => {
            if let Some(llm_rows) = blocks::decode_llm_delta(block.payload, rows) {
                for l in llm_rows {
                    let slot = epoch.llm.entry((l.node_id, l.model_id)).or_default();
                    slot.0 += u64::from(l.llm_calls_delta);
                    slot.1 += l.tokens_in_delta;
                    slot.2 += l.tokens_out_delta;
                    slot.3 += u64::from(l.provider_errs_delta);
                    slot.4 += u64::from(l.parse_errs_delta);
                }
            }
        }
        k if k == segment::BlockKind::SpawnEdge as u8 => {
            if let Some(edges) = blocks::decode_spawn_edge(block.payload, rows) {
                for e in edges {
                    let slot = epoch.spawns.entry(e.edge_id).or_insert((
                        e.parent_node,
                        e.entry_fn,
                        0,
                        0,
                        0,
                        0,
                    ));
                    slot.2 += u64::from(e.spawn_delta);
                    slot.3 += u64::from(e.completed_delta);
                    slot.4 += u64::from(e.errored_delta);
                    slot.5 += u64::from(e.cancelled_delta);
                }
            }
        }
        k if k == segment::BlockKind::PartitionBind as u8 => {
            if let Some(binds) = blocks::decode_partition_bind(block.payload, rows) {
                for bind in binds {
                    fold.partition_binds
                        .push((bind.partition_id, bind.boundary_id));
                }
            }
        }
        k if k == segment::BlockKind::ModelBirth as u8 => {
            if let Some(models) = blocks::decode_model_birth(block.payload, rows) {
                for m in models {
                    fold.models.entry(m.model_id).or_insert(m.name);
                }
            }
        }
        k if k == segment::BlockKind::Marker as u8 => {
            if let Some(markers) = blocks::decode_marker(block.payload, rows) {
                for m in markers {
                    if m.marker_kind == blocks::marker_kind::EPOCH_CLOSE {
                        *epoch_closed = true;
                    } else {
                        // Declared loss/degradation travels with the fold
                        // (§8.4): shed, degraded, saturated, budget.
                        fold.loss_markers.push((m.marker_kind, m.detail));
                    }
                }
            }
        }
        _ => {}
    }
}

/// Merge one epoch's segment-local tree into the fold by ancestor path.
fn merge_epoch(fold: &mut CctFold, epoch: Epoch) {
    if epoch.births.is_empty() && epoch.checkpoint.is_empty() && epoch.delta.is_empty() {
        return;
    }
    // Resolve each local node to a fold-global id by walking parents. Memo
    // across calls; cycles (corrupt input) cut off at the root.
    let mut memo: FxHashMap<u32, u32> = FxHashMap::default();

    fn resolve(
        local: u32,
        epoch: &Epoch,
        fold: &mut CctFold,
        memo: &mut FxHashMap<u32, u32>,
        guard: usize,
    ) -> u32 {
        if local == 0 || guard > 4096 {
            return 0;
        }
        if let Some(&g) = memo.get(&local) {
            return g;
        }
        let Some(&(parent, function, thread, partition)) = epoch.births.get(&local) else {
            return 0; // unannounced node: charge to root rather than drop
        };
        let parent_global = resolve(parent, epoch, fold, memo, guard + 1);
        // Find or create the (parent_global, function) child via the
        // intern map — O(1) per node.
        let key = u64::from(parent_global) << 32 | u64::from(function);
        let global = match fold.intern.get(&key) {
            Some(&id) if id != 0 => id,
            _ => {
                let depth = fold.depth[parent_global as usize] + 1;
                fold.push_node(parent_global, function, thread, depth, partition)
            }
        };
        memo.insert(local, global);
        global
    }

    let mut ids: Vec<u32> = epoch.births.keys().copied().collect();
    ids.sort_unstable();
    for local in ids {
        let global = resolve(local, &epoch, fold, &mut memo, 0);
        if global == 0 {
            continue;
        }
        let mut totals = epoch.checkpoint.get(&local).copied().unwrap_or_default();
        if let Some(d) = epoch.delta.get(&local) {
            totals.add(d);
        }
        let g = global as usize;
        fold.enters[g] += totals.enters;
        fold.ends_ok[g] += totals.ends_ok;
        fold.ends_err[g] += totals.ends_err;
        fold.ends_cancel[g] += totals.ends_cancel;
        fold.ends_exit[g] += totals.ends_exit;
        fold.total_ns[g] += totals.total_ns;
        fold.self_ns[g] += totals.self_ns;
        fold.await_ns[g] += totals.await_ns;
        if let Some(h) = epoch.hist.get(&local) {
            for (a, b) in fold.hist[g].iter_mut().zip(h.iter()) {
                *a = a.saturating_add(*b);
            }
        }
    }
    for (&(local, model_id), &(calls, tin, tout, perr, parse)) in &epoch.llm {
        let global = memo.get(&local).copied().unwrap_or(0);
        let slot = fold.llm.entry((global, model_id)).or_default();
        slot.0 += calls;
        slot.1 += tin;
        slot.2 += tout;
        slot.3 += perr;
        slot.4 += parse;
    }
    for (_, (parent_local, entry_fn, s, c, e, x)) in epoch.spawns {
        let parent_global = memo.get(&parent_local).copied().unwrap_or(0);
        fold.spawns.push((parent_global, entry_fn, s, c, e, x));
    }
}

fn finish(fold: &mut CctFold) {
    let n = fold.len();
    let mut children: Vec<Vec<u32>> = vec![Vec::new(); n];
    for i in 1..n {
        let p = fold.parent[i] as usize;
        if p < n && p != i {
            children[p].push(u32::try_from(i).unwrap_or(u32::MAX));
        }
    }
    for list in &mut children {
        list.sort_by_key(|&c| std::cmp::Reverse(fold.total_ns[c as usize]));
    }
    fold.children = children;
    fold.bands.sort_by_key(|b| (b.thread, b.first_ts_ns));
}

/// §9.6 Left Heavy: preorder SoA emission of nodes with extent ≥
/// 1/(2·pixel_width), one synthetic "smaller" node (function id
/// `u32::MAX`) per truncated parent — visible aggregation, never silent.
#[derive(Debug, Default)]
pub struct LeftHeavyRows {
    pub depth: Vec<u32>,
    pub function: Vec<u32>,
    pub total_ns: Vec<u64>,
    pub self_ns: Vec<u64>,
    pub enters: Vec<u64>,
    pub errors: Vec<u64>,
    /// Number of nodes folded into a synthetic "smaller" row (0 = real).
    pub folded: Vec<u32>,
}

#[must_use]
pub fn left_heavy(fold: &CctFold, pixel_width: u32) -> LeftHeavyRows {
    let mut rows = LeftHeavyRows::default();
    let root_total: u64 = fold.children[0]
        .iter()
        .map(|&c| fold.total_ns[c as usize])
        .sum();
    if root_total == 0 {
        return rows;
    }
    let min_ns = root_total / (2 * u64::from(pixel_width.clamp(1, 8192)));

    fn emit(fold: &CctFold, node: u32, depth: u32, min_ns: u64, rows: &mut LeftHeavyRows) {
        let mut folded = 0u32;
        let mut folded_ns = 0u64;
        let mut folded_calls = 0u64;
        for &child in &fold.children[node as usize] {
            let c = child as usize;
            if fold.total_ns[c] >= min_ns.max(1) {
                rows.depth.push(depth);
                rows.function.push(fold.function[c]);
                rows.total_ns.push(fold.total_ns[c]);
                rows.self_ns.push(fold.self_ns[c]);
                rows.enters.push(fold.enters[c]);
                rows.errors.push(fold.ends_err[c]);
                rows.folded.push(0);
                emit(fold, child, depth + 1, min_ns, rows);
            } else {
                folded += 1;
                folded_ns += fold.total_ns[c];
                folded_calls += fold.enters[c];
            }
        }
        if folded > 0 {
            rows.depth.push(depth);
            rows.function.push(u32::MAX);
            rows.total_ns.push(folded_ns);
            rows.self_ns.push(folded_ns);
            rows.enters.push(folded_calls);
            rows.errors.push(0);
            rows.folded.push(folded);
        }
    }
    emit(fold, 0, 0, min_ns, &mut rows);
    rows
}

/// Per-function totals (§9.6 top-functions table).
#[derive(Debug, Default)]
pub struct FunctionTotals {
    pub function: Vec<u32>,
    pub calls: Vec<u64>,
    pub total_ns: Vec<u64>,
    pub self_ns: Vec<u64>,
    pub errors: Vec<u64>,
}

#[must_use]
pub fn top_functions(fold: &CctFold, limit: usize) -> FunctionTotals {
    let mut by_fn: FxHashMap<u32, (u64, u64, u64, u64)> = FxHashMap::default();
    for i in 1..fold.len() {
        let slot = by_fn.entry(fold.function[i]).or_default();
        slot.0 += fold.enters[i];
        slot.1 += fold.total_ns[i];
        slot.2 += fold.self_ns[i];
        slot.3 += fold.ends_err[i];
    }
    let mut rows: Vec<(u32, (u64, u64, u64, u64))> = by_fn.into_iter().collect();
    rows.sort_by_key(|(_, v)| std::cmp::Reverse(v.2));
    rows.truncate(limit);
    let mut out = FunctionTotals::default();
    for (f, (calls, total, self_ns, errors)) in rows {
        out.function.push(f);
        out.calls.push(calls);
        out.total_ns.push(total);
        out.self_ns.push(self_ns);
        out.errors.push(errors);
    }
    out
}

/// §9.3 LOD climb: merge each thread's bands in groups of `factor`
/// adjacent windows. Sums are exact; the dominant function of a merged
/// band is the dominant of its busiest constituent.
#[must_use]
pub fn coarsen_bands(bands: &[BandRow], factor: usize) -> Vec<BandRow> {
    if factor <= 1 || bands.is_empty() {
        return bands.to_vec();
    }
    let mut out = Vec::with_capacity(bands.len() / factor + 1);
    let mut i = 0;
    while i < bands.len() {
        let thread = bands[i].thread;
        let mut merged = bands[i];
        let mut best_busy = bands[i].busy_ns;
        let mut n = 1;
        while n < factor && i + n < bands.len() && bands[i + n].thread == thread {
            let b = &bands[i + n];
            merged.first_ts_ns = merged.first_ts_ns.min(b.first_ts_ns);
            merged.last_ts_ns = merged.last_ts_ns.max(b.last_ts_ns);
            merged.busy_ns += b.busy_ns;
            merged.await_ns += b.await_ns;
            merged.errors += b.errors;
            if b.busy_ns > best_busy {
                best_busy = b.busy_ns;
                merged.dominant_function = b.dominant_function;
            }
            n += 1;
        }
        out.push(merged);
        i += n;
    }
    out
}
