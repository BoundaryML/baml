//! Host run lifecycle history and compatibility readers for older log artifacts.
//!
//! New logs use the shared evidence/CAS store. Lifecycle records remain separate.

#[cfg(not(target_arch = "wasm32"))]
mod lifecycle_writer;
pub mod logs;
#[cfg(not(target_arch = "wasm32"))]
const LOG_STORE_LINK: &str = "log-store-v1.json";
#[cfg(not(target_arch = "wasm32"))]
pub mod path;

#[cfg(not(target_arch = "wasm32"))]
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use std::{io, path::Path};

#[cfg(not(target_arch = "wasm32"))]
use self::{
    lifecycle_writer::LifecycleWriter,
    path::{
        BoundaryHistoryPath, build_boundary_history_path, find_boundary_dir, list_boundary_dirs,
    },
};
#[cfg(not(target_arch = "wasm32"))]
use crate::run::RunOutcome;
use crate::{
    ids::BoundaryId,
    run::{
        CancellationState, DiagnosticSeverity, FunctionName, PayloadEvent, PayloadId,
        RedactionMetadata, Run, RunDiagnostic, RunError, RunErrorClass, RunFilter, RunResult,
        RunRetentionState, RunStatus, RunSummary, RunTarget, RunVisibility, RunVisibilityFilter,
    },
    value::{
        BlobRef, BlobStore, CaptureLossRecord, RunCompletedRecord, RunStartedRecord, ValueCodec,
        ValueFileRecord, read_bamlvalue_from_bytes,
    },
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryValueBody {
    pub codec: ValueCodec,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HistoryValueBodyUnavailableReason {
    BlobStoreUnavailable,
    BlobMissing,
    BlobInvalid,
    BlobIntegrityMismatch,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryValueBodyUnavailable {
    pub reason: HistoryValueBodyUnavailableReason,
    pub diagnostic: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HistoryValueReadResult {
    Available(HistoryValueBody),
    Missing,
    BodyUnavailable(HistoryValueBodyUnavailable),
}

impl HistoryValueReadResult {
    #[must_use]
    pub fn into_body(self) -> Option<HistoryValueBody> {
        match self {
            Self::Available(body) => Some(body),
            Self::Missing | Self::BodyUnavailable(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryValueSegment {
    pub label: String,
    pub bytes: Vec<u8>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug)]
pub struct HistoryStore {
    inner: Arc<Mutex<HistoryStoreInner>>,
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Debug)]
struct HistoryStoreInner {
    search_roots: Vec<PathBuf>,
    log_store_root: Option<PathBuf>,
    boundaries: HashMap<BoundaryId, BoundaryState>,
}

#[cfg(not(target_arch = "wasm32"))]
struct BoundaryState {
    path: BoundaryHistoryPath,
    started: RunStartedRecord,
    writer: LifecycleWriter,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for BoundaryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundaryState")
            .field("path", &self.path)
            .field("started", &self.started)
            .finish_non_exhaustive()
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HistoryStore {
    #[must_use]
    pub fn new(search_roots: Vec<PathBuf>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HistoryStoreInner {
                search_roots,
                log_store_root: None,
                boundaries: HashMap::new(),
            })),
        }
    }

    #[must_use]
    pub fn with_log_store_root(self, root: PathBuf) -> Self {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .log_store_root = Some(root);
        self
    }

    pub fn begin(
        &self,
        project_root: impl AsRef<Path>,
        start: &crate::run::StartRunContext,
    ) -> io::Result<()> {
        let path = build_boundary_history_path(project_root.as_ref(), start);
        let started = RunStartedRecord {
            request: start.request.clone(),
            created_at_ms: start.created_at_ms,
            time_anchor: start.time_anchor,
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let writer = LifecycleWriter::new(path.clone(), start.boundary_id);
        writer.write_run_started(&started)?;
        if let Some(root) = &inner.log_store_root {
            let root = if root.is_absolute() {
                root.clone()
            } else {
                std::env::current_dir()?.join(root)
            };
            std::fs::write(
                path.boundary_dir.join(LOG_STORE_LINK),
                serde_json::to_vec(&root).map_err(io::Error::other)?,
            )?;
        }
        if !inner.search_roots.contains(&path.project_root) {
            inner.search_roots.push(path.project_root.clone());
        }
        inner.boundaries.insert(
            start.boundary_id,
            BoundaryState {
                path,
                started,
                writer,
            },
        );
        Ok(())
    }

    pub fn complete(
        &self,
        boundary_id: BoundaryId,
        outcome: &RunOutcome,
        completed_at_ms: u64,
    ) -> io::Result<()> {
        let record = completed_record(outcome, completed_at_ms);
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Remove first: a completion that fails to write must not leave the
        // boundary (and its open writer) parked in the map forever. The
        // error still reaches the caller.
        let Some(state) = inner.boundaries.remove(&boundary_id) else {
            return Ok(());
        };
        state.writer.write_run_completed(&record)?;
        Ok(())
    }

    #[must_use]
    pub fn list(&self, filter: &RunFilter) -> Vec<RunSummary> {
        let search_roots = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .search_roots
            .clone();
        let mut summaries = list_boundary_dirs(&search_roots)
            .into_iter()
            .filter_map(|dir| open_boundary_from_dir(&dir).ok())
            .filter(|run| history_run_matches_filter(run, filter))
            .map(|run| summarize_history_run(&run))
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.created_at_ms));
        summaries
    }

    pub fn open(&self, boundary_id: BoundaryId) -> io::Result<Run> {
        let (known_dir, search_roots) = {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                inner
                    .boundaries
                    .get(&boundary_id)
                    .map(|state| state.path.boundary_dir.clone()),
                inner.search_roots.clone(),
            )
        };
        let dir = known_dir
            .or_else(|| find_boundary_dir(&search_roots, boundary_id))
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "history boundary {} was not found",
                        boundary_id.to_wire_string()
                    ),
                )
            })?;
        let mut run = open_boundary_from_dir(&dir)?;
        match read_shared_log_history(&dir, boundary_id, &search_roots) {
            Ok(Some((payloads, incomplete))) => {
                if !payloads.is_empty() {
                    run.payloads = payloads;
                }
                if incomplete {
                    run.diagnostics.push(history_diagnostic(
                        "logHistoryIncomplete",
                        "Some captured history records or values are unavailable".to_owned(),
                    ));
                }
            }
            Ok(None) => {}
            Err(error) => run.diagnostics.push(history_diagnostic(
                "logHistoryUnavailable",
                format!("Could not read shared log history: {error}"),
            )),
        }
        Ok(run)
    }

    pub fn read_value(
        &self,
        boundary_id: BoundaryId,
        value_ref_id: &str,
    ) -> io::Result<Option<HistoryValueBody>> {
        self.read_value_result(boundary_id, value_ref_id)
            .map(HistoryValueReadResult::into_body)
    }

    pub fn read_value_result(
        &self,
        boundary_id: BoundaryId,
        value_ref_id: &str,
    ) -> io::Result<HistoryValueReadResult> {
        let (known_dir, search_roots) = {
            let inner = self
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            (
                inner
                    .boundaries
                    .get(&boundary_id)
                    .map(|state| state.path.boundary_dir.clone()),
                inner.search_roots.clone(),
            )
        };
        let Some(dir) = known_dir.or_else(|| find_boundary_dir(&search_roots, boundary_id)) else {
            return Ok(HistoryValueReadResult::Missing);
        };
        if let Some(cid) = logs::value_ref_cid(value_ref_id) {
            return read_shared_log_value(&dir, boundary_id, cid, &search_roots);
        }
        let value_segments = read_value_segments(&dir)?;
        let blob_store = BlobStore::for_boundary_dir(&dir);
        read_value_from_segments_with_blobs_result(&value_segments, value_ref_id, Some(&blob_store))
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn shared_log_reader(
    boundary_dir: &Path,
    boundary_id: BoundaryId,
    search_roots: &[PathBuf],
) -> io::Result<Option<crate::prof::backend::ExecutionReader>> {
    use crate::prof::backend::{ProfilerSession, StreamReader, list_executions};

    let project_root = boundary_dir.ancestors().nth(3).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid lifecycle history directory",
        )
    })?;
    let mut roots = Vec::new();
    match std::fs::read(boundary_dir.join(LOG_STORE_LINK)) {
        Ok(bytes) => {
            let root: PathBuf = serde_json::from_slice(&bytes).map_err(io::Error::other)?;
            if !root.is_absolute() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "log store path must be absolute",
                ));
            }
            roots.push(root);
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    for root in std::iter::once(project_root).chain(search_roots.iter().map(PathBuf::as_path)) {
        let root = ProfilerSession::resolve_store_root(root);
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    for root in roots {
        if !root.exists() {
            continue;
        }
        let executions =
            list_executions(&root).map_err(|error| io::Error::other(format!("{error:?}")))?;
        if let Some(execution) = executions
            .into_iter()
            .find(|execution| execution.runtime_id == Some(boundary_id))
        {
            return StreamReader::open(&root, execution.stream)
                .and_then(|reader| reader.execution(execution.id))
                .map(Some)
                .map_err(|error| io::Error::other(format!("{error:?}")));
        }
    }
    Ok(None)
}

