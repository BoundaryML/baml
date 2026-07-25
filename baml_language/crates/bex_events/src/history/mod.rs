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

#[cfg(not(target_arch = "wasm32"))]
use self::router::{BoundaryTraceRouter, HistoryProfileRecordId};
#[cfg(not(target_arch = "wasm32"))]
use self::{
    boundary_writer::{BoundaryWriter, SegmentRotationPolicy},
    path::{
        BoundaryHistoryPath, build_boundary_history_path, find_boundary_dir, list_boundary_dirs,
    },
};
use crate::{
    ids::BoundaryId,
    prof::{pb, read::read_bamlprof_from_bytes},
    run::{
        CancellationState, DiagnosticSeverity, FunctionName, PayloadEvent, PayloadId,
        ProfileEventEnvelope, RedactionMetadata, Run, RunDiagnostic, RunError, RunErrorClass,
        RunFilter, RunResult, RunRetentionState, RunStatus, RunSummary, RunTarget, RunVisibility,
        RunVisibilityFilter, attach_payload_ids_to_calls, call_node_id,
        reconstruct_with_function_table,
    },
    value::{
        BlobRef, BlobStore, CaptureLossRecord, RunCompletedRecord, RunStartedRecord,
        ValueCaptureKind, ValueCodec, ValueFileRecord, ValueRef, read_bamlvalue_from_bytes,
    },
};
#[cfg(not(target_arch = "wasm32"))]
use crate::{
    ids::CallRef,
    run::{RunOutcome, TraceCallKey},
    value::{LogEventRecord, ValueCapture, ValueWriteOutcome},
};

#[cfg(not(target_arch = "wasm32"))]
pub trait HistoryEventObserver: Send + Sync + 'static {
    fn ingest_history_profile_event(
        &self,
        envelope: ProfileEventEnvelope,
        disk_event: pb::DiskEventV1,
    );

    /// Called after an engine has been dropped and all of its remaining
    /// profile events have been delivered.
    fn engine_closed(&self, engine_id: crate::ids::EngineId) {
        let _ = engine_id;
    }
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
pub(crate) fn publish_history_engine_closed(engine_id: crate::ids::EngineId) {
    let observers = history_observers()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .observers
        .iter()
        .map(|(_, observer)| observer.clone())
        .collect::<Vec<_>>();
    for observer in observers {
        let _ = std::panic::catch_unwind(AssertUnwindSafe(|| observer.engine_closed(engine_id)));
    }
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
    rotation_policy: SegmentRotationPolicy,
    boundaries: HashMap<BoundaryId, BoundaryState>,
}

#[cfg(not(target_arch = "wasm32"))]
struct BoundaryState {
    path: BoundaryHistoryPath,
    started: RunStartedRecord,
    root_trace: Option<TraceCallKey>,
    claimed_profile_record_ids: HashSet<HistoryProfileRecordId>,
    profile_write_error: Option<String>,
    completed: Option<RunCompletedRecord>,
    /// Set when the boundary's engine closed before the run completed; the
    /// entry is released as soon as the completion record is flushed.
    engine_closed: bool,
    writer: BoundaryWriter,
}

