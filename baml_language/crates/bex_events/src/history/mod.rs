#[cfg(not(target_arch = "wasm32"))]
pub mod boundary_writer;
#[cfg(not(target_arch = "wasm32"))]
pub mod path;
pub mod router;

#[cfg(not(target_arch = "wasm32"))]
use std::{
    collections::{HashMap, HashSet},
    panic::AssertUnwindSafe,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use std::{io, path::Path};

use crate::{
    ids::BoundaryId,
    prof::{pb, read::read_bamlprof_from_bytes},
    run::{
        CancellationState, DiagnosticSeverity, FunctionName, PayloadEvent, PayloadId,
        ProfileEventEnvelope, RedactionMetadata, Run, RunDiagnostic, RunError, RunErrorClass,
        RunFilter, RunResult, RunRetentionState, RunStatus, RunSummary, RunTarget, RunVisibility,
        RunVisibilityFilter, call_node_id, reconstruct_with_function_table,
    },
    value::{
        RunCompletedRecord, RunStartedRecord, ValueCaptureKind, ValueCodec, ValueFileRecord,
        ValueRef, read_bamlvalue_from_bytes,
    },
};

#[cfg(not(target_arch = "wasm32"))]
use self::router::BoundaryTraceRouter;
#[cfg(not(target_arch = "wasm32"))]
use self::{
    boundary_writer::BoundaryWriter,
    path::{
        BoundaryHistoryPath, build_boundary_history_path, find_boundary_dir, list_boundary_dirs,
    },
};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    ids::CallRef,
    run::{RunOutcome, TraceCallKey},
    value::{ValueCapture, ValueWriteOutcome},
};

#[cfg(not(target_arch = "wasm32"))]
pub trait HistoryEventObserver: Send + Sync + 'static {
    fn ingest_history_profile_event(
        &self,
        envelope: ProfileEventEnvelope,
        disk_event: pb::DiskEventV1,
    );
}

#[cfg(not(target_arch = "wasm32"))]
#[must_use]
pub fn register_history_observer<T>(observer: Arc<T>) -> HistoryObserverRegistration
where
    T: HistoryEventObserver,
{
    let mut state = history_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let id = state.next_id;
    state.next_id = state.next_id.saturating_add(1);
    state.observers.push((id, observer));
    HistoryObserverRegistration { id }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) fn publish_history_profile_event(
    envelope: &ProfileEventEnvelope,
    disk_event: &pb::DiskEventV1,
) {
    let observers = history_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .observers
        .iter()
        .map(|(_, observer)| observer.clone())
        .collect::<Vec<_>>();
    for observer in observers {
        let envelope = envelope.clone();
        let disk_event = disk_event.clone();
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| {
            observer.ingest_history_profile_event(envelope, disk_event);
        }));
    }
}

#[cfg(target_arch = "wasm32")]
pub(crate) fn publish_history_profile_event(
    _envelope: &ProfileEventEnvelope,
    _disk_event: &pb::DiskEventV1,
) {
}

#[cfg(not(target_arch = "wasm32"))]
pub struct HistoryObserverRegistration {
    id: u64,
}

#[cfg(not(target_arch = "wasm32"))]
impl Drop for HistoryObserverRegistration {
    fn drop(&mut self) {
        history_observers()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .observers
            .retain(|(id, _)| *id != self.id);
    }
}

#[cfg(not(target_arch = "wasm32"))]
struct HistoryObserverState {
    next_id: u64,
    observers: Vec<(u64, Arc<dyn HistoryEventObserver>)>,
}

