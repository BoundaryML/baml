//! Catalog-v1 relations served directly from canonical local artifacts.
//!
//! Grain honesty is enforced at build: population rows come from folds
//! (every call counted), retained rows only from actually-retained
//! evidence, and every unavailable value is a typed handle — never a
//! silent blank. Running runs are part of the snapshot (D15) with
//! explicit pending states and so-far counters bound at snapshot time.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use baml_query::catalog::RelationDef;
use baml_query::error::QueryError;
use baml_query::outcome::UnavailableReason;
use baml_query::provider::RelationProviderFactory;
use baml_query::scope::Snapshot;
use bex_events::prof::cct::meta::{self, MetaRecord};
use bex_query::cct::{CctFold, fold_segments};
use bex_query::source::{FileId, Poll, SegmentSource};
use datafusion::arrow::array::{
    ArrayRef, BinaryBuilder, FixedSizeBinaryBuilder, FixedSizeListBuilder, ListBuilder,
    StringBuilder, TimestampNanosecondBuilder, UInt32Builder, UInt64Builder,
};
use datafusion::arrow::record_batch::RecordBatch;
use datafusion::datasource::TableProvider;
use datafusion::datasource::memory::MemTable;

use crate::resolver::{cid_handle, legacy_handle, unavailable_handle};
use crate::universe::{BoundRun, LocalUniverse};

/// The local backend's relation factory.
pub struct LocalProviderFactory {
    universe: Arc<LocalUniverse>,
    /// Per-run fold cache (several relations fold the same run).
    folds: Mutex<HashMap<String, Arc<RunFold>>>,
    /// Per-run value-scan cache.
    values: Mutex<HashMap<String, Arc<ValueScan>>>,
}

/// One run's fold plus its provenance flags.
struct RunFold {
    fold: CctFold,
    torn: bool,
    /// For running runs: only nodes of this partition belong to the run.
    partition_filter: Option<u32>,
}

/// One run's scanned value evidence.
#[derive(Default)]
struct ValueScan {
    /// (thread, call) → per-role capture evidence.
    calls: Vec<CallCapture>,
    losses: Vec<(String, String, u64, u64)>, // (kind, reason, count, at_ms)
    truncated: bool,
}

struct CallCapture {
    thread_id: u64,
    call_id: u64,
    function_id: u32,
    is_root: bool,
    /// role → (handle bytes, promoted_by).
    roles: Vec<(&'static str, Vec<u8>, Option<String>)>,
    value_ids: Vec<String>,
}

/// In-memory byte source for the fold reader.
struct BytesSource(Vec<Vec<u8>>);

impl SegmentSource for BytesSource {
    fn committed_len(&self, file: FileId) -> u64 {
        self.0.get(file.0 as usize).map_or(0, |b| b.len() as u64)
    }
    fn generation(&self, _file: FileId) -> u64 {
        0
    }
    fn view(&self, range: &bex_query::source::ByteRange) -> Option<&[u8]> {
        let bytes = self.0.get(range.file.0 as usize)?;
        let start = usize::try_from(range.offset).ok()?;
        let end = start + usize::try_from(range.len).ok()?;
        bytes.get(start..end)
    }
}

impl LocalProviderFactory {
    #[must_use]
    pub fn new(universe: Arc<LocalUniverse>) -> LocalProviderFactory {
        LocalProviderFactory {
            universe,
            folds: Mutex::new(HashMap::new()),
            values: Mutex::new(HashMap::new()),
        }
    }

    fn run_key(run: &BoundRun) -> String {
        run.row
            .dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    }

    /// Fold one run at the bound snapshot: the sealed `cct.bamlcct` for
    /// completed runs, the bound session's committed segment prefixes
    /// (partition-filtered) for running ones.
    fn fold_of(&self, run: &BoundRun) -> Option<Arc<RunFold>> {
        let key = Self::run_key(run);
        if let Some(hit) = self.folds.lock().unwrap().get(&key) {
            return Some(hit.clone());
        }
        let outcome = if let Some(snapshot) = &run.snapshot_file {
            let bytes = snapshot.read().ok()?;
            let source = BytesSource(vec![bytes]);
            let Poll::Ready(fold) = fold_segments(&source, &[FileId(0)]) else {
                return None;
            };
            let torn = fold.torn;
            RunFold {
                fold,
                torn,
                partition_filter: None,
            }
        } else {
            // Running/crashed without a sealed snapshot: fold the bound
            // session's committed prefixes and attribute by partition.
            let session = self.universe.session_of(&run.row.session_dir)?;
            let mut buffers = Vec::new();
            for file in &session.cct_files {
                buffers.push(file.read().ok()?);
            }
            let ids: Vec<FileId> = (0..buffers.len())
                .map(|i| FileId(u32::try_from(i).unwrap_or(u32::MAX)))
                .collect();
            let source = BytesSource(buffers);
            let Poll::Ready(fold) = fold_segments(&source, &ids) else {
                return None;
            };
            let boundary_bytes = bex_events::ids::BoundaryId::from_wire_str(&run.row.boundary_id)
                .map(|id| id.as_bytes())?;
            let partition = fold
                .partition_binds
                .iter()
                .find(|(_, bound)| *bound == boundary_bytes)
                .map(|(partition, _)| *partition)?;
            let torn = fold.torn;
            RunFold {
                fold,
                torn,
                partition_filter: Some(partition),
            }
        };
        let outcome = Arc::new(outcome);
        self.folds.lock().unwrap().insert(key, outcome.clone());
        Some(outcome)
    }

