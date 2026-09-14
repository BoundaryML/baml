//! Catalog-v1 relation providers over the bound store
//! (TASK/baml-query-scope.md §5.3–§5.4).
//!
//! Physical strategy: a scan prunes executions with the pushed-down
//! `execution_id` equality filters (evaluated at the universe, meta
//! plane only), folds the survivors through the shared cache, and
//! materializes Arrow batches. Value columns carry opaque handles
//! (`resolver::encode_handle`); the core's `TrustedRelation` keeps value
//! predicates out of this provider entirely.

use std::{collections::HashMap, sync::Arc};

use baml_query::{
    catalog::RelationDef, error::QueryError, provider::RelationProviderFactory, scope::Snapshot,
};
use bex_prof_store::prof::backend::{
    CctCounters, ContextRef, EdgeKind, ExecutionProfile, ExecutionStatus, ExecutionSummary,
    FunctionTable, IndexState, OverflowReason, TerminalErrorTarget, ValueState,
};
use datafusion::{
    arrow::{
        array::{
            ArrayRef, BinaryBuilder, BooleanBuilder, ListBuilder, StringBuilder,
            TimestampNanosecondBuilder, UInt32Builder, UInt64Builder,
        },
        datatypes::{DataType, Field},
        record_batch::RecordBatch,
    },
    catalog::Session,
    common::Result as DfResult,
    datasource::{TableProvider, TableType, memory::MemTable},
    logical_expr::{Expr, Operator, TableProviderFilterPushDown},
    physical_plan::ExecutionPlan,
    scalar::ScalarValue,
};

use crate::{
    fold::FoldCache,
    resolver::{encode_handle, encode_missing_handle},
    universe::{BoundStream, ProfilesUniverse},
};

/// The provider factory: one per bound session.
pub(crate) struct ProfilesFactory {
    universe: Arc<ProfilesUniverse>,
    folds: Arc<FoldCache>,
}

impl ProfilesFactory {
    #[must_use]
    pub(crate) fn new(
        universe: Arc<ProfilesUniverse>,
        folds: Arc<FoldCache>,
    ) -> Arc<ProfilesFactory> {
        Arc::new(ProfilesFactory { universe, folds })
    }
}

impl RelationProviderFactory for ProfilesFactory {
    fn provider(
        &self,
        relation: &RelationDef,
        _snapshot: &Snapshot,
    ) -> Result<Option<Arc<dyn TableProvider>>, QueryError> {
        Ok(Some(Arc::new(ProfilesTable {
            relation: relation.clone(),
            universe: self.universe.clone(),
            folds: self.folds.clone(),
        })))
    }
}

/// One relation served from the bound universe.
struct ProfilesTable {
    relation: RelationDef,
    universe: Arc<ProfilesUniverse>,
    folds: Arc<FoldCache>,
}

impl std::fmt::Debug for ProfilesTable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ProfilesTable")
            .field("relation", &self.relation.name)
            .finish_non_exhaustive()
    }
}

/// `execution_id = '…'` / `execution_id IN ('…', …)` extracted from one
/// pushed filter, when the relation has that column.
fn execution_id_targets(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::BinaryExpr(binary) if binary.op == Operator::Eq => {
            let ((Expr::Column(column), Expr::Literal(literal, _))
            | (Expr::Literal(literal, _), Expr::Column(column))) =
                (binary.left.as_ref(), binary.right.as_ref())
            else {
                return None;
            };
            if column.name != "execution_id" {
                return None;
            }
            match literal {
                ScalarValue::Utf8(Some(value)) | ScalarValue::LargeUtf8(Some(value)) => {
                    Some(vec![value.clone()])
                }
                _ => None,
            }
        }
        Expr::InList(in_list) if !in_list.negated => {
            let Expr::Column(column) = in_list.expr.as_ref() else {
                return None;
            };
            if column.name != "execution_id" {
                return None;
            }
            let mut values = Vec::with_capacity(in_list.list.len());
            for item in &in_list.list {
                match item {
                    Expr::Literal(
                        ScalarValue::Utf8(Some(value)) | ScalarValue::LargeUtf8(Some(value)),
                        _,
                    ) => {
                        values.push(value.clone());
                    }
                    _ => return None,
                }
            }
            Some(values)
        }
        _ => None,
    }
}

#[async_trait::async_trait]
impl TableProvider for ProfilesTable {
    fn schema(&self) -> datafusion::arrow::datatypes::SchemaRef {
        self.relation.schema()
    }

    fn table_type(&self) -> TableType {
        TableType::Base
    }

    fn supports_filters_pushdown(
        &self,
        filters: &[&Expr],
    ) -> DfResult<Vec<TableProviderFilterPushDown>> {
        // `execution_id` equality prunes executions before any fold and is
        // applied exactly; everything else stays above the scan.
        Ok(filters
            .iter()
            .map(|filter| {
                if self.relation.column("execution_id").is_some()
                    && execution_id_targets(filter).is_some()
                {
                    TableProviderFilterPushDown::Exact
                } else {
                    TableProviderFilterPushDown::Unsupported
                }
            })
            .collect())
    }

    async fn scan(
        &self,
        state: &dyn Session,
        projection: Option<&Vec<usize>>,
        filters: &[Expr],
        limit: Option<usize>,
    ) -> DfResult<Arc<dyn ExecutionPlan>> {
        // `None` = no execution_id filter pushed; `Some(vec![])` = a
        // legitimately empty intersection (contradictory equalities) that
        // must return zero rows — never widen it back.
        let mut targets: Option<Vec<String>> = None;
        for filter in filters {
            if let Some(values) = execution_id_targets(filter) {
                targets = Some(match targets.take() {
                    None => values,
                    Some(current) => current.into_iter().filter(|v| values.contains(v)).collect(),
                });
            }
        }
        let batch = build_relation_batch(
            &self.relation,
            &self.universe,
            &self.folds,
            targets.as_deref(),
        )
        .map_err(|err| datafusion::common::DataFusionError::External(Box::new(err)))?;
        let mem = MemTable::try_new(self.relation.schema(), vec![vec![batch]])?;
        mem.scan(state, projection, &[], limit).await
    }
}

// ── row building ───────────────────────────────────────────────────────

/// A dynamically-typed column builder driven by the catalog's column
/// list, so batches always match the catalog schema in order and type.
enum Col {
    Utf8(StringBuilder),
    U32(UInt32Builder),
    U64(UInt64Builder),
    Bool(BooleanBuilder),
    Ts(TimestampNanosecondBuilder),
    Utf8List(ListBuilder<StringBuilder>),
    Bin(BinaryBuilder),
}

