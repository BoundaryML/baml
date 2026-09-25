//! The first index of a recording, built in memory.
//!
//! A recording with no indexed facts has nothing to reconcile against. Its
//! files merge into maps under the same rules the incremental `Applier`
//! applies with SQL: the first definition wins and a different one is a
//! conflict with an issue, references without definitions become
//! placeholders, and announcements, completions, raises and ends merge by
//! identity. Derived columns (execution roots, path depths, child time,
//! raise paths and sites) are computed once from the final evidence instead
//! of being repaired after every batch. Then every row is written once, in
//! key order.
use std::collections::BTreeMap;

use btel_reader::{
    evidence::{self, ArgumentNames},
    source_map::{SiteState, SourceMap},
    timing::EpochStatus,
};
use btel_recorder::proto::{self, function_definition::Resolution, span_event::Event};
use prost::Message as _;
use rusqlite::{Transaction, params};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    Totals, id, quantity, sequence_i64,
    sites::{self, Site, Source},
    snapshot_id,
};
use crate::Error;

/// Callers listed per raise path; with the raising frame, `MAX_ERROR_FRAMES`.
const MAX_PATH_CALLERS: usize = 63;

struct Issue {
    sequence: u64,
    code: &'static str,
    subject: Option<String>,
    detail: String,
}

#[derive(Default)]
struct FunctionRow {
    /// 0 referenced only, 1 lookup-unavailable observed, 2 metadata recorded.
    state: i64,
    conflict: bool,
    metadata: Option<Box<FunctionMetadata>>,
}

struct FunctionMetadata {
    wire: proto::FunctionMetadata,
    encoded: Vec<u8>,
    argument_names: Option<Vec<u8>>,
    map_blob: Option<Vec<u8>>,
    map_state: Option<i64>,
}

#[derive(Clone, Copy)]
struct PathDef {
    thread: u64,
    parent: u32,
    caller: Option<u64>,
    caller_pc: u32,
    callee: u64,
    edge: i32,
}

impl PathDef {
    fn from_wire(path: &proto::CallPathDefinition) -> Self {
        Self {
            thread: path.thread_id,
            parent: path.parent_call_path_id,
            caller: path.visible_caller_function_id,
            caller_pc: path.caller_pc,
            callee: path.callee_function_id,
            edge: path.edge,
        }
    }

    fn same(&self, other: &Self) -> bool {
        self.thread == other.thread
            && self.parent == other.parent
            && self.caller == other.caller
            && self.caller_pc == other.caller_pc
            && self.callee == other.callee
            && self.edge == other.edge
    }
}

/// Summed aggregate deltas of one node while every sum fits `u64`: half
/// the size of `Totals`, whose `u128` sums take over when one would not.
#[derive(Clone, Copy, Default)]
struct SmallTotals {
    count: u64,
    duration: u64,
    self_await: u64,
    errored: u64,
    cancelled: u64,
    evidence: u8,
    /// The node's sums are in `Bulk::large_aggregates` instead.
    large: bool,
}

impl SmallTotals {
    /// Add a delta; `false` (and unchanged) when a sum would not fit.
    fn add(&mut self, delta: &Totals) -> bool {
        let sum = |a: u64, b: u128| u64::try_from(b).ok().and_then(|b| a.checked_add(b));
        let (Some(count), Some(duration), Some(self_await), Some(errored), Some(cancelled)) = (
            sum(self.count, delta.count),
            sum(self.duration, delta.duration),
            sum(self.self_await, delta.self_await),
            sum(self.errored, delta.outcomes.errored),
            sum(self.cancelled, delta.outcomes.cancelled),
        ) else {
            return false;
        };
        let Ok(evidence) = u8::try_from(i64::from(self.evidence) | delta.outcomes.evidence) else {
            return false;
        };
        *self = Self {
            count,
            duration,
            self_await,
            errored,
            cancelled,
            evidence,
            large: false,
        };
        true
    }

    fn totals(self) -> Totals {
        Totals {
            count: u128::from(self.count),
            duration: u128::from(self.duration),
            self_await: u128::from(self.self_await),
            outcomes: crate::outcomes::Totals {
                evidence: i64::from(self.evidence),
                errored: u128::from(self.errored),
                cancelled: u128::from(self.cancelled),
            },
        }
    }
}

/// A column derived after every file is merged.
#[derive(Clone, Copy)]
enum Derived<T> {
    Pending,
    /// Computed; `None` when it cannot be resolved (NULL).
    Done(Option<T>),
}

impl<T> Derived<T> {
    fn value(self) -> Option<T> {
        match self {
            Self::Pending => None,
            Self::Done(value) => value,
        }
    }
}

// Flags packing a path row's optional fields. There can be millions of
// rows, so each is kept to 56 bytes.
const PATH_DEFINED: u8 = 1;
const PATH_CONFLICT: u8 = 1 << 1;
const PATH_HAS_CALLER: u8 = 1 << 2;
const PATH_TICKS_NULL: u8 = 1 << 3;
const PATH_DEPTH_DONE: u8 = 1 << 4;
const PATH_DEPTH_NULL: u8 = 1 << 5;

/// A call path: its first definition, or a placeholder for a referenced
/// path, and its derived depth and child time.
#[derive(Default)]
struct PathRow {
    thread: u64,
    caller: u64,
    callee: u64,
    /// Checked sum of synchronous children's normal-node totals.
    child_ticks: i64,
    depth: i64,
    parent: u32,
    caller_pc: u32,
    edge: i32,
    flags: u8,
}

impl PathRow {
    fn has(&self, flag: u8) -> bool {
        self.flags & flag != 0
    }