#[cfg(not(target_arch = "wasm32"))]
fn read_shared_log_history(
    boundary_dir: &Path,
    boundary_id: BoundaryId,
    search_roots: &[PathBuf],
) -> io::Result<Option<(Vec<PayloadEvent>, bool)>> {
    use crate::prof::backend::{DataState, ValueState};

    let Some(reader) = shared_log_reader(boundary_dir, boundary_id, search_roots)? else {
        return Ok(None);
    };
    let mut profile = reader
        .load()
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let incomplete = !matches!(profile.data_state, DataState::Complete)
        || profile.logs.iter().any(|log| {
            matches!(log.data, ValueState::Lost(_)) || matches!(log.context, ValueState::Lost(_))
        })
        || profile.summary.health.is_some_and(|health| {
            health.value_attempt_transport_exceeded > 0
                || health.structural_transport_exceeded > 0
                || health.evidence_segment_publish_failed > 0
                || health.evidence_queue_full > 0
        });
    profile.logs.sort_by_key(|log| log.timestamp_ms);
    let payloads = profile
        .logs
        .iter()
        .enumerate()
        .map(|(index, log)| logs::log_payload(log, index))
        .collect();
    Ok(Some((payloads, incomplete)))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_shared_log_value(
    boundary_dir: &Path,
    boundary_id: BoundaryId,
    cid: crate::prof::backend::ValueCid,
    search_roots: &[PathBuf],
) -> io::Result<HistoryValueReadResult> {
    use crate::prof::backend::ValueState;

    let Some(reader) = shared_log_reader(boundary_dir, boundary_id, search_roots)? else {
        return Ok(HistoryValueReadResult::Missing);
    };
    let profile = reader
        .load()
        .map_err(|error| io::Error::other(format!("{error:?}")))?;
    let belongs_to_run = profile.logs.iter().any(|log| {
        [log.data, log.context].into_iter().any(|state| {
            matches!(state, ValueState::Available { cid: value_cid, .. } if value_cid == cid)
        })
    });
    if !belongs_to_run {
        return Ok(HistoryValueReadResult::Missing);
    }
    Ok(match reader.read_value(cid) {
        Ok(value) if value.codec.0 == 1 => HistoryValueReadResult::Available(HistoryValueBody {
            codec: ValueCodec::BamlOutboundValue,
            body: value.body,
        }),
        Ok(value) => HistoryValueReadResult::BodyUnavailable(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobInvalid,
            diagnostic: format!("unsupported log value codec {}", value.codec.0),
        }),
        Err(error) => HistoryValueReadResult::BodyUnavailable(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobInvalid,
            diagnostic: format!("shared log value is unavailable: {error:?}"),
        }),
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn completed_record(outcome: &RunOutcome, completed_at_ms: u64) -> RunCompletedRecord {
    RunCompletedRecord {
        status: outcome.status(),
        completed_at_ms,
        renderer_hint: match outcome {
            RunOutcome::Succeeded(result) => result.renderer_hint.clone(),
            RunOutcome::Failed(_) | RunOutcome::Cancelled(_) | RunOutcome::Panicked(_) => None,
        },
        result_value_ref: match outcome {
            RunOutcome::Succeeded(result) => result.value_ref.clone(),
            RunOutcome::Failed(_) | RunOutcome::Cancelled(_) | RunOutcome::Panicked(_) => None,
        },
        error: match outcome {
            RunOutcome::Failed(error) | RunOutcome::Panicked(error) => Some(error.clone()),
            RunOutcome::Succeeded(_) | RunOutcome::Cancelled(_) => None,
        },
        cancellation: match outcome {
            RunOutcome::Cancelled(cancellation) => Some(cancellation.clone()),
            RunOutcome::Succeeded(_) | RunOutcome::Failed(_) | RunOutcome::Panicked(_) => None,
        },
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn open_boundary_from_dir(dir: &Path) -> io::Result<Run> {
    open_boundary_from_segments(&read_value_segments(dir)?, Some(dir))
}

#[cfg(not(target_arch = "wasm32"))]
fn read_value_segments(dir: &Path) -> io::Result<Vec<HistoryValueSegment>> {
    value_segment_paths(dir)
        .into_iter()
        .map(|path| {
            std::fs::read(&path)
                .map(|bytes| HistoryValueSegment {
                    label: path.display().to_string(),
                    bytes,
                })
                .map_err(|error| {
                    io::Error::new(
                        error.kind(),
                        format!("failed to read value segment {}: {error}", path.display()),
                    )
                })
        })
        .collect()
}

pub fn open_boundary_from_value_segments(
    value_segments: &[HistoryValueSegment],
) -> io::Result<Run> {
    open_boundary_from_segments(value_segments, None)
}

fn open_boundary_from_segments(
    value_segments: &[HistoryValueSegment],
    fallback_dir: Option<&Path>,
) -> io::Result<Run> {
    let mut header_boundary_ids = Vec::new();
    let mut started = None;
    let mut completed = None;
    let mut logs = Vec::new();
    let mut capture_losses = Vec::new();
    let mut diagnostics = Vec::new();

    for segment in value_segments {
        let parsed = read_bamlvalue_from_bytes(&segment.bytes)?;
        header_boundary_ids.push(boundary_id_from_header(&parsed.header.boundary_id)?);
        if parsed.truncated {
            diagnostics.push(history_diagnostic(
                "historyValueTornTail",
                format!(
                    "Value segment {} ended with a torn trailing record; complete prefix was retained",
                    segment.label
                ),
            ));
        }
        for record in parsed.records {
            match record {
                ValueFileRecord::RunStarted(record) => started = Some(record),
                ValueFileRecord::RunCompleted(record) => completed = Some(record),
                ValueFileRecord::LogEvent(record) => logs.push(record),
                ValueFileRecord::CaptureLoss(record) => capture_losses.push(record),
            }
        }
    }

    let started = started.ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "history boundary {} omitted run started record",
                fallback_dir
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_else(|| "byte segments".to_string())
            ),
        )
    })?;
    let boundary_id =
        boundary_id_from_header_or_fallback(fallback_dir, &header_boundary_ids, &started)?;

    diagnostics.extend(
        capture_losses
            .into_iter()
            .map(capture_loss_replay_diagnostic),
    );
    // Segments are read thread-major; replay in event order so payload ids
    // follow time, not which BEX thread wrote first. The sort is stable, so
    // same-millisecond logs keep their on-disk order.
    logs.sort_by_key(|record| record.event.timestamp_ms);
    let payloads = logs
        .into_iter()
        .enumerate()
        .map(|(index, record)| PayloadEvent {
            id: PayloadId(u64::try_from(index).unwrap_or(u64::MAX).saturating_add(1)),
            timestamp_ms: record.event.timestamp_ms,
            kind: crate::run::PayloadKind::Log(crate::run::LogPayload {
                level: record.event.level,
                message: record
                    .event
                    .message_preview
                    .unwrap_or_else(|| "captured log".to_string()),
                source: record.event.source,
                value_ref: Some(record.value_ref),
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        })
        .collect();

    let completed_at_ms = completed.as_ref().map(|record| record.completed_at_ms);
    let status = completed
        .as_ref()
        .map_or(RunStatus::Running, |record| record.status);
    let (result, error, cancellation) = outcome_fields_from_replay(completed);

    Ok(Run {
        boundary_id,
        target: started.request.target.clone(),
        visibility: started.request.target.default_visibility(None),
        status,
        created_at_ms: started.created_at_ms,
        started_at_ms: Some(started.created_at_ms),
        completed_at_ms,
        time_anchor: started.time_anchor,
        request: started.request,
        result,
        error,
        cancellation,
        payloads,
        diagnostics,
        cursor: crate::run::RunCursor(0),
    })
}

pub fn read_value_from_segments_result(
    value_segments: &[HistoryValueSegment],
    value_ref_id: &str,
) -> io::Result<HistoryValueReadResult> {
    read_value_from_segments_with_blobs_result(value_segments, value_ref_id, None)
}

pub fn read_value_from_segments_with_blobs_result(
    value_segments: &[HistoryValueSegment],
    value_ref_id: &str,
    blob_store: Option<&BlobStore>,
) -> io::Result<HistoryValueReadResult> {
    for segment in value_segments {
        let parsed = read_bamlvalue_from_bytes(&segment.bytes)?;
        for record in parsed.records {
            let ValueFileRecord::LogEvent(record) = record else {
                continue;
            };
            if record.value_ref.id == value_ref_id {
                return match hydrate_value_body(record.body, record.blob_ref.as_ref(), blob_store)?
                {
                    Ok(body) => Ok(HistoryValueReadResult::Available(HistoryValueBody {
                        codec: record.value_ref.codec,
                        body,
                    })),
                    Err(unavailable) => Ok(HistoryValueReadResult::BodyUnavailable(unavailable)),
                };
            }
        }
    }
    Ok(HistoryValueReadResult::Missing)
}

fn hydrate_value_body(
    inline_body: Vec<u8>,
    blob_ref: Option<&BlobRef>,
    blob_store: Option<&BlobStore>,
) -> io::Result<Result<Vec<u8>, HistoryValueBodyUnavailable>> {
    let Some(blob_ref) = blob_ref else {
        return Ok(Ok(inline_body));
    };
    if let Err(error) = blob_ref.validate() {
        return Ok(Err(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobInvalid,
            diagnostic: format!(
                "value body blob ref {} is invalid: {error}",
                blob_ref_label(blob_ref)
            ),
        }));
    }
    let Some(blob_store) = blob_store else {
        return Ok(Err(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobStoreUnavailable,
            diagnostic: format!(
                "value body is blob-backed but no blob store is available for {}",
                blob_ref_label(blob_ref)
            ),
        }));
    };
    match blob_store.read_blob(blob_ref) {
        Ok(Some(body)) => Ok(Ok(body)),
        Ok(None) => Ok(Err(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobMissing,
            diagnostic: format!("value body blob {} is missing", blob_ref_label(blob_ref)),
        })),
        Err(error) if error.kind() == io::ErrorKind::InvalidData => {
            Ok(Err(HistoryValueBodyUnavailable {
                reason: HistoryValueBodyUnavailableReason::BlobIntegrityMismatch,
                diagnostic: format!(
                    "value body blob {} failed integrity verification: {error}",
                    blob_ref_label(blob_ref)
                ),
            }))
        }
        Err(error) => Err(error),
    }
}