/// One row's cell values, keyed by column name.
#[derive(Default)]
struct Row<'a> {
    utf8: HashMap<&'static str, Option<String>>,
    u32: HashMap<&'static str, Option<u32>>,
    u64: HashMap<&'static str, Option<u64>>,
    bool: HashMap<&'static str, Option<bool>>,
    ts: HashMap<&'static str, Option<i64>>,
    list: HashMap<&'static str, Vec<String>>,
    bin: HashMap<&'static str, Option<Vec<u8>>>,
    _marker: std::marker::PhantomData<&'a ()>,
}

struct BatchBuilder {
    columns: Vec<(&'static str, Col)>,
}

impl BatchBuilder {
    fn new(relation: &RelationDef) -> BatchBuilder {
        let columns = relation
            .columns
            .iter()
            .map(|column| {
                let col = match &column.data_type {
                    DataType::Utf8 if column.value_role.is_none() => {
                        Col::Utf8(StringBuilder::new())
                    }
                    DataType::UInt32 => Col::U32(UInt32Builder::new()),
                    DataType::UInt64 => Col::U64(UInt64Builder::new()),
                    DataType::Boolean => Col::Bool(BooleanBuilder::new()),
                    DataType::Timestamp(..) => {
                        Col::Ts(TimestampNanosecondBuilder::new().with_timezone("UTC"))
                    }
                    DataType::List(_) => Col::Utf8List(
                        ListBuilder::new(StringBuilder::new()).with_field(Field::new(
                            "item",
                            DataType::Utf8,
                            false,
                        )),
                    ),
                    DataType::Binary => Col::Bin(BinaryBuilder::new()),
                    other => unreachable!("catalog v1 has no {other:?} columns"),
                };
                (column.name, col)
            })
            .collect();
        BatchBuilder { columns }
    }

    fn push(&mut self, mut row: Row<'_>) {
        for (name, col) in &mut self.columns {
            match col {
                Col::Utf8(b) => b.append_option(row.utf8.remove(name).flatten()),
                Col::U32(b) => b.append_option(row.u32.remove(name).flatten()),
                Col::U64(b) => b.append_option(row.u64.remove(name).flatten()),
                Col::Bool(b) => b.append_option(row.bool.remove(name).flatten()),
                Col::Ts(b) => b.append_option(row.ts.remove(name).flatten()),
                Col::Utf8List(b) => {
                    for item in row.list.remove(name).unwrap_or_default() {
                        b.values().append_value(item);
                    }
                    b.append(true);
                }
                Col::Bin(b) => b.append_option(row.bin.remove(name).flatten()),
            }
        }
        // A leftover key is a builder cell whose name matches no catalog
        // column (a typo, or a column removed from the catalog) — without
        // this check it would silently drop data / read back NULL.
        debug_assert!(
            row.utf8.is_empty()
                && row.u32.is_empty()
                && row.u64.is_empty()
                && row.bool.is_empty()
                && row.ts.is_empty()
                && row.list.is_empty()
                && row.bin.is_empty(),
            "row cells with no matching catalog column: {:?}",
            row.utf8
                .keys()
                .chain(row.u32.keys())
                .chain(row.u64.keys())
                .chain(row.bool.keys())
                .chain(row.ts.keys())
                .chain(row.list.keys())
                .chain(row.bin.keys())
                .collect::<Vec<_>>()
        );
    }

    fn finish(self, relation: &RelationDef) -> Result<RecordBatch, QueryError> {
        let arrays: Vec<ArrayRef> = self
            .columns
            .into_iter()
            .map(|(_, col)| -> ArrayRef {
                match col {
                    Col::Utf8(mut b) => Arc::new(b.finish()),
                    Col::U32(mut b) => Arc::new(b.finish()),
                    Col::U64(mut b) => Arc::new(b.finish()),
                    Col::Bool(mut b) => Arc::new(b.finish()),
                    Col::Ts(mut b) => Arc::new(b.finish()),
                    Col::Utf8List(mut b) => Arc::new(b.finish()),
                    Col::Bin(mut b) => Arc::new(b.finish()),
                }
            })
            .collect();
        RecordBatch::try_new(relation.schema(), arrays).map_err(|err| {
            QueryError::new(
                baml_query::error::QueryErrorCode::Internal,
                format!("relation batch mismatch: {err}"),
            )
        })
    }
}

impl Row<'_> {
    fn utf8(mut self, name: &'static str, value: impl Into<String>) -> Self {
        self.utf8.insert(name, Some(value.into()));
        self
    }
    fn utf8_opt(mut self, name: &'static str, value: Option<String>) -> Self {
        self.utf8.insert(name, value);
        self
    }
    fn u32(mut self, name: &'static str, value: u32) -> Self {
        self.u32.insert(name, Some(value));
        self
    }
    fn u32_opt(mut self, name: &'static str, value: Option<u32>) -> Self {
        self.u32.insert(name, value);
        self
    }
    fn u64(mut self, name: &'static str, value: u64) -> Self {
        self.u64.insert(name, Some(value));
        self
    }
    fn u64_opt(mut self, name: &'static str, value: Option<u64>) -> Self {
        self.u64.insert(name, value);
        self
    }
    fn bool(mut self, name: &'static str, value: bool) -> Self {
        self.bool.insert(name, Some(value));
        self
    }
    fn ts_opt(mut self, name: &'static str, value: Option<i64>) -> Self {
        self.ts.insert(name, value);
        self
    }
    fn list(mut self, name: &'static str, values: Vec<String>) -> Self {
        self.list.insert(name, values);
        self
    }
    fn bin_opt(mut self, name: &'static str, value: Option<Vec<u8>>) -> Self {
        self.bin.insert(name, value);
        self
    }
}

fn build_relation_batch(
    relation: &RelationDef,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<RecordBatch, QueryError> {
    let Some(rows) = provider_for(relation.name) else {
        return Err(QueryError::new(
            baml_query::error::QueryErrorCode::Internal,
            format!("no provider for relation {}", relation.name),
        ));
    };
    let mut builder = BatchBuilder::new(relation);
    rows(&mut builder, universe, folds, targets)?;
    builder.finish(relation)
}

type RowsFn = fn(
    &mut BatchBuilder,
    &ProfilesUniverse,
    &FoldCache,
    Option<&[String]>,
) -> Result<(), QueryError>;

/// Catalog-name → row builder. A coverage test walks the internal catalog
/// against this map, so renaming a relation without re-mapping its builder
/// fails at test time instead of as a runtime "no provider" error.
fn provider_for(name: &str) -> Option<RowsFn> {
    Some(match name {
        "threads_v1" => threads_rows,
        "call_path_stats_v1" => contexts_rows,
        "calls_v1" => calls_rows,
        "errors_v1" => errors_rows,
        "function_definitions_v1" => |b, u, f, _| functions_rows(b, u, f),
        "health_v1" => health_rows,
        "processes_v1" => |b, u, _, _| {
            streams_rows(b, u);
            Ok(())
        },
        "store_files_v1" => |b, u, _, _| {
            segments_rows(b, u);
            Ok(())
        },
        "value_index_v1" => |b, u, _, _| {
            cas_rows(b, u);
            Ok(())
        },
        _ => return None,
    })
}

/// The bound executions, pruned by pushed-down execution ids.
fn selected<'a>(
    universe: &'a ProfilesUniverse,
    targets: Option<&[String]>,
) -> Vec<(&'a BoundStream, &'a ExecutionSummary)> {
    universe
        .executions()
        .filter(|(_, summary)| {
            targets.is_none_or(|ids| {
                let encoded = summary.id.encode();
                ids.contains(&encoded)
            })
        })
        .collect()
}