    fn values_of(&self, run: &BoundRun) -> Arc<ValueScan> {
        let key = Self::run_key(run);
        if let Some(hit) = self.values.lock().unwrap().get(&key) {
            return hit.clone();
        }
        let mut scan = ValueScan::default();
        let mut by_call: HashMap<(u64, u64), usize> = HashMap::new();
        for file in &run.value_files {
            let Ok(bytes) = file.read() else { continue };
            let Ok(contents) = bex_events::value::read_bamlvalue_from_bytes(&bytes) else {
                scan.truncated = true;
                continue;
            };
            scan.truncated |= contents.truncated;
            for record in contents.records {
                match record {
                    bex_events::value::ValueFileRecord::CapturedValue(record) => {
                        let Some(capture) = &record.capture else {
                            continue;
                        };
                        if capture.kind == bex_events::value::ValueCaptureKind::LogBody {
                            continue;
                        }
                        let role = match capture.kind {
                            bex_events::value::ValueCaptureKind::RootInput
                            | bex_events::value::ValueCaptureKind::CallInput => "args",
                            bex_events::value::ValueCaptureKind::RootOutput
                            | bex_events::value::ValueCaptureKind::CallOutput => "return",
                            _ => "error",
                        };
                        let is_root = matches!(
                            capture.kind,
                            bex_events::value::ValueCaptureKind::RootInput
                                | bex_events::value::ValueCaptureKind::RootOutput
                                | bex_events::value::ValueCaptureKind::RootError
                        );
                        let handle = if let Some(dag) = &record.dag_ref {
                            cid_handle(&dag.root_cid)
                        } else if !record.body.is_empty() || record.blob_ref.is_some() {
                            legacy_handle(&key, &record.value_ref.id)
                        } else {
                            unavailable_handle(UnavailableReason::Lost)
                        };
                        let idx = *by_call
                            .entry((capture.call.thread_id.0, capture.call.call_id.0))
                            .or_insert_with(|| {
                                scan.calls.push(CallCapture {
                                    thread_id: capture.call.thread_id.0,
                                    call_id: capture.call.call_id.0,
                                    function_id: capture.function_id,
                                    is_root: false,
                                    roles: Vec::new(),
                                    value_ids: Vec::new(),
                                });
                                scan.calls.len() - 1
                            });
                        let call = &mut scan.calls[idx];
                        call.is_root |= is_root;
                        if call.function_id == 0 {
                            call.function_id = capture.function_id;
                        }
                        call.roles.push((role, handle, record.promoted_by.clone()));
                        call.value_ids.push(record.value_ref.id.clone());
                    }
                    bex_events::value::ValueFileRecord::CaptureLoss(loss) => {
                        scan.losses.push((
                            format!("{:?}", loss.kind).to_ascii_lowercase(),
                            format!("{:?}", loss.reason).to_ascii_lowercase(),
                            loss.skipped_count,
                            loss.timestamp_ms,
                        ));
                    }
                    _ => {}
                }
            }
        }
        let scan = Arc::new(scan);
        self.values.lock().unwrap().insert(key, scan.clone());
        scan
    }