#[cfg(not(target_arch = "wasm32"))]
fn history_observers() -> &'static Mutex<HistoryObserverState> {
    static OBSERVERS: std::sync::OnceLock<Mutex<HistoryObserverState>> = std::sync::OnceLock::new();
    OBSERVERS.get_or_init(|| {
        Mutex::new(HistoryObserverState {
            next_id: 1,
            observers: Vec::new(),
        })
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryValueBody {
    pub codec: ValueCodec,
    pub body: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryProfileSegment {
    pub label: String,
    pub bytes: Vec<u8>,
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
    router: BoundaryTraceRouter,
    boundaries: HashMap<BoundaryId, BoundaryState>,
}

#[cfg(not(target_arch = "wasm32"))]
struct BoundaryState {
    path: BoundaryHistoryPath,
    started: RunStartedRecord,
    root_trace: Option<TraceCallKey>,
    claimed_profile_indices: HashSet<usize>,
    completed: Option<RunCompletedRecord>,
    writer: BoundaryWriter,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for BoundaryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundaryState")
            .field("path", &self.path)
            .field("started", &self.started)
            .field("root_trace", &self.root_trace)
            .field("claimed_profile_indices", &self.claimed_profile_indices)
            .field("completed", &self.completed)
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
                router: BoundaryTraceRouter::default(),
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
        let writer = BoundaryWriter::create(path.clone(), start.boundary_id, start.created_at_ms)?;
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !inner.search_roots.contains(&path.project_root) {
            inner.search_roots.push(path.project_root.clone());
        }
        inner.boundaries.insert(
            start.boundary_id,
            BoundaryState {
                path,
                started,
                root_trace: None,
                claimed_profile_indices: HashSet::new(),
                completed: None,
                writer,
            },
        );
        Ok(())
    }

    pub fn attach_root_trace(
        &self,
        boundary_id: BoundaryId,
        root_call_ref: CallRef,
    ) -> io::Result<()> {
        let root_trace = TraceCallKey {
            process_euid: root_call_ref.process_euid,
            engine_id: root_call_ref.engine_id,
            thread_id: root_call_ref.thread_id,
            call_id: root_call_ref.call_id,
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
            return Ok(());
        };
        match state.root_trace {
            Some(existing) if existing == root_trace => {}
            Some(existing) => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "history root trace conflict for {}: existing {}, new {}",
                        boundary_id.to_wire_string(),
                        existing.call_ref().encode(),
                        root_trace.call_ref().encode()
                    ),
                ));
            }
            None => state.root_trace = Some(root_trace),
        }
        route_claimed_locked(&mut inner, boundary_id)
    }

    pub fn append_value_body(
        &self,
        boundary_id: BoundaryId,
        capture: ValueCapture,
        codec: ValueCodec,
        body: Vec<u8>,
    ) -> io::Result<ValueWriteOutcome> {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "history boundary {} was not begun",
                    boundary_id.to_wire_string()
                ),
            ));
        };
        ensure_run_started_written(state)?;
        state.writer.append_value_body(capture, codec, body)
    }

    pub fn complete(
        &self,
        boundary_id: BoundaryId,
        outcome: &RunOutcome,
        completed_at_ms: u64,
    ) -> io::Result<()> {
        let record = RunCompletedRecord {
            status: outcome.status(),
            completed_at_ms,
            renderer_hint: match outcome {
                RunOutcome::Succeeded(result) => result.renderer_hint.clone(),
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
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
            return Ok(());
        };
        ensure_run_started_written(state)?;
        let thread_id = state.root_trace.map_or(0, |root| root.thread_id.0);
        state.writer.write_run_completed(thread_id, &record)?;
        state.completed = Some(record);
        state.writer.flush()
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
            .filter_map(|dir| self.open_from_dir(&dir).ok())
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
        self.open_from_dir(&dir)
    }

    pub fn read_value(
        &self,
        boundary_id: BoundaryId,
        value_ref_id: &str,
    ) -> io::Result<Option<HistoryValueBody>> {
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
            return Ok(None);
        };
        let value_segments = value_segment_paths(&dir)
            .into_iter()
            .filter_map(|path| {
                std::fs::read(&path).ok().map(|bytes| HistoryValueSegment {
                    label: path.display().to_string(),
                    bytes,
                })
            })
            .collect::<Vec<_>>();
        read_value_from_segments(&value_segments, value_ref_id)
    }

    fn open_from_dir(&self, dir: &Path) -> io::Result<Run> {
        open_boundary_from_dir(dir)
    }
}