fn status_str(status: ExecutionStatus) -> &'static str {
    match status {
        ExecutionStatus::Running => "running",
        ExecutionStatus::Abandoned => "abandoned",
        ExecutionStatus::Succeeded => "succeeded",
        ExecutionStatus::Failed => "failed",
        ExecutionStatus::Cancelled => "cancelled",
        ExecutionStatus::Panicked => "panicked",
    }
}

fn index_state_str(state: IndexState) -> &'static str {
    match state {
        IndexState::Complete => "complete",
        IndexState::NoRootEnded => "no_root_ended",
        IndexState::RootStartedLost => "root_started_lost",
        IndexState::IndexCorrupt => "index_corrupt",
    }
}

fn edge_kind_str(kind: EdgeKind) -> &'static str {
    match kind {
        EdgeKind::Root => "root",
        EdgeKind::Call => "call",
        EdgeKind::Spawn => "spawn",
    }
}

fn overflow_reason_str(reason: OverflowReason) -> &'static str {
    match reason {
        OverflowReason::ContextMemoryUnavailableAfterDrain => "call_path_memory_unavailable",
        OverflowReason::InvalidParentContext => "invalid_parent_call_path",
    }
}

fn context_id(context: &ContextRef) -> Option<String> {
    match context {
        ContextRef::Normal(key) => Some(hex::encode(key.0)),
        ContextRef::Overflow { .. } => None,
    }
}

fn context_overflow(context: &ContextRef) -> Option<String> {
    match context {
        ContextRef::Overflow { reason, edge_kind } => Some(format!(
            "overflow:{}:{}",
            overflow_reason_str(*reason),
            edge_kind_str(*edge_kind)
        )),
        ContextRef::Normal(_) => None,
    }
}

fn error_id_wire(id: bex_prof_store::prof::backend::ErrorCaptureId) -> String {
    format!("{}#{}", id.thread_ref.encode(), id.unwind_ordinal)
}

fn wall_clock(zero_unix_ns: Option<u64>, ns: Option<u64>) -> Option<i64> {
    let zero = zero_unix_ns?;
    let ns = ns?;
    i64::try_from(zero.checked_add(ns)?).ok()
}

fn value_state_str(state: Option<ValueState>, role_present: bool) -> String {
    match state {
        Some(ValueState::Available { .. }) => "available".to_string(),
        Some(ValueState::Lost(reason)) => format!("lost:{}", loss_str(reason)),
        None if role_present => "not_captured".to_string(),
        None => "not_applicable".to_string(),
    }
}

fn loss_str(reason: bex_prof_store::prof::backend::ValueLossReason) -> &'static str {
    use bex_prof_store::prof::backend::ValueLossReason as R;
    match reason {
        R::ValueMemoryExceeded => "value_memory_exceeded",
        R::ValueAttemptTransportExceeded => "value_attempt_transport_exceeded",
        R::ErrorCaptureAttemptTransportExceeded => "error_capture_attempt_transport_exceeded",
        R::ValueTooLarge => "value_too_large",
        R::CopyFailed => "copy_failed",
        R::EncodeFailed => "encode_failed",
        R::CasWriteFailed => "cas_write_failed",
        R::CasConflict => "cas_conflict",
        R::DiskGuardExceeded => "disk_guard_exceeded",
        R::EvidenceSegmentPublishFailed => "evidence_file_publish_failed",
        R::StoreUnavailable => "store_unavailable",
    }
}

fn value_handle(state: Option<ValueState>, role_present: bool) -> Option<Vec<u8>> {
    match state {
        Some(state) => Some(encode_handle(state)),
        None if role_present => Some(encode_missing_handle()),
        None => None,
    }
}

fn value_cid_wire(state: Option<ValueState>) -> Option<String> {
    match state {
        Some(ValueState::Available { cid, .. }) => Some(format!("bamlv_1_{}", hex::encode(cid.0))),
        _ => None,
    }
}

/// Saturating u128 → u64 with a completeness flag.
fn sat_u64(value: u128, complete: &mut bool) -> u64 {
    u64::try_from(value).unwrap_or_else(|_| {
        *complete = false;
        u64::MAX
    })
}

// ── threads ────────────────────────────────────────────────────────────