    fn set(&mut self, flag: u8, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    /// `None`: a placeholder.
    fn def(&self) -> Option<PathDef> {
        self.has(PATH_DEFINED).then(|| PathDef {
            thread: self.thread,
            parent: self.parent,
            caller: self.has(PATH_HAS_CALLER).then_some(self.caller),
            caller_pc: self.caller_pc,
            callee: self.callee,
            edge: self.edge,
        })
    }

    fn define(&mut self, def: &PathDef) {
        self.thread = def.thread;
        self.parent = def.parent;
        self.caller = def.caller.unwrap_or(0);
        self.set(PATH_HAS_CALLER, def.caller.is_some());
        self.caller_pc = def.caller_pc;
        self.callee = def.callee;
        self.edge = def.edge;
        self.set(PATH_DEFINED, true);
    }

    fn conflict(&self) -> bool {
        self.has(PATH_CONFLICT)
    }

    fn depth(&self) -> Derived<i64> {
        match (self.has(PATH_DEPTH_DONE), self.has(PATH_DEPTH_NULL)) {
            (false, _) => Derived::Pending,
            (true, true) => Derived::Done(None),
            (true, false) => Derived::Done(Some(self.depth)),
        }
    }

    fn set_depth(&mut self, depth: Option<i64>) {
        self.set(PATH_DEPTH_DONE, true);
        self.set(PATH_DEPTH_NULL, depth.is_none());
        self.depth = depth.unwrap_or(0);
    }

    fn child_ticks(&self) -> Option<i64> {
        (!self.has(PATH_TICKS_NULL)).then_some(self.child_ticks)
    }

    fn set_child_ticks(&mut self, ticks: Option<i64>) {
        self.set(PATH_TICKS_NULL, ticks.is_none());
        self.child_ticks = ticks.unwrap_or(0);
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct ThreadDef {
    parent: Option<u64>,
    spawn_path: u32,
    started: Option<i64>,
    epoch: u64,
}

const THREAD_DEFINED: u16 = 1;
const THREAD_HAS_PARENT: u16 = 1 << 1;
const THREAD_HAS_STARTED: u16 = 1 << 2;
const THREAD_ANNOUNCED: u16 = 1 << 3;
const THREAD_COMPLETED: u16 = 1 << 4;
const THREAD_HAS_COMPLETED_TICKS: u16 = 1 << 5;
const THREAD_COMPLETION_CONFLICT: u16 = 1 << 6;
const THREAD_CONFLICT: u16 = 1 << 7;
const THREAD_ROOT_DONE: u16 = 1 << 8;
const THREAD_HAS_ROOT: u16 = 1 << 9;

/// A thread: its first definition and first completion, or a placeholder,
/// and its derived execution root. Packed like `PathRow`.
#[derive(Default)]
struct ThreadRow {
    parent: u64,
    started: i64,
    epoch: u64,
    completed_ticks: i64,
    outcome: i64,
    root: u64,
    spawn_path: u32,
    flags: u16,
}

impl ThreadRow {
    fn has(&self, flag: u16) -> bool {
        self.flags & flag != 0
    }

    fn set(&mut self, flag: u16, on: bool) {
        if on {
            self.flags |= flag;
        } else {
            self.flags &= !flag;
        }
    }

    fn def(&self) -> Option<ThreadDef> {
        self.has(THREAD_DEFINED).then(|| ThreadDef {
            parent: self.has(THREAD_HAS_PARENT).then_some(self.parent),
            spawn_path: self.spawn_path,
            started: self.has(THREAD_HAS_STARTED).then_some(self.started),
            epoch: self.epoch,
        })
    }

    fn define(&mut self, def: ThreadDef) {
        self.parent = def.parent.unwrap_or(0);
        self.set(THREAD_HAS_PARENT, def.parent.is_some());
        self.spawn_path = def.spawn_path;
        self.started = def.started.unwrap_or(0);
        self.set(THREAD_HAS_STARTED, def.started.is_some());
        self.epoch = def.epoch;
        self.set(THREAD_DEFINED, true);
    }

    /// The first completion: its ticks and outcome.
    fn completion(&self) -> Option<(Option<i64>, i64)> {
        self.has(THREAD_COMPLETED).then(|| {
            (
                self.has(THREAD_HAS_COMPLETED_TICKS)
                    .then_some(self.completed_ticks),
                self.outcome,
            )
        })
    }

    fn complete(&mut self, ticks: Option<i64>, outcome: i64) {
        self.completed_ticks = ticks.unwrap_or(0);
        self.set(THREAD_HAS_COMPLETED_TICKS, ticks.is_some());
        self.outcome = outcome;
        self.set(THREAD_COMPLETED, true);
    }

    fn root(&self) -> Derived<u64> {
        match (self.has(THREAD_ROOT_DONE), self.has(THREAD_HAS_ROOT)) {
            (false, _) => Derived::Pending,
            (true, false) => Derived::Done(None),
            (true, true) => Derived::Done(Some(self.root)),
        }
    }

    fn set_root(&mut self, root: Option<u64>) {
        self.set(THREAD_ROOT_DONE, true);
        self.set(THREAD_HAS_ROOT, root.is_some());
        self.root = root.unwrap_or(0);
    }
}

struct EpochDef {
    domain: u64,
    source: i32,
    multiplier: Option<i64>,
    shift: i64,
    utc_ticks: Option<i64>,
    utc_unix_ns: Option<i64>,
    definition: Vec<u8>,
}

#[derive(Default)]
struct EpochRow {
    def: Option<EpochDef>,
    conflict: bool,
}

struct CallRow {
    thread: u64,
    parent: u64,
    path: u32,
    reentry: Option<bool>,
    announced_sequence: Option<i64>,
    entered: Option<i64>,
    inputs_cas: Option<[u8; 16]>,
    completed_sequence: Option<i64>,
    late: Option<bool>,
    exited: Option<i64>,
    self_await: Option<i64>,
    outcome: Option<i64>,
    needs_announcement: Option<bool>,
    value_cas: Option<[u8; 16]>,
    conflict: i64,
}

struct RaiseFields {
    sequence: i64,
    thread: u64,
    raised_ticks: Option<i64>,
    kind: i32,
    function: Option<u64>,
    pc: Option<u32>,
    origin_state: i32,
    origin_raise: Option<u64>,
    previous_raise: Option<u64>,
    origin_via: Option<i32>,
    origin_candidates: Option<u32>,
    unresolved_reason: Option<i32>,
    frame_count: u32,
    inherited_count: u32,
    inherited: Option<String>,
    evidence: Vec<u8>,
    call_path: Option<u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
struct EndFields {
    sequence: i64,
    result: i32,
    handler_function: Option<u64>,
    handler_pc: Option<u32>,
    unwound_frames: u32,
}

#[derive(Default)]
struct RaiseRow {
    /// `None`: an end whose raise is not indexed.
    raise: Option<RaiseFields>,
    end: Option<EndFields>,
    conflict: bool,
    end_conflict: bool,
}

struct FrameRow {
    function: Option<u64>,
    pc: Option<u32>,
    native: bool,
}

/// One recording's facts, merged in memory.
#[derive(Default)]
pub(super) struct Bulk {
    functions: FxHashMap<u64, FunctionRow>,
    paths: Table<u32, PathRow>,
    threads: Table<u64, ThreadRow>,
    epochs: FxHashMap<u64, EpochRow>,
    /// Most severe status and finality per epoch.
    epoch_states: FxHashMap<u64, (i64, bool)>,
    aggregates: Table<u64, SmallTotals>,
    /// Nodes whose sums outgrew `SmallTotals`, summed exactly.
    large_aggregates: FxHashMap<u64, Totals>,
    calls: Table<u64, CallRow>,
    raises: Table<u64, RaiseRow>,
    frames: BTreeMap<(u64, i64), FrameRow>,
    /// `(raise, call, role)` to the first link's sequence.
    links: BTreeMap<(u64, u64, i32), i64>,
    /// A call is unwound by at most one raise.
    unwound_by: FxHashMap<u64, u64>,
    raise_paths: FxHashSet<u32>,
    issues: Vec<Issue>,
}

/// Rows per storage chunk of a `Table`.
const TABLE_CHUNK: usize = 1 << 16;

/// Rows by key, kept in fixed-size chunks with a small index from key to
/// slot. A hash map would store the rows themselves in a power-of-two table
/// with up to twice the slots it needs; these rows are large and there can
/// be millions of them.
struct Table<K, V> {
    index: FxHashMap<K, u32>,
    chunks: Vec<Vec<(K, V)>>,
}

impl<K, V> Default for Table<K, V> {
    fn default() -> Self {
        Self {
            index: FxHashMap::default(),
            chunks: Vec::new(),
        }
    }
}

impl<K: Copy + Eq + std::hash::Hash, V> Table<K, V> {
    fn len(&self) -> usize {
        self.index.len()
    }

    fn slot(&self, at: u32) -> &(K, V) {
        let at = at as usize;
        &self.chunks[at / TABLE_CHUNK][at % TABLE_CHUNK]
    }

    fn slot_mut(&mut self, at: u32) -> &mut (K, V) {
        let at = at as usize;
        &mut self.chunks[at / TABLE_CHUNK][at % TABLE_CHUNK]
    }

    fn get(&self, key: &K) -> Option<&V> {
        self.index.get(key).map(|at| &self.slot(*at).1)
    }

    fn get_mut(&mut self, key: &K) -> Option<&mut V> {
        let at = *self.index.get(key)?;
        Some(&mut self.slot_mut(at).1)
    }

    fn contains_key(&self, key: &K) -> bool {
        self.index.contains_key(key)
    }

    /// Add a row for a key that has none.
    fn insert(&mut self, key: K, value: V) -> &mut V {
        debug_assert!(!self.index.contains_key(&key));
        if self
            .chunks
            .last()
            .is_none_or(|chunk| chunk.len() == TABLE_CHUNK)
        {
            self.chunks.push(Vec::new());
        }
        let at = u32::try_from(self.index.len()).expect("fewer than 2^32 rows");
        self.chunks.last_mut().expect("pushed").push((key, value));
        self.index.insert(key, at);
        &mut self.slot_mut(at).1
    }

    /// The key's row, a default one if it has none.
    fn row(&mut self, key: K) -> &mut V
    where
        V: Default,
    {
        match self.index.get(&key) {
            Some(&at) => &mut self.slot_mut(at).1,
            None => self.insert(key, V::default()),
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&K, &V)> {
        self.chunks
            .iter()
            .flatten()
            .map(|(key, value)| (key, value))
    }

    fn keys(&self) -> impl Iterator<Item = &K> {
        self.iter().map(|(key, _)| key)
    }
}

impl<K: Copy + Eq + std::hash::Hash, V> std::ops::Index<&K> for Table<K, V> {
    type Output = V;
    fn index(&self, key: &K) -> &V {
        self.get(key).expect("row exists")
    }
}

fn valid_enum(value: i32, max: i32) -> bool {
    (1..=max).contains(&value)
}

/// Record an issue while a map entry is still borrowed.
fn push_issue(
    issues: &mut Vec<Issue>,
    sequence: u64,
    code: &'static str,
    subject: Option<String>,
    detail: &str,
) {
    issues.push(Issue {
        sequence,
        code,
        subject,
        detail: detail.to_owned(),
    });
}

impl Bulk {
    /// A referenced path, thread, function or epoch: a placeholder row
    /// unless its definition arrives.
    fn refer_path(&mut self, path: u32) {
        self.paths.row(path);
    }
    fn refer_thread(&mut self, thread: u64) {
        self.threads.row(thread);
    }
    fn refer_function(&mut self, function: u64) {
        self.functions.entry(function).or_default();
    }
    fn refer_epoch(&mut self, epoch: u64) {
        self.epochs.entry(epoch).or_default();
    }

    fn issue(&mut self, sequence: u64, code: &'static str, subject: Option<String>, detail: &str) {
        push_issue(&mut self.issues, sequence, code, subject, detail);
    }

    fn invalid_error(&mut self, sequence: u64, what: &str) {
        self.issue(
            sequence,
            "error_evidence_invalid",
            None,
            &format!("a malformed {what} was skipped; other evidence in the file was applied"),
        );
    }

    /// Fold one file, like `Applier::apply`.
    pub(super) fn apply(&mut self, sequence: u64, file: &proto::RecordingFile) {
        let mut ticks_out_of_range = false;
        let mut tick = |value: u64| {
            let converted = quantity(value);
            ticks_out_of_range |= converted.is_none();
            converted
        };
        if let Some(definitions) = &file.definitions {
            for function in &definitions.functions {
                self.function(sequence, function);
            }
            for path in &definitions.call_paths {
                self.call_path(sequence, path);
            }
            for epoch in &definitions.clock_epochs {
                self.epoch(sequence, epoch);
            }
            for thread in &definitions.threads {
                self.thread(sequence, thread, tick(thread.started_at_ticks));
            }
        }
        if let Some(states) = &file.clock_states {
            for state in &states.states {
                self.refer_epoch(state.epoch_id);
                let Some(status) = EpochStatus::from_wire(state.status) else {
                    continue;
                };
                let entry = self
                    .epoch_states
                    .entry(state.epoch_id)
                    .or_insert((status.code(), state.r#final));
                entry.0 = entry.0.max(status.code());
                entry.1 |= state.r#final;
            }
        }
        if let Some(batch) = &file.aggregates {
            for delta in &batch.entries {
                self.refer_path(evidence::split_node(delta.node).0);
                let totals = Totals {
                    count: u128::from(delta.count),
                    duration: u128::from(delta.total_duration_ticks),
                    self_await: u128::from(delta.total_self_await_ticks),
                    outcomes: crate::outcomes::Totals::from_wire(
                        delta.count,
                        delta.outcomes.as_ref(),
                    ),
                };
                if totals.outcomes.evidence & crate::outcomes::INVALID != 0 {
                    self.issue(
                        sequence,
                        "aggregate_outcome_invalid",
                        Some(format!("n{}", delta.node)),
                        "error and cancellation counts exceed completed calls; population outcomes are unavailable",
                    );
                }
                let small = self.aggregates.row(delta.node);
                if !small.large && small.add(&totals) {
                    continue;
                }
                small.large = true;
                let small = *small;
                let exact = self
                    .large_aggregates
                    .entry(delta.node)
                    .or_insert_with(|| small.totals());
                *exact = exact.add(totals);
            }
        }
        if let Some(batch) = &file.errors {
            for raise in &batch.raises {
                self.raise(sequence, raise, &mut tick);
            }
            for link in &batch.call_links {
                self.link(sequence, link);
            }
            for end in &batch.unwind_ends {
                self.unwind_end(sequence, end);
            }
        }
        if let Some(spans) = &file.spans {
            for section in &spans.sections {
                self.refer_thread(section.thread_id);
                for event in &section.events {
                    match event.event.as_ref() {
                        Some(Event::ThreadAnnouncement(_)) => {
                            self.threads
                                .row(section.thread_id)
                                .set(THREAD_ANNOUNCED, true);
                        }
                        Some(Event::ThreadCompletion(done)) => {
                            let completed = tick(done.completed_at_ticks);
                            let outcome = i64::from(done.outcome);
                            let row = self.threads.row(section.thread_id);
                            match row.completion() {
                                None => row.complete(completed, outcome),
                                Some(first) if first == (completed, outcome) => {}
                                Some(_) => {
                                    row.set(THREAD_COMPLETION_CONFLICT, true);
                                    push_issue(
                                        &mut self.issues,
                                        sequence,
                                        "thread_completion_conflict",
                                        Some(section.thread_id.to_string()),
                                        "a thread completed twice with different evidence; the first is kept",
                                    );
                                }
                            }
                        }
                        Some(Event::FunctionAnnouncement(entry)) => {
                            self.refer_path(entry.call_path_id);
                            let announced = CallRow {
                                thread: section.thread_id,
                                parent: entry.parent_id,
                                path: entry.call_path_id,
                                reentry: None,
                                announced_sequence: Some(saturating_sequence(sequence)),
                                entered: tick(entry.entered_at_ticks),
                                inputs_cas: entry.inputs_cas_id.as_ref().map(snapshot_id),
                                completed_sequence: None,
                                late: None,
                                exited: None,
                                self_await: None,
                                outcome: None,
                                needs_announcement: None,
                                value_cas: None,
                                conflict: 0,
                            };
                            match self.calls.get_mut(&entry.id) {
                                None => {
                                    self.calls.insert(entry.id, announced);
                                }
                                Some(row) => {
                                    if row.conflict == 0
                                        && (row.announced_sequence.is_some()
                                            || row.thread != announced.thread
                                            || row.parent != announced.parent
                                            || row.path != announced.path
                                            || row.entered != announced.entered)
                                    {
                                        row.conflict = 1;
                                    }
                                    row.announced_sequence = announced.announced_sequence;
                                    row.inputs_cas = announced.inputs_cas;
                                }
                            }
                        }
                        Some(Event::FunctionCompletion(done)) => {
                            self.completion(sequence, section.thread_id, done, false, &mut tick);
                        }
                        Some(Event::LateFunctionCompletion(done)) => {
                            self.completion(sequence, section.thread_id, done, true, &mut tick);
                        }
                        None => {}
                    }
                }
            }
        }
        if ticks_out_of_range {
            self.issue(
                sequence,
                "tick_out_of_range",
                None,
                "a clock tick exceeds the index's signed 64-bit range; affected timings are unavailable",
            );
        }
    }

    fn completion(
        &mut self,
        sequence: u64,
        thread: u64,
        done: &proto::FunctionCompletion,
        late: bool,
        tick: &mut impl FnMut(u64) -> Option<i64>,
    ) {
        let (outcome, needs_announcement) =
            evidence::completion(done, late).expect("validated completion flags");
        let (path, reentry) = evidence::split_node(done.node);
        self.refer_path(path);
        let entered = tick(done.entered_at_ticks);
        let exited = tick(done.exited_at_ticks);
        let self_await = tick(done.self_await_ticks);
        let value_cas = done.value_cas_id.as_ref().map(snapshot_id);
        let completed_sequence = Some(saturating_sequence(sequence));
        match self.calls.get_mut(&done.id) {
            None => {
                self.calls.insert(
                    done.id,
                    CallRow {
                        thread,
                        parent: done.parent_id,
                        path,
                        reentry: Some(reentry),
                        announced_sequence: None,
                        entered,
                        inputs_cas: None,
                        completed_sequence,
                        late: Some(late),
                        exited,
                        self_await,
                        outcome: Some(outcome.code()),
                        needs_announcement: Some(needs_announcement),
                        value_cas,
                        conflict: 0,
                    },
                );
            }
            Some(row) => {
                if row.conflict == 0
                    && (row.completed_sequence.is_some()
                        || row.thread != thread
                        || row.parent != done.parent_id
                        || row.path != path
                        || (row.entered.is_some() && row.entered != entered))
                {
                    row.conflict = 1;
                }
                row.reentry = Some(reentry);
                row.completed_sequence = completed_sequence;
                row.late = Some(late);
                row.entered = row.entered.or(entered);
                row.exited = exited;
                row.self_await = self_await;
                row.outcome = Some(outcome.code());
                row.needs_announcement = Some(needs_announcement);
                row.value_cas = value_cas;
            }
        }
    }

    fn function(&mut self, sequence: u64, function: &proto::FunctionDefinition) {
        match function.resolution.as_ref() {
            Some(Resolution::Metadata(metadata)) => {
                let encoded = metadata.encode_to_vec();
                if let Some(row) = self.functions.get_mut(&function.function_id)
                    && let Some(previous) = &row.metadata
                {
                    if previous.encoded != encoded {
                        row.conflict = true;
                        push_issue(
                            &mut self.issues,
                            sequence,
                            "function_metadata_conflict",
                            Some(format!("f{}", function.function_id)),
                            "two different metadata definitions for one function; the first is kept",
                        );
                    }
                    return;
                }
                let span = metadata.source_span.as_ref();
                let map = metadata
                    .source_map
                    .as_ref()
                    .map(|map| SourceMap::from_wire(map, span.map(|s| s.file_id)));
                if let Some(Err(reason)) = &map {
                    self.issue(
                        sequence,
                        "source_map_invalid",
                        Some(format!("f{}", function.function_id)),
                        &format!(
                            "the recorded source map is malformed ({reason:?}); sites in this function are unavailable"
                        ),
                    );
                }
                let (map_blob, map_state) = match map {
                    None => (None, None),
                    Some(Ok(map)) => (Some(map.encode()), Some(1)),
                    Some(Err(_)) => (None, Some(2)),
                };
                let row = self.functions.entry(function.function_id).or_default();
                row.state = 2;
                row.metadata = Some(Box::new(FunctionMetadata {
                    argument_names: metadata
                        .argument_layout
                        .as_ref()
                        .map(|layout| ArgumentNames::from_wire(layout).encode()),
                    wire: metadata.clone(),
                    encoded,
                    map_blob,
                    map_state,
                }));
            }
            // An observation that lookup failed; never erases known metadata.
            _ => {
                let row = self.functions.entry(function.function_id).or_default();
                row.state = row.state.max(1);
            }
        }
    }

    fn call_path(&mut self, sequence: u64, path: &proto::CallPathDefinition) {
        self.refer_thread(path.thread_id);
        self.refer_function(path.callee_function_id);
        if let Some(caller) = path.visible_caller_function_id {
            self.refer_function(caller);
        }
        if path.parent_call_path_id != 0 {
            self.refer_path(path.parent_call_path_id);
        }
        let def = PathDef::from_wire(path);
        let row = self.paths.row(path.call_path_id);
        match row.def() {
            None => row.define(&def),
            Some(first) if first.same(&def) => {}
            Some(_) => {
                row.set(PATH_CONFLICT, true);
                push_issue(
                    &mut self.issues,
                    sequence,
                    "call_path_conflict",
                    Some(format!("p{}", path.call_path_id)),
                    "two different definitions for one call path; the first is kept",
                );
            }
        }
    }

    fn thread(&mut self, sequence: u64, thread: &proto::ThreadDefinition, started: Option<i64>) {
        self.refer_epoch(thread.clock_epoch_id);
        if thread.spawn_call_path_id != 0 {
            self.refer_path(thread.spawn_call_path_id);
        }
        let def = ThreadDef {
            parent: thread.parent_id,
            spawn_path: thread.spawn_call_path_id,
            started,
            epoch: thread.clock_epoch_id,
        };
        let row = self.threads.row(thread.thread_id);
        match row.def() {
            None => row.define(def),
            Some(first) if first == def => {}
            Some(_) => {
                row.set(THREAD_CONFLICT, true);
                push_issue(
                    &mut self.issues,
                    sequence,
                    "thread_definition_conflict",
                    Some(thread.thread_id.to_string()),
                    "two different definitions for one thread; the first is kept",
                );
            }
        }
    }

    fn epoch(&mut self, sequence: u64, epoch: &proto::ClockEpochDefinition) {
        let definition = epoch.encode_to_vec();
        let row = self.epochs.entry(epoch.epoch_id).or_default();
        match &row.def {
            Some(first) if first.definition == definition => {}
            Some(_) => {
                row.conflict = true;
                push_issue(
                    &mut self.issues,
                    sequence,
                    "clock_epoch_conflict",
                    Some(format!("c{}", epoch.epoch_id)),
                    "two different conversions for one clock epoch; timings using it are unavailable",
                );
            }
            None => {
                let utc = epoch
                    .utc
                    .as_ref()
                    .and_then(btel_reader::timing::UtcAnchor::from_wire);
                row.def = Some(EpochDef {
                    domain: epoch.domain_id,
                    source: epoch.source,
                    multiplier: quantity(epoch.multiplier),
                    shift: i64::from(epoch.shift),
                    utc_ticks: utc.and_then(|u| quantity(u.ticks)),
                    utc_unix_ns: utc.and_then(|u| i64::try_from(u.unix_ns).ok()),
                    definition,
                });
            }
        }
    }

    fn raise(
        &mut self,
        sequence: u64,
        raise: &proto::ErrorRaise,
        tick: &mut impl FnMut(u64) -> Option<i64>,
    ) {
        let origin_ok = match raise.origin_state {
            1 => raise.origin_raise_id.is_none(),
            2 => raise.origin_raise_id.is_some_and(|id| id != 0),
            3 | 4 => raise.origin_raise_id.is_none(),
            _ => false,
        };
        if raise.raise_id == 0
            || raise.thread_id == 0
            || !valid_enum(raise.kind, 8)
            || !origin_ok
            || raise.function_id == Some(0)
            || raise.frames.len() > usize::try_from(raise.frame_count).unwrap_or(usize::MAX)
            || raise.call_path_id == Some(0)
            || (raise.call_path_id.is_some() && !raise.frames.is_empty())
        {
            return self.invalid_error(sequence, "error raise");
        }
        let encoded = raise.encode_to_vec();
        if let Some(existing) = self.raises.get_mut(&raise.raise_id)
            && let Some(first) = &existing.raise
        {
            if first.evidence != encoded {
                existing.conflict = true;
                push_issue(
                    &mut self.issues,
                    sequence,
                    "error_raise_conflict",
                    Some(raise.raise_id.to_string()),
                    "two different records for one raise; the first is kept",
                );
            }
            return;
        }
        self.refer_thread(raise.thread_id);
        if let Some(function) = raise.function_id {
            self.refer_function(function);
        }
        if let Some(path) = raise.call_path_id {
            self.refer_path(path);
            self.raise_paths.insert(path);
        }
        let inherited = (!raise.inherited_frames.is_empty()).then(|| {
            serde_json::Value::Array(
                raise
                    .inherited_frames
                    .iter()
                    .map(|frame| {
                        serde_json::json!({
                            "function": frame.function_name,
                            "file": frame.file,
                            "line": (frame.line != 0).then_some(frame.line),
                        })
                    })
                    .collect(),
            )
            .to_string()
        });
        let fields = RaiseFields {
            sequence: saturating_sequence(sequence),
            thread: raise.thread_id,
            raised_ticks: tick(raise.raised_at_ticks),
            kind: raise.kind,
            function: raise.function_id,
            pc: raise.pc,
            origin_state: raise.origin_state,
            origin_raise: raise.origin_raise_id,
            previous_raise: raise.previous_raise_id.filter(|id| *id != 0),
            origin_via: (raise.origin_via != 0).then_some(raise.origin_via),
            origin_candidates: (raise.origin_state == 3).then_some(raise.origin_candidates),
            unresolved_reason: (raise.origin_state == 4).then_some(raise.unresolved_reason),
            frame_count: raise.frame_count,
            inherited_count: raise.inherited_frame_count,
            inherited,
            evidence: encoded,
            call_path: raise.call_path_id,
        };
        self.raises.row(raise.raise_id).raise = Some(fields);
        for (position, entry) in raise.frames.iter().enumerate() {
            let function = entry.function_id.filter(|id| *id != 0);
            if let Some(function) = function {
                self.refer_function(function);
            }
            self.frames.insert(
                (raise.raise_id, i64::try_from(position).unwrap_or(i64::MAX)),
                FrameRow {
                    function,
                    pc: entry.pc,
                    native: entry.native,
                },
            );
        }
    }

    fn link(&mut self, sequence: u64, link: &proto::ErrorCallLink) {
        if link.raise_id == 0 || link.call_id == 0 || !valid_enum(link.role, 2) {
            return self.invalid_error(sequence, "error call link");
        }
        let key = (link.raise_id, link.call_id, link.role);
        if link.role == 2 {
            // The unique unwound link and the key are ignored alike; only a
            // different owner is a conflict.
            if let Some(owner) = self.unwound_by.get(&link.call_id) {
                if *owner != link.raise_id {
                    self.issue(
                        sequence,
                        "error_link_conflict",
                        Some(link.call_id.to_string()),
                        "two raises claim to have failed one call; the first is kept",
                    );
                }
                return;
            }
            self.unwound_by.insert(link.call_id, link.raise_id);
        }
        self.links
            .entry(key)
            .or_insert_with(|| saturating_sequence(sequence));
    }

    fn unwind_end(&mut self, sequence: u64, end: &proto::ErrorUnwindEnd) {
        let caught = end.result == 1;
        if end.raise_id == 0
            || !valid_enum(end.result, 4)
            || end.handler_function_id == Some(0)
            || caught != end.handler_pc.is_some()
        {
            return self.invalid_error(sequence, "unwind end");
        }
        if let Some(function) = end.handler_function_id {
            self.refer_function(function);
        }
        let fields = EndFields {
            sequence: saturating_sequence(sequence),
            result: end.result,
            handler_function: end.handler_function_id,
            handler_pc: end.handler_pc,
            unwound_frames: end.unwound_frames,
        };
        let row = self.raises.row(end.raise_id);
        match row.end {
            None => row.end = Some(fields),
            Some(first)
                if first.result == fields.result
                    && first.handler_function == fields.handler_function
                    && first.handler_pc == fields.handler_pc
                    && first.unwound_frames == fields.unwound_frames => {}
            Some(_) => {
                row.end_conflict = true;
                push_issue(
                    &mut self.issues,
                    sequence,
                    "error_end_conflict",
                    Some(end.raise_id.to_string()),
                    "a raise ended twice with different evidence; the first is kept",
                );
            }
        }
    }

    /// Derive and write every row of recording `rec`, which has no facts.
    pub(super) fn write(mut self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        self.report_call_conflicts();
        self.resolve_roots();
        self.resolve_depths();
        self.sum_child_ticks();
        let mut sites = Sites::new(&self.functions);
        self.write_functions(tx, rec)?;
        with_indexes_deferred(tx, "call_path", self.paths.len(), || {
            self.write_paths(tx, rec, &mut sites)
        })?;
        with_indexes_deferred(tx, "thread", self.threads.len(), || {
            self.write_threads(tx, rec)
        })?;
        self.write_clocks(tx, rec)?;
        self.write_aggregates(tx, rec)?;
        with_indexes_deferred(tx, "call", self.calls.len(), || self.write_calls(tx, rec))?;
        self.write_errors(tx, rec, &mut sites)?;
        let mut insert = tx.prepare_cached(
            "INSERT INTO issue (rec, sequence, code, subject, detail) VALUES (?1, ?2, ?3, ?4, ?5)",
        )?;
        for issue in &self.issues {
            insert.execute(params![
                rec,
                sequence_i64(issue.sequence)?,
                issue.code,
                issue.subject,
                issue.detail
            ])?;
        }
        Ok(())
    }

    /// Each conflicted call is reported once, like the incremental path.
    fn report_call_conflicts(&mut self) {
        let mut conflicted: Vec<u64> = self
            .calls
            .iter()
            .filter(|(_, call)| call.conflict == 1)
            .map(|(id, _)| *id)
            .collect();
        conflicted.sort_unstable();
        for call_id in conflicted {
            let call = self.calls.get_mut(&call_id).expect("collected above");
            call.conflict = 2;
            let sequence = call.completed_sequence.or(call.announced_sequence);
            self.issues.push(Issue {
                sequence: sequence.map_or(0, |s| u64::try_from(s).unwrap_or(0)),
                code: "call_evidence_conflict",
                subject: Some(call_id.to_string()),
                detail: "evidence for one call disagrees (announcement vs completion, or two completions); its status is conflicted".to_owned(),
            });
        }
    }

    /// Execution roots: a defined thread without a parent is a root; a
    /// spawned thread belongs to its parent thread's execution, or to the
    /// execution of the thread running its parent call. Cycles and missing
    /// parents stay unresolved.
    fn resolve_roots(&mut self) {
        let starts: Vec<u64> = self.threads.keys().copied().collect();
        let mut chain = Vec::new();
        let mut on_chain = FxHashSet::default();
        for start in starts {
            let mut current = start;
            let resolved = loop {
                let Some(row) = self.threads.get(&current) else {
                    break None;
                };
                if let Derived::Done(known) = row.root() {
                    break known;
                }
                if !on_chain.insert(current) {
                    break None;
                }
                chain.push(current);
                let Some(def) = row.def() else {
                    break None;
                };
                let Some(parent) = def.parent else {
                    break Some(current);
                };
                if self.threads.contains_key(&parent) {
                    current = parent;
                } else if let Some(call) = self.calls.get(&parent) {
                    current = call.thread;
                } else {
                    break None;
                }
            };
            on_chain.clear();
            for thread in chain.drain(..) {
                self.threads
                    .get_mut(&thread)
                    .expect("walked")
                    .set_root(resolved);
            }
        }
    }

    /// Distance from a top-level path, for defined paths whose every
    /// ancestor is defined. Cycles stay unresolved.
    fn resolve_depths(&mut self) {
        let starts: Vec<u32> = self.paths.keys().copied().collect();
        let mut chain = Vec::new();
        let mut on_chain = FxHashSet::default();
        for start in starts {
            let mut current = start;
            let base = loop {
                let Some(row) = self.paths.get(&current) else {
                    break None;
                };
                if let Derived::Done(known) = row.depth() {
                    break known;
                }
                let Some(def) = row.def() else {
                    break None;
                };
                if !on_chain.insert(current) {
                    // A cycle: nothing on it has a top-level ancestor.
                    break None;
                }
                chain.push(current);
                if def.parent == 0 {
                    break Some(-1);
                }
                current = def.parent;
            };
            on_chain.clear();
            for (distance, path) in chain.drain(..).rev().enumerate() {
                let depth = base.map(|base| base + 1 + i64::try_from(distance).unwrap_or(i64::MAX));
                self.paths.get_mut(&path).expect("walked").set_depth(depth);
            }
        }
    }

    /// Checked sums of synchronous children's normal-node duration totals.
    fn sum_child_ticks(&mut self) {
        let contributions: Vec<(u32, Option<i64>)> = self
            .paths
            .iter()
            .filter_map(|(child, row)| {
                let def = row.def()?;
                if def.edge != 1 || def.parent == 0 {
                    return None;
                }
                let ticks = match self.totals(u64::from(*child) * 2) {
                    None => Some(0),
                    Some(totals) => i64::try_from(totals.duration).ok(),
                };
                Some((def.parent, ticks))
            })
            .collect();
        for (parent, ticks) in contributions {
            if let Some(row) = self.paths.get_mut(&parent) {
                let sum = row
                    .child_ticks()
                    .zip(ticks)
                    .and_then(|(a, b)| a.checked_add(b));
                row.set_child_ticks(sum);
            }
        }
    }

    fn write_functions(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut ids: Vec<u64> = self.functions.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "function_def (rec, function_id, state, fqn, display_name, definition_key, kind,
               kind_detail, origin, source_file, source_file_id, source_start, source_end,
               package, namespace, namespace_json, owner_type_key, parent_function_key,
               lambda_path, argument_names, parameter_count, metadata, source_map,
               source_map_state, conflict)",
            25,
            &ids,
            |b, function_id| {
                let row = &self.functions[&function_id];
                b.bind(rec)?;
                b.bind(id(function_id))?;
                b.bind(row.state)?;
                let Some(meta) = &row.metadata else {
                    b.nulls(21)?;
                    return b.bind(row.conflict);
                };
                let metadata = &meta.wire;
                let span = metadata.source_span.as_ref();
                let layout = metadata.argument_layout.as_ref();
                b.bind(&metadata.fqn)?;
                b.bind(&metadata.display_name)?;
                b.bind(&metadata.definition_key)?;
                b.bind(evidence::function_kind_label(metadata.kind))?;
                b.bind(&metadata.sys_op_name)?;
                b.bind(evidence::function_origin_label(metadata.origin))?;
                b.bind(&metadata.source_file)?;
                b.bind(span.map(|s| s.file_id))?;
                b.bind(span.map(|s| s.start))?;
                b.bind(span.map(|s| s.end))?;
                b.bind(&metadata.package_name)?;
                b.bind((!metadata.namespace.is_empty()).then(|| metadata.namespace.join(".")))?;
                b.bind((!metadata.namespace.is_empty()).then(|| {
                    serde_json::to_string(&metadata.namespace).expect("strings serialize")
                }))?;
                b.bind(&metadata.owner_type_key)?;
                b.bind(&metadata.parent_function_key)?;
                b.bind(&metadata.lambda_path)?;
                b.bind(&meta.argument_names)?;
                b.bind(layout.map(|l| i64::try_from(l.slots.len()).unwrap_or(i64::MAX)))?;
                b.bind(&meta.encoded)?;
                b.bind(&meta.map_blob)?;
                b.bind(meta.map_state)?;
                b.bind(row.conflict)
            },
        )?;
        let mut params: Vec<(u64, usize)> = Vec::new();
        for function_id in &ids {
            if let Some(layout) = self.functions[function_id]
                .metadata
                .as_ref()
                .and_then(|meta| meta.wire.argument_layout.as_ref())
            {
                params.extend((0..layout.slots.len()).map(|position| (*function_id, position)));
            }
        }
        insert_rows(
            tx,
            "function_param (rec, function_id, position, name, receiver)",
            5,
            &params,
            |b, (function_id, position)| {
                let layout = self.functions[&function_id]
                    .metadata
                    .as_ref()
                    .and_then(|meta| meta.wire.argument_layout.as_ref())
                    .expect("collected above");
                let slot = &layout.slots[position];
                b.bind(rec)?;
                b.bind(id(function_id))?;
                b.bind(i64::try_from(position).unwrap_or(i64::MAX))?;
                b.bind(&slot.name)?;
                b.bind(slot.receiver)
            },
        )
    }

    fn write_paths(
        &self,
        tx: &Transaction<'_>,
        rec: i64,
        sites: &mut Sites<'_>,
    ) -> Result<(), Error> {
        let mut ids: Vec<u32> = self.paths.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "call_path (rec, call_path_id, defined, thread_id, parent_call_path_id,
               caller_function_id, caller_pc, callee_function_id, edge, depth,
               direct_child_ticks, site_state, site_file, site_line, site_start, site_end,
               conflict)",
            17,
            &ids,
            |b, path| {
                let row = &self.paths[&path];
                b.bind(rec)?;
                b.bind(i64::from(path))?;
                let def = row.def();
                match def {
                    None => {
                        b.bind(0)?;
                        b.nulls(7)?;
                    }
                    Some(def) => {
                        b.bind(1)?;
                        b.bind(id(def.thread))?;
                        b.bind(i64::from(def.parent))?;
                        b.bind(def.caller.map(id))?;
                        b.bind(i64::from(def.caller_pc))?;
                        b.bind(id(def.callee))?;
                        b.bind(def.edge)?;
                        b.bind(row.depth().value())?;
                    }
                }
                b.bind(row.child_ticks())?;
                match def {
                    None => b.nulls(5)?,
                    Some(def) => {
                        let site = sites.site(
                            def.caller,
                            Some(i64::from(def.caller_pc)),
                            SiteState::NoCaller,
                        );
                        b.site(sites.get(site))?;
                    }
                }
                b.bind(row.conflict())
            },
        )
    }

    fn write_threads(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut ids: Vec<u64> = self.threads.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "thread (rec, thread_id, defined, parent_id, spawn_call_path_id, started_ticks,
               epoch_id, announced, completed_ticks, outcome, completion_conflict, root_id,
               conflict)",
            13,
            &ids,
            |b, thread| {
                let row = &self.threads[&thread];
                let def = row.def();
                let completion = row.completion();
                b.bind(rec)?;
                b.bind(id(thread))?;
                b.bind(def.is_some())?;
                b.bind(def.and_then(|d| d.parent).map(id))?;
                b.bind(def.map(|d| i64::from(d.spawn_path)))?;
                b.bind(def.and_then(|d| d.started))?;
                b.bind(def.map(|d| id(d.epoch)))?;
                b.bind(row.has(THREAD_ANNOUNCED))?;
                b.bind(completion.and_then(|(ticks, _)| ticks))?;
                b.bind(completion.map(|(_, outcome)| outcome))?;
                b.bind(row.has(THREAD_COMPLETION_CONFLICT))?;
                b.bind(row.root().value().map(id))?;
                b.bind(row.has(THREAD_CONFLICT))
            },
        )
    }

    fn write_clocks(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut ids: Vec<u64> = self.epochs.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "epoch (rec, epoch_id, defined, domain_id, source, multiplier, shift, utc_ticks,
               utc_unix_ns, definition, conflict)",
            11,
            &ids,
            |b, epoch| {
                let row = &self.epochs[&epoch];
                let def = row.def.as_ref();
                b.bind(rec)?;
                b.bind(id(epoch))?;
                b.bind(def.is_some())?;
                b.bind(def.map(|d| id(d.domain)))?;
                b.bind(def.map(|d| d.source))?;
                b.bind(def.and_then(|d| d.multiplier))?;
                b.bind(def.map(|d| d.shift))?;
                b.bind(def.and_then(|d| d.utc_ticks))?;
                b.bind(def.and_then(|d| d.utc_unix_ns))?;
                b.bind(def.map(|d| d.definition.as_slice()))?;
                b.bind(row.conflict)
            },
        )?;
        let mut ids: Vec<u64> = self.epoch_states.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "epoch_state (rec, epoch_id, status, final)",
            4,
            &ids,
            |b, epoch| {
                let (status, r#final) = self.epoch_states[&epoch];
                b.bind(rec)?;
                b.bind(id(epoch))?;
                b.bind(status)?;
                b.bind(r#final)
            },
        )
    }