#[cfg(not(target_arch = "wasm32"))]
impl HistoryEventObserver for HistoryStore {
    fn ingest_history_profile_event(
        &self,
        envelope: ProfileEventEnvelope,
        disk_event: pb::DiskEventV1,
    ) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.router.ingest(envelope, disk_event);
        let boundary_ids = inner.boundaries.keys().copied().collect::<Vec<_>>();
        for boundary_id in boundary_ids {
            let _ = route_claimed_locked(&mut inner, boundary_id);
        }
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn route_claimed_locked(inner: &mut HistoryStoreInner, boundary_id: BoundaryId) -> io::Result<()> {
    let Some(root_trace) = inner
        .boundaries
        .get(&boundary_id)
        .and_then(|state| state.root_trace)
    else {
        return Ok(());
    };
    let component_indices = inner.router.component_indices(root_trace);
    let records = component_indices
        .iter()
        .filter_map(|index| {
            inner
                .router
                .record(*index)
                .cloned()
                .map(|record| (*index, record))
        })
        .collect::<Vec<_>>();
    let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
        return Ok(());
    };
    ensure_run_started_written(state)?;
    for (index, record) in records {
        if !state.claimed_profile_indices.insert(index) {
            continue;
        }
        state
            .writer
            .write_profile_event(&record.envelope, &record.disk_event)?;
    }
    state.writer.flush()
}

#[cfg(not(target_arch = "wasm32"))]
fn ensure_run_started_written(state: &mut BoundaryState) -> io::Result<()> {
    let thread_id = state.root_trace.map_or(0, |root| root.thread_id.0);
    state.writer.write_run_started(thread_id, &state.started)
}