    fn dict_of(&self, revision_wire: &str) -> Option<bex_events::dict::pb::RevisionDictionaryV1> {
        let bytes = self.universe.dicts.get(revision_wire)?;
        bex_events::dict::read_dict(bytes).ok()
    }
}

fn status_public(raw: &str) -> &str {
    match raw {
        // IN-Q1-5: begin-without-complete under a dead session.
        "crashed" => "abandoned",
        other => other,
    }
}

fn ms_to_ns(ms: u64) -> i64 {
    i64::try_from(ms.saturating_mul(1_000_000)).unwrap_or(i64::MAX)
}

impl RelationProviderFactory for LocalProviderFactory {
    fn provider(
        &self,
        relation: &RelationDef,
        _snapshot: &Snapshot,
    ) -> Result<Option<Arc<dyn TableProvider>>, QueryError> {
        let batch = match relation.name {
            "runs_v1" => self.runs_batch(relation),
            "cct_population_v1" => self.population_batch(relation),
            "retained_calls_v1" => self.retained_calls_batch(relation),
            "functions_v1" => self.functions_batch(relation),
            "revisions_v1" => self.revisions_batch(relation),
            "evidence_issues_v1" => self.issues_batch(relation),
            "exact_windows_v1" => self.windows_batch(relation),
            "llm_population_v1" => self.llm_batch(relation),
            "spawn_edges_v1" => self.spawn_batch(relation),
            _ => return Ok(None),
        };
        let mem = MemTable::try_new(relation.schema(), vec![vec![batch]]).map_err(|e| {
            QueryError::new(
                baml_query::error::QueryErrorCode::Internal,
                format!("local provider for {}: {e}", relation.name),
            )
        })?;
        Ok(Some(Arc::new(mem)))
    }
}

/// Column-store row assembly: every relation collects plain row structs
/// and renders them column-by-column against the catalog schema, so a
/// drifting column list fails loudly at build.
macro_rules! columns {
    ($relation:expr, $rows:expr, |$row:ident, $name:ident| $cell:expr) => {{
        let arrays: Vec<ArrayRef> = $relation
            .columns
            .iter()
            .map(|column| {
                let $name = column.name;
                let mut cells = Cells::for_type(&column.data_type);
                for $row in $rows.iter() {
                    cells.push($cell);
                }
                cells.finish()
            })
            .collect();
        RecordBatch::try_new($relation.schema(), arrays).expect("rows match catalog schema")
    }};
}

/// One cell value, dynamically typed against the catalog column types.
enum Cell {
    Null,
    Str(String),
    U32(u32),
    U64(u64),
    TsNs(i64),
    Bytes(Vec<u8>),
    StrList(Vec<String>),
    Hist(Vec<u64>),
    Hash([u8; 32]),
}

enum Cells {
    Str(StringBuilder),
    U32(UInt32Builder),
    U64(UInt64Builder),
    Ts(TimestampNanosecondBuilder),
    Bin(BinaryBuilder),
    StrList(ListBuilder<StringBuilder>),
    Hist(FixedSizeListBuilder<UInt64Builder>),
    Hash(FixedSizeBinaryBuilder),
}

impl Cells {
    fn for_type(data_type: &datafusion::arrow::datatypes::DataType) -> Cells {
        use datafusion::arrow::datatypes::DataType;
        match data_type {
            DataType::Utf8 => Cells::Str(StringBuilder::new()),
            DataType::UInt32 => Cells::U32(UInt32Builder::new()),
            DataType::UInt64 => Cells::U64(UInt64Builder::new()),
            DataType::Timestamp(..) => {
                Cells::Ts(TimestampNanosecondBuilder::new().with_timezone("UTC"))
            }
            DataType::Binary => Cells::Bin(BinaryBuilder::new()),
            DataType::List(_) => Cells::StrList(ListBuilder::new(StringBuilder::new()).with_field(
                datafusion::arrow::datatypes::Field::new("item", DataType::Utf8, false),
            )),
            DataType::FixedSizeList(..) => Cells::Hist(
                FixedSizeListBuilder::new(UInt64Builder::new(), 16).with_field(
                    datafusion::arrow::datatypes::Field::new("item", DataType::UInt64, false),
                ),
            ),
            DataType::FixedSizeBinary(32) => Cells::Hash(FixedSizeBinaryBuilder::new(32)),
            other => unreachable!("catalog v1 has no {other:?} columns"),
        }
    }

    fn push(&mut self, cell: Cell) {
        match (self, cell) {
            (Cells::Str(b), Cell::Str(v)) => b.append_value(v),
            (Cells::Str(b), Cell::Null) => b.append_null(),
            (Cells::U32(b), Cell::U32(v)) => b.append_value(v),
            (Cells::U32(b), Cell::Null) => b.append_null(),
            (Cells::U64(b), Cell::U64(v)) => b.append_value(v),
            (Cells::U64(b), Cell::Null) => b.append_null(),
            (Cells::Ts(b), Cell::TsNs(v)) => b.append_value(v),
            (Cells::Ts(b), Cell::Null) => b.append_null(),
            (Cells::Bin(b), Cell::Bytes(v)) => b.append_value(&v),
            (Cells::Bin(b), Cell::Null) => b.append_null(),
            (Cells::StrList(b), Cell::StrList(items)) => {
                for item in items {
                    b.values().append_value(item);
                }
                b.append(true);
            }
            (Cells::StrList(b), Cell::Null) => b.append(true),
            (Cells::Hist(b), Cell::Hist(buckets)) => {
                for bucket in buckets {
                    b.values().append_value(bucket);
                }
                b.append(true);
            }
            (Cells::Hist(b), Cell::Null) => {
                for _ in 0..16 {
                    b.values().append_value(0);
                }
                b.append(false);
            }
            (Cells::Hash(b), Cell::Hash(v)) => {
                b.append_value(v).expect("32-byte hash");
            }
            (Cells::Hash(b), Cell::Null) => b.append_null(),
            (_, _) => unreachable!("cell type mismatches catalog column"),
        }
    }

