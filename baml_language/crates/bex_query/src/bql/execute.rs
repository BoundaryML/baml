#![cfg_attr(not(feature = "native"), allow(dead_code, unused_imports))]

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value as JsonValue, json};

use super::{
    HARD_MAX_ROWS,
    catalog::{Availability, QueryPlan, ScriptPlan, SetKind, plan, stage_catalog},
    syntax::{CompareOp, Expression, Pipeline, Script, StageCall, Value, bind_params, parse},
};
use crate::{CaptureLoss, Completeness, FileId, QueryError, SourceSnapshot, Watermark};

pub type BqlRow = Map<String, JsonValue>;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotEntry {
    pub file: u64,
    pub generation: u64,
    pub committed_len: u64,
    pub parsed_through: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SnapshotToken {
    pub entries: Vec<SnapshotEntry>,
}

impl SnapshotToken {
    pub fn parse(value: &str) -> Result<Self, QueryError> {
        let payload = value.strip_prefix("bqsnap_1_").ok_or_else(|| {
            QueryError::InvalidRequest("snapshot must start with `bqsnap_1_`".to_owned())
        })?;
        if payload.is_empty() {
            return Ok(Self::default());
        }
        let mut entries = Vec::new();
        for item in payload.split(',') {
            let parts = item.split('-').collect::<Vec<_>>();
            if parts.len() != 4 {
                return Err(QueryError::InvalidRequest(
                    "snapshot entry must contain file-generation-length-parsed".to_owned(),
                ));
            }
            let parse_hex = |part: &str| {
                u64::from_str_radix(part, 16).map_err(|_| {
                    QueryError::InvalidRequest("snapshot contains invalid hex".to_owned())
                })
            };
            entries.push(SnapshotEntry {
                file: parse_hex(parts[0])?,
                generation: parse_hex(parts[1])?,
                committed_len: parse_hex(parts[2])?,
                parsed_through: parse_hex(parts[3])?,
            });
        }
        entries.sort_by_key(|entry| entry.file);
        entries.dedup_by_key(|entry| entry.file);
        Ok(Self { entries })
    }

    #[must_use]
    pub fn encode(&self) -> String {
        let mut entries = self.entries.clone();
        entries.sort_by_key(|entry| entry.file);
        entries.dedup_by_key(|entry| entry.file);
        format!(
            "bqsnap_1_{}",
            entries
                .iter()
                .map(|entry| format!(
                    "{:x}-{:x}-{:x}-{:x}",
                    entry.file, entry.generation, entry.committed_len, entry.parsed_through
                ))
                .collect::<Vec<_>>()
                .join(",")
        )
    }

    #[must_use]
    pub fn get(&self, file: FileId) -> Option<SourceSnapshot> {
        self.entries
            .iter()
            .find(|entry| entry.file == file.0)
            .map(|entry| SourceSnapshot {
                generation: entry.generation,
                committed_len: entry.committed_len,
            })
    }

    fn from_completeness(meta: &Completeness) -> Self {
        let mut entries = meta
            .snapshot
            .iter()
            .map(|watermark| SnapshotEntry {
                file: watermark.file.0,
                generation: watermark.source.generation,
                committed_len: watermark.source.committed_len,
                parsed_through: watermark.parsed_through,
            })
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file);
        entries.dedup_by_key(|entry| entry.file);
        Self { entries }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BqlCursor {
    pub created_ms: u64,
    pub boundary_id: [u8; 16],
}

impl BqlCursor {
    pub fn parse(value: &str) -> Result<Self, QueryError> {
        let payload = value.strip_prefix("bqcur_1_").ok_or_else(|| {
            QueryError::InvalidRequest("cursor must start with `bqcur_1_`".to_owned())
        })?;
        let (created, boundary) = payload
            .split_once('-')
            .ok_or_else(|| QueryError::InvalidRequest("cursor payload is incomplete".to_owned()))?;
        let created_ms = u64::from_str_radix(created, 16)
            .map_err(|_| QueryError::InvalidRequest("cursor timestamp is invalid".to_owned()))?;
        if boundary.len() != 32 {
            return Err(QueryError::InvalidRequest(
                "cursor boundary payload must be 16 bytes".to_owned(),
            ));
        }
        let mut boundary_id = [0_u8; 16];
        for (index, slot) in boundary_id.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&boundary[index * 2..index * 2 + 2], 16)
                .map_err(|_| QueryError::InvalidRequest("cursor boundary is invalid".to_owned()))?;
        }
        Ok(Self {
            created_ms,
            boundary_id,
        })
    }

    #[must_use]
    pub fn encode(self) -> String {
        let boundary = self
            .boundary_id
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("bqcur_1_{:x}-{boundary}", self.created_ms)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryMeta {
    pub complete: bool,
    pub watermarks: Vec<QueryWatermark>,
    pub capture_loss: Vec<QueryCaptureLoss>,
    pub sources_consulted: Vec<u64>,
    pub truncated: bool,
    pub next_cursor: Option<String>,
    pub warnings: Vec<String>,
    pub snapshot: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryWatermark {
    pub wall_epoch_ns: u64,
    pub drained_through_ts_ns: u64,
    pub events_drained: u64,
    pub durable_kind: u8,
    pub reason: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryCaptureLoss {
    pub kind: String,
    pub timestamp_ns: u64,
    pub node_id: Option<u32>,
    pub count: u64,
    pub message: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct QueryEnvelope {
    pub kind: SetKind,
    pub columns: Vec<String>,
    pub rows: Vec<BqlRow>,
    pub meta: QueryMeta,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NamedQueryResult {
    pub name: Option<String>,
    pub result: QueryEnvelope,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScriptResult {
    pub results: Vec<NamedQueryResult>,
}

#[derive(Clone, Debug)]
pub struct ExecuteOptions {
    pub max_rows: usize,
    pub max_bytes: usize,
    pub cursor: Option<BqlCursor>,
    pub snapshot: Option<SnapshotToken>,
    pub params: BTreeMap<String, String>,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            max_rows: super::DEFAULT_LIMIT,
            max_bytes: crate::DEFAULT_MAX_BYTES,
            cursor: None,
            snapshot: None,
            params: BTreeMap::new(),
        }
    }
}

impl ExecuteOptions {
    pub fn validate(&self) -> Result<(), QueryError> {
        if self.max_rows == 0 || self.max_rows > HARD_MAX_ROWS {
            return Err(QueryError::InvalidRequest(format!(
                "max_rows must be in 1..={HARD_MAX_ROWS}"
            )));
        }
        if self.max_bytes < 1024 || self.max_bytes > crate::HARD_MAX_BYTES {
            return Err(QueryError::InvalidRequest(format!(
                "max_bytes must be in 1024..={}",
                crate::HARD_MAX_BYTES
            )));
        }
        Ok(())
    }
}

#[derive(Clone, Debug)]
struct DataSet {
    kind: SetKind,
    rows: Vec<BqlRow>,
    meta: Completeness,
    run_ids: Vec<[u8; 16]>,
    diff_left_run_ids: Vec<[u8; 16]>,
    diff_right_run_ids: Vec<[u8; 16]>,
    series: Vec<BqlRow>,
    spawns: Vec<BqlRow>,
    next_cursor: Option<BqlCursor>,
}

#[derive(Clone, Debug, Default)]
struct MatchedIoSide {
    run_ids: BTreeSet<String>,
    outputs: Vec<String>,
    calls: u64,
}

struct MatchedIoComparison {
    rows: Vec<BqlRow>,
    matched: usize,
    unmatched: usize,
    truncated: bool,
}

fn compare_captured_io(
    left: &BTreeMap<String, MatchedIoSide>,
    right: &BTreeMap<String, MatchedIoSide>,
    max_rows: usize,
) -> MatchedIoComparison {
    let inputs = left
        .keys()
        .chain(right.keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    let matched = inputs
        .iter()
        .filter(|cid| left.contains_key(*cid) && right.contains_key(*cid))
        .count();
    let unmatched = inputs.len().saturating_sub(matched);
    let truncated = inputs.len() > max_rows;
    let rows = inputs
        .into_iter()
        .take(max_rows)
        .map(|input_cid| {
            let left_io = left.get(&input_cid);
            let right_io = right.get(&input_cid);
            let is_matched = left_io.is_some() && right_io.is_some();
            let output_equal = left_io
                .zip(right_io)
                .map(|(left, right)| left.outputs == right.outputs);
            object([
                ("input_cid", json!(input_cid)),
                ("matched_input", json!(is_matched)),
                ("output_equal", json!(output_equal)),
                (
                    "verdict",
                    json!(match (left_io, right_io, output_equal) {
                        (Some(_), Some(_), Some(true)) => "unchanged",
                        (Some(_), Some(_), Some(false)) => "changed",
                        (Some(_), None, _) => "left_only",
                        (None, Some(_), _) => "right_only",
                        _ => "unavailable",
                    }),
                ),
                (
                    "left_run_ids",
                    json!(
                        left_io
                            .map(|side| side.run_ids.iter().cloned().collect::<Vec<_>>())
                            .unwrap_or_default()
                    ),
                ),
                (
                    "right_run_ids",
                    json!(
                        right_io
                            .map(|side| side.run_ids.iter().cloned().collect::<Vec<_>>())
                            .unwrap_or_default()
                    ),
                ),
                (
                    "left_output_cids",
                    json!(left_io.map(|side| side.outputs.clone()).unwrap_or_default()),
                ),
                (
                    "right_output_cids",
                    json!(
                        right_io
                            .map(|side| side.outputs.clone())
                            .unwrap_or_default()
                    ),
                ),
                ("left_calls", json!(left_io.map_or(0, |side| side.calls))),
                ("right_calls", json!(right_io.map_or(0, |side| side.calls))),
            ])
        })
        .collect();
    MatchedIoComparison {
        rows,
        matched,
        unmatched,
        truncated,
    }
}

impl DataSet {
    fn rows(kind: SetKind, rows: Vec<BqlRow>, meta: Completeness) -> Self {
        Self {
            kind,
            rows,
            meta,
            run_ids: Vec::new(),
            diff_left_run_ids: Vec::new(),
            diff_right_run_ids: Vec::new(),
            series: Vec::new(),
            spawns: Vec::new(),
            next_cursor: None,
        }
    }
}

pub fn parse_and_plan(
    source: &str,
    params: &BTreeMap<String, String>,
) -> Result<(Script, ScriptPlan), QueryError> {
    let mut script = parse(source)?;
    bind_params(&mut script, params)?;
    let plan = plan(&script)?;
    Ok((script, plan))
}

#[cfg(feature = "native")]
pub struct NativeBqlEngine {
    roots: Vec<std::path::PathBuf>,
}

#[cfg(feature = "native")]
impl NativeBqlEngine {
    #[must_use]
    pub fn new(roots: Vec<std::path::PathBuf>) -> Self {
        Self { roots }
    }

    pub fn query(&self, source: &str, options: ExecuteOptions) -> Result<ScriptResult, QueryError> {
        options.validate()?;
        let (script, plans) = parse_and_plan(source, &options.params)?;
        let mut results = Vec::new();
        for (statement, (_, plan)) in script.statements.iter().zip(&plans.statements) {
            let dataset = self.execute_pipeline(&statement.pipeline, plan, &options)?;
            results.push(NamedQueryResult {
                name: statement.name.clone(),
                result: self.envelope(dataset, &options)?,
            });
        }
        Ok(ScriptResult { results })
    }

    pub fn explain(
        &self,
        source: &str,
        params: &BTreeMap<String, String>,
    ) -> Result<ScriptPlan, QueryError> {
        parse_and_plan(source, params).map(|(_, plan)| plan)
    }

    fn execute_pipeline(
        &self,
        pipeline: &Pipeline,
        plan: &QueryPlan,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        let mut current = None;
        for (index, call) in pipeline.stages.iter().enumerate() {
            let planned = &plan.stages[index];
            if planned.implicit_run_to_ctx {
                current =
                    Some(self.load_contexts(current.take().expect("planned input"), options)?);
            }
            current = Some(self.execute_stage(call, current, options)?);
        }
        current.ok_or_else(|| QueryError::InvalidRequest("empty BQL pipeline".to_owned()))
    }

    fn execute_stage(
        &self,
        call: &StageCall,
        input: Option<DataSet>,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        let spec = stage_catalog()
            .into_iter()
            .find(|spec| spec.name == call.name)
            .expect("planner resolved stage");
        if spec.availability == Availability::TypedUnavailable {
            return Err(unavailable(
                call,
                &call.name,
                &format!(
                    "`{}` requires an exact/value/cloud source not present in this local artifact; inspect `baml q --schema` for remedies",
                    call.name
                ),
            ));
        }
        match call.name.as_str() {
            "runs" => self.runs(call, options),
            "run" => self.one_run(call, options),
            "ctx" => {
                if call.named("range").is_some()
                    || call.named("align").is_some()
                    || matches!(call.named("rev"), Some(Value::List(_)))
                {
                    return Err(unavailable(
                        call,
                        "ctx",
                        "range/alignment and multi-revision joins require the revision index; use one `rev=` string or omit those arguments",
                    ));
                }
                let runs = self.runs(call, options)?;
                self.load_contexts(runs, options)
            }
            "health" => self.health(call, options),
            "triggers" => self.triggers(call, options),
            "audit" => self.audit(call, options),
            "diff" => self.diff(call, options),
            "calls" => self.calls(input.expect("planned input"), call),
            "errors" => Ok(filter_errors(input.expect("planned input"))),
            "failure" => self.failure(input.expect("planned input"), options),
            "dumps" => self.dumps(input.expect("planned input"), call, options),
            "events" => self.events(input.expect("planned input"), call, options),
            "values" => self.values(input.expect("planned input"), call, options),
            "get" => self.get_values(input.expect("planned input"), call, options),
            "vdiff" => self.vdiff(input.expect("planned input"), call, options),
            "where" => where_stage(input.expect("planned input"), call),
            "limit" => limit_stage(input.expect("planned input"), call, options.max_rows),
            "sort" => sort_stage(input.expect("planned input"), call),
            "select" => select_stage(input.expect("planned input"), call),
            "rollup" => rollup_stage(input.expect("planned input"), call),
            "spawns" => {
                let mut input = input.expect("planned input");
                input.rows = std::mem::take(&mut input.spawns);
                input.kind = SetKind::SpawnSet;
                Ok(input)
            }
            "series" => series_stage(input.expect("planned input"), call),
            "compare" => self.compare_stage(input.expect("planned input"), call, options),
            "top" => top_stage(input.expect("planned input"), call, options.max_rows),
            "stats" => stats_stage(input.expect("planned input"), call),
            "hist" => hist_stage(input.expect("planned input"), call),
            "table" | "tree" | "flame" => {
                let mut input = input.expect("planned input");
                input.kind = SetKind::Table;
                Ok(input)
            }
            "explain" => {
                let mut rows = Vec::new();
                for (index, stage) in stage_catalog().iter().enumerate() {
                    rows.push(object([
                        ("index", json!(index)),
                        ("stage", json!(stage.name)),
                        ("availability", json!(format!("{:?}", stage.availability))),
                    ]));
                }
                Ok(DataSet::rows(
                    SetKind::Table,
                    rows,
                    input.expect("planned input").meta,
                ))
            }
            "completeness" => {
                let input = input.expect("planned input");
                let token = SnapshotToken::from_completeness(&input.meta).encode();
                Ok(DataSet::rows(
                    SetKind::Table,
                    vec![object([
                        ("complete", json!(input.meta.complete)),
                        ("truncated", json!(input.meta.truncated)),
                        ("capture_loss", json!(input.meta.capture_loss.len())),
                        ("snapshot", json!(token)),
                    ])],
                    input.meta,
                ))
            }
            _ => Err(unavailable(
                call,
                &call.name,
                "stage execution is not implemented",
            )),
        }
    }

    fn runs(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        use crate::open_run_meta;
        use bex_events::history::path::list_boundary_dirs;

        let latest = call
            .positional(0)
            .and_then(Value::as_str)
            .is_some_and(|value| value == "latest");
        let current = call
            .positional(0)
            .and_then(Value::as_str)
            .is_some_and(|value| value == "current");
        let requested_limit = call
            .named("limit")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(if latest { 1 } else { options.max_rows })
            .min(options.max_rows)
            .min(1000);
        let status = if current {
            Some("running")
        } else {
            call.named("status").and_then(Value::as_str)
        };
        let revision = call.named("rev").and_then(Value::as_str);
        let minimum_created_ms = call
            .named("last")
            .or_else(|| call.positional(0).filter(|_| !latest && !current))
            .and_then(Value::as_str)
            .and_then(parse_duration_ns)
            .map(|duration_ns| {
                epoch_ms().saturating_sub(duration_ns.saturating_add(999_999) / 1_000_000)
            });
        let mut metas = Vec::new();
        let mut warnings = Vec::new();
        for directory in list_boundary_dirs(&self.roots) {
            let opened = if let Some(pin) = &options.snapshot {
                match open_run_meta(&directory) {
                    Ok(current)
                        if current
                            .meta
                            .snapshot
                            .first()
                            .is_some_and(|source| pin.get(source.file).is_none()) =>
                    {
                        continue;
                    }
                    Ok(_) => open_run_meta_for_options(&directory, options),
                    Err(error) => Err(error),
                }
            } else {
                open_run_meta(&directory)
            };
            match opened {
                Ok(meta) => {
                    if !run_matches_status(&meta.summary, status)
                        || !run_matches_revision(&meta.summary, revision)
                        || minimum_created_ms
                            .is_some_and(|minimum| meta.summary.created_ms < minimum)
                    {
                        continue;
                    }
                    metas.push(meta);
                }
                Err(error) => warnings.push(format!("{}: {error}", directory.display())),
            }
        }
        metas.sort_by_key(|meta| {
            (
                std::cmp::Reverse(meta.summary.created_ms),
                std::cmp::Reverse(meta.summary.boundary_id),
            )
        });
        if let Some(cursor) = options.cursor {
            metas.retain(|meta| {
                (meta.summary.created_ms, meta.summary.boundary_id)
                    < (cursor.created_ms, cursor.boundary_id)
            });
        }
        let truncated = metas.len() > requested_limit;
        metas.truncate(requested_limit);
        let next_cursor = truncated
            .then(|| metas.last())
            .flatten()
            .map(|meta| BqlCursor {
                created_ms: meta.summary.created_ms,
                boundary_id: meta.summary.boundary_id,
            });
        let mut meta = Completeness {
            complete: !truncated && warnings.is_empty(),
            truncated,
            warnings,
            ..Completeness::default()
        };
        let mut rows = Vec::new();
        let mut run_ids = Vec::new();
        for run in metas {
            merge_completeness(&mut meta, &run.meta);
            run_ids.push(run.summary.boundary_id);
            rows.push(run_row(&run.summary));
        }
        meta.finalize();
        Ok(DataSet {
            kind: SetKind::RunSet,
            rows,
            meta,
            run_ids,
            diff_left_run_ids: Vec::new(),
            diff_right_run_ids: Vec::new(),
            series: Vec::new(),
            spawns: Vec::new(),
            next_cursor,
        })
    }

    fn one_run(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};

        let value = call
            .positional(0)
            .or_else(|| call.named("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| QueryError::InvalidRequest("run() requires a boundary id".to_owned()))?;
        let boundary = BoundaryId::from_wire_str(value).ok_or_else(|| {
            QueryError::InvalidRequest("run() requires a valid `baml_id_1_…` id".to_owned())
        })?;
        let directory = find_boundary_dir(&self.roots, boundary)
            .ok_or_else(|| QueryError::NotFound(value.to_owned()))?;
        let run = open_run_meta_for_options(&directory, options)?;
        Ok(DataSet {
            kind: SetKind::RunSet,
            rows: vec![run_row(&run.summary)],
            meta: run.meta,
            run_ids: vec![boundary.as_bytes()],
            diff_left_run_ids: Vec::new(),
            diff_right_run_ids: Vec::new(),
            series: Vec::new(),
            spawns: Vec::new(),
            next_cursor: None,
        })
    }

    fn load_contexts(
        &self,
        runs: DataSet,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use crate::{FileSource, QueryEngine, QueryPoll};
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};

        let source = FileSource::new();
        let engine = QueryEngine::new(source);
        let mut meta = runs.meta;
        let mut rows = Vec::new();
        let mut series = Vec::new();
        let mut spawns = Vec::new();
        for raw in &runs.run_ids {
            let boundary = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, boundary)
                .ok_or_else(|| QueryError::NotFound(boundary.to_wire_string()))?;
            let run = open_run_meta_for_options(&directory, options)?;
            let native = engine.register_native_run(&run)?;
            if let Some(pin) = &options.snapshot {
                for (file, path) in native.files.iter().zip(&native.paths) {
                    let snapshot = pin.get(*file).ok_or_else(|| {
                        snapshot_error(format!(
                            "pinned snapshot does not include CCT source {}",
                            file.0
                        ))
                    })?;
                    engine.source().open_pinned(*file, path, snapshot)?;
                }
            }
            let cct = match engine.open_run(&native.files, native.partition_id)? {
                QueryPoll::Ready(cct) => cct,
                QueryPoll::NeedData { ranges } => {
                    return Err(QueryError::InvalidData(format!(
                        "native CCT source unexpectedly missing {} byte ranges",
                        ranges.len()
                    )));
                }
            };
            merge_completeness(&mut meta, &cct.meta);
            let revision = run
                .summary
                .revision_id
                .map(hex_32)
                .unwrap_or_else(|| "unknown".to_owned());
            for node in cct.nodes.values() {
                rows.push(object([
                    ("run_id", json!(boundary.to_wire_string())),
                    ("revision_id", json!(revision)),
                    ("node_id", json!(node.node_id)),
                    ("parent_node_id", json!(node.parent_node_id)),
                    ("function_id", json!(node.function_id)),
                    ("path", json!(format!("fn#{}", node.function_id))),
                    ("calls", json!(node.counters.enters)),
                    ("errors", json!(node.counters.errors())),
                    ("total_ns", json!(node.counters.total_ns)),
                    ("self_ns", json!(node.counters.self_ns)),
                    ("awaiting_ns", json!(node.counters.await_ns)),
                    ("tokens_in", json!(node.llm.tokens_in)),
                    ("tokens_out", json!(node.llm.tokens_out)),
                    ("duration_buckets", json!(node.duration_buckets)),
                ]));
            }
            for window in &cct.windows {
                series.push(object([
                    ("run_id", json!(boundary.to_wire_string())),
                    ("node_id", json!(window.node_id)),
                    ("bucket_start_ns", json!(window.first_ts_ns)),
                    ("bucket_end_ns", json!(window.last_ts_ns)),
                    ("calls", json!(window.counters.enters)),
                    ("errors", json!(window.counters.errors())),
                    ("total_ns", json!(window.counters.total_ns)),
                    ("self_ns", json!(window.counters.self_ns)),
                    ("awaiting_ns", json!(window.counters.await_ns)),
                ]));
            }
            for edge in cct.spawn_edges.values() {
                spawns.push(object([
                    ("run_id", json!(boundary.to_wire_string())),
                    ("edge_id", json!(edge.edge_id)),
                    ("parent_node", json!(edge.parent_node)),
                    ("entry_fn", json!(edge.entry_fn)),
                    ("spawns", json!(edge.spawns)),
                    ("completed", json!(edge.completed)),
                    ("errored", json!(edge.errored)),
                    ("cancelled", json!(edge.cancelled)),
                    ("running_ns", json!(edge.running_ns)),
                    ("awaiting_ns", json!(edge.awaiting_ns)),
                ]));
            }
        }
        meta.finalize();
        Ok(DataSet {
            kind: SetKind::CtxSet,
            rows,
            meta,
            run_ids: runs.run_ids,
            diff_left_run_ids: runs.diff_left_run_ids,
            diff_right_run_ids: runs.diff_right_run_ids,
            series,
            spawns,
            next_cursor: runs.next_cursor,
        })
    }

    fn health(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        if !call.arguments.is_empty() {
            return Err(unavailable(
                call,
                "health",
                "time-range and process-scoped health needs the session health index; use `health()` for run-level health",
            ));
        }
        let runs = self.runs(call, options)?;
        let rows = runs
            .rows
            .iter()
            .map(|row| {
                object([
                    (
                        "run_id",
                        row.get("run_id").cloned().unwrap_or(JsonValue::Null),
                    ),
                    (
                        "status",
                        row.get("status").cloned().unwrap_or(JsonValue::Null),
                    ),
                    (
                        "complete",
                        json!(row.get("status").and_then(JsonValue::as_str) == Some("complete")),
                    ),
                    (
                        "capture_loss",
                        json!(if runs.meta.capture_loss.is_empty() {
                            0
                        } else {
                            1
                        }),
                    ),
                    ("warnings", json!(&runs.meta.warnings)),
                ])
            })
            .collect();
        Ok(DataSet::rows(SetKind::Table, rows, runs.meta))
    }

    fn triggers(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};
        let runs = self.runs(call, options)?;
        let mut rows = Vec::new();
        let mut meta = runs.meta;
        for raw in &runs.run_ids {
            let id = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, id)
                .ok_or_else(|| QueryError::NotFound(id.to_wire_string()))?;
            let run = open_run_meta_for_options(&directory, options)?;
            merge_completeness(&mut meta, &run.meta);
            for trigger in run.triggers {
                rows.push(object([
                    ("run_id", json!(id.to_wire_string())),
                    ("trigger", json!(trigger.trigger)),
                    ("timestamp_ns", json!(trigger.timestamp_ns)),
                    ("dump_ref", json!(trigger.dump_ref)),
                ]));
            }
        }
        meta.finalize();
        Ok(DataSet::rows(SetKind::Table, rows, meta))
    }

    fn audit(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        use bex_events::{
            history::path::find_boundary_dir,
            ids::BoundaryId,
            value::{ValueAuditRecord, ValueFileRecord, read_bamlvalue_from_bytes},
        };
        let runs = self.runs(call, options)?;
        let mut rows = Vec::new();
        let mut meta = runs.meta;
        for raw in &runs.run_ids {
            let id = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, id)
                .ok_or_else(|| QueryError::NotFound(id.to_wire_string()))?;
            let refs = crate::list_value_refs(
                &directory,
                crate::ValueRefsRequest {
                    max_rows: options.max_rows,
                    max_bytes: options.max_bytes,
                },
            )?;
            merge_completeness(&mut meta, &refs.meta);
            for path in files_with_extension(&directory, "bamlvalue")? {
                let contents = read_bamlvalue_from_bytes(&std::fs::read(path)?)?;
                for record in contents.records {
                    let ValueFileRecord::Audit(audit) = record else {
                        continue;
                    };
                    rows.push(match audit {
                        ValueAuditRecord::CapturePolicyChanged(audit) => object([
                            ("run_id", json!(id.to_wire_string())),
                            ("kind", json!("capture_policy_changed")),
                            ("timestamp_ms", json!(audit.timestamp_ms)),
                            ("scope", json!(audit.scope)),
                            ("trigger", JsonValue::Null),
                            ("records", JsonValue::Null),
                            ("staged_evicted", JsonValue::Null),
                            ("previous_policy", json!(audit.previous_policy)),
                            ("current_policy", json!(audit.current_policy)),
                        ]),
                        ValueAuditRecord::PromotionOccurred(audit) => object([
                            ("run_id", json!(id.to_wire_string())),
                            ("kind", json!("promotion_occurred")),
                            ("timestamp_ms", json!(audit.timestamp_ms)),
                            ("scope", json!(audit.scope)),
                            ("trigger", json!(audit.trigger)),
                            ("records", json!(audit.records)),
                            ("staged_evicted", json!(audit.staged_evicted)),
                            ("previous_policy", JsonValue::Null),
                            ("current_policy", JsonValue::Null),
                        ]),
                    });
                }
            }
        }
        meta.finalize();
        Ok(DataSet::rows(SetKind::Table, rows, meta))
    }

    fn dumps(
        &self,
        mut input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};
        let trigger_filter = call.named("trigger").and_then(Value::as_str);
        let mut rows = Vec::new();
        let mut remaining_bytes = options.max_bytes;
        for raw in &input.run_ids {
            let id = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, id)
                .ok_or_else(|| QueryError::NotFound(id.to_wire_string()))?;
            for path in exact_artifact_files(&directory)? {
                if remaining_bytes == 0 {
                    input.meta.truncated = true;
                    break;
                }
                let (contents, source, bytes_read, budget_truncated) =
                    read_exact_artifact_bounded(&path, remaining_bytes)?;
                remaining_bytes = remaining_bytes.saturating_sub(bytes_read);
                input.meta.sources_consulted.push(source.file);
                input.meta.snapshot.push(source);
                input.meta.partial_tail |= contents.truncated && !budget_truncated;
                input.meta.truncated |= budget_truncated;
                let trigger = contents
                    .header
                    .trigger_reason
                    .clone()
                    .unwrap_or_else(|| "unknown".to_owned());
                if trigger_filter.is_some_and(|filter| !trigger.starts_with(filter)) {
                    continue;
                }
                let relative = path
                    .strip_prefix(&directory)
                    .map_err(|_| QueryError::invalid_data("exact artifact escaped boundary"))?
                    .to_string_lossy()
                    .into_owned();
                rows.push(object([
                    ("run_id", json!(id.to_wire_string())),
                    ("dump_ref", json!(relative)),
                    ("trigger", json!(trigger)),
                    ("events", json!(contents.events.len())),
                    ("truncated", json!(contents.truncated)),
                    ("engine_id", json!(contents.header.engine_id)),
                ]));
                if rows.len() >= options.max_rows {
                    input.meta.truncated = true;
                    break;
                }
                if budget_truncated {
                    break;
                }
            }
        }
        input.meta.finalize();
        let mut dataset = DataSet::rows(SetKind::EventSet, rows, input.meta);
        dataset.run_ids = input.run_ids;
        Ok(dataset)
    }

    fn events(
        &self,
        input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};
        let before = call.named("before").and_then(Value::as_u64).unwrap_or(200);
        let after = call.named("after").and_then(Value::as_u64).unwrap_or(20);
        let requested = usize::try_from(before.saturating_add(after).saturating_add(1))
            .unwrap_or(usize::MAX)
            .min(options.max_rows);
        let mut rows = Vec::new();
        let mut meta = input.meta;
        let mut remaining_bytes = options.max_bytes;
        for dump in &input.rows {
            if remaining_bytes == 0 {
                meta.truncated = true;
                break;
            }
            let Some(run_id) = dump.get("run_id").and_then(JsonValue::as_str) else {
                continue;
            };
            let Some(boundary_id) = BoundaryId::from_wire_str(run_id) else {
                continue;
            };
            let Some(dump_ref) = dump.get("dump_ref").and_then(JsonValue::as_str) else {
                continue;
            };
            let directory = find_boundary_dir(&self.roots, boundary_id)
                .ok_or_else(|| QueryError::NotFound(run_id.to_owned()))?;
            let path = safe_boundary_child(&directory, dump_ref)?;
            let (contents, source, bytes_read, budget_truncated) =
                read_exact_artifact_bounded(&path, remaining_bytes)?;
            remaining_bytes = remaining_bytes.saturating_sub(bytes_read);
            meta.sources_consulted.push(source.file);
            meta.snapshot.push(source);
            let process_id: [u8; 16] = contents
                .header
                .process_id
                .as_slice()
                .try_into()
                .map_err(|_| QueryError::invalid_data("exact artifact process id is invalid"))?;
            let mut artifact_rows = contents
                .events
                .iter()
                .enumerate()
                .map(|(sequence, event)| {
                    exact_event_row(
                        run_id,
                        dump_ref,
                        process_id,
                        contents.header.engine_id,
                        sequence,
                        event,
                    )
                })
                .collect::<Vec<_>>();
            artifact_rows.sort_by_key(|row| {
                (
                    row.get("timestamp_ns")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0),
                    row.get("thread_id")
                        .and_then(JsonValue::as_u64)
                        .unwrap_or(0),
                    row.get("sequence").and_then(JsonValue::as_u64).unwrap_or(0),
                )
            });
            if artifact_rows.len() > requested {
                artifact_rows.drain(..artifact_rows.len() - requested);
                meta.truncated = true;
            }
            rows.extend(artifact_rows);
            meta.partial_tail |= contents.truncated && !budget_truncated;
            meta.truncated |= budget_truncated;
            if rows.len() >= options.max_rows {
                rows.truncate(options.max_rows);
                meta.truncated = true;
                break;
            }
            if budget_truncated {
                break;
            }
        }
        meta.complete &= !meta.truncated && !meta.partial_tail;
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::EventSet, rows, meta);
        dataset.run_ids = input.run_ids;
        Ok(dataset)
    }

    fn values(
        &self,
        input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use bex_events::{
            history::path::find_boundary_dir,
            ids::{BexCallId, BexThreadId, BoundaryId, CallRef, EngineId, ProcessEuid},
        };
        let roles = match call.named("role") {
            Some(Value::List(values)) => values
                .iter()
                .filter_map(Value::as_str)
                .collect::<BTreeSet<_>>(),
            Some(value) => value.as_str().into_iter().collect(),
            None => BTreeSet::new(),
        };
        let mut rows = Vec::new();
        let mut meta = input.meta;
        for raw in &input.run_ids {
            let id = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, id)
                .ok_or_else(|| QueryError::NotFound(id.to_wire_string()))?;
            let response = crate::list_value_refs(
                &directory,
                crate::ValueRefsRequest {
                    max_rows: options.max_rows,
                    max_bytes: options.max_bytes,
                },
            )?;
            merge_completeness(&mut meta, &response.meta);
            for value in response.values {
                if !roles.is_empty() && !roles.contains(value.role.as_str()) {
                    continue;
                }
                let call_id = value.call.map(|call| {
                    CallRef {
                        process_euid: ProcessEuid(call.process_euid),
                        engine_id: EngineId(call.engine_id),
                        thread_id: BexThreadId(call.logical_thread_id),
                        call_id: BexCallId(call.call_id),
                    }
                    .encode()
                });
                rows.push(object([
                    ("run_id", json!(id.to_wire_string())),
                    ("value_ref", json!(value.value_ref_id)),
                    ("role", json!(value.role)),
                    ("availability", json!(value.availability as u8)),
                    ("call", json!(call_id)),
                    ("promotion_trigger", json!(value.promotion_trigger)),
                    ("root_cid", json!(value.root_cid.map(|cid| cid.to_hex()))),
                    ("logical_len", json!(value.logical_len)),
                    ("diagnostic", json!(value.diagnostic)),
                ]));
                if rows.len() >= options.max_rows {
                    meta.truncated = true;
                    break;
                }
            }
        }
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::ValueSet, rows, meta);
        dataset.run_ids = input.run_ids;
        Ok(dataset)
    }

    fn get_values(
        &self,
        input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};
        let max_depth = call
            .named("depth")
            .and_then(Value::as_u64)
            .unwrap_or(1)
            .min(u64::from(u16::MAX)) as u16;
        let mut rows = Vec::new();
        let mut meta = input.meta;
        for value in &input.rows {
            let Some(run_id) = value.get("run_id").and_then(JsonValue::as_str) else {
                continue;
            };
            let Some(boundary_id) = BoundaryId::from_wire_str(run_id) else {
                continue;
            };
            let Some(cid) = value
                .get("root_cid")
                .and_then(JsonValue::as_str)
                .and_then(|value| value.parse().ok())
            else {
                continue;
            };
            let directory = find_boundary_dir(&self.roots, boundary_id)
                .ok_or_else(|| QueryError::NotFound(run_id.to_owned()))?;
            let project_root = boundary_project_root(&directory)?;
            let store = crate::NativeValueStore::open(&directory, &project_root)?;
            let hydration =
                crate::hydrate_value(&store, cid, max_depth, options.max_rows, options.max_bytes)?;
            meta.truncated |= hydration.truncated;
            for node in hydration.nodes {
                rows.push(object([
                    ("run_id", json!(run_id)),
                    ("root_cid", json!(hydration.root.to_hex())),
                    ("cid", json!(node.cid.to_hex())),
                    ("depth", json!(node.depth)),
                    ("logical_len", json!(node.logical_len)),
                    ("canonical_hex", json!(node.canonical_bytes.map(hex_bytes))),
                    (
                        "child_cids",
                        json!(
                            node.child_cids
                                .iter()
                                .map(|cid| cid.to_hex())
                                .collect::<Vec<_>>()
                        ),
                    ),
                ]));
            }
        }
        meta.complete &= !meta.truncated;
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::Table, rows, meta);
        dataset.run_ids = input.run_ids;
        Ok(dataset)
    }

    fn vdiff(
        &self,
        input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId, value_cas::Cid};

        struct PairValueStore {
            left: crate::NativeValueStore,
            right: crate::NativeValueStore,
        }

        impl crate::ValueChunkSource for PairValueStore {
            fn read_chunk(&self, cid: Cid) -> Result<Option<crate::StoredValueChunk>, QueryError> {
                Ok(self.left.read_chunk(cid)?.or(self.right.read_chunk(cid)?))
            }
        }

        let role = call.named("role").and_then(Value::as_str);
        let mut roots = input
            .rows
            .iter()
            .filter(|row| {
                role.is_none_or(|requested| {
                    row.get("role")
                        .and_then(JsonValue::as_str)
                        .is_some_and(|actual| value_role_matches(actual, requested))
                })
            })
            .filter_map(|row| {
                Some((
                    row.get("run_id")?.as_str()?.to_owned(),
                    row.get("root_cid")?.as_str()?.parse::<Cid>().ok()?,
                ))
            });
        let Some((left_run, left_cid)) = roots.next() else {
            return Err(unavailable(
                call,
                "vdiff",
                "no captured value root matches this role; select values with an available root CID",
            ));
        };
        let Some((right_run, right_cid)) = roots.next() else {
            return Err(unavailable(
                call,
                "vdiff",
                "Merkle comparison needs two captured value roots; widen the run/value selection",
            ));
        };
        let left_boundary = BoundaryId::from_wire_str(&left_run)
            .ok_or_else(|| QueryError::invalid_data("left value has an invalid run id"))?;
        let right_boundary = BoundaryId::from_wire_str(&right_run)
            .ok_or_else(|| QueryError::invalid_data("right value has an invalid run id"))?;
        let left_dir = find_boundary_dir(&self.roots, left_boundary)
            .ok_or_else(|| QueryError::NotFound(left_run.clone()))?;
        let right_dir = find_boundary_dir(&self.roots, right_boundary)
            .ok_or_else(|| QueryError::NotFound(right_run.clone()))?;
        let store = PairValueStore {
            left: crate::NativeValueStore::open(&left_dir, &boundary_project_root(&left_dir)?)?,
            right: crate::NativeValueStore::open(&right_dir, &boundary_project_root(&right_dir)?)?,
        };
        let max_nodes = call
            .named("max_nodes")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(1000)
            .min(options.max_rows);
        let diff = crate::diff_values(&store, left_cid, right_cid, max_nodes, options.max_bytes)?;
        let mut rows = diff
            .nodes
            .into_iter()
            .map(|node| {
                object([
                    ("left_run_id", json!(left_run)),
                    ("right_run_id", json!(right_run)),
                    ("left_cid", json!(node.left.map(|cid| cid.to_hex()))),
                    ("right_cid", json!(node.right.map(|cid| cid.to_hex()))),
                    ("equal", json!(node.equal)),
                    (
                        "left_children",
                        json!(
                            node.left_children
                                .iter()
                                .map(|cid| cid.to_hex())
                                .collect::<Vec<_>>()
                        ),
                    ),
                    (
                        "right_children",
                        json!(
                            node.right_children
                                .iter()
                                .map(|cid| cid.to_hex())
                                .collect::<Vec<_>>()
                        ),
                    ),
                    ("resume", json!(false)),
                ])
            })
            .collect::<Vec<_>>();
        rows.extend(diff.resume_pairs.into_iter().map(|(left, right)| {
            object([
                ("left_run_id", json!(left_run)),
                ("right_run_id", json!(right_run)),
                ("left_cid", json!(left.map(|cid| cid.to_hex()))),
                ("right_cid", json!(right.map(|cid| cid.to_hex()))),
                ("equal", JsonValue::Null),
                ("left_children", json!([])),
                ("right_children", json!([])),
                ("resume", json!(true)),
            ])
        }));
        let mut meta = input.meta;
        meta.truncated |= diff.truncated;
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::DiffSet, rows, meta);
        dataset.run_ids = input.run_ids;
        Ok(dataset)
    }

    fn calls(&self, mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
        if let Some(function) = call.named("fn").and_then(Value::as_str) {
            if let Some(id) = function
                .strip_prefix("fn#")
                .and_then(|id| id.parse::<u64>().ok())
            {
                input
                    .rows
                    .retain(|row| row.get("function_id").and_then(JsonValue::as_u64) == Some(id));
            } else {
                return Err(unavailable(
                    call,
                    "calls(fn=)",
                    "function-name filtering needs the revision dictionary; use `fn=\"fn#<id>\"` for this artifact",
                ));
            }
        }
        if call.named("path").is_some() || call.named("kind").is_some() {
            return Err(unavailable(
                call,
                "calls(path=/kind=)",
                "path and LLM-kind filters need revision/model joins not present in this artifact",
            ));
        }
        Ok(input)
    }

    fn failure(&self, input: DataSet, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};
        let mut rows = Vec::new();
        let mut meta = input.meta;
        for raw in &input.run_ids {
            let id = BoundaryId::from_bytes(*raw);
            let directory = find_boundary_dir(&self.roots, id)
                .ok_or_else(|| QueryError::NotFound(id.to_wire_string()))?;
            let run = open_run_meta_for_options(&directory, options)?;
            merge_completeness(&mut meta, &run.meta);
            let complete = run.complete.as_ref();
            rows.push(object([
                ("run_id", json!(id.to_wire_string())),
                ("status", json!(run.summary.completion_status)),
                (
                    "calls",
                    json!(complete.map_or(0, |value| value.counts.calls)),
                ),
                (
                    "errors",
                    json!(complete.map_or(0, |value| value.counts.errors)),
                ),
                ("capture_loss", json!(run.losses.len())),
                ("trigger_count", json!(run.triggers.len())),
                (
                    "diagnostics",
                    json!(complete.map_or(&[][..], |value| value.diagnostics.as_slice())),
                ),
                (
                    "evidence_available",
                    json!(run.summary.has_snapshot || !run.triggers.is_empty()),
                ),
            ]));
        }
        meta.finalize();
        Ok(DataSet::rows(SetKind::Table, rows, meta))
    }

    fn diff(&self, call: &StageCall, options: &ExecuteOptions) -> Result<DataSet, QueryError> {
        let left = nested_stage(
            call.positional(0).or_else(|| call.named("left")),
            call,
            "left",
        )?;
        let right = nested_stage(
            call.positional(1).or_else(|| call.named("right")),
            call,
            "right",
        )?;
        let left_plan = plan(&Script {
            source: left.name.clone(),
            statements: vec![super::syntax::Statement {
                name: None,
                pipeline: Pipeline {
                    stages: vec![left.clone()],
                    span: left.span,
                },
            }],
        })?;
        let right_plan = plan(&Script {
            source: right.name.clone(),
            statements: vec![super::syntax::Statement {
                name: None,
                pipeline: Pipeline {
                    stages: vec![right.clone()],
                    span: right.span,
                },
            }],
        })?;
        let mut left_data = self.execute_pipeline(
            &Pipeline {
                stages: vec![left.clone()],
                span: left.span,
            },
            &left_plan.statements[0].1,
            options,
        )?;
        let mut right_data = self.execute_pipeline(
            &Pipeline {
                stages: vec![right.clone()],
                span: right.span,
            },
            &right_plan.statements[0].1,
            options,
        )?;
        if left_data.kind == SetKind::RunSet {
            left_data = self.load_contexts(left_data, options)?;
        }
        if right_data.kind == SetKind::RunSet {
            right_data = self.load_contexts(right_data, options)?;
        }
        let left_run_ids = left_data.run_ids.clone();
        let right_run_ids = right_data.run_ids.clone();
        let left_revs = distinct_strings(&left_data.rows, "revision_id");
        let right_revs = distinct_strings(&right_data.rows, "revision_id");
        let same_single_revision = left_revs.len() == 1 && left_revs == right_revs;
        if !same_single_revision {
            if left_revs.len() != 1 || right_revs.len() != 1 {
                return Err(unavailable(
                    call,
                    "diff(align=fqn)",
                    "each side must select exactly one revision before stable FQN alignment; add \
                     `rev=` to both nested sources",
                ));
            }
            let left_dictionary = self.dictionary_for_dataset(&left_data, options, call)?;
            let right_dictionary = self.dictionary_for_dataset(&right_data, options, call)?;
            let response = crate::diff_cct(
                &folded_cct_from_rows(&left_data),
                &left_dictionary,
                &folded_cct_from_rows(&right_data),
                &right_dictionary,
                crate::DiffRequest {
                    max_rows: options.max_rows.min(10_000),
                    max_bytes: options.max_bytes,
                },
            )?;
            let rows = response
                .rows
                .into_iter()
                .map(|row| {
                    object([
                        ("definition_key", json!(row.definition_key)),
                        ("fqn", json!(row.fqn)),
                        ("left_function_id", json!(row.left_function_id)),
                        ("right_function_id", json!(row.right_function_id)),
                        (
                            "presence",
                            json!(match row.presence {
                                crate::DiffPresence::Both => "both",
                                crate::DiffPresence::Added => "added",
                                crate::DiffPresence::Removed => "removed",
                            }),
                        ),
                        ("definition_changed", json!(row.definition_changed)),
                        ("left_calls", json!(row.left.enters)),
                        ("right_calls", json!(row.right.enters)),
                        ("delta_calls", json!(row.delta.calls)),
                        ("left_errors", json!(row.left.errors())),
                        ("right_errors", json!(row.right.errors())),
                        ("delta_errors", json!(row.delta.errors)),
                        ("left_self_ns", json!(row.left.self_ns)),
                        ("right_self_ns", json!(row.right.self_ns)),
                        ("delta_self_ns", json!(row.delta.self_ns)),
                        ("left_awaiting_ns", json!(row.left.await_ns)),
                        ("right_awaiting_ns", json!(row.right.await_ns)),
                        ("delta_awaiting_ns", json!(row.delta.awaiting_ns)),
                    ])
                })
                .collect();
            let mut dataset = DataSet::rows(SetKind::DiffSet, rows, response.meta);
            dataset.run_ids = left_run_ids.iter().chain(&right_run_ids).copied().collect();
            dataset.diff_left_run_ids = left_run_ids;
            dataset.diff_right_run_ids = right_run_ids;
            return Ok(dataset);
        }
        let left = aggregate_by_function(&left_data.rows);
        let right = aggregate_by_function(&right_data.rows);
        let keys = left
            .keys()
            .chain(right.keys())
            .copied()
            .collect::<BTreeSet<_>>();
        let rows = keys
            .into_iter()
            .map(|function_id| {
                let left = left.get(&function_id).copied().unwrap_or_default();
                let right = right.get(&function_id).copied().unwrap_or_default();
                object([
                    ("function_id", json!(function_id)),
                    ("left_calls", json!(left.calls)),
                    ("right_calls", json!(right.calls)),
                    ("delta_calls", json!(signed_delta(right.calls, left.calls))),
                    ("left_errors", json!(left.errors)),
                    ("right_errors", json!(right.errors)),
                    (
                        "delta_errors",
                        json!(signed_delta(right.errors, left.errors)),
                    ),
                    ("left_self_ns", json!(left.self_ns)),
                    ("right_self_ns", json!(right.self_ns)),
                    (
                        "delta_self_ns",
                        json!(signed_delta(right.self_ns, left.self_ns)),
                    ),
                    ("left_awaiting_ns", json!(left.awaiting_ns)),
                    ("right_awaiting_ns", json!(right.awaiting_ns)),
                    (
                        "delta_awaiting_ns",
                        json!(signed_delta(right.awaiting_ns, left.awaiting_ns)),
                    ),
                ])
            })
            .collect();
        let mut meta = left_data.meta;
        merge_completeness(&mut meta, &right_data.meta);
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::DiffSet, rows, meta);
        dataset.run_ids = left_run_ids.iter().chain(&right_run_ids).copied().collect();
        dataset.diff_left_run_ids = left_run_ids;
        dataset.diff_right_run_ids = right_run_ids;
        Ok(dataset)
    }

    fn compare_stage(
        &self,
        mut input: DataSet,
        call: &StageCall,
        options: &ExecuteOptions,
    ) -> Result<DataSet, QueryError> {
        if !call
            .named("match_io")
            .and_then(Value::as_bool)
            .unwrap_or(false)
        {
            input.kind = SetKind::DiffSet;
            return Ok(input);
        }
        if input.diff_left_run_ids.is_empty() || input.diff_right_run_ids.is_empty() {
            return Err(unavailable(
                call,
                "compare(match_io=true)",
                "matched-I/O comparison requires the output of diff(left, right)",
            ));
        }

        let mut meta = input.meta;
        let left = self.captured_io_by_input(&input.diff_left_run_ids, options, &mut meta)?;
        let right = self.captured_io_by_input(&input.diff_right_run_ids, options, &mut meta)?;
        let comparison = compare_captured_io(&left, &right, options.max_rows);
        let matched = comparison.matched;
        if matched == 0 {
            return Err(unavailable(
                call,
                "compare(match_io=true)",
                "no byte-identical captured input CID occurs on both sides",
            ));
        }

        let unmatched = comparison.unmatched;
        if unmatched != 0 {
            meta.warnings.push(format!(
                "{unmatched} captured input CID(s) occurred on only one side"
            ));
        }
        meta.truncated |= comparison.truncated;
        meta.finalize();
        let mut dataset = DataSet::rows(SetKind::DiffSet, comparison.rows, meta);
        dataset.run_ids = input.run_ids;
        dataset.diff_left_run_ids = input.diff_left_run_ids;
        dataset.diff_right_run_ids = input.diff_right_run_ids;
        Ok(dataset)
    }

    fn captured_io_by_input(
        &self,
        run_ids: &[[u8; 16]],
        options: &ExecuteOptions,
        meta: &mut Completeness,
    ) -> Result<BTreeMap<String, MatchedIoSide>, QueryError> {
        use bex_events::{history::path::find_boundary_dir, ids::BoundaryId};

        #[derive(Default)]
        struct CallIo {
            run_id: String,
            input: Option<String>,
            outputs: Vec<String>,
        }

        let mut calls = BTreeMap::<([u8; 16], [u8; 16], u64, u64, u64), CallIo>::new();
        for raw in run_ids {
            let boundary = BoundaryId::from_bytes(*raw);
            let run_id = boundary.to_wire_string();
            let directory = find_boundary_dir(&self.roots, boundary)
                .ok_or_else(|| QueryError::NotFound(run_id.clone()))?;
            let response = crate::list_value_refs(
                &directory,
                crate::ValueRefsRequest {
                    max_rows: options.max_rows,
                    max_bytes: options.max_bytes,
                },
            )?;
            merge_completeness(meta, &response.meta);
            for value in response.values {
                let (Some(call), Some(cid)) = (value.call, value.root_cid) else {
                    continue;
                };
                let key = (
                    *raw,
                    call.process_euid,
                    call.engine_id,
                    call.logical_thread_id,
                    call.call_id,
                );
                let entry = calls.entry(key).or_default();
                entry.run_id.clone_from(&run_id);
                match value.role.as_str() {
                    "rootInput" | "callInput" => entry.input = Some(cid.to_hex()),
                    "rootOutput" | "callOutput" => {
                        entry.outputs.push(format!("ok:{}", cid.to_hex()));
                    }
                    "rootError" | "callError" => {
                        entry.outputs.push(format!("error:{}", cid.to_hex()));
                    }
                    _ => {}
                }
            }
        }

        let mut by_input = BTreeMap::<String, MatchedIoSide>::new();
        for call in calls.into_values() {
            let Some(input) = call.input else {
                continue;
            };
            if call.outputs.is_empty() {
                continue;
            }
            let side = by_input.entry(input).or_default();
            side.run_ids.insert(call.run_id);
            side.calls = side.calls.saturating_add(1);
            side.outputs.extend(call.outputs);
        }
        for side in by_input.values_mut() {
            side.outputs.sort();
        }
        Ok(by_input)
    }

    fn dictionary_for_dataset(
        &self,
        dataset: &DataSet,
        options: &ExecuteOptions,
        call: &StageCall,
    ) -> Result<crate::FunctionDictionary, QueryError> {
        use bex_events::{
            history::path::find_boundary_dir,
            ids::BoundaryId,
            revision_dictionary::file::{DictionaryReadError, RevisionDictionaryStore},
        };

        let raw = dataset.run_ids.first().copied().ok_or_else(|| {
            unavailable(
                call,
                "diff(align=fqn)",
                "the selected side has no runs from which to resolve a revision dictionary",
            )
        })?;
        let boundary = BoundaryId::from_bytes(raw);
        let directory = find_boundary_dir(&self.roots, boundary)
            .ok_or_else(|| QueryError::NotFound(boundary.to_wire_string()))?;
        let run = open_run_meta_for_options(&directory, options)?;
        let revision_bytes = run.summary.revision_id.ok_or_else(|| {
            unavailable(
                call,
                "diff(align=fqn)",
                "the selected run has no persisted revision id",
            )
        })?;
        let revision_id = bex_events::revision_dictionary::RevisionId::from_bytes(revision_bytes);
        let project_root = run
            .boundary_dir
            .parent()
            .and_then(std::path::Path::parent)
            .and_then(std::path::Path::parent)
            .ok_or_else(|| {
                QueryError::InvalidData("boundary directory is not under .baml/history".to_owned())
            })?;
        let dictionary = RevisionDictionaryStore::new(project_root)
            .read(revision_id)
            .map_err(|error| match error {
                DictionaryReadError::DictionaryMissing { .. } => unavailable(
                    call,
                    "diff(align=fqn)",
                    &format!(
                        "run {} references {revision_id}, but its .bamldict artifact is missing; \
                         recompile the same revision to regenerate it",
                        run.summary.boundary_id_wire
                    ),
                ),
                DictionaryReadError::InvalidData(error) => QueryError::InvalidData(format!(
                    "revision dictionary {revision_id} failed validation: {error}"
                )),
                DictionaryReadError::Io(error) => QueryError::Io(error),
            })?;
        Ok(crate::FunctionDictionary::from_revision_dictionary(
            &dictionary,
        ))
    }

    fn envelope(
        &self,
        mut dataset: DataSet,
        options: &ExecuteOptions,
    ) -> Result<QueryEnvelope, QueryError> {
        let hard_rows = options.max_rows.min(HARD_MAX_ROWS);
        if dataset.rows.len() > hard_rows {
            dataset.rows.truncate(hard_rows);
            dataset.meta.truncated = true;
        }
        while serde_json::to_vec(&dataset.rows)
            .map_err(|error| QueryError::InvalidData(error.to_string()))?
            .len()
            > options.max_bytes
        {
            if dataset.rows.is_empty() {
                return Err(QueryError::BudgetExceeded {
                    required: options.max_bytes.saturating_add(1),
                    max_bytes: options.max_bytes,
                });
            }
            dataset.rows.pop();
            dataset.meta.truncated = true;
        }
        if dataset.meta.truncated && dataset.kind == SetKind::RunSet {
            dataset.next_cursor = cursor_from_last_run(&dataset.rows);
        }
        dataset.meta.finalize();
        let snapshot = SnapshotToken::from_completeness(&dataset.meta);
        if let Some(pin) = &options.snapshot {
            let touched = snapshot
                .entries
                .iter()
                .map(|entry| entry.file)
                .collect::<BTreeSet<_>>();
            for entry in &pin.entries {
                if touched.contains(&entry.file)
                    && snapshot.entries.iter().find(|item| item.file == entry.file) != Some(entry)
                {
                    return Err(snapshot_error(format!(
                        "source {} no longer matches the pinned watermark",
                        entry.file
                    )));
                }
            }
        }
        let columns = dataset
            .rows
            .iter()
            .flat_map(|row| row.keys().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(QueryEnvelope {
            // Every BQL pipeline has an implicit terminal table projection.
            kind: SetKind::Table,
            columns,
            rows: dataset.rows,
            meta: query_meta(dataset.meta, dataset.next_cursor, snapshot),
        })
    }
}