fn threads_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<(), QueryError> {
    for (stream, summary) in selected(universe, targets) {
        let execution_id = summary.id.encode();
        let zero = stream.reader.header.as_ref().map(|h| h.zero_unix_ns);
        let profile = folds.fold(stream, summary)?;
        let table = folds.function_table(stream, summary)?;
        let engine = stream
            .reader
            .engines
            .iter()
            .find(|e| e.engine_id == summary.engine_id);

        // Root span: entry function + exact duration when retained.
        let root_span = profile.spans.values().find(|span| {
            span.start
                .as_ref()
                .is_some_and(|s| s.edge_kind == EdgeKind::Root)
        });
        let entry_function_id = root_span
            .and_then(|s| s.start.as_ref())
            .map(|s| s.function_id.0);
        let entry_fqn = entry_function_id.and_then(|id| {
            table.as_ref().as_ref().and_then(|t| {
                t.function(bex_prof_store::ids::FunctionId(id))
                    .map(|f| f.fqn.clone())
            })
        });
        let root_duration = root_span
            .and_then(|s| s.end.as_ref())
            .map(|end| end.inclusive_ns)
            .or_else(|| {
                summary
                    .ended_ns
                    .zip(summary.started_ns)
                    .map(|(end, start)| end.saturating_sub(start))
            });

        let mut total_calls = 0u64;
        let mut total_errors = 0u64;
        let mut total_cancelled = 0u64;
        for counters in profile
            .contexts
            .values()
            .map(|c| &c.counters)
            .chain(profile.overflow.values())
        {
            total_calls = total_calls.saturating_add(counters.invocations_started);
            total_errors = total_errors.saturating_add(counters.completed_error);
            total_cancelled = total_cancelled.saturating_add(counters.completed_cancelled);
        }
        let calls_retained = profile.spans.values().filter(|s| s.start.is_some()).count() as u64;
        let value_states: Vec<ValueState> = profile
            .spans
            .values()
            .flat_map(|s| [s.input.as_ref(), s.output.as_ref()])
            .flatten()
            .map(|occurrence| occurrence.state)
            .collect();
        let value_state = if value_states.is_empty() {
            "none"
        } else if value_states
            .iter()
            .any(|state| matches!(state, ValueState::Lost(_)))
        {
            "partial"
        } else {
            "complete"
        };

        let mut emitted_root = false;
        for (thread_ref, evidence) in &profile.threads {
            let is_root = *thread_ref == summary.id.0;
            emitted_root |= is_root;
            let start = evidence.start.as_ref();
            let end = evidence.end.as_ref();
            let started_ns =
                start.map_or_else(|| summary.started_ns.unwrap_or(0), |s| s.started_ns);
            let mut row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8("thread_id", thread_ref.encode())
                .utf8_opt(
                    "parent_thread_id",
                    start.and_then(|s| s.parent.map(|p| p.encode())),
                )
                .utf8_opt(
                    "spawn_call_id",
                    start.and_then(|s| s.spawn_call.map(|c| c.encode())),
                )
                .u32_opt(
                    "spawn_function_id",
                    start
                        .and_then(|s| s.spawn_call)
                        .and_then(|call| profile.spans.get(&call))
                        .and_then(|span| span.start.as_ref())
                        .map(|s| s.function_id.0),
                )
                .utf8_opt(
                    "spawn_fqn",
                    start
                        .and_then(|s| s.spawn_call)
                        .and_then(|call| profile.spans.get(&call))
                        .and_then(|span| span.start.as_ref())
                        .and_then(|s| {
                            table
                                .as_ref()
                                .as_ref()
                                .and_then(|t| t.function(s.function_id))
                                .map(|f| f.fqn.clone())
                        }),
                )
                .utf8_opt(
                    "spawn_site_file",
                    start.and_then(|s| s.spawn_site.as_ref()).and_then(|site| {
                        table
                            .as_ref()
                            .as_ref()
                            .and_then(|t| t.file_path(site.file_id).map(str::to_string))
                    }),
                )
                .u32_opt(
                    "spawn_site_line",
                    start
                        .and_then(|s| s.spawn_site.as_ref())
                        .map(|site| site.line),
                )
                .utf8_opt(
                    "name",
                    start.and_then(|s| {
                        if s.name.is_empty() {
                            None
                        } else {
                            Some(s.name.clone())
                        }
                    }),
                )
                .utf8("kind", if is_root { "root" } else { "spawn" })
                .u64("started_ns", started_ns)
                .u64_opt("ended_ns", end.map(|e| e.ended_ns))
                .ts_opt("started_at", wall_clock(zero, Some(started_ns)))
                .ts_opt("ended_at", wall_clock(zero, end.map(|e| e.ended_ns)))
                .utf8_opt(
                    "end_status",
                    end.map(|e| thread_end_status_str(e.status).to_string()),
                );
            if is_root {
                row = execution_columns(
                    row,
                    stream,
                    summary,
                    engine,
                    entry_function_id,
                    entry_fqn.clone(),
                    root_duration,
                    total_calls,
                    total_errors,
                    total_cancelled,
                    calls_retained,
                    profile.threads.len() as u64,
                    value_state,
                );
            }
            builder.push(row);
        }
        if !emitted_root {
            // No durable ThreadStart survived for the root (running
            // execution or lost facts): the meta plane still lists it.
            let started_ns = summary.started_ns.unwrap_or(0);
            let row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8("thread_id", execution_id.clone())
                .utf8("kind", "root")
                .u64("started_ns", started_ns)
                .u64_opt("ended_ns", summary.ended_ns)
                .ts_opt("started_at", wall_clock(zero, summary.started_ns))
                .ts_opt("ended_at", wall_clock(zero, summary.ended_ns));
            builder.push(execution_columns(
                row,
                stream,
                summary,
                engine,
                entry_function_id,
                entry_fqn,
                root_duration,
                total_calls,
                total_errors,
                total_cancelled,
                calls_retained,
                profile.threads.len() as u64,
                value_state,
            ));
        }
    }
    Ok(())
}

#[expect(clippy::too_many_arguments, reason = "one column per argument")]
fn execution_columns<'a>(
    row: Row<'a>,
    stream: &BoundStream,
    summary: &ExecutionSummary,
    engine: Option<&bex_prof_store::prof::backend::EngineStarted>,
    entry_function_id: Option<u32>,
    entry_fqn: Option<String>,
    duration_ns: Option<u64>,
    total_calls: u64,
    total_errors: u64,
    total_cancelled: u64,
    calls_retained: u64,
    threads_total: u64,
    value_state: &str,
) -> Row<'a> {
    row.utf8("process_id", hex::encode(stream.reader.stream.0.0))
        .u64("engine_id", summary.engine_id.0)
        .utf8_opt("program_id", summary.program_id.map(|p| hex::encode(p.0)))
        .utf8_opt("revision_id", engine.and_then(|e| e.revision_label.clone()))
        .utf8_opt("source_label", engine.and_then(|e| e.source_label.clone()))
        .utf8_opt(
            "runtime_id",
            summary
                .runtime_id
                .map(bex_prof_store::ids::BoundaryId::to_wire_string),
        )
        .u32_opt("entry_function_id", entry_function_id)
        .utf8_opt("entry_fqn", entry_fqn)
        .utf8("status", status_str(summary.status))
        .utf8("index_state", index_state_str(summary.index_state))
        .u64_opt("duration_ns", duration_ns)
        .u64("total_calls", total_calls)
        .u64("total_errors", total_errors)
        .u64("total_cancelled", total_cancelled)
        .u64("calls_retained", calls_retained)
        .u64("threads_total", threads_total)
        .utf8("value_state", value_state)
        .u64("data_first_seq", summary.data_first_seq)
        .u64("data_last_seq", summary.data_last_seq)
        .u64("data_file_count", summary.data_segment_count)
}

fn thread_end_status_str(status: bex_prof_store::prof::record::ThreadEndStatus) -> &'static str {
    use bex_prof_store::prof::record::ThreadEndStatus as S;
    match status {
        S::Completed => "completed",
        S::Cancelled => "cancelled",
        S::Errored => "errored",
    }
}

// ── contexts ───────────────────────────────────────────────────────────