    /// A node's exact summed totals.
    fn totals(&self, node: u64) -> Option<Totals> {
        let small = self.aggregates.get(&node)?;
        Some(if small.large {
            self.large_aggregates[&node]
        } else {
            small.totals()
        })
    }

    fn write_aggregates(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut nodes: Vec<u64> = self.aggregates.keys().copied().collect();
        nodes.sort_unstable();
        let fits = |v: u128| i64::try_from(v).ok();
        insert_rows(
            tx,
            "aggregate (rec, node, call_count, duration_ticks, self_await_ticks, count_exact,
               duration_exact, self_await_exact, outcome_evidence, ok_calls, errored_calls,
               cancelled_calls, errored_exact, cancelled_exact)",
            14,
            &nodes,
            |b, node| {
                let total = self.totals(node).expect("collected above");
                let [ok_calls, errored_calls, cancelled_calls] = total.outcomes.counts(total.count);
                b.bind(rec)?;
                b.bind(i64::try_from(node).expect("validated 33-bit node"))?;
                b.bind(fits(total.count))?;
                b.bind(fits(total.duration))?;
                b.bind(fits(total.self_await))?;
                b.bind(total.count.to_string())?;
                b.bind(total.duration.to_string())?;
                b.bind(total.self_await.to_string())?;
                b.bind(total.outcomes.evidence)?;
                b.bind(ok_calls)?;
                b.bind(errored_calls)?;
                b.bind(cancelled_calls)?;
                b.bind(total.outcomes.errored.to_string())?;
                b.bind(total.outcomes.cancelled.to_string())
            },
        )
    }

    fn write_calls(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut ids: Vec<u64> = self.calls.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "call (rec, call_id, thread_id, parent_id, call_path_id, reentry,
               announced_sequence, entered_ticks, inputs_cas, completed_sequence, late,
               exited_ticks, self_await_ticks, outcome, needs_announcement, value_cas, conflict)",
            17,
            &ids,
            |b, call| {
                let row = &self.calls[&call];
                b.bind(rec)?;
                b.bind(id(call))?;
                b.bind(id(row.thread))?;
                b.bind(id(row.parent))?;
                b.bind(i64::from(row.path))?;
                b.bind(row.reentry)?;
                b.bind(row.announced_sequence)?;
                b.bind(row.entered)?;
                b.bind(row.inputs_cas.as_ref().map(<[u8; 16]>::as_slice))?;
                b.bind(row.completed_sequence)?;
                b.bind(row.late)?;
                b.bind(row.exited)?;
                b.bind(row.self_await)?;
                b.bind(row.outcome)?;
                b.bind(row.needs_announcement)?;
                b.bind(row.value_cas.as_ref().map(<[u8; 16]>::as_slice))?;
                b.bind(row.conflict)
            },
        )
    }

    fn write_errors(
        &self,
        tx: &Transaction<'_>,
        rec: i64,
        sites: &mut Sites<'_>,
    ) -> Result<(), Error> {
        let mut ids: Vec<u64> = self.raises.keys().copied().collect();
        ids.sort_unstable();
        let resolved: Vec<(Option<usize>, Option<usize>)> = ids
            .iter()
            .map(|raise_id| {
                let row = &self.raises[raise_id];
                let site = row.raise.as_ref().map(|raise| {
                    let absent = if raise.pc.is_some() {
                        SiteState::NoFunction
                    } else {
                        SiteState::NoPc
                    };
                    sites.site(raise.function, raise.pc.map(i64::from), absent)
                });
                let handler = row.end.and_then(|end| {
                    (end.handler_function.is_some() || end.handler_pc.is_some()).then(|| {
                        sites.site(
                            end.handler_function,
                            end.handler_pc.map(i64::from),
                            SiteState::NoFunction,
                        )
                    })
                });
                (site, handler)
            })
            .collect();
        let rows: Vec<usize> = (0..ids.len()).collect();
        insert_rows(
            tx,
            "error_raise (rec, raise_id, defined, sequence, thread_id, raised_ticks, kind,
               function_id, pc, origin_state, origin_raise_id, previous_raise_id, origin_via,
               origin_candidates, unresolved_reason, frame_count, call_path_id,
               inherited_count, inherited, evidence, site_state, site_file, site_line,
               site_start, site_end, end_sequence, end_result, handler_function_id,
               handler_pc, unwound_frames, handler_site_state, handler_site_file,
               handler_site_line, handler_site_start, handler_site_end, conflict,
               end_conflict)",
            37,
            &rows,
            |b, index| {
                let raise_id = ids[index];
                let row = &self.raises[&raise_id];
                let raise = row.raise.as_ref();
                let (site, handler) = resolved[index];
                b.bind(rec)?;
                b.bind(id(raise_id))?;
                b.bind(raise.is_some())?;
                b.bind(raise.map(|r| r.sequence))?;
                b.bind(raise.map(|r| id(r.thread)))?;
                b.bind(raise.and_then(|r| r.raised_ticks))?;
                b.bind(raise.map(|r| r.kind))?;
                b.bind(raise.and_then(|r| r.function).map(id))?;
                b.bind(raise.and_then(|r| r.pc))?;
                b.bind(raise.map(|r| r.origin_state))?;
                b.bind(raise.and_then(|r| r.origin_raise).map(id))?;
                b.bind(raise.and_then(|r| r.previous_raise).map(id))?;
                b.bind(raise.and_then(|r| r.origin_via))?;
                b.bind(raise.and_then(|r| r.origin_candidates))?;
                b.bind(raise.and_then(|r| r.unresolved_reason))?;
                b.bind(raise.map(|r| r.frame_count))?;
                b.bind(raise.and_then(|r| r.call_path).map(i64::from))?;
                b.bind(raise.map(|r| r.inherited_count))?;
                b.bind(raise.and_then(|r| r.inherited.as_deref()))?;
                b.bind(raise.map(|r| r.evidence.as_slice()))?;
                match site {
                    None => b.nulls(5)?,
                    Some(site) => b.site(sites.get(site))?,
                }
                let end = row.end;
                b.bind(end.map(|e| e.sequence))?;
                b.bind(end.map(|e| e.result))?;
                b.bind(end.and_then(|e| e.handler_function).map(id))?;
                b.bind(end.and_then(|e| e.handler_pc))?;
                b.bind(end.map(|e| e.unwound_frames))?;
                match handler {
                    None => b.nulls(5)?,
                    Some(site) => b.site(sites.get(site))?,
                }
                b.bind(row.conflict)?;
                b.bind(row.end_conflict)
            },
        )?;
        let frames: Vec<(&(u64, i64), &FrameRow)> = self.frames.iter().collect();
        let frame_sites: Vec<usize> = frames
            .iter()
            .map(|(_, frame)| {
                if frame.native {
                    // Native code has no BAML source; its caller frame has the site.
                    sites.site(None, None, SiteState::NoPc)
                } else {
                    sites.site(
                        frame.function,
                        frame.pc.map(i64::from),
                        SiteState::NoFunction,
                    )
                }
            })
            .collect();
        let rows: Vec<usize> = (0..frames.len()).collect();
        insert_rows(
            tx,
            "error_frame (rec, raise_id, position, function_id, pc, native, site_state,
               site_file, site_line, site_start, site_end)",
            11,
            &rows,
            |b, index| {
                let ((raise, position), frame) = frames[index];
                b.bind(rec)?;
                b.bind(id(*raise))?;
                b.bind(*position)?;
                b.bind(frame.function.map(id))?;
                b.bind(frame.pc)?;
                b.bind(frame.native)?;
                b.site(sites.get(frame_sites[index]))
            },
        )?;
        let links: Vec<(&(u64, u64, i32), &i64)> = self.links.iter().collect();
        let rows: Vec<usize> = (0..links.len()).collect();
        insert_rows(
            tx,
            "error_link (rec, raise_id, call_id, role, sequence)",
            5,
            &rows,
            |b, index| {
                let ((raise, call, role), sequence) = links[index];
                b.bind(rec)?;
                b.bind(id(*raise))?;
                b.bind(id(*call))?;
                b.bind(*role)?;
                b.bind(*sequence)
            },
        )?;
        self.write_raise_paths(tx, rec)
    }

    /// The callers of each raise path, when every path up to the thread's
    /// first frame is defined, like `errors::build_raise_paths`.
    fn write_raise_paths(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let mut paths: Vec<u32> = self.raise_paths.iter().copied().collect();
        paths.sort_unstable();
        let mut state = tx.prepare_cached(
            "INSERT INTO raise_path (rec, call_path_id, built) VALUES (?1, ?2, ?3)",
        )?;
        let mut frame = tx.prepare_cached(
            "INSERT INTO raise_path_frame (rec, call_path_id, position, frame_path_id)
             VALUES (?1, ?2, ?3, ?4)",
        )?;
        for path in paths {
            let mut frames = Vec::new();
            let mut current = path;
            let complete = loop {
                let Some(def) = self.paths.get(&current).and_then(PathRow::def) else {
                    break false;
                };
                if def.caller.is_none() || def.edge != 1 {
                    break true;
                }
                frames.push(current);
                if def.parent != 0 && frames.len() < MAX_PATH_CALLERS {
                    current = def.parent;
                } else {
                    break true;
                }
            };
            state.execute(params![rec, i64::from(path), complete])?;
            if complete {
                for (index, callers) in frames.iter().enumerate() {
                    frame.execute(params![
                        rec,
                        i64::from(path),
                        i64::try_from(index + 1).unwrap_or(i64::MAX),
                        i64::from(*callers)
                    ])?;
                }
            }
        }
        Ok(())
    }
}