#[cfg(not(target_arch = "wasm32"))]
impl std::fmt::Debug for BoundaryState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BoundaryState")
            .field("path", &self.path)
            .field("started", &self.started)
            .field("root_trace", &self.root_trace)
            .field(
                "claimed_profile_record_ids",
                &self.claimed_profile_record_ids,
            )
            .field("profile_write_error", &self.profile_write_error)
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
                rotation_policy: SegmentRotationPolicy::default(),
                boundaries: HashMap::new(),
            })),
        }
    }

    #[cfg(test)]
    #[must_use]
    fn new_with_rotation_policy(
        search_roots: Vec<PathBuf>,
        rotation_policy: SegmentRotationPolicy,
    ) -> Self {
        Self::new_with_rotation_policy_and_router_capacity(search_roots, rotation_policy, 100_000)
    }

    #[cfg(test)]
    #[must_use]
    fn new_with_rotation_policy_and_router_capacity(
        search_roots: Vec<PathBuf>,
        rotation_policy: SegmentRotationPolicy,
        router_max_records: usize,
    ) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HistoryStoreInner {
                search_roots,
                router: BoundaryTraceRouter::new(router_max_records),
                rotation_policy,
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
        let writer = BoundaryWriter::create_with_rotation_policy(
            path.clone(),
            start.boundary_id,
            start.created_at_ms,
            inner.rotation_policy,
        )?;
        if !inner.search_roots.contains(&path.project_root) {
            inner.search_roots.push(path.project_root.clone());
        }
        inner.boundaries.insert(
            start.boundary_id,
            BoundaryState {
                path,
                started,
                root_trace: None,
                claimed_profile_record_ids: HashSet::new(),
                profile_write_error: None,
                completed: None,
                engine_closed: false,
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
        match route_claimed_locked(&mut inner, boundary_id) {
            Ok(()) => Ok(()),
            Err(err) => {
                remember_profile_write_error(&mut inner, boundary_id, &err);
                Err(err)
            }
        }
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
        state.writer.append_capture_loss(record)
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
        };
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
            return Ok(());
        };
        if let Some(error) = &state.profile_write_error {
            return Err(history_profile_write_error(boundary_id, error));
        }
        ensure_run_started_written(state)?;
        let thread_id = state.root_trace.map_or(0, |root| root.thread_id.0);
        state.writer.write_run_completed(thread_id, &record)?;
        state.completed = Some(record);
        state.writer.flush()?;
        // The run is durable on disk; if its engine is already gone, no more
        // events can arrive for it and the in-memory state can be released
        // (open() and read_value() fall back to scanning the disk history).
        if state.engine_closed {
            inner.boundaries.remove(&boundary_id);
        }
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
            .filter_map(|dir| Self::open_from_dir(&dir).ok())
            .filter(|run| history_run_matches_filter(run, filter))
            .map(|run| summarize_history_run(&run))
            .collect::<Vec<_>>();
        summaries.sort_by_key(|summary| std::cmp::Reverse(summary.created_at_ms));
        summaries
    }

    pub fn open(&self, boundary_id: BoundaryId) -> io::Result<Run> {
        let (known_dir, search_roots, profile_write_error) = {
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
                inner
                    .boundaries
                    .get(&boundary_id)
                    .and_then(|state| state.profile_write_error.clone()),
            )
        };
        if let Some(error) = profile_write_error {
            return Err(history_profile_write_error(boundary_id, &error));
        }
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
        Self::open_from_dir(&dir)
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
        let value_segments = value_segment_paths(&dir)
            .into_iter()
            .map(|path| {
                std::fs::read(&path)
                    .map(|bytes| HistoryValueSegment {
                        label: path.display().to_string(),
                        bytes,
                    })
                    .map_err(|err| {
                        io::Error::new(
                            err.kind(),
                            format!("failed to read value segment {}: {err}", path.display()),
                        )
                    })
            })
            .collect::<io::Result<Vec<_>>>()?;
        let blob_store = BlobStore::for_boundary_dir(&dir);
        read_value_from_segments_with_blobs_result(&value_segments, value_ref_id, Some(&blob_store))
    }

    fn open_from_dir(dir: &Path) -> io::Result<Run> {
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
            if let Err(err) = route_claimed_locked(&mut inner, boundary_id) {
                remember_profile_write_error(&mut inner, boundary_id, &err);
            }
        }
    }

    fn engine_closed(&self, engine_id: crate::ids::EngineId) {
        let mut inner = self
            .inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        // Everything for the engine has been drained and routed; release the
        // per-boundary bookkeeping (open writer fd, claimed-record set) for
        // boundaries that already flushed their completion record. Boundaries
        // still awaiting complete() are flagged and released there.
        inner.boundaries.retain(|_, state| {
            let matches_engine = state
                .root_trace
                .is_some_and(|root| root.engine_id == engine_id);
            if !matches_engine {
                return true;
            }
            if state.completed.is_some() {
                let _ = state.writer.flush();
                false
            } else {
                state.engine_closed = true;
                true
            }
        });
        inner.router.release_engine(engine_id);
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
    let component_record_ids = inner.router.component_record_ids(root_trace);
    let records = component_record_ids
        .iter()
        .filter_map(|record_id| {
            inner
                .router
                .record(*record_id)
                .cloned()
                .map(|record| (*record_id, record))
        })
        .collect::<Vec<_>>();
    let Some(state) = inner.boundaries.get_mut(&boundary_id) else {
        return Ok(());
    };
    ensure_run_started_written(state)?;
    for (record_id, record) in records {
        if state.claimed_profile_record_ids.contains(&record_id) {
            continue;
        }
        state
            .writer
            .write_profile_event(&record.envelope, &record.disk_event)?;
        state.claimed_profile_record_ids.insert(record_id);
    }
    state.writer.flush()
}