fn query_meta(
    meta: Completeness,
    next_cursor: Option<BqlCursor>,
    snapshot: SnapshotToken,
) -> QueryMeta {
    QueryMeta {
        complete: meta.complete,
        watermarks: meta
            .watermarks
            .into_iter()
            .map(QueryWatermark::from)
            .collect(),
        capture_loss: meta
            .capture_loss
            .into_iter()
            .map(QueryCaptureLoss::from)
            .collect(),
        sources_consulted: meta
            .sources_consulted
            .into_iter()
            .map(|file| file.0)
            .collect(),
        truncated: meta.truncated,
        next_cursor: next_cursor.map(BqlCursor::encode),
        warnings: meta.warnings,
        snapshot: snapshot.encode(),
    }
}

impl From<Watermark> for QueryWatermark {
    fn from(value: Watermark) -> Self {
        Self {
            wall_epoch_ns: value.wall_epoch_ns,
            drained_through_ts_ns: value.drained_through_ts_ns,
            events_drained: value.events_drained,
            durable_kind: value.durable_kind,
            reason: value.reason,
        }
    }
}

impl From<CaptureLoss> for QueryCaptureLoss {
    fn from(value: CaptureLoss) -> Self {
        Self {
            kind: format!("{:?}", value.kind),
            timestamp_ns: value.timestamp_ns,
            node_id: value.node_id,
            count: value.count,
            message: value.message,
        }
    }
}