/// Rows per multi-row `INSERT`: one statement step per this many rows.
const ROWS_PER_INSERT: usize = 64;

/// Binds one row's values in order, continuing across rows of a multi-row
/// statement.
struct Binder<'s, 't> {
    statement: &'s mut rusqlite::Statement<'t>,
    index: usize,
}

impl Binder<'_, '_> {
    fn bind<T: rusqlite::ToSql>(&mut self, value: T) -> rusqlite::Result<()> {
        self.index += 1;
        self.statement.raw_bind_parameter(self.index, value)
    }

    fn nulls(&mut self, count: usize) -> rusqlite::Result<()> {
        for _ in 0..count {
            self.bind(rusqlite::types::Null)?;
        }
        Ok(())
    }

    fn site(&mut self, site: &Site) -> rusqlite::Result<()> {
        self.bind(site.state)?;
        self.bind(site.file.as_deref())?;
        self.bind(site.line)?;
        self.bind(site.start)?;
        self.bind(site.end)
    }
}

/// Insert one row per key into `target` (`table (columns)`), `width`
/// values each, `ROWS_PER_INSERT` rows per statement.
fn insert_rows<K: Copy>(
    tx: &Transaction<'_>,
    target: &str,
    width: usize,
    keys: &[K],
    mut bind: impl FnMut(&mut Binder<'_, '_>, K) -> rusqlite::Result<()>,
) -> Result<(), Error> {
    let values = |rows: usize| {
        let row = format!("({})", vec!["?"; width].join(","));
        vec![row; rows].join(",")
    };
    let (chunks, remainder) = keys.as_chunks::<ROWS_PER_INSERT>();
    if !chunks.is_empty() {
        let mut statement = tx.prepare(&format!(
            "INSERT INTO {target} VALUES {}",
            values(ROWS_PER_INSERT)
        ))?;
        for chunk in chunks {
            let mut binder = Binder {
                statement: &mut statement,
                index: 0,
            };
            for key in chunk {
                bind(&mut binder, *key)?;
            }
            debug_assert_eq!(binder.index, width * ROWS_PER_INSERT);
            statement.raw_execute()?;
        }
    }
    if !remainder.is_empty() {
        let mut statement = tx.prepare(&format!("INSERT INTO {target} VALUES {}", values(1)))?;
        for key in remainder {
            let mut binder = Binder {
                statement: &mut statement,
                index: 0,
            };
            bind(&mut binder, *key)?;
            debug_assert_eq!(binder.index, width);
            statement.raw_execute()?;
        }
    }
    Ok(())
}