fn blob_ref_label(blob_ref: &BlobRef) -> String {
    format!(
        "{}:{} ({} bytes)",
        blob_ref.algorithm, blob_ref.digest, blob_ref.size_bytes
    )
}

fn outcome_fields_from_replay(
    completed: Option<RunCompletedRecord>,
) -> (
    Option<RunResult>,
    Option<RunError>,
    Option<CancellationState>,
) {
    let Some(completed) = completed else {
        return (None, None, None);
    };
    match completed.status {
        RunStatus::Succeeded => (
            Some(RunResult {
                value_ref: completed.result_value_ref,
                // The durable record does not carry the inline bytes; a
                // replayed run's value comes from the value store via its
                // `value_ref`.
                value: None,
                renderer_hint: completed.renderer_hint,
                supporting_payload_ids: Vec::new(),
            }),
            None,
            None,
        ),
        RunStatus::Failed | RunStatus::Panicked => (
            None,
            Some(completed.error.unwrap_or_else(|| RunError {
                class: if completed.status == RunStatus::Panicked {
                    RunErrorClass::Panic
                } else {
                    RunErrorClass::Runtime
                },
                message: "run failed".to_string(),
                details: None,
                value_ref: None,
            })),
            None,
        ),
        RunStatus::Cancelled => (None, None, completed.cancellation),
        RunStatus::Pending
        | RunStatus::Running
        | RunStatus::WaitingForInput
        | RunStatus::WaitingForEnv
        | RunStatus::Cancelling => (None, None, None),
    }
}