fn filter_errors(mut input: DataSet) -> DataSet {
    input
        .rows
        .retain(|row| numeric(row.get("errors")).is_some_and(|errors| errors > 0.0));
    sync_run_ids(&mut input);
    input
}

fn where_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    let expression = match call.positional(0) {
        Some(Value::Expr(expression)) => expression,
        _ => {
            return Err(QueryError::InvalidRequest(
                "where() expects a comparison such as `errors > 0`".to_owned(),
            ));
        }
    };
    let valid = input
        .rows
        .iter()
        .flat_map(|row| row.keys().cloned())
        .collect::<BTreeSet<_>>();
    if !input.rows.is_empty() && !valid.contains(&expression.field) {
        let mut error = unavailable(
            call,
            "where",
            &format!("unknown field `{}`", expression.field),
        );
        if let QueryError::Bql(diagnostic) = &mut error {
            diagnostic.code = "E_UNKNOWN_FIELD";
            diagnostic.valid = valid.into_iter().collect();
        }
        return Err(error);
    }
    input
        .rows
        .retain(|row| compare_json(row.get(&expression.field), expression));
    sync_run_ids(&mut input);
    Ok(input)
}

fn limit_stage(
    mut input: DataSet,
    call: &StageCall,
    max_rows: usize,
) -> Result<DataSet, QueryError> {
    let limit = call
        .positional(0)
        .or_else(|| call.named("rows"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| QueryError::InvalidRequest("limit() requires an integer".to_owned()))?
        .min(max_rows);
    if input.rows.len() > limit {
        input.rows.truncate(limit);
        input.meta.truncated = true;
        if input.kind == SetKind::RunSet {
            input.next_cursor = cursor_from_last_run(&input.rows);
        }
    }
    sync_run_ids(&mut input);
    Ok(input)
}

fn sort_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    let field = call
        .named("by")
        .or_else(|| call.positional(0))
        .and_then(Value::as_str)
        .ok_or_else(|| QueryError::InvalidRequest("sort() requires `by=`".to_owned()))?;
    ensure_field(&input, field, call)?;
    let descending = call.named("order").and_then(Value::as_str) != Some("asc");
    input.rows.sort_by(|left, right| {
        let ordering = compare_order(left.get(field), right.get(field));
        if descending {
            ordering.reverse()
        } else {
            ordering
        }
    });
    sync_run_ids(&mut input);
    Ok(input)
}