/// Run `write` without `table`'s secondary indexes when it adds more rows
/// than the table holds: building an index once from sorted keys beats
/// updating it per row. The indexes are recreated from their own DDL in the
/// same transaction.
fn with_indexes_deferred(
    tx: &Transaction<'_>,
    table: &str,
    rows: usize,
    write: impl FnOnce() -> Result<(), Error>,
) -> Result<(), Error> {
    let existing: usize = tx.query_row(
        &format!("SELECT count(*) FROM (SELECT 1 FROM {table} LIMIT ?1)"),
        [i64::try_from(rows).unwrap_or(i64::MAX)],
        |r| r.get(0),
    )?;
    if existing >= rows {
        return write();
    }
    let indexes: Vec<(String, String)> = tx
        .prepare(
            "SELECT name, sql FROM sqlite_schema
             WHERE type = 'index' AND tbl_name = ?1 AND sql IS NOT NULL",
        )?
        .query_map([table], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    for (name, _) in &indexes {
        tx.execute_batch(&format!("DROP INDEX \"{}\"", name.replace('"', "\"\"")))?;
    }
    write()?;
    for (_, sql) in &indexes {
        tx.execute_batch(sql)?;
    }
    Ok(())
}

fn saturating_sequence(sequence: u64) -> i64 {
    i64::try_from(sequence).unwrap_or(i64::MAX)
}

/// Sites memoized by function, PC and the state for an absent function:
/// every call path at one call site resolves once.
struct Sites<'f> {
    functions: &'f FxHashMap<u64, FunctionRow>,
    sources: FxHashMap<u64, Option<Source>>,
    memo: FxHashMap<(Option<u64>, Option<i64>, &'static str), usize>,
    resolved: Vec<Site>,
}

impl<'f> Sites<'f> {
    fn new(functions: &'f FxHashMap<u64, FunctionRow>) -> Self {
        Self {
            functions,
            sources: FxHashMap::default(),
            memo: FxHashMap::default(),
            resolved: Vec::new(),
        }
    }

    fn get(&self, index: usize) -> &Site {
        &self.resolved[index]
    }

    /// The index of a site; `get` reads it.
    fn site(&mut self, function: Option<u64>, pc: Option<i64>, absent: SiteState) -> usize {
        let key = (function, pc, absent.label());
        match self.memo.get(&key) {
            Some(index) => *index,
            None => {
                let site = match function {
                    None => Site::missing(absent),
                    Some(function) => {
                        let functions = self.functions;
                        let source = self
                            .sources
                            .entry(function)
                            .or_insert_with(|| functions.get(&function).map(source_of));
                        sites::site_in(source.as_ref(), pc)
                    }
                };
                self.resolved.push(site);
                self.memo.insert(key, self.resolved.len() - 1);
                self.resolved.len() - 1
            }
        }
    }
}

fn source_of(row: &FunctionRow) -> Source {
    let meta = row.metadata.as_deref();
    Source {
        state: row.state,
        conflict: row.conflict,
        file: meta.and_then(|m| m.wire.source_file.clone()),
        map_state: meta.and_then(|m| m.map_state),
        map: meta
            .and_then(|m| m.map_blob.as_deref())
            .and_then(SourceMap::decode),
    }
}
