//! The first index of a recording, built in memory.
//!
//! A recording with no indexed facts has nothing to reconcile against. Its
//! files merge into maps under the same rules the incremental `Applier`
//! applies with SQL: the first definition wins and a different one is a
//! conflict with an issue, references without definitions become
//! placeholders, and announcements and completions merge by identity.
//! Derived columns (execution roots, path depths, child time, profiler
//! nodes) are computed once from the final evidence instead of being
//! repaired after every batch. Then every row is written once, in key order.
use std::{borrow::Cow, collections::BTreeMap};

use btel_reader::{
    evidence::{self, ArgumentNames},
    source_map::SourceMap,
    timing::EpochStatus,
};
use btel_recorder::proto::{self, function_definition::Resolution, span_event::Event};
use prost::Message as _;
use rusqlite::{Transaction, params};
use rustc_hash::{FxHashMap, FxHashSet};

use super::{Totals, id, quantity, sequence_i64, snapshot_id};
use crate::Error;

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
    panicked: u64,
    evidence: u8,
    /// The node's sums are in `Bulk::large_aggregates` instead.
    large: bool,
}

impl SmallTotals {
    /// Add a delta; `false` (and unchanged) when a sum would not fit.
    fn add(&mut self, delta: &Totals) -> bool {
        let sum = |a: u64, b: u128| u64::try_from(b).ok().and_then(|b| a.checked_add(b));
        let (
            Some(count),
            Some(duration),
            Some(self_await),
            Some(errored),
            Some(cancelled),
            Some(panicked),
        ) = (
            sum(self.count, delta.count),
            sum(self.duration, delta.duration),
            sum(self.self_await, delta.self_await),
            sum(self.errored, delta.outcomes.errored),
            sum(self.cancelled, delta.outcomes.cancelled),
            sum(self.panicked, delta.outcomes.panicked),
        )
        else {
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
            panicked,
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
                panicked: u128::from(self.panicked),
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
const THREAD_PANICKED: u16 = 1 << 10;

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

    fn complete(&mut self, ticks: Option<i64>, outcome: i64, panicked: bool) {
        self.completed_ticks = ticks.unwrap_or(0);
        self.set(THREAD_HAS_COMPLETED_TICKS, ticks.is_some());
        self.outcome = outcome;
        self.set(THREAD_PANICKED, panicked);
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
    panicked: Option<bool>,
    needs_announcement: Option<bool>,
    value_cas: Option<[u8; 16]>,
    conflict: i64,
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
    /// Futures' names, first one wins; rare next to threads.
    thread_names: FxHashMap<u64, String>,
    /// Sysop count and ticks per call path.
    sysops: BTreeMap<u32, (u128, u128)>,
    /// `(sequence, position)` to one model turn's usage.
    usage: BTreeMap<(u64, usize), proto::ModelUsage>,
    /// The first header that names the process, and how it ended.
    process: Option<proto::RecordingHeader>,
    process_end: Option<proto::ProcessEnd>,
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

    /// Fold one file, like `Applier::apply`.
    pub(super) fn apply(&mut self, sequence: u64, file: &proto::RecordingFile) {
        let mut ticks_out_of_range = false;
        let mut tick = |value: u64| {
            let converted = quantity(value);
            ticks_out_of_range |= converted.is_none();
            converted
        };
        if self.process.is_none()
            && let Some(header) = file.header.as_ref().filter(|h| h.process_id.is_some())
        {
            self.process = Some(header.clone());
        }
        if let Some(end) = file.end.as_ref().and_then(|end| end.process_end) {
            self.process_end = Some(end);
        }
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
        if let Some(batch) = &file.aggregates {
            for time in &batch.sysop_times {
                let (path, _) = evidence::split_node(time.node);
                self.refer_path(path);
                let entry = self.sysops.entry(path).or_default();
                entry.0 = entry.0.saturating_add(u128::from(time.sysops));
                entry.1 = entry.1.saturating_add(u128::from(time.total_ticks));
            }
        }
        if let Some(usage) = &file.usage {
            for (position, entry) in usage.entries.iter().enumerate() {
                self.usage.insert((sequence, position), entry.clone());
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
                                None => row.complete(completed, outcome, done.panicked),
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
                                panicked: None,
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
                        panicked: Some(done.panicked),
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
                row.panicked = Some(done.panicked);
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
        if let Some(name) = &thread.name {
            self.thread_names
                .entry(thread.thread_id)
                .or_insert_with(|| name.clone());
        }
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

    pub(super) fn write(mut self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        self.report_call_conflicts();
        self.resolve_roots();
        self.resolve_depths();
        self.sum_child_ticks();
        let nodes = self.resolve_nodes();
        self.write_process(tx, rec)?;
        self.write_functions(tx, rec)?;
        with_indexes_deferred(tx, "call_path", self.paths.len(), || {
            self.write_paths(tx, rec, &nodes)
        })?;
        with_indexes_deferred(tx, "thread", self.threads.len(), || {
            self.write_threads(tx, rec)
        })?;
        self.write_clocks(tx, rec)?;
        self.write_aggregates(tx, rec)?;
        with_indexes_deferred(tx, "call", self.calls.len(), || self.write_calls(tx, rec))?;
        self.write_sysops(tx, rec)?;
        self.write_usage(tx, rec)?;
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

    /// Each path's profiler node, like `ingest::resolve_nodes`: known once
    /// the path's function and every ancestor's are, cycles never.
    fn resolve_nodes(&self) -> FxHashMap<u32, i64> {
        let mut nodes: FxHashMap<u32, Option<i64>> = FxHashMap::default();
        let mut chain = Vec::new();
        let mut on_chain = FxHashSet::default();
        for start in self.paths.keys().copied() {
            let mut current = start;
            on_chain.clear();
            // Walk up to a resolved ancestor, a top-level path, or a gap.
            let mut parent_node: Option<Option<i64>> = loop {
                if let Some(known) = nodes.get(&current) {
                    break Some(*known);
                }
                let Some(def) = self.paths.get(&current).and_then(PathRow::def) else {
                    break Some(None);
                };
                if !on_chain.insert(current) {
                    break Some(None);
                }
                chain.push((current, def));
                if def.parent == 0 {
                    break None;
                }
                current = def.parent;
            };
            // Resolve downward: `None` is the top level, `Some(None)` a gap.
            while let Some((path, def)) = chain.pop() {
                let node = match parent_node {
                    Some(None) => None,
                    parent => self.node_name(def.callee).map(|name| {
                        crate::functions::node_hash(parent.flatten(), &name, i64::from(def.edge))
                    }),
                };
                nodes.insert(path, node);
                parent_node = Some(node);
            }
        }
        nodes
            .into_iter()
            .filter_map(|(path, node)| node.map(|node| (path, node)))
            .collect()
    }

    /// A callee's name in profiler nodes; `None` until anything is known.
    fn node_name(&self, function: u64) -> Option<Cow<'_, str>> {
        let row = self.functions.get(&function)?;
        match (row.state, &row.metadata) {
            (2, Some(meta)) => Some(Cow::Borrowed(&meta.wire.fqn)),
            (1, _) => Some(Cow::Owned(format!("#{function:016X}"))),
            _ => None,
        }
    }

    fn write_process(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let file = proto::RecordingFile {
            header: self.process.clone(),
            end: self.process_end.map(|end| proto::RecordingEnd {
                process_end: Some(end),
            }),
            ..Default::default()
        };
        if file.header.is_some() {
            super::record_process(tx, rec, &file)?;
        } else if let Some(end) = self.process_end {
            tx.execute(
                "UPDATE recording SET process_end_status = ?2, process_end_ns = ?3 WHERE rec = ?1",
                params![rec, end.status, end.at_unix_ns],
            )?;
        }
        Ok(())
    }

    fn write_sysops(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let paths: Vec<u32> = self.sysops.keys().copied().collect();
        insert_rows(
            tx,
            "sysop (rec, call_path_id, sysops, total_ticks)",
            4,
            &paths,
            |b, path| {
                let (sysops, ticks) = self.sysops[&path];
                b.bind(rec)?;
                b.bind(i64::from(path))?;
                b.bind(i64::try_from(sysops).unwrap_or(i64::MAX))?;
                b.bind(i64::try_from(ticks).ok())
            },
        )
    }

    fn write_usage(&self, tx: &Transaction<'_>, rec: i64) -> Result<(), Error> {
        let keys: Vec<(u64, usize)> = self.usage.keys().copied().collect();
        let saturating = |value: u64| i64::try_from(value).unwrap_or(i64::MAX);
        insert_rows(
            tx,
            "model_usage (rec, sequence, position, node_id, model, input_tokens, output_tokens,
               cache_read_tokens, cache_write_tokens, reasoning_tokens)",
            10,
            &keys,
            |b, key| {
                let entry = &self.usage[&key];
                b.bind(rec)?;
                b.bind(saturating(key.0))?;
                b.bind(i64::try_from(key.1).unwrap_or(i64::MAX))?;
                b.bind(id(entry.node_id))?;
                b.bind(&entry.model)?;
                b.bind(saturating(entry.input_tokens))?;
                b.bind(saturating(entry.output_tokens))?;
                b.bind(entry.cache_read_tokens.map(saturating))?;
                b.bind(entry.cache_write_tokens.map(saturating))?;
                b.bind(entry.reasoning_tokens.map(saturating))
            },
        )
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
        nodes: &FxHashMap<u32, i64>,
    ) -> Result<(), Error> {
        let mut ids: Vec<u32> = self.paths.keys().copied().collect();
        ids.sort_unstable();
        insert_rows(
            tx,
            "call_path (rec, call_path_id, defined, thread_id, parent_call_path_id,
               caller_function_id, caller_pc, callee_function_id, edge, depth, node_id,
               direct_child_ticks, conflict)",
            13,
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
                b.bind(nodes.get(&path).copied())?;
                b.bind(row.child_ticks())?;
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
               epoch_id, announced, completed_ticks, outcome, panicked, name,
               completion_conflict, root_id, conflict, started_ms, start_pending)",
            17,
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
                b.bind(row.has(THREAD_PANICKED))?;
                b.bind(self.thread_names.get(&thread))?;
                b.bind(row.has(THREAD_COMPLETION_CONFLICT))?;
                b.bind(row.root().value().map(id))?;
                b.bind(row.has(THREAD_CONFLICT))?;
                // A root's start, as `ingest::resolve_starts` stores it.
                b.bind(def.filter(|d| d.parent.is_none()).and_then(|d| {
                    let epoch = self.epochs.get(&d.epoch)?;
                    let clock = epoch.def.as_ref()?;
                    crate::functions::unix_ms(
                        d.started?,
                        clock.utc_ticks.filter(|_| !epoch.conflict)?,
                        clock.utc_unix_ns?,
                        clock.multiplier?,
                        clock.shift,
                    )
                }))?;
                b.bind(false)
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
               cancelled_calls, errored_exact, cancelled_exact, panicked_calls, panicked_exact)",
            16,
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
                b.bind(total.outcomes.cancelled.to_string())?;
                b.bind(total.outcomes.panicked())?;
                b.bind(total.outcomes.panicked.to_string())
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
               exited_ticks, self_await_ticks, outcome, panicked, needs_announcement, value_cas,
               conflict)",
            18,
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
                b.bind(row.panicked)?;
                b.bind(row.needs_announcement)?;
                b.bind(row.value_cas.as_ref().map(<[u8; 16]>::as_slice))?;
                b.bind(row.conflict)
            },
        )
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
    // Dropping an index drops its planner statistics.
    crate::store::write_statistics(tx)
}

fn saturating_sequence(sequence: u64) -> i64 {
    i64::try_from(sequence).unwrap_or(i64::MAX)
}