fn select_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    let fields = call
        .arguments
        .iter()
        .flat_map(|argument| match &argument.value {
            Value::List(values) => values.iter().filter_map(Value::as_str).collect::<Vec<_>>(),
            value => value.as_str().into_iter().collect(),
        })
        .collect::<Vec<_>>();
    if fields.is_empty() {
        return Err(QueryError::InvalidRequest(
            "select() requires at least one field".to_owned(),
        ));
    }
    if let Some(field) = fields.iter().find(|field| {
        !input.rows.is_empty() && !input.rows.iter().any(|row| row.contains_key(**field))
    }) {
        ensure_field(&input, field, call)?;
    }
    for row in &mut input.rows {
        row.retain(|key, _| fields.contains(&key.as_str()));
    }
    Ok(input)
}

fn rollup_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    if call.named("by").and_then(Value::as_str).unwrap_or("fn") != "fn" {
        return Err(unavailable(
            call,
            "rollup",
            "this artifact has function ids but no revision dictionary for path/file/package grouping; use `by=fn`",
        ));
    }
    let aggregates = aggregate_by_function(&input.rows);
    input.rows = aggregates
        .into_iter()
        .map(|(function_id, metric)| {
            object([
                ("function_id", json!(function_id)),
                ("path", json!(format!("fn#{function_id}"))),
                ("calls", json!(metric.calls)),
                ("errors", json!(metric.errors)),
                ("total_ns", json!(metric.total_ns)),
                ("self_ns", json!(metric.self_ns)),
                ("awaiting_ns", json!(metric.awaiting_ns)),
            ])
        })
        .collect();
    Ok(input)
}