    fn finish(self) -> ArrayRef {
        match self {
            Cells::Str(mut b) => Arc::new(b.finish()),
            Cells::U32(mut b) => Arc::new(b.finish()),
            Cells::U64(mut b) => Arc::new(b.finish()),
            Cells::Ts(mut b) => Arc::new(b.finish()),
            Cells::Bin(mut b) => Arc::new(b.finish()),
            Cells::StrList(mut b) => Arc::new(b.finish()),
            Cells::Hist(mut b) => Arc::new(b.finish()),
            Cells::Hash(mut b) => Arc::new(b.finish()),
        }
    }
}

impl LocalProviderFactory {
    fn runs_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            started_ns: i64,
            ended_ns: Option<i64>,
            duration_ns: u64,
            status: String,
            revision_id: String,
            entry_function_id: Option<u32>,
            entrypoint: String,
            total_calls: u64,
            total_errors: u64,
            structure_state: &'static str,
            value_state: &'static str,
            integrity_state: &'static str,
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let fold = self.fold_of(run);
            let scan = self.values_of(run);
            let running = run.row.status == "running";
            let (total_calls, total_errors, entry_function_id, torn, degraded) = match &fold {
                Some(rf) => {
                    let keep = |i: usize| {
                        rf.partition_filter
                            .is_none_or(|p| rf.fold.partition[i] == p)
                    };
                    let mut calls = 0;
                    let mut errors = 0;
                    let mut entries: Vec<u32> = Vec::new();
                    for i in 1..rf.fold.len() {
                        if !keep(i) || rf.fold.function[i] == 0 {
                            continue;
                        }
                        calls += rf.fold.enters[i];
                        errors += rf.fold.ends_err[i];
                        let parent = rf.fold.parent[i] as usize;
                        if rf.fold.function[parent] == 0 && !entries.contains(&rf.fold.function[i])
                        {
                            entries.push(rf.fold.function[i]);
                        }
                    }
                    let entry = (entries.len() == 1).then(|| entries[0]);
                    (
                        calls,
                        errors,
                        entry,
                        rf.torn,
                        !rf.fold.loss_markers.is_empty(),
                    )
                }
                None => (0, 0, None, false, false),
            };
            let structure_state = if running {
                "pending"
            } else if fold.is_none() {
                "lost"
            } else if torn || degraded {
                "incomplete"
            } else {
                "complete"
            };
            let value_state = if running {
                "pending"
            } else if !scan.losses.is_empty() || scan.truncated {
                "partial"
            } else if scan.calls.is_empty() {
                "not_captured"
            } else {
                "complete"
            };
            let integrity_state = if torn || scan.truncated {
                "corrupt"
            } else {
                "verified"
            };
            let started_ns = ms_to_ns(run.row.created_ms);
            let ended_ns = (run.row.completed_ms > 0).then(|| ms_to_ns(run.row.completed_ms));
            let duration_ns = if let Some(end) = ended_ns {
                u64::try_from(end.saturating_sub(started_ns)).unwrap_or(0)
            } else {
                self.universe
                    .bound_at_ns
                    .saturating_sub(u64::try_from(started_ns).unwrap_or(0))
            };
            rows.push(Row {
                run_id: run.row.boundary_id.clone(),
                started_ns,
                ended_ns,
                duration_ns,
                status: status_public(&run.row.status).to_string(),
                revision_id: run.row.revision_id.clone(),
                entry_function_id,
                entrypoint: run.row.target.clone(),
                total_calls,
                total_errors,
                structure_state,
                value_state,
                integrity_state,
            });
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "started_at" => Cell::TsNs(row.started_ns),
            "ended_at" => row.ended_ns.map_or(Cell::Null, Cell::TsNs),
            "duration_ns" => Cell::U64(row.duration_ns),
            "status" => Cell::Str(row.status.clone()),
            "revision_id" => Cell::Str(row.revision_id.clone()),
            "entry_function_id" => row.entry_function_id.map_or(Cell::Null, Cell::U32),
            "entrypoint" => Cell::Str(row.entrypoint.clone()),
            "total_calls" => Cell::U64(row.total_calls),
            "total_errors" => Cell::U64(row.total_errors),
            "structure_state" => Cell::Str(row.structure_state.to_string()),
            "value_state" => Cell::Str(row.value_state.to_string()),
            "integrity_state" => Cell::Str(row.integrity_state.to_string()),
            "projection_state" => Cell::Str("active".to_string()),
            "retention_state" => Cell::Str("retained".to_string()),
            other => unreachable!("runs_v1 column {other}"),
        })
    }

    fn population_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            node_id: u32,
            parent_node_id: Option<u32>,
            depth: u32,
            function_id: u32,
            revision_id: String,
            definition_key: Option<String>,
            local_hash: Option<[u8; 32]>,
            fqn: String,
            counts: [u64; 5],
            times: [u64; 3],
            hist: Vec<u64>,
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let Some(rf) = self.fold_of(run) else {
                continue;
            };
            let dict = self.dict_of(&run.row.revision_id);
            let lookup = |function_id: u32| {
                dict.as_ref()
                    .and_then(|d| bex_events::dict::function_row_by_id(d, function_id))
            };
            for i in 1..rf.fold.len() {
                if rf.fold.function[i] == 0 {
                    continue;
                }
                if let Some(p) = rf.partition_filter
                    && rf.fold.partition[i] != p
                {
                    continue;
                }
                let function_id = rf.fold.function[i];
                let row = lookup(function_id);
                let parent = rf.fold.parent[i] as usize;
                let parent_public = (rf.fold.function[parent] != 0)
                    .then(|| u32::try_from(parent).unwrap_or(u32::MAX));
                rows.push(Row {
                    run_id: run.row.boundary_id.clone(),
                    node_id: u32::try_from(i).unwrap_or(u32::MAX),
                    parent_node_id: parent_public,
                    depth: rf.fold.depth[i].saturating_sub(2) + 1,
                    function_id,
                    revision_id: run.row.revision_id.clone(),
                    definition_key: row
                        .as_ref()
                        .map(|r| r.definition_key.clone())
                        .filter(|k| !k.is_empty()),
                    local_hash: row
                        .as_ref()
                        .and_then(|r| <[u8; 32]>::try_from(r.def_content_hash.as_slice()).ok()),
                    fqn: row
                        .as_ref()
                        .map_or_else(|| format!("fn#{function_id}"), |r| r.fqn.clone()),
                    counts: [
                        rf.fold.enters[i],
                        rf.fold.ends_ok[i],
                        rf.fold.ends_err[i],
                        rf.fold.ends_cancel[i],
                        rf.fold.ends_exit[i],
                    ],
                    times: [rf.fold.total_ns[i], rf.fold.self_ns[i], rf.fold.await_ns[i]],
                    hist: rf.fold.hist[i].iter().map(|&b| u64::from(b)).collect(),
                });
            }
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "node_id" => Cell::U32(row.node_id),
            "parent_node_id" => row.parent_node_id.map_or(Cell::Null, Cell::U32),
            "depth" => Cell::U32(row.depth),
            "function_id" => Cell::U32(row.function_id),
            "revision_id" => Cell::Str(row.revision_id.clone()),
            "definition_key" => row.definition_key.clone().map_or(Cell::Null, Cell::Str),
            "local_definition_hash" => row.local_hash.map_or(Cell::Null, Cell::Hash),
            "fqn" => Cell::Str(row.fqn.clone()),
            "calls_started" => Cell::U64(row.counts[0]),
            "calls_succeeded" => Cell::U64(row.counts[1]),
            "calls_errored" => Cell::U64(row.counts[2]),
            "calls_cancelled" => Cell::U64(row.counts[3]),
            "calls_exited" => Cell::U64(row.counts[4]),
            "inclusive_ns" => Cell::U64(row.times[0]),
            "self_ns" => Cell::U64(row.times[1]),
            "await_ns" => Cell::U64(row.times[2]),
            "duration_histogram" => Cell::Hist(row.hist.clone()),
            other => unreachable!("cct_population_v1 column {other}"),
        })
    }

    fn retained_calls_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            call_id: u64,
            thread_id: u64,
            definition_key: Option<String>,
            status: Option<String>,
            reasons: Vec<String>,
            evidence: Vec<String>,
            roles: HashMap<&'static str, Vec<u8>>,
            states: HashMap<&'static str, &'static str>,
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let scan = self.values_of(run);
            let dict = self.dict_of(&run.row.revision_id);
            for call in &scan.calls {
                let mut roles: HashMap<&'static str, Vec<u8>> = HashMap::new();
                let mut states: HashMap<&'static str, &'static str> = HashMap::new();
                let mut reasons = vec!["policy".to_string()];
                for (role, handle, promoted_by) in &call.roles {
                    roles.insert(role, handle.clone());
                    states.insert(
                        role,
                        if handle.first() == Some(&crate::resolver::TAG_UNAVAILABLE) {
                            "lost"
                        } else {
                            "available"
                        },
                    );
                    if promoted_by.is_some() && !reasons.iter().any(|r| r == "promotion") {
                        reasons.push("promotion".to_string());
                    }
                }
                let definition_key = dict
                    .as_ref()
                    .and_then(|d| bex_events::dict::function_row_by_id(d, call.function_id))
                    .map(|r| r.definition_key.clone())
                    .filter(|k| !k.is_empty());
                // Only the root call's terminal state is individually
                // recorded (it is the run's); helper calls stay NULL.
                let status = call
                    .is_root
                    .then(|| status_public(&run.row.status).to_string())
                    .filter(|s| s != "running");
                rows.push(Row {
                    run_id: run.row.boundary_id.clone(),
                    call_id: call.call_id,
                    thread_id: call.thread_id,
                    definition_key,
                    status,
                    reasons,
                    evidence: call
                        .value_ids
                        .iter()
                        .map(|id| format!("value:{id}"))
                        .collect(),
                    roles,
                    states,
                });
            }
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "call_id" => Cell::U64(row.call_id),
            "parent_call_id"
            | "node_id"
            | "capture_policy_version"
            | "started_at"
            | "ended_at"
            | "duration_ns" => Cell::Null,
            "thread_id" => Cell::U64(row.thread_id),
            "definition_key" => row.definition_key.clone().map_or(Cell::Null, Cell::Str),
            "status" => row.status.clone().map_or(Cell::Null, Cell::Str),
            "retention_reasons" => Cell::StrList(row.reasons.clone()),
            "exact_window_ids" => Cell::StrList(Vec::new()),
            "evidence_ids" => Cell::StrList(row.evidence.clone()),
            "args_state" | "return_state" | "error_state" => {
                let role = name.split('_').next().unwrap_or("args");
                let role = if role == "args" {
                    "args"
                } else if role == "return" {
                    "return"
                } else {
                    "error"
                };
                Cell::Str(
                    row.states
                        .get(role)
                        .copied()
                        .unwrap_or(if role == "args" {
                            "not_captured"
                        } else {
                            "not_applicable"
                        })
                        .to_string(),
                )
            }
            "args" | "return" | "error" =>
                row.roles.get(name).cloned().map_or(Cell::Null, Cell::Bytes),
            other => unreachable!("retained_calls_v1 column {other}"),
        })
    }

    fn functions_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            revision_id: String,
            f: bex_events::dict::pb::FunctionDictRow,
            path: Option<String>,
        }
        let mut rows = Vec::new();
        for (revision_wire, _) in self.universe.dicts.iter() {
            let Some(dict) = self.dict_of(revision_wire) else {
                continue;
            };
            let files: HashMap<u32, String> = dict
                .files
                .as_ref()
                .map(|s| {
                    s.files
                        .iter()
                        .map(|r| (r.file_id, r.path.clone()))
                        .collect()
                })
                .unwrap_or_default();
            for f in dict.functions.iter().flat_map(|s| &s.functions) {
                rows.push(Row {
                    revision_id: revision_wire.clone(),
                    f: f.clone(),
                    path: files.get(&f.file_id).cloned(),
                });
            }
        }
        let capture = |flags: u32, shift: u32| match (flags >> shift) & 0b11 {
            0 => "disabled",
            1 => "auto",
            _ => "enabled",
        };
        columns!(relation, rows, |row, name| match name {
            "revision_id" => Cell::Str(row.revision_id.clone()),
            "function_id" => Cell::U32(row.f.function_id),
            "definition_key" => (!row.f.definition_key.is_empty())
                .then(|| Cell::Str(row.f.definition_key.clone()))
                .unwrap_or(Cell::Null),
            "local_definition_hash" => <[u8; 32]>::try_from(row.f.def_content_hash.as_slice())
                .map_or(Cell::Null, Cell::Hash),
            "fqn" => Cell::Str(row.f.fqn.clone()),
            "display_name" => Cell::Str(row.f.display_name.clone()),
            "source_path" => row.path.clone().map_or(Cell::Null, Cell::Str),
            "source_start" => (row.f.file_id != u32::MAX)
                .then(|| Cell::U32(row.f.span_start))
                .unwrap_or(Cell::Null),
            "source_end" => (row.f.file_id != u32::MAX)
                .then(|| Cell::U32(row.f.span_end))
                .unwrap_or(Cell::Null),
            "source_line" => (row.f.file_id != u32::MAX)
                .then(|| Cell::U32(row.f.line))
                .unwrap_or(Cell::Null),
            "kind" => Cell::Str(row.f.kind.clone()),
            "origin" => Cell::Str(row.f.origin.clone()),
            "capture_inputs" => Cell::Str(capture(row.f.capture_flags, 0).to_string()),
            "capture_output" => Cell::Str(capture(row.f.capture_flags, 2).to_string()),
            "capture_error" => Cell::Str(capture(row.f.capture_flags, 4).to_string()),
            "promote_on_error" => Cell::Str(capture(row.f.capture_flags, 6).to_string()),
            other => unreachable!("functions_v1 column {other}"),
        })
    }

    fn revisions_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            revision_id: String,
            source_snapshot_id: String,
            compiler_id: String,
            policy: u32,
            fallback: bool,
        }
        let mut rows = Vec::new();
        for (revision_wire, _) in self.universe.dicts.iter() {
            let Some(dict) = self.dict_of(revision_wire) else {
                continue;
            };
            let identity = dict.identity.unwrap_or_default();
            let snapshot_wire = <[u8; 32]>::try_from(identity.source_snapshot_id.as_slice())
                .map(|bytes| {
                    format!(
                        "baml_src_1_{}",
                        bex_events::store::canon::cid_wire(&bytes).trim_start_matches("bamlv_1_")
                    )
                })
                .unwrap_or_default();
            rows.push(Row {
                revision_id: revision_wire.clone(),
                source_snapshot_id: snapshot_wire,
                compiler_id: identity.compiler_id,
                policy: dict.capture_policy_version,
                fallback: identity.fallback_identity,
            });
        }
        columns!(relation, rows, |row, name| match name {
            "revision_id" => Cell::Str(row.revision_id.clone()),
            "source_snapshot_id" => Cell::Str(row.source_snapshot_id.clone()),
            "compiler_id" => Cell::Str(row.compiler_id.clone()),
            "capture_policy_version" => Cell::U32(row.policy),
            "identity_state" => Cell::Str(
                if row.fallback {
                    "fallback_legacy"
                } else {
                    "verified"
                }
                .to_string()
            ),
            "first_seen_at" => Cell::TsNs(0),
            other => unreachable!("revisions_v1 column {other}"),
        })
    }

    fn issues_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            issue_id: String,
            run_id: String,
            source: &'static str,
            kind: String,
            reason: String,
            count: u64,
            first_ms: u64,
            last_ms: u64,
        }
        let mut rows: Vec<Row> = Vec::new();
        for run in &self.universe.runs {
            let scan = self.values_of(run);
            let mut grouped: HashMap<(String, String), (u64, u64, u64)> = HashMap::new();
            for (kind, reason, count, at_ms) in &scan.losses {
                let slot =
                    grouped
                        .entry((kind.clone(), reason.clone()))
                        .or_insert((0, u64::MAX, 0));
                slot.0 += count;
                slot.1 = slot.1.min(*at_ms);
                slot.2 = slot.2.max(*at_ms);
            }
            for ((kind, reason), (count, first, last)) in grouped {
                rows.push(Row {
                    issue_id: format!("{}:{kind}:{reason}", run.row.boundary_id),
                    run_id: run.row.boundary_id.clone(),
                    source: "value_capture",
                    kind,
                    reason,
                    count,
                    first_ms: if first == u64::MAX {
                        run.row.created_ms
                    } else {
                        first
                    },
                    last_ms: if last == 0 { run.row.created_ms } else { last },
                });
            }
            // Structural loss/diagnostics from the boundary meta stream.
            if let Ok(bytes) = std::fs::read(run.row.dir.join("boundary.bamlmeta"))
                && let Ok(contents) = meta::read_meta(&bytes)
            {
                for record in &contents.records {
                    if let MetaRecord::BoundaryLoss { kind, detail: _ } = record {
                        rows.push(Row {
                            issue_id: format!("{}:structure:{kind}", run.row.boundary_id),
                            run_id: run.row.boundary_id.clone(),
                            source: "profiler",
                            kind: "structure".to_string(),
                            reason: kind.clone(),
                            count: 1,
                            first_ms: run.row.created_ms,
                            last_ms: run.row.completed_ms.max(run.row.created_ms),
                        });
                    }
                }
            }
        }
        columns!(relation, rows, |row, name| match name {
            "issue_id" => Cell::Str(row.issue_id.clone()),
            "run_id" => Cell::Str(row.run_id.clone()),
            "session_id" | "evidence_id" | "policy_version" => Cell::Null,
            "source" => Cell::Str(row.source.to_string()),
            "kind" => Cell::Str(row.kind.clone()),
            "reason" => Cell::Str(row.reason.clone()),
            "count" => Cell::U64(row.count),
            "first_seen_at" => Cell::TsNs(ms_to_ns(row.first_ms)),
            "last_seen_at" => Cell::TsNs(ms_to_ns(row.last_ms)),
            other => unreachable!("evidence_issues_v1 column {other}"),
        })
    }

    fn windows_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            window_id: String,
            session_id: String,
            trigger: String,
            started_ms: u64,
            ended_ms: u64,
            event_count: u64,
            state: &'static str,
            incomplete: Vec<String>,
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let Ok(bytes) = std::fs::read(run.row.dir.join("boundary.bamlmeta")) else {
                continue;
            };
            let Ok(contents) = meta::read_meta(&bytes) else {
                continue;
            };
            for record in &contents.records {
                let MetaRecord::BoundaryTrigger {
                    trigger,
                    at_ms,
                    detail,
                } = record
                else {
                    continue;
                };
                let Some(dump_name) = detail.strip_prefix("flight:") else {
                    continue;
                };
                let session = self.universe.session_of(&run.row.session_dir);
                let dump = session.and_then(|s| {
                    s.flight_files
                        .iter()
                        .find(|f| f.path.file_name().is_some_and(|n| n == dump_name))
                });
                let (event_count, state, incomplete) = match dump.map(|f| f.read()) {
                    Some(Ok(bytes)) => {
                        match bex_events::prof::read::read_bamlprof_from_bytes(&bytes) {
                            Ok(contents) => (
                                contents.events.len() as u64,
                                if contents.truncated {
                                    "incomplete"
                                } else {
                                    "available"
                                },
                                if contents.truncated {
                                    vec!["truncated".to_string()]
                                } else {
                                    Vec::new()
                                },
                            ),
                            Err(_) => (0, "corrupt", vec!["unsupported".to_string()]),
                        }
                    }
                    _ => (0, "lost", Vec::new()),
                };
                rows.push(Row {
                    run_id: run.row.boundary_id.clone(),
                    window_id: dump_name.to_string(),
                    session_id: run.row.session_dir.clone(),
                    trigger: trigger.clone(),
                    started_ms: *at_ms,
                    ended_ms: *at_ms,
                    event_count,
                    state,
                    incomplete,
                });
            }
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "window_id" => Cell::Str(row.window_id.clone()),
            "session_id" => Cell::Str(row.session_id.clone()),
            "source" => Cell::Str("flight_dump".to_string()),
            "trigger" => Cell::Str(row.trigger.clone()),
            "trigger_node_id" | "trigger_call_id" => Cell::Null,
            "started_at" => Cell::TsNs(ms_to_ns(row.started_ms)),
            "ended_at" => Cell::TsNs(ms_to_ns(row.ended_ms)),
            "event_count" => Cell::U64(row.event_count),
            "evidence_state" => Cell::Str(row.state.to_string()),
            "incomplete_reasons" => Cell::StrList(row.incomplete.clone()),
            "evidence_id" => Cell::Str(format!("flight:{}", row.window_id)),
            other => unreachable!("exact_windows_v1 column {other}"),
        })
    }

    fn llm_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            node_id: u32,
            model: String,
            calls: u64,
            tokens: (u64, u64),
            errors: (u64, u64),
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let Some(rf) = self.fold_of(run) else {
                continue;
            };
            for (&(node, model_id), &(calls, tin, tout, perr, parse)) in &rf.fold.llm {
                let node_usize = node as usize;
                if node_usize >= rf.fold.len() {
                    continue;
                }
                if let Some(p) = rf.partition_filter
                    && rf.fold.partition[node_usize] != p
                {
                    continue;
                }
                rows.push(Row {
                    run_id: run.row.boundary_id.clone(),
                    node_id: node,
                    model: rf
                        .fold
                        .models
                        .get(&model_id)
                        .cloned()
                        .unwrap_or_else(|| format!("model#{model_id}")),
                    calls,
                    tokens: (tin, tout),
                    errors: (perr, parse),
                });
            }
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "node_id" => Cell::U32(row.node_id),
            "model" => Cell::Str(row.model.clone()),
            "llm_calls" => Cell::U64(row.calls),
            "token_state" => Cell::Str("available".to_string()),
            "input_tokens" => Cell::U64(row.tokens.0),
            "output_tokens" => Cell::U64(row.tokens.1),
            "provider_errors" => Cell::U64(row.errors.0),
            "parse_errors" => Cell::U64(row.errors.1),
            other => unreachable!("llm_population_v1 column {other}"),
        })
    }

    fn spawn_batch(&self, relation: &RelationDef) -> RecordBatch {
        struct Row {
            run_id: String,
            edge_id: u32,
            parent_node_id: u32,
            child_function_id: u32,
            counts: [u64; 4],
        }
        let mut rows = Vec::new();
        for run in &self.universe.runs {
            let Some(rf) = self.fold_of(run) else {
                continue;
            };
            for (edge_id, &(parent_node, entry_fn, spawned, completed, errored, cancelled)) in
                rf.fold.spawns.iter().enumerate()
            {
                let parent_usize = parent_node as usize;
                if parent_usize >= rf.fold.len() {
                    continue;
                }
                if let Some(p) = rf.partition_filter
                    && rf.fold.partition[parent_usize] != p
                {
                    continue;
                }
                rows.push(Row {
                    run_id: run.row.boundary_id.clone(),
                    edge_id: u32::try_from(edge_id).unwrap_or(u32::MAX),
                    parent_node_id: parent_node,
                    child_function_id: entry_fn,
                    counts: [spawned, completed, errored, cancelled],
                });
            }
        }
        columns!(relation, rows, |row, name| match name {
            "run_id" => Cell::Str(row.run_id.clone()),
            "edge_id" => Cell::U32(row.edge_id),
            "parent_node_id" => Cell::U32(row.parent_node_id),
            "child_function_id" => Cell::U32(row.child_function_id),
            "spawned" => Cell::U64(row.counts[0]),
            "completed" => Cell::U64(row.counts[1]),
            "errored" => Cell::U64(row.counts[2]),
            "cancelled" => Cell::U64(row.counts[3]),
            "running_ns" | "awaiting_ns" | "retained_instances" | "instances_dropped" => Cell::Null,
            other => unreachable!("spawn_edges_v1 column {other}"),
        })
    }
}