fn boundary_id_from_header_or_fallback(
    fallback_dir: Option<&Path>,
    header_boundary_ids: &[BoundaryId],
    started: &RunStartedRecord,
) -> io::Result<BoundaryId> {
    if let Some(first) = header_boundary_ids.first().copied() {
        if header_boundary_ids.iter().all(|id| *id == first) {
            return Ok(first);
        }
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "history boundary {} has inconsistent value segment boundary ids",
                fallback_dir
                    .map(|dir| dir.display().to_string())
                    .unwrap_or_else(|| "byte segments".to_string())
            ),
        ));
    }
    fallback_dir
        .and_then(boundary_id_from_dir_name)
        .ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "history boundary for project {} omitted canonical boundary id",
                    started.request.project_id.0
                ),
            )
        })
}

fn boundary_id_from_header(bytes: &[u8]) -> io::Result<BoundaryId> {
    let bytes: [u8; 16] = bytes.try_into().map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                ".bamlvalue header boundary id must be 16 bytes, got {}",
                bytes.len()
            ),
        )
    })?;
    Ok(BoundaryId::from_bytes(bytes))
}

fn boundary_id_from_dir_name(dir: &Path) -> Option<BoundaryId> {
    let name = dir.file_name()?.to_str()?;
    name.char_indices()
        .rev()
        .find_map(|(index, _)| BoundaryId::from_wire_str(&name[index..]))
}