fn contexts_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<(), QueryError> {
    for (stream, summary) in selected(universe, targets) {
        let execution_id = summary.id.encode();
        let profile = folds.fold(stream, summary)?;
        let table = folds.function_table(stream, summary)?;
        let timing_ok = !profile.cct_health.counter_saturated
            && !profile.cct_health.await_counter_saturated
            && !profile.cct_health.self_time_underflow;

        // Depths by an iterative memoized parent walk (0 = root); a
        // missing parent or a cycle stops the walk at the known prefix.
        let mut exact: HashMap<[u8; 32], u32> = HashMap::new();
        for (key, context) in &profile.contexts {
            let depth = context_depth(key.0, &profile, &mut exact);
            let function = context.tuple.and_then(|t| {
                table
                    .as_ref()
                    .as_ref()
                    .and_then(|table| table.function(t.function_id).cloned())
            });
            let mut complete = timing_ok;
            let counters = &context.counters;
            let derived = counters.derived_timing(timing_ok);
            let row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8("call_path_id", hex::encode(key.0))
                .utf8_opt(
                    "parent_call_path_id",
                    context
                        .tuple
                        .and_then(|t| t.parent_context_key.map(|p| hex::encode(p.0))),
                )
                .u32("depth", depth)
                .u32("function_id", context.tuple.map_or(0, |t| t.function_id.0))
                .utf8_opt("fqn", function.as_ref().map(|f| f.fqn.clone()))
                .utf8_opt(
                    "definition_key",
                    function.as_ref().and_then(|f| f.definition_key.clone()),
                )
                .utf8_opt(
                    "kind",
                    function
                        .as_ref()
                        .and_then(|f| f.kind.map(|k| function_kind_str(k).to_string())),
                )
                .utf8_opt(
                    "origin",
                    function
                        .as_ref()
                        .and_then(|f| f.origin.map(|o| function_origin_str(o).to_string())),
                )
                .utf8_opt(
                    "call_site_file",
                    context.tuple.and_then(|t| t.call_site).and_then(|site| {
                        table
                            .as_ref()
                            .as_ref()
                            .and_then(|t| t.file_path(site.file_id).map(str::to_string))
                    }),
                )
                .u32_opt(
                    "call_site_line",
                    context.tuple.and_then(|t| t.call_site).map(|s| s.line),
                )
                .u32_opt(
                    "call_site_start",
                    context
                        .tuple
                        .and_then(|t| t.call_site)
                        .map(|s| s.start_offset),
                )
                .u32_opt(
                    "call_site_end",
                    context
                        .tuple
                        .and_then(|t| t.call_site)
                        .map(|s| s.end_offset),
                )
                .utf8(
                    "edge_kind",
                    context.tuple.map_or("call", |t| edge_kind_str(t.edge_kind)),
                )
                .u64("calls_started", counters.invocations_started)
                .u64("calls_selected", counters.spans_selected)
                .u64("completed_ok", counters.completed_ok)
                .u64("completed_error", counters.completed_error)
                .u64("completed_cancelled", counters.completed_cancelled)
                .u64("completed_exit", counters.completed_exit)
                .u64(
                    "inclusive_ns",
                    sat_u64(counters.inclusive_ns, &mut complete),
                )
                .u64(
                    "direct_child_ns",
                    sat_u64(counters.direct_call_child_inclusive_ns, &mut complete),
                )
                .u64("await_ns", sat_u64(counters.await_ns, &mut complete))
                .u64("self_ns", sat_u64(derived.self_ns, &mut complete))
                .u64("await_count", counters.await_count)
                .bool("timing_complete", derived.complete && complete)
                .utf8_opt("overflow_reason", None);
            builder.push(row);
        }
        for ((reason, edge), counters) in &profile.overflow {
            let mut complete = false;
            let row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8(
                    "call_path_id",
                    format!(
                        "overflow:{}:{}",
                        overflow_reason_str(*reason),
                        edge_kind_str(*edge)
                    ),
                )
                .u32("depth", 0)
                .u32("function_id", 0)
                .utf8("edge_kind", edge_kind_str(*edge))
                .u64("calls_started", counters.invocations_started)
                .u64("calls_selected", counters.spans_selected)
                .u64("completed_ok", counters.completed_ok)
                .u64("completed_error", counters.completed_error)
                .u64("completed_cancelled", counters.completed_cancelled)
                .u64("completed_exit", counters.completed_exit)
                .u64(
                    "inclusive_ns",
                    sat_u64(counters.inclusive_ns, &mut complete),
                )
                .u64(
                    "direct_child_ns",
                    sat_u64(counters.direct_call_child_inclusive_ns, &mut complete),
                )
                .u64("await_ns", sat_u64(counters.await_ns, &mut complete))
                .u64("self_ns", 0)
                .u64("await_count", counters.await_count)
                .bool("timing_complete", false)
                .utf8("overflow_reason", overflow_reason_str(*reason));
            builder.push(row);
        }
    }
    Ok(())
}

/// Iterative memoized depth: walk unresolved ancestors onto a stack,
/// then assign depths downward. A context with no resolvable parent
/// tuple is depth 0.
fn context_depth(
    key: [u8; 32],
    profile: &ExecutionProfile,
    memo: &mut HashMap<[u8; 32], u32>,
) -> u32 {
    let mut chain = Vec::new();
    let mut cursor = key;
    let base = loop {
        if let Some(known) = memo.get(&cursor) {
            break *known;
        }
        if chain.contains(&cursor) || chain.len() > profile.contexts.len() {
            break 0;
        }
        chain.push(cursor);
        match profile
            .contexts
            .get(&bex_prof_store::prof::backend::ContextKey(cursor))
            .and_then(|c| c.tuple)
            .and_then(|t| t.parent_context_key)
        {
            Some(parent) => cursor = parent.0,
            None => {
                memo.insert(cursor, 0);
                break 0;
            }
        }
    };
    let mut depth = base;
    // `chain` still holds the unresolved suffix ending at the entry whose
    // parent resolved to `base`; the last chain entry with a resolved
    // parent sits at base (+1 when the walk ended on a parent link).
    for entry in chain.iter().rev() {
        depth = if memo.contains_key(entry) {
            memo[entry]
        } else {
            let parent_depth = profile
                .contexts
                .get(&bex_prof_store::prof::backend::ContextKey(*entry))
                .and_then(|c| c.tuple)
                .and_then(|t| t.parent_context_key)
                .and_then(|p| memo.get(&p.0).copied());
            let d = parent_depth.map_or(0, |p| p.saturating_add(1));
            memo.insert(*entry, d);
            d
        };
    }
    memo.get(&key).copied().unwrap_or(depth)
}

fn function_kind_str(kind: bex_prof_store::prof::backend::FunctionKindCode) -> &'static str {
    use bex_prof_store::prof::backend::FunctionKindCode as K;
    match kind {
        K::Bytecode => "bytecode",
        K::SysOp => "sysop",
        K::Native => "native",
        K::NativeUnresolved => "native_unresolved",
    }
}

fn function_origin_str(origin: bex_prof_store::prof::backend::FunctionOriginCode) -> &'static str {
    use bex_prof_store::prof::backend::FunctionOriginCode as O;
    match origin {
        O::UserDefined => "user",
        O::Companion => "companion",
        O::Internal => "internal",
        O::Builtin => "builtin",
        O::AutoDerive => "auto_derive",
    }
}

// ── calls ──────────────────────────────────────────────────────────────