#[cfg(not(target_arch = "wasm32"))]
fn open_boundary_from_dir(dir: &Path) -> io::Result<Run> {
    let value_segments = value_segment_paths(dir)
        .into_iter()
        .map(|path| {
            std::fs::read(&path).map(|bytes| HistoryValueSegment {
                label: path.display().to_string(),
                bytes,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    let profile_segments = stack_segment_paths(dir)
        .into_iter()
        .map(|path| {
            std::fs::read(&path).map(|bytes| HistoryProfileSegment {
                label: path.display().to_string(),
                bytes,
            })
        })
        .collect::<io::Result<Vec<_>>>()?;
    open_boundary_from_segments_with_fallback(&profile_segments, &value_segments, Some(dir))
}

pub fn open_boundary_from_segments(
    profile_segments: &[HistoryProfileSegment],
    value_segments: &[HistoryValueSegment],
) -> io::Result<Run> {
    open_boundary_from_segments_with_fallback(profile_segments, value_segments, None)
}

fn open_boundary_from_segments_with_fallback(
    profile_segments: &[HistoryProfileSegment],
    value_segments: &[HistoryValueSegment],
    fallback_dir: Option<&Path>,
) -> io::Result<Run> {
    let mut header_boundary_ids = Vec::new();
    let mut value_records = Vec::new();
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
        value_records.extend(parsed.records);
    }

    let mut started = None;
    let mut completed = None;
    let mut captured_values = Vec::new();
    for record in value_records {
        match record {
            ValueFileRecord::RunStarted(record) => started = Some(record),
            ValueFileRecord::RunCompleted(record) => completed = Some(record),
            ValueFileRecord::CapturedValue(record) => captured_values.push(record),
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

    let mut profile_events = Vec::new();
    let mut function_table = Vec::new();
    for segment in profile_segments {
        let contents = read_bamlprof_from_bytes(&segment.bytes)?;
        if contents.truncated {
            diagnostics.push(history_diagnostic(
                "historyStackTornTail",
                format!(
                    "Stack segment {} ended with a torn trailing record; complete prefix was retained",
                    segment.label
                ),
            ));
        }
        if function_table.is_empty() {
            function_table = crate::run::bamlprof::function_table(&contents.header);
        }
        profile_events.extend(crate::run::bamlprof::normalized_events(&contents).map_err(
            |err| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!("invalid .bamlprof: {err}"),
                )
            },
        )?);
    }
    let reconstructed = reconstruct_with_function_table(profile_events, function_table);
    diagnostics.extend(reconstructed.diagnostics.into_iter().map(|diagnostic| {
        history_diagnostic(
            "historyReconstruction",
            format!("{:?}: {}", diagnostic.code, diagnostic.message),
        )
    }));

    let root_trace = captured_values
        .iter()
        .find_map(|record| record.capture.as_ref().map(|capture| capture.call))
        .or_else(|| {
            reconstructed
                .calls
                .iter()
                .find(|call| call.parent_id.is_none())
                .map(|call| call.trace_key)
        });
    let root_call_node_id = root_trace.map(|trace| call_node_id(&trace));
    let boundary_id =
        boundary_id_from_header_or_fallback(fallback_dir, &header_boundary_ids, &started)?;

    let root_input_ref = captured_values
        .iter()
        .find(|record| {
            record
                .capture
                .as_ref()
                .is_some_and(|capture| capture.kind == ValueCaptureKind::RootInput)
        })
        .map(|record| record.value_ref.clone());
    let output_ref = captured_values
        .iter()
        .find(|record| {
            record
                .capture
                .as_ref()
                .is_some_and(|capture| capture.kind == ValueCaptureKind::RootOutput)
        })
        .map(|record| record.value_ref.clone());
    let error_ref = captured_values
        .iter()
        .find(|record| {
            record
                .capture
                .as_ref()
                .is_some_and(|capture| capture.kind == ValueCaptureKind::RootError)
        })
        .map(|record| record.value_ref.clone());

    let mut payloads = Vec::new();
    if let Some(value_ref) = root_input_ref {
        payloads.push(PayloadEvent {
            id: PayloadId(1),
            call_node_id: root_call_node_id,
            timestamp_ms: started.created_at_ms,
            kind: crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                role: crate::run::CapturedValueRole::RootInput,
                label: Some("inputs".to_string()),
                value_ref: Some(value_ref),
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        });
    }

    let completed_at_ms = completed.as_ref().map(|record| record.completed_at_ms);
    let status = completed
        .as_ref()
        .map_or(RunStatus::Running, |record| record.status);
    let (result, error, cancellation) =
        outcome_fields_from_replay(completed, output_ref, error_ref);
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
        root_call_node_id,
        graph_runtime_overlay: None,
        calls: reconstructed.calls,
        threads: reconstructed.threads,
        payloads,
        diagnostics,
        cursor: crate::run::RunCursor(0),
    })
}

pub fn read_value_from_segments(
    value_segments: &[HistoryValueSegment],
    value_ref_id: &str,
) -> io::Result<Option<HistoryValueBody>> {
    for segment in value_segments {
        let parsed = read_bamlvalue_from_bytes(&segment.bytes)?;
        for record in parsed.records {
            let ValueFileRecord::CapturedValue(record) = record else {
                continue;
            };
            if record.value_ref.id == value_ref_id {
                return Ok(Some(HistoryValueBody {
                    codec: record.value_ref.codec,
                    body: record.body,
                }));
            }
        }
    }
    Ok(None)
}

fn outcome_fields_from_replay(
    completed: Option<RunCompletedRecord>,
    output_ref: Option<ValueRef>,
    error_ref: Option<ValueRef>,
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
                value_ref: output_ref,
                renderer_hint: completed.renderer_hint,
                supporting_payload_ids: Vec::new(),
            }),
            None,
            None,
        ),
        RunStatus::Failed | RunStatus::Panicked => {
            let mut error = completed.error.unwrap_or_else(|| RunError {
                class: if completed.status == RunStatus::Panicked {
                    RunErrorClass::Panic
                } else {
                    RunErrorClass::Runtime
                },
                message: "run failed".to_string(),
                details: None,
                value_ref: None,
            });
            error.value_ref = error_ref;
            (None, Some(error), None)
        }
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