fn capture_loss_replay_diagnostic(record: CaptureLossRecord) -> RunDiagnostic {
    history_diagnostic(
        "valueCaptureLoss",
        record.message.unwrap_or_else(|| {
            format!(
                "Skipped {} captured {} value(s) because the trace capture queue was full",
                record.skipped_count,
                record.kind.as_wire_str()
            )
        }),
    )
}

#[cfg(not(target_arch = "wasm32"))]
fn value_segment_paths(dir: &Path) -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let Ok(threads) = std::fs::read_dir(dir) else {
        return paths;
    };
    for thread in threads.flatten() {
        let thread_path = thread.path();
        if !thread_path.is_dir() {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&thread_path) else {
            continue;
        };
        paths.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("value-"))
                && path.extension().and_then(|ext| ext.to_str()) == Some("bamlvalue")
        }));
    }
    paths.sort_by_key(|left| value_segment_sort_key(left));
    paths
}

#[cfg(not(target_arch = "wasm32"))]
fn value_segment_sort_key(path: &Path) -> (u64, u64, String) {
    let thread_id = path
        .parent()
        .and_then(|parent| parent.file_name())
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("thread-"))
        .and_then(|id| id.parse::<u64>().ok())
        .unwrap_or(u64::MAX);
    let segment_id = path
        .file_stem()
        .and_then(|name| name.to_str())
        .and_then(|name| name.strip_prefix("value-"))
        .and_then(|id| id.parse::<u64>().ok())
        .unwrap_or(u64::MAX);
    (thread_id, segment_id, path.display().to_string())
}