fn top_stage(mut input: DataSet, call: &StageCall, max_rows: usize) -> Result<DataSet, QueryError> {
    let k = call
        .positional(0)
        .or_else(|| call.named("k"))
        .and_then(Value::as_u64)
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(10)
        .min(max_rows);
    let field = call
        .named("by")
        .and_then(Value::as_str)
        .unwrap_or("total_ns");
    ensure_field(&input, field, call)?;
    input
        .rows
        .sort_by(|left, right| compare_order(right.get(field), left.get(field)));
    if input.rows.len() > k {
        input.rows.truncate(k);
        input.meta.truncated = true;
    }
    sync_run_ids(&mut input);
    input.kind = SetKind::Table;
    Ok(input)
}

#[derive(Clone, Debug)]
enum StatsAggregate {
    Count,
    Sum(String),
    Min(String),
    Max(String),
    Avg(String),
}

#[derive(Clone, Copy, Debug, Default)]
struct StatsAccumulator {
    count: u64,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
}

fn stats_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    let group_fields = call
        .named("by")
        .map(value_strings)
        .transpose()?
        .unwrap_or_default();
    for field in &group_fields {
        ensure_field(&input, field, call)?;
    }

    let aggregates = stats_aggregates(call)?;
    for (_, aggregate) in &aggregates {
        if let Some(field) = aggregate.field() {
            ensure_field(&input, field, call)?;
        }
    }

    let mut groups = BTreeMap::<String, (Vec<JsonValue>, Vec<StatsAccumulator>)>::new();
    if group_fields.is_empty() {
        groups.insert(
            "[]".to_owned(),
            (
                Vec::new(),
                vec![StatsAccumulator::default(); aggregates.len()],
            ),
        );
    }
    for row in &input.rows {
        let values = group_fields
            .iter()
            .map(|field| row.get(field).cloned().unwrap_or(JsonValue::Null))
            .collect::<Vec<_>>();
        let key = serde_json::to_string(&values)
            .map_err(|error| QueryError::InvalidData(error.to_string()))?;
        let (_, states) = groups
            .entry(key)
            .or_insert_with(|| (values, vec![StatsAccumulator::default(); aggregates.len()]));
        for ((_, aggregate), state) in aggregates.iter().zip(states) {
            state.observe(aggregate, row);
        }
    }

    input.rows = groups
        .into_values()
        .map(|(group_values, states)| {
            let mut row = group_fields
                .iter()
                .cloned()
                .zip(group_values)
                .collect::<BqlRow>();
            for ((name, aggregate), state) in aggregates.iter().zip(states) {
                row.insert(name.clone(), state.result(aggregate));
            }
            row
        })
        .collect();
    input.kind = SetKind::Table;
    input.run_ids.clear();
    input.series.clear();
    input.spawns.clear();
    input.next_cursor = None;
    Ok(input)
}