fn calls_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<(), QueryError> {
    for (stream, summary) in selected(universe, targets) {
        let execution_id = summary.id.encode();
        let zero = stream.reader.header.as_ref().map(|h| h.zero_unix_ns);
        let profile = folds.fold(stream, summary)?;
        let table = folds.function_table(stream, summary)?;
        for (call_ref, span) in &profile.spans {
            // A retained call is a span whose start fact survived;
            // start-less join state is health accounting, not a row.
            let Some(start) = &span.start else { continue };
            let function = table
                .as_ref()
                .as_ref()
                .and_then(|t| t.function(start.function_id).cloned());
            let mut reasons = Vec::new();
            if start.selection_reasons.root() {
                reasons.push("root".to_string());
            }
            if start.selection_reasons.llm() {
                reasons.push("llm".to_string());
            }
            if start.selection_reasons.manual() {
                reasons.push("manual".to_string());
            }
            let mut roles = Vec::new();
            if start.roles.inputs() {
                roles.push("input".to_string());
            }
            if start.roles.output() {
                roles.push("output".to_string());
            }
            if start.roles.error() {
                roles.push("error".to_string());
            }
            let mut runtime_ids = Vec::new();
            if let Some(initial) = &start.runtime_id {
                runtime_ids.push(initial.runtime_id.to_wire_string());
            }
            let mut overrides: Vec<_> = span.runtime_ids.clone();
            overrides.sort_by_key(|o| o.annotation_ordinal);
            runtime_ids.extend(overrides.iter().map(|o| o.runtime_id.to_wire_string()));

            let input_state = span.input.as_ref().map(|o| o.state);
            let output_state = span.output.as_ref().map(|o| o.state);
            let (error_state, error_id, error_lost) = match &span.terminal_error {
                Some(link) => match link.target {
                    TerminalErrorTarget::Capture(id) => {
                        let value = profile.errors.get(&id).map(|c| c.value);
                        (value, Some(error_id_wire(id)), None)
                    }
                    TerminalErrorTarget::Lost(reason) => (None, None, Some(format!("{reason:?}"))),
                },
                None => (None, None, None),
            };
            let duration = span.end.as_ref().map(|e| e.inclusive_ns);
            let row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8("call_id", call_ref.encode())
                .utf8_opt("parent_call_id", start.parent_call_ref.map(|p| p.encode()))
                .utf8("thread_id", start.thread_ref.encode())
                .utf8_opt("call_path_id", context_id(&start.context_ref))
                .utf8_opt(
                    "call_path_overflow_reason",
                    context_overflow(&start.context_ref),
                )
                .u32("function_id", start.function_id.0)
                .utf8_opt("fqn", function.as_ref().map(|f| f.fqn.clone()))
                .utf8_opt(
                    "definition_key",
                    function.as_ref().and_then(|f| f.definition_key.clone()),
                )
                .utf8_opt(
                    "kind",
                    function
                        .as_ref()
                        .and_then(|f| f.kind.map(|k| function_kind_str(k).to_string())),
                )
                .utf8("edge_kind", edge_kind_str(start.edge_kind))
                .utf8_opt(
                    "call_site_file",
                    start.call_site.and_then(|site| {
                        table
                            .as_ref()
                            .as_ref()
                            .and_then(|t| t.file_path(site.file_id).map(str::to_string))
                    }),
                )
                .u32_opt("call_site_line", start.call_site.map(|s| s.line))
                .u32_opt("call_site_start", start.call_site.map(|s| s.start_offset))
                .u32_opt("call_site_end", start.call_site.map(|s| s.end_offset))
                .u64("started_ns", start.started_ns)
                .u64_opt("ended_ns", span.end.as_ref().map(|e| e.ended_ns))
                .u64_opt("duration_ns", duration)
                .ts_opt("started_at", wall_clock(zero, Some(start.started_ns)))
                .ts_opt(
                    "ended_at",
                    wall_clock(zero, span.end.as_ref().map(|e| e.ended_ns)),
                )
                .utf8_opt(
                    "status",
                    span.end
                        .as_ref()
                        .map(|e| call_status_str(e.status).to_string()),
                )
                .list("selection_reasons", reasons)
                .list("roles", roles.clone())
                .list("runtime_ids", runtime_ids)
                .utf8(
                    "args_state",
                    value_state_str(input_state, start.roles.inputs()),
                )
                .utf8(
                    "output_state",
                    value_state_str(output_state, start.roles.output()),
                )
                .utf8(
                    "error_state",
                    value_state_str(error_state, start.roles.error()),
                )
                .utf8_opt("args_cid", value_cid_wire(input_state))
                .utf8_opt("output_cid", value_cid_wire(output_state))
                .utf8_opt("error_cid", value_cid_wire(error_state))
                .bin_opt("args", value_handle(input_state, start.roles.inputs()))
                .bin_opt("output", value_handle(output_state, start.roles.output()))
                .bin_opt("error", value_handle(error_state, start.roles.error()))
                .utf8_opt("error_id", error_id)
                .utf8_opt("error_lost_reason", error_lost);
            builder.push(row);
        }
    }
    Ok(())
}

fn call_status_str(status: bex_prof_store::prof::record::FunctionEndStatus) -> &'static str {
    use bex_prof_store::prof::record::FunctionEndStatus as S;
    match status {
        S::Ok => "ok",
        S::Errored => "errored",
        S::Cancelled => "cancelled",
        S::Exited => "exited",
    }
}

// ── errors ─────────────────────────────────────────────────────────────

