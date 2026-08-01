#[cfg(feature = "native")]
use std::{
    collections::BTreeMap,
    fs,
    io::Read as _,
    path::{Path, PathBuf},
};

#[cfg(feature = "native")]
use bex_events::{
    history::path::list_boundary_dirs,
    ids::BoundaryId,
    prof::storage::{
        BoundaryBeginMeta, BoundaryBoundMeta, BoundaryCompleteMeta, BoundaryLossMeta,
        BoundaryTriggerMeta, SessionBeginMeta, SessionEndMeta, SessionHeartbeatMeta,
        TypedBoundaryMeta, TypedSessionMeta, decode_typed_boundary_meta, decode_typed_session_meta,
        scan_meta_bytes,
    },
};

use crate::Completeness;

#[cfg(feature = "native")]
use crate::{FileId, HARD_MAX_BYTES, QueryError};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum RunState {
    #[default]
    MissingMeta = 0,
    Begun = 1,
    Bound = 2,
    Running = 3,
    Crashed = 4,
    Complete = 5,
    PartialWithLoss = 6,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RunCursor {
    pub created_ms: u64,
    pub boundary_id: [u8; 16],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunSummary {
    pub boundary_id: [u8; 16],
    pub boundary_id_wire: String,
    pub created_ms: u64,
    pub target: String,
    pub state: RunState,
    pub has_snapshot: bool,
    pub meta_torn_tail: bool,
    pub meta_records: u32,
    pub revision_id: Option<[u8; 32]>,
    pub source: Option<String>,
    pub completion_status: Option<String>,
    pub partition_id: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunListing {
    pub runs: Vec<RunSummary>,
    pub next_cursor: Option<RunCursor>,
    pub meta: Completeness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunMeta {
    pub summary: RunSummary,
    #[cfg(feature = "native")]
    pub boundary_dir: PathBuf,
    #[cfg(not(feature = "native"))]
    pub boundary_dir: String,
    pub records: Vec<RunMetaRecord>,
    #[cfg(feature = "native")]
    pub begin: Option<BoundaryBeginMeta>,
    #[cfg(feature = "native")]
    pub bound: Option<BoundaryBoundMeta>,
    #[cfg(feature = "native")]
    pub complete: Option<BoundaryCompleteMeta>,
    #[cfg(feature = "native")]
    pub triggers: Vec<BoundaryTriggerMeta>,
    #[cfg(feature = "native")]
    pub losses: Vec<BoundaryLossMeta>,
    #[cfg(feature = "native")]
    pub session: Option<SessionMeta>,
    pub committed_meta_len: u64,
    pub meta: Completeness,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunMetaRecord {
    pub kind: u8,
    pub payload: Vec<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(u8)]
pub enum ProcessLiveness {
    #[default]
    Unknown = 0,
    Alive = 1,
    Dead = 2,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionMeta {
    #[cfg(feature = "native")]
    pub path: PathBuf,
    #[cfg(not(feature = "native"))]
    pub path: String,
    #[cfg(feature = "native")]
    pub begin: Option<SessionBeginMeta>,
    #[cfg(feature = "native")]
    pub heartbeat: Option<SessionHeartbeatMeta>,
    #[cfg(feature = "native")]
    pub end: Option<SessionEndMeta>,
    pub liveness: ProcessLiveness,
    pub torn_tail: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ListRunsRequest {
    pub limit: u16,
    pub max_bytes: usize,
    pub cursor: Option<RunCursor>,
}

impl Default for ListRunsRequest {
    fn default() -> Self {
        Self {
            limit: 1000,
            max_bytes: crate::DEFAULT_MAX_BYTES,
            cursor: None,
        }
    }
}

#[cfg(feature = "native")]
pub fn list_runs(
    search_roots: &[PathBuf],
    request: ListRunsRequest,
) -> Result<RunListing, QueryError> {
    if request.limit == 0 || request.limit > 1000 {
        return Err(QueryError::invalid_request("run limit must be in 1..=1000"));
    }
    if request.max_bytes < 1024 || request.max_bytes > HARD_MAX_BYTES {
        return Err(QueryError::invalid_request(format!(
            "max_bytes must be in 1024..={HARD_MAX_BYTES}"
        )));
    }
    let mut candidates = Vec::new();
    let mut warnings = Vec::new();
    let mut sources_consulted = Vec::new();
    let mut snapshots = Vec::new();
    for directory in list_boundary_dirs(search_roots) {
        match open_run_meta(&directory) {
            Ok(meta) => {
                sources_consulted.extend(meta.meta.sources_consulted);
                snapshots.extend(meta.meta.snapshot);
                if meta.meta.partial_tail {
                    warnings.extend(meta.meta.warnings);
                }
                candidates.push(meta.summary);
            }
            Err(error) => warnings.push(format!("{}: {error}", directory.display())),
        }
    }
    candidates.sort_by_key(|run| {
        (
            std::cmp::Reverse(run.created_ms),
            std::cmp::Reverse(run.boundary_id),
        )
    });
    if let Some(cursor) = request.cursor {
        candidates.retain(|run| {
            (run.created_ms, run.boundary_id) < (cursor.created_ms, cursor.boundary_id)
        });
    }
    let mut runs = Vec::new();
    let mut retained_bytes = 512_usize;
    let mut more = false;
    for run in candidates {
        let bytes = estimated_summary_bytes(&run);
        if runs.len() >= usize::from(request.limit)
            || retained_bytes.saturating_add(bytes) > request.max_bytes
        {
            more = true;
            break;
        }
        retained_bytes = retained_bytes.saturating_add(bytes);
        runs.push(run);
    }
    let next_cursor = more.then(|| runs.last()).flatten().map(|run| RunCursor {
        created_ms: run.created_ms,
        boundary_id: run.boundary_id,
    });
    let mut meta = Completeness {
        complete: !more && warnings.is_empty(),
        truncated: more,
        warnings,
        sources_consulted,
        snapshot: snapshots,
        ..Completeness::default()
    };
    meta.finalize();
    Ok(RunListing {
        runs,
        next_cursor,
        meta,
    })
}

#[cfg(feature = "native")]
pub fn open_run_meta(directory: &Path) -> Result<RunMeta, QueryError> {
    open_run_meta_with_pins(directory, None)
}

/// Opens run metadata at the exact committed prefixes carried by a BQL
/// snapshot. Append-only growth remains invisible; truncation or replacement
/// fails closed.
#[cfg(feature = "native")]
pub fn open_run_meta_pinned(
    directory: &Path,
    pins: &BTreeMap<FileId, crate::SourceSnapshot>,
) -> Result<RunMeta, QueryError> {
    open_run_meta_with_pins(directory, Some(pins))
}

#[cfg(feature = "native")]
fn open_run_meta_with_pins(
    directory: &Path,
    pins: Option<&BTreeMap<FileId, crate::SourceSnapshot>>,
) -> Result<RunMeta, QueryError> {
    let (directory_boundary_id, directory_created_ms, directory_target) =
        parse_boundary_directory(directory)?;
    let meta_path = directory.join("boundary.bamlmeta");
    let (records, committed_meta_len, meta_torn_tail, meta_source) =
        match read_snapshot_source(&meta_path, pins)? {
            Some((bytes, mut source)) => {
                let scan = scan_meta_bytes(&bytes);
                source.parsed_through = scan.committed_len;
                (
                    scan.records,
                    scan.committed_len,
                    scan.torn_tail,
                    Some(source),
                )
            }
            None if pins
                .is_some_and(|pins| pins.contains_key(&FileId(stable_path_id(&meta_path)))) =>
            {
                return Err(QueryError::invalid_request(format!(
                    "metadata source {} from the snapshot is missing",
                    stable_path_id(&meta_path)
                )));
            }
            None => (Vec::new(), 0, false, None),
        };
    let typed = records
        .iter()
        .map(decode_typed_boundary_meta)
        .collect::<Result<Vec<_>, _>>()?;
    let mut begin = None;
    let mut bound = None;
    let mut complete = None;
    let mut triggers = Vec::new();
    let mut losses = Vec::new();
    for record in typed {
        match record {
            TypedBoundaryMeta::Begin(value) => begin = Some(value),
            TypedBoundaryMeta::Bound(value) => bound = Some(value),
            TypedBoundaryMeta::Complete(value) => complete = Some(value),
            TypedBoundaryMeta::Trigger(value) => triggers.push(value),
            TypedBoundaryMeta::Loss(value) => losses.push(value),
            TypedBoundaryMeta::Unknown(_) => {}
        }
    }
    if let Some(value) = &begin
        && value.boundary_id != directory_boundary_id.as_bytes()
    {
        return Err(QueryError::invalid_data(
            "boundary begin id does not match its directory",
        ));
    }
    let boundary_id = begin.as_ref().map_or(directory_boundary_id, |value| {
        BoundaryId::from_bytes(value.boundary_id)
    });
    let created_ms = begin
        .as_ref()
        .map_or(directory_created_ms, |value| value.created_ms);
    let target = begin
        .as_ref()
        .map_or(directory_target, |value| value.target.clone());
    let session_source = bound
        .as_ref()
        .and_then(|binding| read_bound_session(directory, binding, pins).transpose())
        .transpose()?;
    let (session, session_watermark) = session_source
        .map(|(session, source)| (Some(session), Some(source)))
        .unwrap_or((None, None));
    let state = state_from_typed(
        begin.as_ref(),
        bound.as_ref(),
        complete.as_ref(),
        &losses,
        session.as_ref(),
    );
    let summary = RunSummary {
        boundary_id: boundary_id.as_bytes(),
        boundary_id_wire: boundary_id.to_wire_string(),
        created_ms,
        target,
        state,
        has_snapshot: directory.join("cct.bamlcct").is_file(),
        meta_torn_tail,
        meta_records: u32::try_from(records.len()).unwrap_or(u32::MAX),
        revision_id: begin.as_ref().map(|value| value.revision_id),
        source: begin.as_ref().map(|value| value.source.clone()),
        completion_status: complete.as_ref().map(|value| value.status.clone()),
        partition_id: bound.as_ref().map(|value| value.partition_id),
    };
    let records = records
        .into_iter()
        .map(|record| RunMetaRecord {
            kind: record.kind,
            payload: record.payload,
        })
        .collect();
    let session_torn_tail = session.as_ref().is_some_and(|session| session.torn_tail);
    let mut warnings = Vec::new();
    if meta_torn_tail {
        warnings.push(format!(
            "boundary metadata has a torn tail after byte {committed_meta_len}"
        ));
    }
    if session_torn_tail {
        warnings.push("session metadata has a torn tail".to_owned());
    }
    let mut sources_consulted = Vec::new();
    let mut snapshot = Vec::new();
    if let Some(source) = meta_source {
        sources_consulted.push(source.file);
        snapshot.push(source);
    }
    if let Some(source) = session_watermark {
        sources_consulted.push(source.file);
        snapshot.push(source);
    }
    let mut meta = Completeness {
        complete: !meta_torn_tail && !session_torn_tail,
        partial_tail: meta_torn_tail || session_torn_tail,
        warnings,
        sources_consulted,
        snapshot,
        ..Completeness::default()
    };
    if matches!(
        state,
        RunState::Begun | RunState::Bound | RunState::Running | RunState::Crashed
    ) {
        meta.complete = false;
    }
    if matches!(state, RunState::Bound) {
        meta.warnings.push(
            "boundary has no complete record and session liveness is not provable".to_owned(),
        );
    }
    if matches!(state, RunState::Crashed) {
        meta.warnings
            .push("boundary is partial because its session is no longer alive".to_owned());
    }
    if !losses.is_empty() {
        meta.capture_loss
            .extend(losses.iter().map(|loss| crate::CaptureLoss {
                kind: bex_events::prof::storage::MarkerKind::Loss,
                timestamp_ns: loss.timestamp_ns,
                node_id: None,
                count: loss.count,
                message: format!("{}: {}", loss.kind, loss.detail),
            }));
    }
    meta.finalize();
    Ok(RunMeta {
        summary,
        boundary_dir: directory.to_path_buf(),
        records,
        begin,
        bound,
        complete,
        triggers,
        losses,
        session,
        committed_meta_len,
        meta,
    })
}

#[cfg(feature = "native")]
fn parse_boundary_directory(directory: &Path) -> Result<(BoundaryId, u64, String), QueryError> {
    let name = directory
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| QueryError::invalid_data("boundary directory name is not UTF-8"))?;
    let marker = "baml_id_1_";
    let marker_index = name
        .rfind(marker)
        .ok_or_else(|| QueryError::invalid_data("boundary directory has no boundary id"))?;
    let boundary_wire = &name[marker_index..];
    let boundary_id = BoundaryId::from_wire_str(boundary_wire)
        .ok_or_else(|| QueryError::invalid_data("boundary directory has invalid boundary id"))?;
    let prefix = name[..marker_index].trim_end_matches('-');
    let (created, target) = prefix
        .split_once('-')
        .ok_or_else(|| QueryError::invalid_data("boundary directory has no target slug"))?;
    let created_ms = created
        .parse::<u64>()
        .map_err(|_| QueryError::invalid_data("boundary directory has invalid timestamp"))?;
    Ok((boundary_id, created_ms, target.to_owned()))
}

#[cfg(feature = "native")]
fn state_from_typed(
    begin: Option<&BoundaryBeginMeta>,
    bound: Option<&BoundaryBoundMeta>,
    complete: Option<&BoundaryCompleteMeta>,
    losses: &[BoundaryLossMeta],
    session: Option<&SessionMeta>,
) -> RunState {
    if !losses.is_empty() {
        return RunState::PartialWithLoss;
    }
    if complete.is_some() {
        return RunState::Complete;
    }
    if bound.is_some() {
        return match session {
            Some(session) if session.end.is_some() || session.liveness == ProcessLiveness::Dead => {
                RunState::Crashed
            }
            Some(session) if session.liveness == ProcessLiveness::Alive => RunState::Running,
            _ => RunState::Bound,
        };
    }
    if begin.is_some() {
        RunState::Begun
    } else {
        RunState::MissingMeta
    }
}

impl PartialOrd for RunState {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RunState {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

#[cfg(feature = "native")]
fn estimated_summary_bytes(run: &RunSummary) -> usize {
    96_usize
        .saturating_add(run.boundary_id_wire.len())
        .saturating_add(run.target.len())
        .saturating_add(run.source.as_ref().map_or(0, String::len))
        .saturating_add(run.completion_status.as_ref().map_or(0, String::len))
}

#[cfg(feature = "native")]
fn stable_path_id(path: &Path) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for byte in path.as_os_str().as_encoded_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}

#[cfg(feature = "native")]
fn read_bound_session(
    boundary_dir: &Path,
    binding: &BoundaryBoundMeta,
    pins: Option<&BTreeMap<FileId, crate::SourceSnapshot>>,
) -> Result<Option<(SessionMeta, crate::SourceWatermark)>, QueryError> {
    let candidates = session_meta_candidates(boundary_dir, &binding.session_dir);
    let Some(path) = candidates.iter().find(|path| path.is_file()).cloned() else {
        if let Some(pins) = pins
            && let Some(file) = candidates
                .iter()
                .map(|path| FileId(stable_path_id(path)))
                .find(|file| pins.contains_key(file))
        {
            return Err(QueryError::invalid_request(format!(
                "metadata source {} from the snapshot is missing",
                file.0
            )));
        }
        return Ok(None);
    };
    let Some((bytes, mut source)) = read_snapshot_source(&path, pins)? else {
        return Ok(None);
    };
    let scan = scan_meta_bytes(&bytes);
    source.parsed_through = scan.committed_len;
    let mut begin = None;
    let mut heartbeat = None;
    let mut end = None;
    for record in &scan.records {
        match decode_typed_session_meta(record)? {
            TypedSessionMeta::Begin(value) => begin = Some(value),
            TypedSessionMeta::Heartbeat(value) => heartbeat = Some(value),
            TypedSessionMeta::End(value) => end = Some(value),
            TypedSessionMeta::EpochClose(_) | TypedSessionMeta::Unknown(_) => {}
        }
    }
    let pid = heartbeat
        .as_ref()
        .map(|value: &SessionHeartbeatMeta| value.pid)
        .or_else(|| begin.as_ref().map(|value: &SessionBeginMeta| value.pid));
    Ok(Some((
        SessionMeta {
            path,
            begin,
            heartbeat,
            end,
            liveness: pid.map_or(ProcessLiveness::Unknown, process_liveness),
            torn_tail: scan.torn_tail,
        },
        source,
    )))
}

#[cfg(feature = "native")]
fn read_snapshot_source(
    path: &Path,
    pins: Option<&BTreeMap<FileId, crate::SourceSnapshot>>,
) -> Result<Option<(Vec<u8>, crate::SourceWatermark)>, QueryError> {
    let opened = match fs::File::open(path) {
        Ok(opened) => opened,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let metadata = opened.metadata()?;
    let file = FileId(stable_path_id(path));
    let current_generation = file_generation(&metadata);
    let source = if let Some(pins) = pins {
        let pinned = pins.get(&file).copied().ok_or_else(|| {
            QueryError::invalid_request(format!(
                "snapshot does not include metadata source {}",
                file.0
            ))
        })?;
        if pinned.generation != current_generation {
            return Err(QueryError::invalid_request(format!(
                "metadata source {} was replaced after the snapshot",
                file.0
            )));
        }
        if pinned.committed_len > metadata.len() {
            return Err(QueryError::invalid_request(format!(
                "metadata source {} is shorter than the snapshot",
                file.0
            )));
        }
        pinned
    } else {
        crate::SourceSnapshot {
            committed_len: metadata.len(),
            generation: current_generation,
        }
    };
    let hard_max = u64::try_from(crate::HARD_MAX_BYTES).unwrap_or(u64::MAX);
    if source.committed_len > hard_max {
        return Err(QueryError::BudgetExceeded {
            required: usize::try_from(source.committed_len).unwrap_or(usize::MAX),
            max_bytes: crate::HARD_MAX_BYTES,
        });
    }
    let mut bytes = Vec::with_capacity(
        usize::try_from(source.committed_len)
            .unwrap_or(usize::MAX)
            .min(crate::HARD_MAX_BYTES),
    );
    opened.take(source.committed_len).read_to_end(&mut bytes)?;
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) != source.committed_len {
        return Err(QueryError::invalid_request(format!(
            "metadata source {} changed while reading its snapshot",
            file.0
        )));
    }
    Ok(Some((
        bytes,
        crate::SourceWatermark {
            file,
            source,
            parsed_through: source.committed_len,
        },
    )))
}

#[cfg(all(feature = "native", unix))]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    use std::os::unix::fs::MetadataExt as _;
    let mut hash = 0xcbf2_9ce4_8422_2325_u64;
    for value in [metadata.dev(), metadata.ino()] {
        for byte in value.to_le_bytes() {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

#[cfg(all(feature = "native", not(unix)))]
fn file_generation(metadata: &fs::Metadata) -> u64 {
    metadata
        .created()
        .or_else(|_| metadata.modified())
        .ok()
        .and_then(|created| created.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |duration| {
            u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
        })
}

#[cfg(feature = "native")]
fn session_meta_candidates(boundary_dir: &Path, session_dir: &str) -> Vec<PathBuf> {
    let supplied = PathBuf::from(session_dir);
    let project_root = boundary_dir
        .parent()
        .and_then(Path::parent)
        .and_then(Path::parent);
    let mut candidates = Vec::new();
    if supplied.is_absolute() {
        candidates.push(supplied);
    } else {
        if let Some(project_root) = project_root {
            candidates.push(project_root.join(&supplied));
            candidates.push(project_root.join(".baml").join("sessions").join(&supplied));
        }
        candidates.push(supplied);
    }
    candidates
        .into_iter()
        .map(|path| {
            if path
                .file_name()
                .is_some_and(|name| name == "session.bamlmeta")
            {
                path
            } else {
                path.join("session.bamlmeta")
            }
        })
        .collect()
}

#[cfg(all(feature = "native", target_os = "linux"))]
fn process_liveness(pid: u32) -> ProcessLiveness {
    if Path::new("/proc").join(pid.to_string()).exists() {
        ProcessLiveness::Alive
    } else {
        ProcessLiveness::Dead
    }
}

#[cfg(all(feature = "native", not(target_os = "linux")))]
fn process_liveness(_pid: u32) -> ProcessLiveness {
    ProcessLiveness::Unknown
}
