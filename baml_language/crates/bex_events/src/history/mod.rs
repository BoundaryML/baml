//! Non-profile host history for run lifecycle and structured logs.
//!
//! Profiling data lives exclusively in `profiles-v1`. This module retains the
//! mixed `.bamlvalue` container only for `RunStarted`, `RunCompleted`, log
//! bodies, and log capture loss; it never reads or writes stack/profile data.

#[cfg(not(target_arch = "wasm32"))]
pub mod boundary_writer;
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
    boundary_writer::{BoundaryWriter, SegmentRotationPolicy},
    path::{
        BoundaryHistoryPath, build_boundary_history_path, find_boundary_dir, list_boundary_dirs,
    },
};
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
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    run::RunOutcome,
    value::{LogEventRecord, ValueWriteOutcome},
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
    rotation_policy: SegmentRotationPolicy,
    boundaries: HashMap<BoundaryId, BoundaryState>,
}

#[cfg(not(target_arch = "wasm32"))]
struct BoundaryState {
    path: BoundaryHistoryPath,
    started: RunStartedRecord,
    writer: BoundaryWriter,
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
                rotation_policy: SegmentRotationPolicy::default(),
                boundaries: HashMap::new(),
            })),
        }
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
        let mut writer = BoundaryWriter::create_with_rotation_policy(
            path.clone(),
            start.boundary_id,
            start.created_at_ms,
            inner.rotation_policy,
        )?;
        writer.write_run_started(&started)?;
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

    pub fn append_log_body(
        &self,
        boundary_id: BoundaryId,
        event: LogEventRecord,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = inner.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "history boundary {} was not begun",
                    boundary_id.to_wire_string()
                ),
            )
        })?;
        state.writer.append_log_body(event, codec, body)
    }

    pub fn append_capture_loss(
        &self,
        boundary_id: BoundaryId,
        record: &CaptureLossRecord,
    ) -> io::Result<()> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let state = inner.boundaries.get_mut(&boundary_id).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "history boundary {} was not begun",
                    boundary_id.to_wire_string()
                ),
            )
        })?;
        state.writer.append_capture_loss(record)
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
        let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
            return Ok(());
        };
        state.writer.write_run_completed(&record)?;
        state.writer.flush()?;
        inner.boundaries.remove(&boundary_id);
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
        open_boundary_from_dir(&dir)
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
        let value_segments = read_value_segments(&dir)?;
        let blob_store = BlobStore::for_boundary_dir(&dir);
        read_value_from_segments_with_blobs_result(&value_segments, value_ref_id, Some(&blob_store))
    }
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

    use super::{HistoryStore, HistoryValueReadResult};
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
    fn lifecycle_and_log_round_trip_without_profile_segments() {
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
        let outcome = store
            .append_log_body(
                boundary_id,
                LogEventRecord {
                    call: TraceCallKey {
                        process_euid: ProcessEuid([1; 16]),
                        engine_id: EngineId(2),
                        thread_id: BexThreadId(3),
                        call_id: BexCallId(4),
                    },
                    level: Some("info".to_string()),
                    source: None,
                    timestamp_ms: 11,
                    message_preview: Some("hello".to_string()),
                },
                ValueCodec::BamlOutboundValue,
                vec![1, 2, 3],
            )
            .unwrap();
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: None,
                    renderer_hint: None,
                    supporting_payload_ids: Vec::new(),
                }),
                12,
            )
            .unwrap();

        let run = store.open(boundary_id).unwrap();
        assert_eq!(run.payloads.len(), 1);
        let HistoryValueReadResult::Available(body) = store
            .read_value_result(boundary_id, &outcome.value_ref.id)
            .unwrap()
        else {
            panic!("log body should be available");
        };
        assert_eq!(body.body, vec![1, 2, 3]);
        assert!(
            !walk_files(&project)
                .iter()
                .any(|path| path.extension().and_then(|ext| ext.to_str()) == Some("bamlprof"))
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