impl StatsAggregate {
    fn field(&self) -> Option<&str> {
        match self {
            Self::Count => None,
            Self::Sum(field) | Self::Min(field) | Self::Max(field) | Self::Avg(field) => {
                Some(field)
            }
        }
    }
}

impl StatsAccumulator {
    fn observe(&mut self, aggregate: &StatsAggregate, row: &BqlRow) {
        if matches!(aggregate, StatsAggregate::Count) {
            self.count = self.count.saturating_add(1);
            return;
        }
        let Some(value) = aggregate.field().and_then(|field| numeric(row.get(field))) else {
            return;
        };
        self.count = self.count.saturating_add(1);
        self.sum += value;
        self.min = Some(self.min.map_or(value, |current| current.min(value)));
        self.max = Some(self.max.map_or(value, |current| current.max(value)));
    }

    fn result(self, aggregate: &StatsAggregate) -> JsonValue {
        match aggregate {
            StatsAggregate::Count => json!(self.count),
            StatsAggregate::Sum(_) => stats_number(self.sum),
            StatsAggregate::Min(_) => self.min.map_or(JsonValue::Null, stats_number),
            StatsAggregate::Max(_) => self.max.map_or(JsonValue::Null, stats_number),
            StatsAggregate::Avg(_) => {
                if self.count == 0 {
                    JsonValue::Null
                } else {
                    stats_number(self.sum / self.count as f64)
                }
            }
        }
    }
}

fn stats_aggregates(call: &StageCall) -> Result<Vec<(String, StatsAggregate)>, QueryError> {
    let mut aggregates = Vec::new();
    for argument in &call.arguments {
        if argument.name.as_deref() == Some("by") {
            continue;
        }
        let values = match &argument.value {
            Value::List(values) if argument.name.as_deref() == Some("aggs") => values.as_slice(),
            value => std::slice::from_ref(value),
        };
        for value in values {
            let Value::Stage(stage) = value else {
                return Err(QueryError::InvalidRequest(
                    "stats() aggregates must use count(), sum(field), min(field), max(field), or \
                     avg(field)"
                        .to_owned(),
                ));
            };
            let aggregate = parse_stats_aggregate(stage)?;
            let base_name = argument
                .name
                .as_deref()
                .filter(|name| *name != "aggs")
                .map(str::to_owned)
                .unwrap_or_else(|| aggregate.default_name());
            let mut name = base_name.clone();
            let mut suffix = 2;
            while aggregates.iter().any(|(existing, _)| existing == &name) {
                name = format!("{base_name}_{suffix}");
                suffix += 1;
            }
            aggregates.push((name, aggregate));
        }
    }
    if aggregates.is_empty() {
        aggregates.push(("count".to_owned(), StatsAggregate::Count));
    }
    Ok(aggregates)
}

fn parse_stats_aggregate(stage: &StageCall) -> Result<StatsAggregate, QueryError> {
    let field = || {
        stage
            .positional(0)
            .or_else(|| stage.named("field"))
            .and_then(Value::as_str)
            .map(str::to_owned)
            .ok_or_else(|| {
                QueryError::InvalidRequest(format!("{}() requires one numeric field", stage.name))
            })
    };
    match stage.name.as_str() {
        "count" if stage.arguments.is_empty() => Ok(StatsAggregate::Count),
        "sum" => field().map(StatsAggregate::Sum),
        "min" => field().map(StatsAggregate::Min),
        "max" => field().map(StatsAggregate::Max),
        "avg" => field().map(StatsAggregate::Avg),
        _ => Err(QueryError::InvalidRequest(format!(
            "unsupported stats aggregate `{}`; expected count(), sum(field), min(field), \
             max(field), or avg(field)",
            stage.name
        ))),
    }
}

impl StatsAggregate {
    fn default_name(&self) -> String {
        match self {
            Self::Count => "count".to_owned(),
            Self::Sum(field) => format!("sum_{field}"),
            Self::Min(field) => format!("min_{field}"),
            Self::Max(field) => format!("max_{field}"),
            Self::Avg(field) => format!("avg_{field}"),
        }
    }
}

fn value_strings(value: &Value) -> Result<Vec<String>, QueryError> {
    match value {
        Value::List(values) => values
            .iter()
            .map(|value| {
                value.as_str().map(str::to_owned).ok_or_else(|| {
                    QueryError::InvalidRequest(
                        "stats(by=) accepts a field or list of fields".to_owned(),
                    )
                })
            })
            .collect(),
        value => value
            .as_str()
            .map(|value| vec![value.to_owned()])
            .ok_or_else(|| {
                QueryError::InvalidRequest(
                    "stats(by=) accepts a field or list of fields".to_owned(),
                )
            }),
    }
}

fn stats_number(value: f64) -> JsonValue {
    if value.is_finite() && value.fract() == 0.0 && value >= 0.0 && value <= u64::MAX as f64 {
        json!(value as u64)
    } else if value.is_finite()
        && value.fract() == 0.0
        && value >= i64::MIN as f64
        && value <= i64::MAX as f64
    {
        json!(value as i64)
    } else {
        serde_json::Number::from_f64(value).map_or(JsonValue::Null, JsonValue::Number)
    }
}

fn hist_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    if call
        .named("metric")
        .or_else(|| call.positional(0))
        .and_then(Value::as_str)
        .unwrap_or("total_ns")
        != "total_ns"
    {
        return Err(unavailable(
            call,
            "hist",
            "the v1 CCT histogram is defined for total call duration; use `hist(total_ns)`",
        ));
    }
    let mut buckets = BTreeMap::<u64, u64>::new();
    for row in &input.rows {
        if let Some(values) = row.get("duration_buckets").and_then(JsonValue::as_array) {
            for (bucket, count) in values.iter().enumerate() {
                *buckets.entry(bucket as u64).or_default() += count.as_u64().unwrap_or(0);
            }
        }
    }
    input.rows = buckets
        .into_iter()
        .map(|(bucket, count)| object([("bucket_log2_ns", json!(bucket)), ("count", json!(count))]))
        .collect();
    input.kind = SetKind::Table;
    Ok(input)
}