fn history_diagnostic(code: impl Into<String>, message: String) -> RunDiagnostic {
    RunDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: Some(code.into()),
        message,
        payload_id: None,
    }
}

pub fn summarize_history_run(run: &Run) -> RunSummary {
    let mut touched = Vec::<FunctionName>::new();
    match &run.target {
        RunTarget::Function { function_name } | RunTarget::Companion { function_name, .. } => {
            touched.push(function_name.clone());
        }
        RunTarget::Preview {
            parent_function_name,
            ..
        } => touched.push(parent_function_name.clone()),
        RunTarget::Test { .. } | RunTarget::Internal { .. } => {}
    }
    RunSummary {
        boundary_id: run.boundary_id,
        target: run.target.clone(),
        visibility: run.visibility.clone(),
        status: run.status,
        request: run.request.clone(),
        touched_functions: touched,
        created_at_ms: run.created_at_ms,
        completed_at_ms: run.completed_at_ms,
        retention: RunRetentionState::Full,
    }
}

pub fn history_run_matches_filter(run: &Run, filter: &RunFilter) -> bool {
    if let Some(project_id) = &filter.project_id
        && &run.request.project_id != project_id
    {
        return false;
    }
    if let Some(project_generation) = filter.project_generation
        && run.request.project_generation != project_generation
    {
        return false;
    }
    if !filter.kinds.is_empty() && !filter.kinds.contains(&run.target.kind()) {
        return false;
    }
    if !filter.statuses.is_empty() && !filter.statuses.contains(&run.status) {
        return false;
    }
    if let Some(function_name) = &filter.call_tree_contains_function {
        let target_matches = match &run.target {
            RunTarget::Function {
                function_name: target,
            }
            | RunTarget::Companion {
                function_name: target,
                ..
            } => target == function_name,
            RunTarget::Preview {
                parent_function_name,
                ..
            } => parent_function_name == function_name,
            RunTarget::Test { .. } | RunTarget::Internal { .. } => false,
        };
        if !target_matches {
            return false;
        }
    }
    match (&filter.visibility, &run.visibility) {
        (RunVisibilityFilter::HistoryOnly, RunVisibility::History) => true,
        (RunVisibilityFilter::HistoryOnly, _) => false,
        (
            RunVisibilityFilter::Scope { scope_id },
            RunVisibility::Scoped {
                scope_id: run_scope,
            },
        ) => scope_id == run_scope,
        (RunVisibilityFilter::Scope { .. }, _) => false,
        (RunVisibilityFilter::IncludeHidden, RunVisibility::DebugOnly) => false,
        (RunVisibilityFilter::IncludeHidden | RunVisibilityFilter::AllForDebug, _) => true,
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use std::{
        path::{Path, PathBuf},
        sync::atomic::{AtomicU64, Ordering},
    };

    use super::HistoryStore;
    use crate::{
        ids::{BexCallId, BexThreadId, BoundaryId, EngineId, ProcessEuid},
        run::{
            ProjectGeneration, ProjectId, RequestId, RunOutcome, RunResult, RunTarget,
            RunTimeAnchor, StartGuard, StartRunContext, TraceCallKey,
        },
        value::{LogEventRecord, ValueCodec},
    };

    fn temp_dir() -> PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(1);
        std::env::temp_dir().join(format!(
            "baml-history-log-only-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn lifecycle_round_trip_does_not_write_log_storage() {
        let project = temp_dir();
        std::fs::create_dir_all(&project).unwrap();
        let boundary_id = BoundaryId::from_bytes([8; 16]);
        let start = StartRunContext {
            boundary_id,
            request_id: RequestId(1),
            request: crate::run::RunRequestSummary {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            created_at_ms: 10,
            time_anchor: RunTimeAnchor {
                epoch_created_at_ms: 10,
                trace_zero_ns: 0,
            },
            start_guard: StartGuard::new(),
        };
        let store = HistoryStore::new(vec![project.clone()]);
        store.begin(&project, &start).unwrap();
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: None,
                    value: None,
                    renderer_hint: None,
                    supporting_payload_ids: Vec::new(),
                }),
                12,
            )
            .unwrap();

        let run = store.open(boundary_id).unwrap();
        assert!(run.payloads.is_empty());
        assert_eq!(run.status, crate::run::RunStatus::Succeeded);
        assert!(!project.join(".baml/profiles-v1").exists());
        for path in walk_files(&project) {
            let bytes = std::fs::read(path).unwrap();
            let values = crate::value::read_bamlvalue_from_bytes(&bytes).unwrap();
            assert!(values.records.iter().all(|record| matches!(
                record,
                crate::value::ValueFileRecord::RunStarted(_)
                    | crate::value::ValueFileRecord::RunCompleted(_)
            )));
        }
        let _ = std::fs::remove_dir_all(project);
    }

    fn start_context(boundary_id: BoundaryId) -> StartRunContext {
        StartRunContext {
            boundary_id,
            request_id: RequestId(1),
            request: crate::run::RunRequestSummary {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(1),
                target: RunTarget::Function {
                    function_name: "main".to_string(),
                },
                args_summary: None,
                options_summary: None,
            },
            created_at_ms: 10,
            time_anchor: RunTimeAnchor {
                epoch_created_at_ms: 10,
                trace_zero_ns: 0,
            },
            start_guard: StartGuard::new(),
        }
    }

    fn log_record(thread_id: u64, timestamp_ms: u64, message: &str) -> LogEventRecord {
        LogEventRecord {
            call: TraceCallKey {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(thread_id),
                call_id: BexCallId(4),
            },
            level: Some("info".to_string()),
            source: None,
            timestamp_ms,
            message_preview: Some(message.to_string()),
        }
    }

    #[test]
    fn legacy_log_replay_preserves_chronology_across_threads() {
        let project = temp_dir();
        std::fs::create_dir_all(&project).unwrap();
        let boundary_id = BoundaryId::from_bytes([9; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        store.begin(&project, &start_context(boundary_id)).unwrap();
        // Thread 1 writes its segment first on disk, but thread 2 logged
        // earlier in time; replay must follow time.
        for (thread, ts, msg) in [
            (1, 30, "t1-a"),
            (1, 50, "t1-b"),
            (2, 20, "t2-a"),
            (2, 40, "t2-b"),
        ] {
            let path =
                super::path::build_boundary_history_path(&project, &start_context(boundary_id))
                    .value_segment_path(thread, ts);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            let mut bytes = Vec::new();
            crate::value::encode::encode_header(&mut bytes, boundary_id).unwrap();
            crate::value::encode::encode_log_event(
                &mut bytes,
                &crate::value::LogRecord {
                    value_ref: crate::value::ValueRef::available(
                        format!("legacy-{thread}-{ts}"),
                        ValueCodec::BamlOutboundValue,
                        1,
                        1,
                    ),
                    body: vec![0],
                    blob_ref: None,
                    event: log_record(thread, ts, msg),
                },
            )
            .unwrap();
            std::fs::write(path, bytes).unwrap();
        }
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: None,
                    value: None,
                    renderer_hint: None,
                    supporting_payload_ids: Vec::new(),
                }),
                60,
            )
            .unwrap();

        let run = store.open(boundary_id).unwrap();
        let order = run
            .payloads
            .iter()
            .map(|payload| (payload.id.0, payload.timestamp_ms))
            .collect::<Vec<_>>();
        assert_eq!(order, vec![(1, 20), (2, 30), (3, 40), (4, 50)]);

        // A replayed run that later takes a live log must not reuse a
        // replayed payload id.
        let live = crate::run::InMemoryRunStore::default();
        assert!(live.insert_replayed_run(run));
        live.ingest_log_value_ref(
            boundary_id,
            TraceCallKey {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                thread_id: BexThreadId(1),
                call_id: BexCallId(4),
            },
            None,
            "live".to_string(),
            None,
            None,
        )
        .unwrap();
        let ids = live
            .snapshot(boundary_id)
            .unwrap()
            .payloads
            .iter()
            .map(|payload| payload.id.0)
            .collect::<Vec<_>>();
        assert_eq!(ids, vec![1, 2, 3, 4, 5]);
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn failed_completion_does_not_leak_boundary_state() {
        let project = temp_dir();
        std::fs::create_dir_all(&project).unwrap();
        let boundary_id = BoundaryId::from_bytes([7; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        store.begin(&project, &start_context(boundary_id)).unwrap();
        let boundary_dir = {
            let inner = store.inner.lock().unwrap();
            inner.boundaries[&boundary_id].path.boundary_dir.clone()
        };
        std::fs::remove_dir_all(&boundary_dir).unwrap();
        std::fs::write(&boundary_dir, b"not a directory").unwrap();

        let result = store.complete(
            boundary_id,
            &RunOutcome::Succeeded(RunResult {
                value_ref: None,
                value: None,
                renderer_hint: None,
                supporting_payload_ids: Vec::new(),
            }),
            12,
        );
        assert!(result.is_err(), "completion must surface the write failure");
        assert!(
            !store
                .inner
                .lock()
                .unwrap()
                .boundaries
                .contains_key(&boundary_id),
            "a failed completion must not leave the boundary parked"
        );
        let _ = std::fs::remove_dir_all(project);
    }

    fn walk_files(root: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(root) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.extend(walk_files(&path));
            } else {
                files.push(path);
            }
        }
        files
    }
}