#[cfg(not(target_arch = "wasm32"))]
fn remember_profile_write_error(
    inner: &mut HistoryStoreInner,
    boundary_id: BoundaryId,
    error: &io::Error,
) {
    if let Some(state) = inner.boundaries.get_mut(&boundary_id)
        && state.profile_write_error.is_none()
    {
        state.profile_write_error = Some(error.to_string());
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn history_profile_write_error(boundary_id: BoundaryId, error: &str) -> io::Error {
    io::Error::other(format!(
        "history profile write failed for {}: {error}",
        boundary_id.to_wire_string()
    ))
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
    let mut log_values = Vec::new();
    let mut capture_losses = Vec::new();
    for record in value_records {
        match record {
            ValueFileRecord::RunStarted(record) => started = Some(record),
            ValueFileRecord::RunCompleted(record) => completed = Some(record),
            ValueFileRecord::CapturedValue(record) => captured_values.push(record),
            ValueFileRecord::LogEvent(record) => log_values.push(record),
            ValueFileRecord::CaptureLoss(record) => capture_losses.push(record),
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
    diagnostics.extend(
        capture_losses
            .into_iter()
            .map(capture_loss_replay_diagnostic),
    );

    let root_trace = captured_values
        .iter()
        .find_map(|record| {
            record.capture.as_ref().and_then(|capture| {
                matches!(
                    capture.kind,
                    ValueCaptureKind::RootInput
                        | ValueCaptureKind::RootOutput
                        | ValueCaptureKind::RootError
                )
                .then_some(capture.call)
            })
        })
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
    let mut next_payload_id = 1;
    if let Some(value_ref) = root_input_ref {
        payloads.push(PayloadEvent {
            id: PayloadId(next_payload_id),
            call_node_id: root_call_node_id,
            timestamp_ms: started.created_at_ms,
            kind: crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                role: crate::run::CapturedValueRole::RootInput,
                label: Some("inputs".to_string()),
                value_ref: Some(value_ref),
                trace_call: None,
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        });
        next_payload_id = next_payload_id.saturating_add(1);
    }
    for record in captured_values.iter().filter(|record| {
        record.capture.as_ref().is_some_and(|capture| {
            matches!(
                capture.kind,
                ValueCaptureKind::CallInput
                    | ValueCaptureKind::CallOutput
                    | ValueCaptureKind::CallError
            )
        })
    }) {
        let Some(capture) = record.capture.as_ref() else {
            continue;
        };
        let role = match capture.kind {
            ValueCaptureKind::CallInput => crate::run::CapturedValueRole::CallInput,
            ValueCaptureKind::CallOutput => crate::run::CapturedValueRole::CallOutput,
            ValueCaptureKind::CallError => crate::run::CapturedValueRole::CallError,
            _ => continue,
        };
        let call_node = reconstructed
            .calls
            .iter()
            .any(|call| call.trace_key == capture.call)
            .then(|| call_node_id(&capture.call));
        payloads.push(PayloadEvent {
            id: PayloadId(next_payload_id),
            call_node_id: call_node,
            timestamp_ms: started.created_at_ms,
            kind: crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                role,
                label: Some(
                    match role {
                        crate::run::CapturedValueRole::CallInput => "inputs",
                        crate::run::CapturedValueRole::CallOutput => "output",
                        crate::run::CapturedValueRole::CallError => "error",
                        crate::run::CapturedValueRole::RootInput => "inputs",
                    }
                    .to_string(),
                ),
                value_ref: Some(record.value_ref.clone()),
                trace_call: Some(capture.call),
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        });
        next_payload_id = next_payload_id.saturating_add(1);
    }
    for record in log_values {
        let call_node = reconstructed
            .calls
            .iter()
            .any(|call| call.trace_key == record.event.call)
            .then(|| call_node_id(&record.event.call));
        payloads.push(PayloadEvent {
            id: PayloadId(next_payload_id),
            call_node_id: call_node,
            timestamp_ms: record.event.timestamp_ms,
            kind: crate::run::PayloadKind::Log(crate::run::LogPayload {
                level: record.event.level.clone(),
                message: record
                    .event
                    .message_preview
                    .clone()
                    .unwrap_or_else(|| "captured log".to_string()),
                source: record.event.source.clone(),
                value_ref: Some(record.value_ref),
                trace_call: Some(record.event.call),
            }),
            redaction: RedactionMetadata::display_safe(),
            body: None,
        });
        next_payload_id = next_payload_id.saturating_add(1);
    }

    let completed_at_ms = completed.as_ref().map(|record| record.completed_at_ms);
    let status = completed
        .as_ref()
        .map_or(RunStatus::Running, |record| record.status);
    let (result, error, cancellation) =
        outcome_fields_from_replay(completed, output_ref, error_ref);
    let mut calls = reconstructed.calls;
    attach_payload_ids_to_calls(&mut calls, &payloads);

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
        calls,
        threads: reconstructed.threads,
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
            let (value_ref, body, blob_ref) = match record {
                ValueFileRecord::CapturedValue(record) => {
                    (record.value_ref, record.body, record.blob_ref)
                }
                ValueFileRecord::LogEvent(record) => {
                    (record.value_ref, record.body, record.blob_ref)
                }
                ValueFileRecord::CaptureLoss(_)
                | ValueFileRecord::RunStarted(_)
                | ValueFileRecord::RunCompleted(_) => continue,
            };
            if value_ref.id == value_ref_id {
                return match hydrate_value_body(body, blob_ref.as_ref(), blob_store)? {
                    Ok(body) => Ok(HistoryValueReadResult::Available(HistoryValueBody {
                        codec: value_ref.codec,
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
    if let Err(err) = blob_ref.validate() {
        return Ok(Err(HistoryValueBodyUnavailable {
            reason: HistoryValueBodyUnavailableReason::BlobInvalid,
            diagnostic: format!(
                "value body blob ref {} is invalid: {err}",
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
        Err(err) if err.kind() == io::ErrorKind::InvalidData => {
            Ok(Err(HistoryValueBodyUnavailable {
                reason: HistoryValueBodyUnavailableReason::BlobIntegrityMismatch,
                diagnostic: format!(
                    "value body blob {} failed integrity verification: {err}",
                    blob_ref_label(blob_ref)
                ),
            }))
        }
        Err(err) => Err(err),
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
                value_ref: completed.result_value_ref.or(output_ref),
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
            if error.value_ref.is_none() {
                error.value_ref = error_ref;
            }
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

fn capture_loss_replay_diagnostic(record: CaptureLossRecord) -> RunDiagnostic {
    let mut diagnostic = history_diagnostic(
        "valueCaptureLoss",
        record.message.unwrap_or_else(|| {
            format!(
                "Skipped {} captured {} value(s) because the trace capture queue was full",
                record.skipped_count,
                record.kind.as_wire_str()
            )
        }),
    );
    diagnostic.call_node_id = record.call.map(|call| call_node_id(&call));
    diagnostic
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
    paths.sort_by(|left, right| {
        segment_sort_key(left, prefix).cmp(&segment_sort_key(right, prefix))
    });
    paths
}

#[cfg(not(target_arch = "wasm32"))]
fn segment_sort_key(path: &Path, prefix: &str) -> (u64, u64, String) {
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
        .and_then(|name| name.strip_prefix(prefix))
        .and_then(|id| id.parse::<u64>().ok())
        .unwrap_or(u64::MAX);
    (thread_id, segment_id, path.display().to_string())
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
        value::{
            BlobRef, CaptureLossKind, CaptureLossReason, CaptureLossRecord, ValueCaptureKind,
            ValueRecord,
        },
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

    fn child_call_event(call_id: u64, parent_call_id: u64) -> pb::DiskEventV1 {
        pb::DiskEventV1 {
            event: Some(pb::disk_event_v1::Event::CallFunction(pb::CallFunction {
                thread_id: 1,
                call_id,
                parent_call_id: Some(parent_call_id),
                function_id: u32::try_from(call_id).expect("test call id fits in u32"),
                timestamp_ns: 5 + call_id,
                call_site_file_id: None,
                call_site_start_offset: None,
                call_site_end_offset: None,
                call_site_line: None,
            })),
        }
    }

    fn ingest_profile_event(store: &HistoryStore, trace: TraceCallKey, event: pb::DiskEventV1) {
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
    fn replay_prefers_completed_result_ref_and_falls_back_to_root_output() {
        let completed_ref =
            ValueRef::available("value_completed", ValueCodec::BamlOutboundValue, 3, 3);
        let captured_ref =
            ValueRef::available("value_captured", ValueCodec::BamlOutboundValue, 3, 3);
        let completed = RunCompletedRecord {
            status: RunStatus::Succeeded,
            completed_at_ms: 20,
            renderer_hint: Some("baml.outbound.base64".to_string()),
            result_value_ref: Some(completed_ref.clone()),
            error: None,
            cancellation: None,
        };

        let (result, error, cancellation) =
            outcome_fields_from_replay(Some(completed), Some(captured_ref.clone()), None);
        assert_eq!(result.unwrap().value_ref, Some(completed_ref));
        assert_eq!(error, None);
        assert_eq!(cancellation, None);

        let legacy_completed = RunCompletedRecord {
            status: RunStatus::Succeeded,
            completed_at_ms: 20,
            renderer_hint: None,
            result_value_ref: None,
            error: None,
            cancellation: None,
        };
        let (result, _, _) =
            outcome_fields_from_replay(Some(legacy_completed), Some(captured_ref.clone()), None);
        assert_eq!(result.unwrap().value_ref, Some(captured_ref));
    }

    #[test]
    fn replay_preserves_completed_error_ref_and_falls_back_to_root_error() {
        let completed_ref =
            ValueRef::available("value_completed_error", ValueCodec::BamlOutboundValue, 2, 2);
        let captured_ref =
            ValueRef::available("value_captured_error", ValueCodec::BamlOutboundValue, 2, 2);
        let completed = RunCompletedRecord {
            status: RunStatus::Failed,
            completed_at_ms: 20,
            renderer_hint: None,
            result_value_ref: None,
            error: Some(RunError {
                class: RunErrorClass::Runtime,
                message: "boom".to_string(),
                details: None,
                value_ref: Some(completed_ref.clone()),
            }),
            cancellation: None,
        };

        let (result, error, cancellation) =
            outcome_fields_from_replay(Some(completed), None, Some(captured_ref.clone()));
        assert_eq!(result, None);
        assert_eq!(error.unwrap().value_ref, Some(completed_ref));
        assert_eq!(cancellation, None);

        let legacy_completed = RunCompletedRecord {
            status: RunStatus::Failed,
            completed_at_ms: 20,
            renderer_hint: None,
            result_value_ref: None,
            error: Some(RunError {
                class: RunErrorClass::Runtime,
                message: "old boom".to_string(),
                details: None,
                value_ref: None,
            }),
            cancellation: None,
        };
        let (_, error, _) =
            outcome_fields_from_replay(Some(legacy_completed), None, Some(captured_ref.clone()));
        assert_eq!(error.unwrap().value_ref, Some(captured_ref));
    }

    #[test]
    fn segment_paths_sort_by_numeric_thread_and_segment() {
        let project = temp_project("numeric-segment-sort");
        let boundary_dir = project.join("boundary");
        for relative in [
            "thread-1/value-10.bamlvalue",
            "thread-1/value-2.bamlvalue",
            "thread-10/value-0.bamlvalue",
            "thread-2/value-0.bamlvalue",
        ] {
            let path = boundary_dir.join(relative);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"").unwrap();
        }

        let ordered = value_segment_paths(&boundary_dir)
            .into_iter()
            .map(|path| {
                path.strip_prefix(&boundary_dir)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            ordered,
            vec![
                "thread-1/value-2.bamlvalue",
                "thread-1/value-10.bamlvalue",
                "thread-2/value-0.bamlvalue",
                "thread-10/value-0.bamlvalue",
            ]
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_rotates_stack_and_value_segments_and_replays() {
        let project = temp_project("segment-rotation");
        let boundary_id = BoundaryId::from_bytes([31; 16]);
        let store = HistoryStore::new_with_rotation_policy(
            vec![project.clone()],
            SegmentRotationPolicy::for_tests(u64::MAX, 1, u64::MAX, 2),
        );
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();

        let root = root_trace();
        for event in [call_event(), child_call_event(3, root.call_id.0)] {
            let envelope = crate::run::profile_event_envelope_from_disk_event(
                crate::run::ProfileEventSource::Replay {
                    artifact_id: "test".to_string(),
                },
                root.process_euid,
                root.engine_id,
                &event,
            )
            .unwrap();
            store.ingest_history_profile_event(envelope, event);
        }
        store
            .attach_root_trace(boundary_id, root.call_ref())
            .unwrap();

        let root_output = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::RootOutput,
                    call: root,
                },
                ValueCodec::BamlOutboundValue,
                vec![1, 2, 3],
            )
            .unwrap();
        let child_trace = TraceCallKey {
            call_id: BexCallId(3),
            ..root
        };
        let child_output = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallOutput,
                    call: child_trace,
                },
                ValueCodec::BamlOutboundValue,
                vec![4, 5, 6],
            )
            .unwrap();
        assert_eq!(root_output.value_ref.id, "value_1");
        assert_eq!(child_output.value_ref.id, "value_2");
        store
            .complete(
                boundary_id,
                &RunOutcome::Succeeded(RunResult {
                    value_ref: Some(root_output.value_ref),
                    renderer_hint: None,
                    supporting_payload_ids: Vec::new(),
                }),
                20,
            )
            .unwrap();

        let boundary_dir =
            find_boundary_dir(std::slice::from_ref(&project), boundary_id).expect("boundary dir");
        let stack_names = stack_segment_paths(&boundary_dir)
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(stack_names, vec!["stack-0.bamlprof", "stack-1.bamlprof"]);
        let value_names = value_segment_paths(&boundary_dir)
            .into_iter()
            .map(|path| path.file_name().unwrap().to_string_lossy().to_string())
            .collect::<Vec<_>>();
        assert_eq!(value_names, vec!["value-0.bamlvalue", "value-1.bamlvalue"]);

        let replayed = store.open(boundary_id).unwrap();
        assert_eq!(replayed.status, RunStatus::Succeeded);
        assert_eq!(
            replayed
                .result
                .as_ref()
                .and_then(|result| result.value_ref.as_ref())
                .map(|value_ref| value_ref.id.as_str()),
            Some("value_1")
        );
        assert_eq!(
            store
                .read_value(boundary_id, &child_output.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![4, 5, 6]
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_value_ids_are_unique_across_threads() {
        let project = temp_project("thread-value-ids");
        let boundary_id = BoundaryId::from_bytes([41; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();

        let thread_one = root_trace();
        let thread_two = TraceCallKey {
            thread_id: BexThreadId(2),
            call_id: BexCallId(1),
            ..thread_one
        };
        let first = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallOutput,
                    call: thread_one,
                },
                ValueCodec::BamlOutboundValue,
                vec![1],
            )
            .unwrap();
        let second = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallOutput,
                    call: thread_two,
                },
                ValueCodec::BamlOutboundValue,
                vec![2],
            )
            .unwrap();

        assert_eq!(first.value_ref.id, "value_1");
        assert_eq!(second.value_ref.id, "value_2");
        assert_eq!(
            store
                .read_value(boundary_id, &first.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![1]
        );
        assert_eq!(
            store
                .read_value(boundary_id, &second.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![2]
        );

        let _ = std::fs::remove_dir_all(project);
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
            &find_boundary_dir(std::slice::from_ref(&project), boundary_id).expect("boundary dir"),
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
    fn history_routes_new_profile_records_after_router_drops_claimed_records() {
        let project = temp_project("route-drop");
        let boundary_id = BoundaryId::from_bytes([42; 16]);
        let start = start_context(boundary_id);
        let store = HistoryStore::new_with_rotation_policy_and_router_capacity(
            vec![project.clone()],
            SegmentRotationPolicy::default(),
            1,
        );
        store.begin(&project, &start).unwrap();
        let trace = root_trace();
        store
            .attach_root_trace(boundary_id, trace.call_ref())
            .unwrap();

        ingest_profile_event(&store, trace, call_event());
        ingest_profile_event(&store, trace, child_call_event(3, 2));

        {
            let inner = store
                .inner
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            assert_eq!(inner.router.dropped_records(), 1);
        }

        let boundary_dir =
            find_boundary_dir(std::slice::from_ref(&project), boundary_id).expect("boundary dir");
        let stack_path = stack_segment_paths(&boundary_dir)
            .pop()
            .expect("stack segment");
        let stack_bytes = std::fs::read(stack_path).unwrap();
        let parsed = read_bamlprof_from_bytes(&stack_bytes).unwrap();
        assert_eq!(
            parsed.events.len(),
            2,
            "child profile event should be routed even after router index reuse"
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_replays_log_payloads_and_bodies() {
        let project = temp_project("log-replay");
        let boundary_id = BoundaryId::from_bytes([9; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();

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
        let source = crate::run::SourceLocation {
            file_path: Some("main.baml".to_string()),
            file_id: None,
            line: 17,
            column: 5,
            end_line: None,
            end_column: None,
            start_offset: Some(80),
            end_offset: Some(96),
        };
        let outcome = store
            .append_log_body(
                boundary_id,
                LogEventRecord {
                    call: trace,
                    level: Some("info".to_string()),
                    source: Some(source.clone()),
                    timestamp_ms: 12,
                    message_preview: Some("log preview".to_string()),
                },
                ValueCodec::BamlOutboundValue,
                vec![4, 5, 6],
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
                20,
            )
            .unwrap();

        let replayed = store.open(boundary_id).unwrap();
        let expected_call_id = call_node_id(&trace);
        let payload = replayed
            .payloads
            .iter()
            .find(|payload| matches!(payload.kind, crate::run::PayloadKind::Log(_)))
            .expect("log payload should replay");
        assert_eq!(payload.call_node_id, Some(expected_call_id));
        let crate::run::PayloadKind::Log(log) = &payload.kind else {
            unreachable!("filtered to log");
        };
        assert_eq!(log.level.as_deref(), Some("info"));
        assert_eq!(log.message, "log preview");
        assert_eq!(log.source, Some(source));
        assert_eq!(
            log.value_ref
                .as_ref()
                .map(|value_ref| value_ref.id.as_str()),
            Some("value_1")
        );
        assert_eq!(log.trace_call, Some(trace));
        assert_eq!(
            replayed
                .calls
                .iter()
                .find(|call| call.id == expected_call_id)
                .expect("call should replay")
                .payload_ids,
            vec![payload.id]
        );
        assert_eq!(
            store
                .read_value(boundary_id, &outcome.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![4, 5, 6]
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_blob_value_hydrates_from_boundary_blob_store_and_reports_missing() {
        let project = temp_project("blob-hydrate");
        let boundary_id = BoundaryId::from_bytes([27; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();
        let trace = root_trace();
        store
            .attach_root_trace(boundary_id, trace.call_ref())
            .unwrap();

        let large_body = vec![9; 70 * 1024];
        let outcome = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::RootOutput,
                    call: trace,
                },
                ValueCodec::BamlOutboundValue,
                large_body.clone(),
            )
            .unwrap();

        assert_eq!(
            store
                .read_value(boundary_id, &outcome.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            large_body
        );

        let boundary_dir = find_boundary_dir(std::slice::from_ref(&project), boundary_id).unwrap();
        let value_segments = value_segment_paths(&boundary_dir)
            .into_iter()
            .map(|path| HistoryValueSegment {
                label: path.display().to_string(),
                bytes: std::fs::read(path).unwrap(),
            })
            .collect::<Vec<_>>();
        let no_store = read_value_from_segments_with_blobs_result(
            &value_segments,
            &outcome.value_ref.id,
            None,
        )
        .unwrap();
        let HistoryValueReadResult::BodyUnavailable(unavailable) = no_store else {
            panic!("expected blob-store unavailable result");
        };
        assert_eq!(
            unavailable.reason,
            HistoryValueBodyUnavailableReason::BlobStoreUnavailable
        );
        assert!(unavailable.diagnostic.contains("blob-backed"));

        let blob_paths = std::fs::read_dir(boundary_dir.join("blobs").join("sha256"))
            .unwrap()
            .flatten()
            .flat_map(|entry| std::fs::read_dir(entry.path()).unwrap().flatten())
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "blob"))
            .collect::<Vec<_>>();
        assert_eq!(blob_paths.len(), 1);
        std::fs::remove_file(&blob_paths[0]).unwrap();
        let missing_blob = store
            .read_value_result(boundary_id, &outcome.value_ref.id)
            .unwrap();
        let HistoryValueReadResult::BodyUnavailable(unavailable) = missing_blob else {
            panic!("expected missing blob result");
        };
        assert_eq!(
            unavailable.reason,
            HistoryValueBodyUnavailableReason::BlobMissing
        );
        assert!(unavailable.diagnostic.contains("is missing"));
        assert!(unavailable.diagnostic.contains("sha256:"));

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_reports_invalid_blob_ref_without_path_traversal() {
        let boundary_id = BoundaryId::from_bytes([42; 16]);
        let trace = root_trace();
        let mut bytes = Vec::new();
        crate::value::encode::encode_header(&mut bytes, boundary_id).unwrap();
        crate::value::encode::encode_record(
            &mut bytes,
            &ValueRecord {
                value_ref: ValueRef::available("value_bad", ValueCodec::BamlOutboundValue, 3, 3),
                body: Vec::new(),
                blob_ref: Some(BlobRef {
                    algorithm: BlobRef::ALGORITHM_SHA256.to_string(),
                    digest: "../bad".to_string(),
                    size_bytes: 3,
                }),
                capture: Some(ValueCapture {
                    kind: ValueCaptureKind::RootOutput,
                    call: trace,
                }),
            },
        )
        .unwrap();
        let segments = vec![HistoryValueSegment {
            label: "bad-value-segment".to_string(),
            bytes,
        }];

        let result =
            read_value_from_segments_with_blobs_result(&segments, "value_bad", None).unwrap();
        let HistoryValueReadResult::BodyUnavailable(unavailable) = result else {
            panic!("expected invalid blob ref to make body unavailable");
        };
        assert_eq!(
            unavailable.reason,
            HistoryValueBodyUnavailableReason::BlobInvalid
        );
        assert!(unavailable.diagnostic.contains("invalid"));
    }

    #[test]
    fn history_reports_blob_integrity_mismatch() {
        let project = temp_project("blob-integrity");
        let boundary_id = BoundaryId::from_bytes([43; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();
        let trace = root_trace();
        store
            .attach_root_trace(boundary_id, trace.call_ref())
            .unwrap();

        let large_body = vec![9; 70 * 1024];
        let outcome = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::RootOutput,
                    call: trace,
                },
                ValueCodec::BamlOutboundValue,
                large_body.clone(),
            )
            .unwrap();

        let boundary_dir = find_boundary_dir(std::slice::from_ref(&project), boundary_id).unwrap();
        let blob_path = std::fs::read_dir(boundary_dir.join("blobs").join("sha256"))
            .unwrap()
            .flatten()
            .flat_map(|entry| std::fs::read_dir(entry.path()).unwrap().flatten())
            .map(|entry| entry.path())
            .find(|path| path.extension().is_some_and(|ext| ext == "blob"))
            .expect("blob file should exist");
        std::fs::write(blob_path, vec![8; large_body.len()]).unwrap();

        let result = store
            .read_value_result(boundary_id, &outcome.value_ref.id)
            .unwrap();
        let HistoryValueReadResult::BodyUnavailable(unavailable) = result else {
            panic!("expected tampered blob body to be unavailable");
        };
        assert_eq!(
            unavailable.reason,
            HistoryValueBodyUnavailableReason::BlobIntegrityMismatch
        );
        assert!(unavailable.diagnostic.contains("integrity"));

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_replays_call_value_payloads_without_root_outcome_refs() {
        let project = temp_project("call-value-replay");
        let boundary_id = BoundaryId::from_bytes([12; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();

        let root = root_trace();
        for event in [
            call_event(),
            child_call_event(2, root.call_id.0),
            child_call_event(3, root.call_id.0),
            child_call_event(4, root.call_id.0),
        ] {
            let envelope = crate::run::profile_event_envelope_from_disk_event(
                crate::run::ProfileEventSource::Replay {
                    artifact_id: "test".to_string(),
                },
                root.process_euid,
                root.engine_id,
                &event,
            )
            .unwrap();
            store.ingest_history_profile_event(envelope, event);
        }
        store
            .attach_root_trace(boundary_id, root.call_ref())
            .unwrap();

        let output_call = TraceCallKey {
            call_id: BexCallId(3),
            ..root
        };
        let error_call = TraceCallKey {
            call_id: BexCallId(4),
            ..root
        };
        let input_call = TraceCallKey {
            call_id: BexCallId(2),
            ..root
        };
        let input = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallInput,
                    call: input_call,
                },
                ValueCodec::BamlOutboundValue,
                vec![0, 1],
            )
            .unwrap();
        let output = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallOutput,
                    call: output_call,
                },
                ValueCodec::BamlOutboundValue,
                vec![1, 2, 3],
            )
            .unwrap();
        let error = store
            .append_value_body(
                boundary_id,
                ValueCapture {
                    kind: ValueCaptureKind::CallError,
                    call: error_call,
                },
                ValueCodec::BamlOutboundValue,
                vec![4, 5],
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
                20,
            )
            .unwrap();

        let replayed = store.open(boundary_id).unwrap();
        assert_eq!(replayed.status, RunStatus::Succeeded);
        assert_eq!(
            replayed
                .result
                .as_ref()
                .and_then(|result| result.value_ref.as_ref()),
            None
        );
        assert_eq!(replayed.error, None);

        let input_call_node_id = call_node_id(&input_call);
        let output_call_node_id = call_node_id(&output_call);
        let error_call_node_id = call_node_id(&error_call);
        let input_payload = replayed
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                        role: crate::run::CapturedValueRole::CallInput,
                        ..
                    })
                )
            })
            .expect("call input payload should replay");
        assert_eq!(input_payload.call_node_id, Some(input_call_node_id));
        assert!(matches!(
            &input_payload.kind,
            crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                label: Some(label),
                ..
            }) if label == "inputs"
        ));
        let output_payload = replayed
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                        role: crate::run::CapturedValueRole::CallOutput,
                        ..
                    })
                )
            })
            .expect("call output payload should replay");
        assert_eq!(output_payload.call_node_id, Some(output_call_node_id));
        let error_payload = replayed
            .payloads
            .iter()
            .find(|payload| {
                matches!(
                    &payload.kind,
                    crate::run::PayloadKind::CapturedValue(crate::run::CapturedValuePayload {
                        role: crate::run::CapturedValueRole::CallError,
                        ..
                    })
                )
            })
            .expect("call error payload should replay");
        assert_eq!(error_payload.call_node_id, Some(error_call_node_id));

        assert_eq!(
            replayed
                .calls
                .iter()
                .find(|call| call.id == input_call_node_id)
                .expect("input call should replay")
                .payload_ids,
            vec![input_payload.id]
        );
        assert_eq!(
            replayed
                .calls
                .iter()
                .find(|call| call.id == output_call_node_id)
                .expect("output call should replay")
                .payload_ids,
            vec![output_payload.id]
        );
        assert_eq!(
            replayed
                .calls
                .iter()
                .find(|call| call.id == error_call_node_id)
                .expect("error call should replay")
                .payload_ids,
            vec![error_payload.id]
        );
        assert_eq!(
            store
                .read_value(boundary_id, &input.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![0, 1]
        );
        assert_eq!(
            store
                .read_value(boundary_id, &output.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![1, 2, 3]
        );
        assert_eq!(
            store
                .read_value(boundary_id, &error.value_ref.id)
                .unwrap()
                .unwrap()
                .body,
            vec![4, 5]
        );

        let _ = std::fs::remove_dir_all(project);
    }

    #[test]
    fn history_replays_capture_loss_as_diagnostic() {
        let project = temp_project("capture-loss");
        let boundary_id = BoundaryId::from_bytes([10; 16]);
        let store = HistoryStore::new(vec![project.clone()]);
        let start = start_context(boundary_id);
        store.begin(&project, &start).unwrap();

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

        store
            .append_capture_loss(
                boundary_id,
                &CaptureLossRecord {
                    kind: CaptureLossKind::Log,
                    reason: CaptureLossReason::QueueFull,
                    skipped_count: 4,
                    call: None,
                    message: Some(
                        "Skipped 4 captured log value(s) because the trace capture queue was full"
                            .to_string(),
                    ),
                    timestamp_ms: 30,
                },
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
                40,
            )
            .unwrap();

        let replayed = store.open(boundary_id).unwrap();
        let diagnostic = replayed
            .diagnostics
            .iter()
            .find(|diagnostic| diagnostic.code.as_deref() == Some("valueCaptureLoss"))
            .expect("capture loss should replay as diagnostic");
        assert_eq!(
            diagnostic.message,
            "Skipped 4 captured log value(s) because the trace capture queue was full"
        );
        assert_eq!(diagnostic.call_node_id, None);

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

    #[test]
    fn read_value_result_propagates_unreadable_value_segment() {
        let project = temp_project("unreadable-value-segment");
        let boundary_id = BoundaryId::from_bytes([18; 16]);
        let (store, _outcome) = write_basic_history_run(&project, boundary_id);
        assert_eq!(
            store
                .read_value_result(boundary_id, "value_does_not_exist")
                .unwrap(),
            HistoryValueReadResult::Missing
        );

        let boundary_dir =
            find_boundary_dir(std::slice::from_ref(&project), boundary_id).expect("boundary dir");
        let unreadable_segment = boundary_dir.join("thread-1").join("value-1.bamlvalue");
        std::fs::create_dir(&unreadable_segment).unwrap();

        let err = store
            .read_value_result(boundary_id, "value_does_not_exist")
            .expect_err("unreadable segment should not be reported as a missing value");
        assert!(
            err.to_string().contains("failed to read value segment"),
            "{err}"
        );

        let _ = std::fs::remove_dir_all(project);
    }
}