fn series_stage(mut input: DataSet, call: &StageCall) -> Result<DataSet, QueryError> {
    let bucket_ns = call
        .named("bucket")
        .or_else(|| call.positional(0))
        .and_then(Value::as_str)
        .and_then(parse_duration_ns)
        .filter(|value| *value > 0)
        .ok_or_else(|| {
            QueryError::InvalidRequest(
                "series() requires a non-zero duration such as `bucket=15m`".to_owned(),
            )
        })?;
    let metrics = match call.named("metrics") {
        Some(Value::List(metrics)) => metrics,
        _ => {
            return Err(QueryError::InvalidRequest(
                "series() requires `metrics=[...]`".to_owned(),
            ));
        }
    };
    const AVAILABLE: &[&str] = &[
        "calls",
        "errors",
        "total_ns",
        "self_ns",
        "awaiting_ns",
        "mean_awaiting_ns",
    ];
    for metric in metrics {
        match metric {
            Value::Stage(stage) => {
                return Err(unavailable(
                    call,
                    &stage.name,
                    "window percentile series need time-bucket histogram blocks; use `hist(total_ns)` for the run aggregate",
                ));
            }
            value
                if value
                    .as_str()
                    .is_some_and(|metric| AVAILABLE.contains(&metric)) => {}
            value => {
                let field = value.as_str().unwrap_or("<expression>");
                let mut error = unavailable(
                    call,
                    "series",
                    &format!("unknown or unsupported metric `{field}`"),
                );
                if let QueryError::Bql(diagnostic) = &mut error {
                    diagnostic.code = "E_UNKNOWN_FIELD";
                    diagnostic.valid = AVAILABLE.iter().map(ToString::to_string).collect();
                }
                return Err(error);
            }
        }
    }
    let mut grouped = BTreeMap::<(String, u64, u64), Metric>::new();
    for row in &input.series {
        let run = row
            .get("run_id")
            .and_then(JsonValue::as_str)
            .unwrap_or_default()
            .to_owned();
        let node = row.get("node_id").and_then(JsonValue::as_u64).unwrap_or(0);
        let timestamp = row
            .get("bucket_start_ns")
            .and_then(JsonValue::as_u64)
            .unwrap_or(0);
        let start = timestamp / bucket_ns * bucket_ns;
        let metric = grouped.entry((run, node, start)).or_default();
        metric.calls = metric
            .calls
            .saturating_add(row.get("calls").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.errors = metric
            .errors
            .saturating_add(row.get("errors").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.total_ns = metric
            .total_ns
            .saturating_add(row.get("total_ns").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.self_ns = metric
            .self_ns
            .saturating_add(row.get("self_ns").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.awaiting_ns = metric.awaiting_ns.saturating_add(
            row.get("awaiting_ns")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0),
        );
    }
    input.rows = grouped
        .into_iter()
        .map(|((run_id, node_id, start), metric)| {
            object([
                ("run_id", json!(run_id)),
                ("node_id", json!(node_id)),
                ("bucket_start_ns", json!(start)),
                ("bucket_end_ns", json!(start.saturating_add(bucket_ns))),
                ("calls", json!(metric.calls)),
                ("errors", json!(metric.errors)),
                ("total_ns", json!(metric.total_ns)),
                ("self_ns", json!(metric.self_ns)),
                ("awaiting_ns", json!(metric.awaiting_ns)),
                (
                    "mean_awaiting_ns",
                    json!(metric.awaiting_ns.checked_div(metric.calls).unwrap_or(0)),
                ),
            ])
        })
        .collect();
    input.kind = SetKind::SeriesSet;
    Ok(input)
}

fn ensure_field(input: &DataSet, field: &str, call: &StageCall) -> Result<(), QueryError> {
    let valid = input
        .rows
        .iter()
        .flat_map(|row| row.keys().cloned())
        .collect::<BTreeSet<_>>();
    if input.rows.is_empty() || valid.contains(field) {
        return Ok(());
    }
    let mut error = unavailable(call, &call.name, &format!("unknown field `{field}`"));
    if let QueryError::Bql(diagnostic) = &mut error {
        diagnostic.code = "E_UNKNOWN_FIELD";
        diagnostic.valid = valid.into_iter().collect();
    }
    Err(error)
}

fn compare_json(value: Option<&JsonValue>, expression: &Expression) -> bool {
    let right = bql_value_json(&expression.value);
    match (numeric(value), numeric(Some(&right))) {
        (Some(left), Some(right)) => match expression.op {
            CompareOp::Eq => left == right,
            CompareOp::Ne => left != right,
            CompareOp::Gt => left > right,
            CompareOp::Ge => left >= right,
            CompareOp::Lt => left < right,
            CompareOp::Le => left <= right,
        },
        _ => {
            let left = value.and_then(JsonValue::as_str);
            let right = right.as_str();
            match expression.op {
                CompareOp::Eq => left == right,
                CompareOp::Ne => left != right,
                CompareOp::Gt => left > right,
                CompareOp::Ge => left >= right,
                CompareOp::Lt => left < right,
                CompareOp::Le => left <= right,
            }
        }
    }
}

fn bql_value_json(value: &Value) -> JsonValue {
    match value {
        Value::String(value)
        | Value::Identifier(value)
        | Value::Human(value)
        | Value::Param(value) => json!(value),
        Value::Integer(value) => json!(value),
        Value::Number(value) => json!(value),
        Value::Bool(value) => json!(value),
        Value::Null => JsonValue::Null,
        Value::List(values) => JsonValue::Array(values.iter().map(bql_value_json).collect()),
        Value::Stage(stage) => json!(stage.name),
        Value::Expr(_) => JsonValue::Null,
    }
}

fn numeric(value: Option<&JsonValue>) -> Option<f64> {
    value.and_then(|value| match value {
        JsonValue::Number(number) => number.as_f64(),
        JsonValue::String(value) => value.parse().ok(),
        _ => None,
    })
}

fn value_role_matches(actual: &str, requested: &str) -> bool {
    actual.eq_ignore_ascii_case(requested)
        || actual
            .to_ascii_lowercase()
            .ends_with(&requested.to_ascii_lowercase())
}

fn compare_order(left: Option<&JsonValue>, right: Option<&JsonValue>) -> std::cmp::Ordering {
    match (numeric(left), numeric(right)) {
        (Some(left), Some(right)) => left.total_cmp(&right),
        _ => left
            .map(JsonValue::to_string)
            .cmp(&right.map(JsonValue::to_string)),
    }
}

fn object<const N: usize>(entries: [(&str, JsonValue); N]) -> BqlRow {
    entries
        .into_iter()
        .map(|(key, value)| (key.to_owned(), value))
        .collect()
}

#[cfg(feature = "native")]
fn exact_artifact_files(
    boundary_dir: &std::path::Path,
) -> Result<Vec<std::path::PathBuf>, QueryError> {
    let mut files = Vec::new();
    for root in [boundary_dir.join("flight"), boundary_dir.join("trace")] {
        files.extend(files_with_extension(&root, "bamlprof")?);
    }
    files.sort();
    Ok(files)
}

#[cfg(feature = "native")]
fn read_exact_artifact_bounded(
    path: &std::path::Path,
    max_bytes: usize,
) -> Result<
    (
        bex_events::prof::read::BamlprofContents,
        crate::SourceWatermark,
        usize,
        bool,
    ),
    QueryError,
> {
    use std::io::Read;

    let before = std::fs::metadata(path)?;
    let before_generation = file_generation(&before);
    let read_limit = u64::try_from(max_bytes)
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    let mut bytes = Vec::with_capacity(max_bytes.min(1024 * 1024).saturating_add(1));
    std::fs::File::open(path)?
        .take(read_limit)
        .read_to_end(&mut bytes)?;
    let budget_truncated = bytes.len() > max_bytes;
    if budget_truncated {
        bytes.truncate(max_bytes);
    }
    let after = std::fs::metadata(path)?;
    let after_generation = file_generation(&after);
    if before.len() != after.len() || before_generation != after_generation {
        return Err(QueryError::invalid_data(format!(
            "exact artifact changed while it was being read: {}",
            path.display()
        )));
    }
    let parsed_through = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    let contents = bex_events::prof::read::read_bamlprof_from_bytes(&bytes)?;
    Ok((
        contents,
        crate::SourceWatermark {
            file: FileId(stable_path_id(path)),
            source: SourceSnapshot {
                committed_len: after.len(),
                generation: after_generation,
            },
            parsed_through,
        },
        bytes.len(),
        budget_truncated,
    ))
}

#[cfg(feature = "native")]
fn stable_path_id(path: &std::path::Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(feature = "native")]
fn file_generation(metadata: &std::fs::Metadata) -> u64 {
    metadata
        .modified()
        .ok()
        .and_then(|modified| modified.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
        })
}

#[cfg(feature = "native")]
fn files_with_extension(
    root: &std::path::Path,
    extension: &str,
) -> Result<Vec<std::path::PathBuf>, QueryError> {
    let mut files = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let entries = match std::fs::read_dir(directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        for entry in entries {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                pending.push(entry.path());
            } else if file_type.is_file()
                && entry.path().extension().and_then(|value| value.to_str()) == Some(extension)
            {
                files.push(entry.path());
            }
        }
    }
    files.sort();
    Ok(files)
}

#[cfg(feature = "native")]
fn safe_boundary_child(
    boundary_dir: &std::path::Path,
    relative: &str,
) -> Result<std::path::PathBuf, QueryError> {
    use std::path::{Component, Path};
    let relative = Path::new(relative);
    if relative.as_os_str().is_empty()
        || relative
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(QueryError::InvalidRequest(
            "dump_ref must be a relative boundary artifact path".to_owned(),
        ));
    }
    let path = boundary_dir.join(relative);
    if !path.is_file() {
        return Err(QueryError::NotFound(relative.display().to_string()));
    }
    Ok(path)
}

#[cfg(feature = "native")]
fn boundary_project_root(boundary_dir: &std::path::Path) -> Result<std::path::PathBuf, QueryError> {
    boundary_dir
        .parent()
        .and_then(std::path::Path::parent)
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf)
        .ok_or_else(|| QueryError::invalid_data("boundary is not under <project>/.baml/history"))
}

#[cfg(feature = "native")]
fn exact_event_row(
    run_id: &str,
    dump_ref: &str,
    process_id: [u8; 16],
    engine_id: u64,
    sequence: usize,
    event: &bex_events::prof::pb::DiskEventV1,
) -> BqlRow {
    use bex_events::{
        ids::{BexCallId, BexThreadId, CallRef, EngineId, ProcessEuid},
        prof::pb::disk_event_v1::Event,
    };
    let (kind, timestamp_ns, thread_id, call_id, function_id, status, model_id) =
        match event.event.as_ref() {
            Some(Event::StartThread(value)) => (
                "start_thread",
                value.timestamp_ns,
                Some(value.thread_id),
                None,
                None,
                None,
                None,
            ),
            Some(Event::EndThread(value)) => (
                "end_thread",
                value.timestamp_ns,
                Some(value.thread_id),
                None,
                None,
                Some(value.status),
                None,
            ),
            Some(Event::CallFunction(value)) => (
                "call_function",
                value.timestamp_ns,
                Some(value.thread_id),
                Some(value.call_id),
                Some(value.function_id),
                None,
                None,
            ),
            Some(Event::SetFunctionId(value)) => (
                "set_function_id",
                value.timestamp_ns,
                Some(value.thread_id),
                Some(value.call_id),
                None,
                None,
                None,
            ),
            Some(Event::EndFunction(value)) => (
                "end_function",
                value.timestamp_ns,
                Some(value.thread_id),
                Some(value.call_id),
                None,
                Some(value.status),
                None,
            ),
            Some(Event::Heartbeat(value)) => (
                "heartbeat",
                value.timestamp_ns,
                None,
                None,
                None,
                None,
                None,
            ),
            Some(Event::SuspendThread(value)) => (
                "suspend_thread",
                value.timestamp_ns,
                Some(value.thread_id),
                None,
                None,
                Some(value.reason),
                None,
            ),
            Some(Event::ResumeThread(value)) => (
                "resume_thread",
                value.timestamp_ns,
                Some(value.thread_id),
                None,
                None,
                None,
                None,
            ),
            Some(Event::LlmCallMeta(value)) => (
                "llm_call_meta",
                value.timestamp_ns,
                Some(value.thread_id),
                Some(value.call_id),
                None,
                Some(value.flags as i32),
                Some(value.model_id),
            ),
            None => ("unknown", 0, None, None, None, None, None),
        };
    let public_call = thread_id.zip(call_id).map(|(thread_id, call_id)| {
        CallRef {
            process_euid: ProcessEuid(process_id),
            engine_id: EngineId(engine_id),
            thread_id: BexThreadId(thread_id),
            call_id: BexCallId(call_id),
        }
        .encode()
    });
    object([
        ("run_id", json!(run_id)),
        ("dump_ref", json!(dump_ref)),
        ("sequence", json!(sequence)),
        ("kind", json!(kind)),
        ("timestamp_ns", json!(timestamp_ns)),
        ("thread_id", json!(thread_id)),
        ("call_id", json!(call_id)),
        ("call", json!(public_call)),
        ("function_id", json!(function_id)),
        ("status", json!(status)),
        ("model_id", json!(model_id)),
    ])
}

fn hex_bytes(bytes: Vec<u8>) -> String {
    let mut output = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        use std::fmt::Write as _;
        let _ = write!(output, "{byte:02x}");
    }
    output
}

fn sync_run_ids(input: &mut DataSet) {
    if input.kind != SetKind::RunSet {
        return;
    }
    if !input.rows.is_empty() && input.rows.iter().all(|row| !row.contains_key("run_id")) {
        return;
    }
    input.run_ids = input
        .rows
        .iter()
        .filter_map(|row| row.get("run_id").and_then(JsonValue::as_str))
        .filter_map(|value| bex_events::ids::BoundaryId::from_wire_str(value))
        .map(|value| value.as_bytes())
        .collect();
}

fn cursor_from_last_run(rows: &[BqlRow]) -> Option<BqlCursor> {
    let row = rows.last()?;
    let created_ms = row.get("created_ms")?.as_u64()?;
    let boundary_id = row
        .get("run_id")?
        .as_str()
        .and_then(bex_events::ids::BoundaryId::from_wire_str)?
        .as_bytes();
    Some(BqlCursor {
        created_ms,
        boundary_id,
    })
}

#[cfg(feature = "native")]
fn run_row(run: &crate::RunSummary) -> BqlRow {
    object([
        ("run_id", json!(run.boundary_id_wire)),
        ("created_ms", json!(run.created_ms)),
        ("target", json!(run.target)),
        ("status", json!(run_state_name(run))),
        ("has_snapshot", json!(run.has_snapshot)),
        ("revision_id", json!(run.revision_id.map(hex_32))),
        ("source", json!(run.source)),
        ("completion_status", json!(run.completion_status)),
        ("partition_id", json!(run.partition_id)),
    ])
}