fn errors_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<(), QueryError> {
    use bex_prof_store::prof::backend::ErrorStack;
    for (stream, summary) in selected(universe, targets) {
        let execution_id = summary.id.encode();
        let profile = folds.fold(stream, summary)?;
        let table = folds.function_table(stream, summary)?;
        // One pass over the spans builds the error -> terminal-call index;
        // per-error scans were O(errors x spans).
        let mut terminal_ids: HashMap<bex_prof_store::prof::backend::ErrorCaptureId, Vec<String>> =
            HashMap::new();
        for (call_ref, span) in &profile.spans {
            if let Some(bex_prof_store::prof::backend::TerminalErrorRef {
                target: TerminalErrorTarget::Capture(target),
                ..
            }) = span.terminal_error
            {
                terminal_ids
                    .entry(target)
                    .or_default()
                    .push(call_ref.encode());
            }
        }
        for (id, capture) in &profile.errors {
            let function = table
                .as_ref()
                .as_ref()
                .and_then(|t| t.function(capture.throw_function_id).cloned());
            let (stack_complete, stack) = match profile.error_stack(*id) {
                Ok(ErrorStack::Complete(tuples)) => (
                    true,
                    tuples
                        .iter()
                        .map(|tuple| {
                            table
                                .as_ref()
                                .as_ref()
                                .and_then(|t| t.function(tuple.function_id))
                                .map_or_else(
                                    || format!("function#{}", tuple.function_id.0),
                                    |f| f.fqn.clone(),
                                )
                        })
                        .collect(),
                ),
                _ => (false, Vec::new()),
            };
            let terminal_call_ids = terminal_ids.remove(id).unwrap_or_default();
            let value_state = Some(capture.value);
            let row = Row::default()
                .utf8("execution_id", execution_id.clone())
                .utf8("error_id", error_id_wire(*id))
                .utf8("throw_call_id", capture.throw_call_ref.encode())
                .utf8("throw_thread_id", id.thread_ref.encode())
                .utf8_opt("throw_call_path_id", context_id(&capture.throw_context_ref))
                .u32("throw_function_id", capture.throw_function_id.0)
                .utf8_opt("throw_fqn", function.as_ref().map(|f| f.fqn.clone()))
                .utf8_opt(
                    "throw_site_file",
                    capture.throw_site.and_then(|site| {
                        table
                            .as_ref()
                            .as_ref()
                            .and_then(|t| t.file_path(site.file_id).map(str::to_string))
                    }),
                )
                .u32_opt("throw_site_line", capture.throw_site.map(|s| s.line))
                .u32_opt(
                    "throw_site_start",
                    capture.throw_site.map(|s| s.start_offset),
                )
                .u32_opt("throw_site_end", capture.throw_site.map(|s| s.end_offset))
                .utf8(
                    "kind",
                    match capture.kind {
                        bex_prof_store::prof::backend::ErrorUnwindKind::Fresh => "fresh",
                        bex_prof_store::prof::backend::ErrorUnwindKind::Rethrow => "rethrow",
                    },
                )
                .utf8(
                    "source",
                    match capture.source {
                        bex_prof_store::prof::backend::ErrorSource::Bytecode => "bytecode",
                        bex_prof_store::prof::backend::ErrorSource::NativeCall => "native_call",
                        bex_prof_store::prof::backend::ErrorSource::EngineCall => "engine_call",
                        bex_prof_store::prof::backend::ErrorSource::FutureResume => "future_resume",
                    },
                )
                .utf8("value_state", value_state_str(value_state, true))
                .utf8_opt("value_cid", value_cid_wire(value_state))
                .bin_opt("value", value_handle(value_state, true))
                .bool("stack_complete", stack_complete)
                .list("stack", stack)
                .list("terminal_call_ids", terminal_call_ids);
            builder.push(row);
        }
    }
    Ok(())
}

// ── functions ──────────────────────────────────────────────────────────

fn functions_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
) -> Result<(), QueryError> {
    let mut seen_programs: std::collections::HashSet<[u8; 16]> = std::collections::HashSet::new();
    for (stream, summary) in universe.executions().collect::<Vec<_>>() {
        let Some(program_id) = summary.program_id else {
            continue;
        };
        if !seen_programs.insert(program_id.0) {
            continue;
        }
        let engine = stream
            .reader
            .engines
            .iter()
            .find(|e| e.engine_id == summary.engine_id);
        let table = folds.function_table(stream, summary)?;
        let Some(table) = table.as_ref().as_ref() else {
            continue;
        };
        push_function_rows(builder, &hex::encode(program_id.0), engine, table);
    }
    Ok(())
}

fn push_function_rows(
    builder: &mut BatchBuilder,
    program_id: &str,
    engine: Option<&bex_prof_store::prof::backend::EngineStarted>,
    table: &FunctionTable,
) {
    for entry in &table.functions {
        let row = Row::default()
            .utf8("program_id", program_id.to_string())
            .u32("function_id", entry.function_id.0)
            .utf8("fqn", entry.fqn.clone())
            .utf8("display_name", entry.display_name.clone())
            .utf8_opt("definition_key", entry.definition_key.clone())
            .utf8("kind", entry.kind.map_or("bytecode", function_kind_str))
            .utf8_opt("kind_detail", entry.kind_detail.clone())
            .utf8("origin", entry.origin.map_or("user", function_origin_str))
            .utf8_opt("source_file", entry.source_file.clone())
            .u32_opt("source_start", entry.source_span.map(|s| s.start))
            .u32_opt("source_end", entry.source_span.map(|s| s.end))
            .utf8_opt("package", entry.package_name.clone())
            .utf8("namespace", entry.namespace.join("."))
            .utf8_opt("revision_id", engine.and_then(|e| e.revision_label.clone()))
            .utf8_opt("source_label", engine.and_then(|e| e.source_label.clone()));
        builder.push(row);
    }
}

// ── health ─────────────────────────────────────────────────────────────

fn health_rows(
    builder: &mut BatchBuilder,
    universe: &ProfilesUniverse,
    folds: &FoldCache,
    targets: Option<&[String]>,
) -> Result<(), QueryError> {
    for (stream, summary) in selected(universe, targets) {
        let execution_id = summary.id.encode();
        let profile = folds.fold(stream, summary)?;
        let mut push = |plane: &'static str,
                        metric: String,
                        value: u64,
                        edge: Option<String>,
                        reason: Option<String>| {
            builder.push(
                Row::default()
                    .utf8("execution_id", execution_id.clone())
                    .utf8("metric", metric)
                    .utf8("plane", plane)
                    .u64("value", value)
                    .utf8_opt("edge_kind", edge)
                    .utf8_opt("reason", reason),
            );
        };
        if let Some(health) = &summary.health {
            for (name, value) in health_counters(health) {
                push("execution", name.to_string(), value, None, None);
            }
        }
        push(
            "cct",
            "counter_saturated".to_string(),
            u64::from(profile.cct_health.counter_saturated),
            None,
            None,
        );
        push(
            "cct",
            "await_counter_saturated".to_string(),
            u64::from(profile.cct_health.await_counter_saturated),
            None,
            None,
        );
        push(
            "cct",
            "self_time_underflow".to_string(),
            u64::from(profile.cct_health.self_time_underflow),
            None,
            None,
        );
        for ((reason, edge), counters) in &profile.overflow {
            push(
                "overflow",
                "calls_started".to_string(),
                counters.invocations_started,
                Some(edge_kind_str(*edge).to_string()),
                Some(overflow_reason_str(*reason).to_string()),
            );
        }
        push(
            "process",
            "data_file_count".to_string(),
            summary.data_segment_count,
            None,
            None,
        );
        let issues = match &profile.data_state {
            bex_prof_store::prof::backend::DataState::Complete => None,
            bex_prof_store::prof::backend::DataState::Incomplete(issues) => Some(issues),
        };
        push(
            "data",
            "data_state".to_string(),
            u64::from(issues.is_none()),
            None,
            Some(if issues.is_none() {
                "complete".to_string()
            } else {
                "incomplete".to_string()
            }),
        );
        for issue in issues.into_iter().flatten() {
            push(
                "data",
                "data_issue".to_string(),
                1,
                None,
                Some(format!("{issue:?}")),
            );
        }
    }
    Ok(())
}