#[cfg(not(target_arch = "wasm32"))]
fn value_segment_paths(dir: &Path) -> Vec<PathBuf> {
    segment_paths(dir, "value-", "bamlvalue")
}

#[cfg(not(target_arch = "wasm32"))]
fn stack_segment_paths(dir: &Path) -> Vec<PathBuf> {
    segment_paths(dir, "stack-", "bamlprof")
}

#[cfg(not(target_arch = "wasm32"))]
fn segment_paths(dir: &Path, prefix: &str, extension: &str) -> Vec<PathBuf> {
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
                .is_some_and(|name| name.starts_with(prefix))
                && path.extension().and_then(|ext| ext.to_str()) == Some(extension)
        }));
    }
    paths.sort();
    paths
}

fn history_diagnostic(code: impl Into<String>, message: String) -> RunDiagnostic {
    RunDiagnostic {
        severity: DiagnosticSeverity::Warning,
        code: Some(code.into()),
        message,
        call_node_id: None,
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
    for call in &run.calls {
        if let Some(function_name) = &call.function_name
            && !touched.contains(function_name)
        {
            touched.push(function_name.clone());
        }
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
        let call_matches = run
            .calls
            .iter()
            .any(|call| call.function_name.as_ref() == Some(function_name));
        if !target_matches && !call_matches {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ids::{BexCallId, BexThreadId, EngineId, ProcessEuid},
        prof::{encode::encode_disk_event, read::read_bamlprof_from_bytes},
        run::{
            ProjectGeneration, ProjectId, RequestId, RunRequestSummary, RunTimeAnchor, StartGuard,
        },
        value::ValueCaptureKind,
    };

    fn temp_project(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "baml-history-{}-{tag}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("baml.toml"), "").unwrap();
        dir
    }

    fn start_context(boundary_id: BoundaryId) -> crate::run::StartRunContext {
        crate::run::StartRunContext {
            boundary_id,
            request_id: RequestId(1),
            request: RunRequestSummary {
                project_id: ProjectId("project".to_string()),
                project_generation: ProjectGeneration(7),
                target: RunTarget::Function {
                    function_name: "user.Extract".to_string(),
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

    fn root_trace() -> TraceCallKey {
        TraceCallKey {
            process_euid: ProcessEuid([9; 16]),
            engine_id: EngineId(3),
            thread_id: BexThreadId(1),
            call_id: BexCallId(2),
        }
    }

    #[test]
    fn boundary_id_from_dir_name_handles_hyphen_in_boundary_wire_id() {
        let boundary_id =
            BoundaryId::from_bytes([0xf8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 1]);
        let wire = boundary_id.to_wire_string();
        assert!(
            wire.contains('-'),
            "test boundary id must exercise base64url hyphen: {wire}"
        );
        let dir = PathBuf::from(format!("1782339566657-paulo.StringWidening-{wire}"));

        assert_eq!(boundary_id_from_dir_name(&dir), Some(boundary_id));
    }

    fn call_event() -> pb::DiskEventV1 {
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                thread_id: 1,
                call_id: 2,
                parent_call_id: None,
                function_id: 0,
                timestamp_ns: 4,
                call_site_file_id: None,
                call_site_start_offset: None,
                call_site_end_offset: None,
                call_site_line: None,
            })),
        }
    }

    fn write_basic_history_run(
        project: &Path,
        boundary_id: BoundaryId,
    ) -> (HistoryStore, ValueWriteOutcome) {
        let store = HistoryStore::new(vec![project.to_path_buf()]);
        let start = start_context(boundary_id);
        store.begin(project, &start).unwrap();

        let trace = root_trace();
        let event = call_event();
        let envelope = crate::run::profile_event_envelope_from_disk_event(
            crate::run::ProfileEventSource::Replay {
                artifact_id: "test".to_string(),
            },
            trace.process_euid,
            trace.engine_id,
            &event,
        )
        .unwrap();
        store.ingest_history_profile_event(envelope, event);
        store
            .attach_root_trace(boundary_id, trace.call_ref())
            .unwrap();
        let outcome = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::RootOutput,
                    call: trace,
                },
                ValueCodec::BamlOutboundValue,
                vec![1, 2, 3],
            )
            .unwrap();
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: Some(outcome.value_ref.clone()),
                    renderer_hint: Some("baml.outbound.base64".to_string()),
                    supporting_payload_ids: Vec::new(),
                }),
                20,
            )
            .unwrap();
        (store, outcome)
    }

    #[test]
    fn history_routes_only_after_exact_root_attach_and_replays_value() {
        let project = temp_project("route");
        let boundary_id = BoundaryId::from_bytes([7; 16]);
        let start = start_context(boundary_id);
        let store = HistoryStore::new(vec![project.clone()]);
        store.begin(&project, &start).unwrap();
        let history_root = project.join(".baml").join("history");
        let boundary_dir = std::fs::read_dir(&history_root)
            .unwrap()
            .flatten()
            .next()
            .unwrap()
            .path();
        assert!(stack_segment_paths(&boundary_dir).is_empty());

        let (store, outcome) = write_basic_history_run(&project, boundary_id);

        let replayed = store.open(boundary_id).unwrap();
        assert_eq!(replayed.boundary_id, boundary_id);
        assert_eq!(replayed.status, RunStatus::Succeeded);
        assert_eq!(
            replayed
                .result
                .as_ref()
                .unwrap()
                .value_ref
                .as_ref()
                .unwrap()
                .id,
            outcome.value_ref.id
        );
        assert_eq!(
            store
                .read_value(boundary_id, &outcome.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![1, 2, 3]
        );

        let stack_path = stack_segment_paths(
            &find_boundary_dir(&[project.clone()], boundary_id).expect("boundary dir"),
        )
        .pop()
        .expect("stack segment");
        let stack_bytes = std::fs::read(stack_path).unwrap();
        let parsed = read_bamlprof_from_bytes(&stack_bytes).unwrap();
        assert_eq!(parsed.events.len(), 1);

        let mut event_bytes = Vec::new();
        encode_disk_event(&mut event_bytes, &call_event());
        assert!(!event_bytes.is_empty());
        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn replay_reports_value_torn_tail_and_keeps_complete_prefix() {
        let project = temp_project("torn-value");
        let boundary_id = BoundaryId::from_bytes([8; 16]);
        let (store, outcome) = write_basic_history_run(&project, boundary_id);
        let boundary_dir =
            find_boundary_dir(std::slice::from_ref(&project), boundary_id).expect("boundary dir");
        let value_path = value_segment_paths(&boundary_dir)
            .pop()
            .expect("value segment");
        let len = std::fs::metadata(&value_path).unwrap().len();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&value_path)
            .unwrap()
            .set_len(len - 1)
            .unwrap();

        let replayed = store.open(boundary_id).unwrap();
        assert!(
            replayed
                .diagnostics
                .iter()
                .any(|diagnostic| diagnostic.code.as_deref() == Some("historyValueTornTail")),
            "expected torn-tail diagnostic, got {:?}",
            replayed.diagnostics
        );
        assert_eq!(
            store
                .read_value(boundary_id, &outcome.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![1, 2, 3]
        );

        let _ = std::fs::remove_dir_all(project);
    }
}