#[cfg(feature = "native")]
fn run_state_name(run: &crate::RunSummary) -> &'static str {
    use crate::RunState;
    match run.state {
        RunState::MissingMeta => "missing_meta",
        RunState::Begun => "begun",
        RunState::Bound => "bound",
        RunState::Running => "running",
        RunState::Crashed => "crashed",
        RunState::Complete if run.completion_status.as_deref() == Some("ok") => "complete",
        RunState::Complete => "errored",
        RunState::PartialWithLoss => "partial_with_loss",
    }
}

#[cfg(feature = "native")]
fn run_matches_status(run: &crate::RunSummary, status: Option<&str>) -> bool {
    status.is_none_or(|status| {
        run_state_name(run) == if status == "ok" { "complete" } else { status }
    })
}

#[cfg(feature = "native")]
fn run_matches_revision(run: &crate::RunSummary, revision: Option<&str>) -> bool {
    revision.is_none_or(|revision| {
        run.revision_id
            .map(hex_32)
            .is_some_and(|actual| actual == revision || actual.ends_with(revision))
    })
}

fn hex_32(bytes: [u8; 32]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn merge_completeness(target: &mut Completeness, other: &Completeness) {
    target.complete &= other.complete;
    target.watermarks.extend(other.watermarks.iter().copied());
    target
        .capture_loss
        .extend(other.capture_loss.iter().cloned());
    target
        .sources_consulted
        .extend(other.sources_consulted.iter().copied());
    target.truncated |= other.truncated;
    target.lod_degraded |= other.lod_degraded;
    target.partial_tail |= other.partial_tail;
    target.more_lanes |= other.more_lanes;
    target.warnings.extend(other.warnings.iter().cloned());
    target.snapshot.extend(other.snapshot.iter().copied());
}

#[cfg(feature = "native")]
fn open_run_meta_for_options(
    directory: &std::path::Path,
    options: &ExecuteOptions,
) -> Result<crate::RunMeta, QueryError> {
    let Some(snapshot) = &options.snapshot else {
        return crate::open_run_meta(directory);
    };
    let pins = snapshot
        .entries
        .iter()
        .map(|entry| {
            (
                FileId(entry.file),
                SourceSnapshot {
                    generation: entry.generation,
                    committed_len: entry.committed_len,
                },
            )
        })
        .collect::<BTreeMap<_, _>>();
    crate::open_run_meta_pinned(directory, &pins).map_err(|error| match error {
        QueryError::InvalidRequest(message)
            if message.contains("snapshot")
                || message.contains("replaced")
                || message.contains("shorter") =>
        {
            snapshot_error(message)
        }
        error => error,
    })
}

fn snapshot_error(message: String) -> QueryError {
    QueryError::Bql(crate::BqlDiagnostic {
        code: "E_SNAPSHOT_MISMATCH",
        message,
        start: 0,
        end: 1,
        line: 1,
        column: 1,
        source_line: "--snapshot".to_owned(),
        correction: Some("rerun without --snapshot to mint a fresh watermark".to_owned()),
        valid: Vec::new(),
    })
}

fn unavailable(call: &StageCall, name: &str, message: &str) -> QueryError {
    QueryError::Bql(crate::BqlDiagnostic {
        code: if matches!(
            call.name.as_str(),
            "dumps" | "trace" | "events" | "instances" | "critical_path"
        ) {
            "E_NO_EXACT_SOURCE"
        } else if call.name == "diff" {
            "E_REVISION_MISMATCH"
        } else {
            "E_UNAVAILABLE"
        },
        message: format!("{name}: {message}"),
        start: call.span.start,
        end: call.span.end,
        line: 1,
        column: call.span.start.saturating_add(1),
        source_line: call.name.clone(),
        correction: None,
        valid: Vec::new(),
    })
}

fn nested_stage<'a>(
    value: Option<&'a Value>,
    call: &StageCall,
    side: &str,
) -> Result<&'a StageCall, QueryError> {
    match value {
        Some(Value::Stage(stage)) => Ok(stage),
        _ => Err(unavailable(
            call,
            "diff",
            &format!("{side} input must be a nested source such as `runs(rev=\"...\")`"),
        )),
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Metric {
    calls: u64,
    errors: u64,
    total_ns: u64,
    self_ns: u64,
    awaiting_ns: u64,
}

fn folded_cct_from_rows(dataset: &DataSet) -> crate::FoldedCct {
    let nodes = dataset
        .rows
        .iter()
        .filter_map(|row| {
            let function_id = row
                .get("function_id")
                .and_then(JsonValue::as_u64)
                .and_then(|value| u32::try_from(value).ok())?;
            Some((function_id, row))
        })
        .enumerate()
        .map(|(index, (function_id, row))| {
            let node_id = u32::try_from(index)
                .unwrap_or(u32::MAX - 1)
                .saturating_add(1);
            (
                node_id,
                crate::FoldedNode {
                    node_id,
                    function_id,
                    counters: crate::Counters {
                        enters: row.get("calls").and_then(JsonValue::as_u64).unwrap_or(0),
                        ends_err: row.get("errors").and_then(JsonValue::as_u64).unwrap_or(0),
                        total_ns: row.get("total_ns").and_then(JsonValue::as_u64).unwrap_or(0),
                        self_ns: row.get("self_ns").and_then(JsonValue::as_u64).unwrap_or(0),
                        await_ns: row
                            .get("awaiting_ns")
                            .and_then(JsonValue::as_u64)
                            .unwrap_or(0),
                        ..crate::Counters::default()
                    },
                    ..crate::FoldedNode::default()
                },
            )
        })
        .collect();
    crate::FoldedCct {
        nodes,
        meta: dataset.meta.clone(),
        ..crate::FoldedCct::default()
    }
}

fn aggregate_by_function(rows: &[BqlRow]) -> BTreeMap<u64, Metric> {
    let mut output = BTreeMap::<u64, Metric>::new();
    for row in rows {
        let Some(function) = row.get("function_id").and_then(JsonValue::as_u64) else {
            continue;
        };
        let metric = output.entry(function).or_default();
        metric.calls = metric
            .calls
            .saturating_add(row.get("calls").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.errors = metric
            .errors
            .saturating_add(row.get("errors").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.total_ns = metric
            .total_ns
            .saturating_add(row.get("total_ns").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.self_ns = metric
            .self_ns
            .saturating_add(row.get("self_ns").and_then(JsonValue::as_u64).unwrap_or(0));
        metric.awaiting_ns = metric.awaiting_ns.saturating_add(
            row.get("awaiting_ns")
                .and_then(JsonValue::as_u64)
                .unwrap_or(0),
        );
    }
    output
}

fn distinct_strings(rows: &[BqlRow], field: &str) -> BTreeSet<String> {
    rows.iter()
        .filter_map(|row| {
            row.get(field)
                .and_then(JsonValue::as_str)
                .map(str::to_owned)
        })
        .collect()
}

fn signed_delta(right: u64, left: u64) -> i64 {
    let delta = i128::from(right) - i128::from(left);
    delta.clamp(i128::from(i64::MIN), i128::from(i64::MAX)) as i64
}

fn parse_duration_ns(value: &str) -> Option<u64> {
    const UNITS: &[(&str, u64)] = &[
        ("ns", 1),
        ("us", 1_000),
        ("ms", 1_000_000),
        ("s", 1_000_000_000),
        ("m", 60 * 1_000_000_000),
        ("h", 60 * 60 * 1_000_000_000),
        ("d", 24 * 60 * 60 * 1_000_000_000),
    ];
    UNITS.iter().find_map(|(suffix, multiplier)| {
        value
            .strip_suffix(suffix)
            .and_then(|amount| amount.parse::<u64>().ok())
            .and_then(|amount| amount.checked_mul(*multiplier))
    })
}

fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |duration| {
            u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_cursor_round_trip_without_string_sort_assumptions() {
        let snapshot = SnapshotToken {
            entries: vec![SnapshotEntry {
                file: 7,
                generation: 2,
                committed_len: 4096,
                parsed_through: 4000,
            }],
        };
        assert_eq!(SnapshotToken::parse(&snapshot.encode()).unwrap(), snapshot);
        let cursor = BqlCursor {
            created_ms: 42,
            boundary_id: [9; 16],
        };
        assert_eq!(BqlCursor::parse(&cursor.encode()).unwrap(), cursor);
    }

    #[test]
    fn bounded_row_stages_are_deterministic() {
        let input = DataSet::rows(
            SetKind::CtxSet,
            vec![
                object([("errors", json!(1)), ("function_id", json!(2))]),
                object([("errors", json!(4)), ("function_id", json!(1))]),
            ],
            Completeness {
                complete: true,
                ..Completeness::default()
            },
        );
        let call = parse("top(1, by=errors)")
            .unwrap()
            .statements
            .remove(0)
            .pipeline
            .stages
            .remove(0);
        let output = top_stage(input, &call, 100).unwrap();
        assert_eq!(output.rows.len(), 1);
        assert_eq!(output.rows[0]["errors"], json!(4));
        assert!(output.meta.truncated);
    }

    #[test]
    fn stats_groups_deterministically_with_named_aggregates() {
        let input = DataSet::rows(
            SetKind::ValueSet,
            vec![
                object([("cid", json!("b")), ("bytes", json!(5))]),
                object([("cid", json!("a")), ("bytes", json!(20))]),
                object([("cid", json!("a")), ("bytes", json!(10))]),
            ],
            Completeness {
                complete: true,
                ..Completeness::default()
            },
        );
        let call = parse(
            "stats(n=count(), total=sum(bytes), smallest=min(bytes), largest=max(bytes), \
             mean=avg(bytes), by=cid)",
        )
        .unwrap()
        .statements
        .remove(0)
        .pipeline
        .stages
        .remove(0);
        let output = stats_stage(input, &call).unwrap();
        assert_eq!(output.kind, SetKind::Table);
        assert_eq!(output.rows.len(), 2);
        assert_eq!(output.rows[0]["cid"], json!("a"));
        assert_eq!(output.rows[0]["n"], json!(2));
        assert_eq!(output.rows[0]["total"], json!(30));
        assert_eq!(output.rows[0]["smallest"], json!(10));
        assert_eq!(output.rows[0]["largest"], json!(20));
        assert_eq!(output.rows[0]["mean"], json!(15));
        assert_eq!(output.rows[1]["cid"], json!("b"));
        assert_eq!(output.rows[1]["n"], json!(1));
    }

    #[test]
    fn stats_defaults_to_a_single_count_row() {
        let input = DataSet::rows(
            SetKind::Table,
            Vec::new(),
            Completeness {
                complete: true,
                ..Completeness::default()
            },
        );
        let call = parse("stats()")
            .unwrap()
            .statements
            .remove(0)
            .pipeline
            .stages
            .remove(0);
        let output = stats_stage(input, &call).unwrap();
        assert_eq!(output.rows, vec![object([("count", json!(0))])]);
    }

    #[test]
    fn matched_io_compare_joins_inputs_and_compares_output_multisets() {
        let side = |run: &str, outputs: &[&str]| MatchedIoSide {
            run_ids: BTreeSet::from([run.to_owned()]),
            outputs: outputs.iter().map(|value| (*value).to_owned()).collect(),
            calls: 1,
        };
        let left = BTreeMap::from([
            ("input-a".to_owned(), side("left-1", &["ok:x"])),
            ("input-b".to_owned(), side("left-2", &["ok:y"])),
        ]);
        let right = BTreeMap::from([
            ("input-a".to_owned(), side("right-1", &["ok:x"])),
            ("input-b".to_owned(), side("right-2", &["error:z"])),
            ("input-c".to_owned(), side("right-3", &["ok:q"])),
        ]);
        let compared = compare_captured_io(&left, &right, 10);
        assert_eq!(compared.matched, 2);
        assert_eq!(compared.unmatched, 1);
        assert!(!compared.truncated);
        assert_eq!(compared.rows[0]["verdict"], json!("unchanged"));
        assert_eq!(compared.rows[1]["verdict"], json!("changed"));
        assert_eq!(compared.rows[2]["verdict"], json!("right_only"));
    }
}