fn health_counters(
    health: &bex_prof_store::prof::backend::ExecutionHealthSnapshot,
) -> Vec<(&'static str, u64)> {
    vec![
        ("corrupt_records", health.corrupt_records),
        (
            "active_thread_capacity_exceeded",
            health.active_thread_capacity_exceeded,
        ),
        (
            "active_call_capacity_exceeded",
            health.active_call_capacity_exceeded,
        ),
        ("join_capacity_exceeded", health.join_capacity_exceeded),
        ("unmatched_call_facts", health.unmatched_call_facts),
        ("unmatched_thread_facts", health.unmatched_thread_facts),
        ("clock_invalid", health.clock_invalid),
        ("cct_file_publish_failed", health.cct_segment_publish_failed),
        ("evidence_queue_full", health.evidence_queue_full),
        (
            "evidence_file_publish_failed",
            health.evidence_segment_publish_failed,
        ),
        (
            "structural_transport_exceeded",
            health.structural_transport_exceeded,
        ),
        (
            "value_attempt_transport_exceeded",
            health.value_attempt_transport_exceeded,
        ),
        ("applicable_error_unwinds", health.applicable_error_unwinds),
        ("error_captures_queued", health.error_captures_queued),
        ("error_captures_committed", health.error_captures_committed),
        (
            "error_capture_attempt_transport_exceeded",
            health.error_capture_attempt_transport_exceeded,
        ),
        (
            "error_capture_missing_structural_join",
            health.error_capture_missing_structural_join,
        ),
        (
            "error_capture_start_uncommitted",
            health.error_capture_start_uncommitted,
        ),
        (
            "error_capture_evidence_queue_full",
            health.error_capture_evidence_queue_full,
        ),
        (
            "error_capture_evidence_publish_failed",
            health.error_capture_evidence_publish_failed,
        ),
        (
            "terminal_error_links_observed",
            health.terminal_error_links_observed,
        ),
        (
            "terminal_error_links_queued",
            health.terminal_error_links_queued,
        ),
        (
            "terminal_error_links_committed",
            health.terminal_error_links_committed,
        ),
        (
            "terminal_error_link_transport_exceeded",
            health.terminal_error_link_transport_exceeded,
        ),
        (
            "terminal_error_link_start_uncommitted",
            health.terminal_error_link_start_uncommitted,
        ),
        (
            "terminal_error_link_evidence_publish_failed",
            health.terminal_error_link_evidence_publish_failed,
        ),
    ]
}

// ── internal relations ─────────────────────────────────────────────────

fn streams_rows(builder: &mut BatchBuilder, universe: &ProfilesUniverse) {
    for stream in &universe.streams {
        let header = stream.reader.header.as_ref();
        let row = Row::default()
            .utf8("process_id", hex::encode(stream.reader.stream.0.0))
            .u32_opt("os_pid", header.map(|h| h.pid))
            .u64_opt("zero_unix_ns", header.map(|h| h.zero_unix_ns))
            .utf8_opt("baml_version", header.map(|h| h.baml_version.clone()))
            .utf8_opt("os_arch", header.map(|h| h.os_arch.clone()))
            .bool("alive", stream.reader.alive)
            .u64("meta_hw", stream.reader.high_water.meta)
            .u64("data_hw", stream.reader.high_water.data);
        builder.push(row);
    }
}

fn segments_rows(builder: &mut BatchBuilder, universe: &ProfilesUniverse) {
    for stream in &universe.streams {
        let stream_hex = hex::encode(stream.reader.stream.0.0);
        for (plane, high_water) in [
            ("meta", stream.reader.high_water.meta),
            ("data", stream.reader.high_water.data),
        ] {
            for sequence in 1..=high_water {
                let rel = format!("streams/{stream_hex}/{plane}/{sequence:020}.baml{plane}");
                let path = universe.root().join(&rel);
                let bytes = std::fs::read(&path).ok();
                let (count, payload_len, ok) = match (&bytes, plane) {
                    (Some(bytes), "meta") => {
                        match bex_prof_store::prof::backend::decode_meta_segment(
                            bytes,
                            stream.reader.stream.0,
                        ) {
                            Ok(decoded) => {
                                (Some(decoded.records.len() as u64), bytes.len() as u64, true)
                            }
                            Err(_) => (None, bytes.len() as u64, false),
                        }
                    }
                    (Some(bytes), _) => {
                        match bex_prof_store::prof::backend::decode_data_segment(
                            bytes,
                            stream.reader.stream.0,
                        ) {
                            Ok(decoded) => {
                                (Some(decoded.groups.len() as u64), bytes.len() as u64, true)
                            }
                            Err(_) => (None, bytes.len() as u64, false),
                        }
                    }
                    (None, _) => (None, 0, false),
                };
                let row = Row::default()
                    .utf8("process_id", stream_hex.clone())
                    .utf8("plane", plane)
                    .u64("sequence", sequence)
                    .utf8("path", rel)
                    .u64_opt("record_or_group_count", count)
                    .u64("payload_len", payload_len)
                    .bool("checksum_ok", ok)
                    .bool("decode_ok", ok);
                builder.push(row);
            }
        }
    }
}

fn cas_rows(builder: &mut BatchBuilder, universe: &ProfilesUniverse) {
    let cas_root = universe.root().join("cas/sha256");
    let Ok(shards) = std::fs::read_dir(&cas_root) else {
        return;
    };
    let mut paths: Vec<std::path::PathBuf> = shards
        .filter_map(Result::ok)
        .filter(|entry| entry.path().is_dir())
        .filter_map(|shard| std::fs::read_dir(shard.path()).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|e| e == "bamlvalue"))
        .collect();
    paths.sort();
    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            continue;
        };
        let Ok(object) = bex_prof_store::prof::backend::decode_cas_object(&bytes) else {
            continue;
        };
        let rel = format!(
            "cas/sha256/{}/{}",
            &hex::encode(object.cid.0)[..2],
            path.file_name()
                .map(|n| n.to_string_lossy())
                .unwrap_or_default()
        );
        let row = Row::default()
            .utf8("cid", format!("bamlv_1_{}", hex::encode(object.cid.0)))
            .u32("codec", u32::from(object.codec.0))
            .u64("body_len", object.body.len() as u64)
            .utf8("path", rel);
        builder.push(row);
    }
}

// Referenced for the reader-facing counter derivation; kept here so a
// future direct-counters relation reuses the same import set.
const _: fn(&CctCounters) = |_| {};

#[cfg(test)]
mod provider_coverage {
    use super::provider_for;

    /// Every relation the widest catalog profile exposes must have a row
    /// builder; a rename on either side breaks this before it can become
    /// a runtime error.
    #[test]
    fn every_catalog_relation_has_a_row_builder() {
        for relation in baml_query::catalog::CatalogProfile::internal().relations() {
            assert!(
                provider_for(relation.name).is_some(),
                "catalog relation {} has no provider mapping",
                relation.name
            );
        }
    }
}
