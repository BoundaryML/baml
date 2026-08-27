//! Stream/execution reader for the process-stream profiling store
//! (streams spec §6).
//!
//! Listing reads meta planes only; `ExecutionReader::load()` folds exactly
//! the data segments of one execution's range, skipping foreign groups by
//! slice arithmetic. Readers take the store lease shared for the lifetime of
//! a reader so `baml clean` cannot remove files underneath them; they never
//! take `publish.lock`.

use std::{
    collections::HashMap,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

use fs2::FileExt;

use super::{
    CctCounters, ContextKey, ContextRef, ContextTuple, CounterHealth, DecodedCasObject, EdgeKind,
    ErrorCapture, ErrorCaptureId, EvidenceFact, ExecutionEndStatus, ExecutionHealthSnapshot,
    FunctionTable, MetaRecord, OverflowReason, Plane, SegmentReadError, SpanEnd, SpanRuntimeId,
    SpanStart, StreamHighWater, StreamId, TerminalErrorRef, TerminalErrorTarget, ThreadEnd,
    ThreadStart, ThrowSite, ValueCid, ValueOccurrence, ValueRole, decode_cas_object,
    decode_data_segment, decode_function_table, decode_meta_segment, segment_path,
    stream_directory, stream_open_in_process,
};
use crate::ids::{BoundaryId, CallRef, EngineId, ExecutionId, ProcessEuid, ProgramId, ThreadRef};

#[derive(Debug)]
pub enum ReadError {
    Io {
        path: PathBuf,
        source: io::Error,
    },
    Decode {
        path: PathBuf,
        source: SegmentReadError,
    },
    StreamNotFound(StreamId),
    ExecutionNotFound(ExecutionId),
    MetaDuplicateRootStarted(ThreadRef),
    MetaDuplicateRootEnded(ThreadRef),
    DuplicateThreadStart(ThreadRef),
    DuplicateThreadEnd(ThreadRef),
    ConflictingContextDefinition(ContextKey),
    /// A context's parent chain revisits a key: the segments verified their
    /// checksums, so this is a forged or corrupt CCT, never a reorder.
    CyclicContextChain(ContextKey),
    DuplicateSpanStart(CallRef),
    DuplicateSpanEnd(CallRef),
    DuplicateValueOccurrence {
        call_ref: CallRef,
        role: ValueRole,
    },
    DuplicateErrorCapture(ErrorCaptureId),
    DuplicateTerminalError(CallRef),
    MissingSpanStart(CallRef),
    MissingContextDefinition(ContextKey),
    MissingErrorCapture(ErrorCaptureId),
    MissingOverflowBucket {
        reason: OverflowReason,
        edge_kind: EdgeKind,
    },
    CasIdentityMismatch(ValueCid),
    /// The referenced CAS object is not a `FunctionTableV1` object.
    FunctionTableCodecMismatch(ValueCid),
    FunctionTableInvalid(super::FunctionTableError),
    SequenceExhausted,
}

impl std::fmt::Display for ReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "profiling stream read failed: {self:?}")
    }
}

impl std::error::Error for ReadError {}

/// Decoded index records (reader-facing shapes of the meta plane).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StreamStarted {
    pub pid: u32,
    pub zero_unix_ns: u64,
    pub baml_version: String,
    pub os_arch: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineStarted {
    pub engine_id: EngineId,
    pub program_id: ProgramId,
    pub function_table_cid: Option<ValueCid>,
    pub revision_label: Option<String>,
    pub source_label: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootStarted {
    pub root: ThreadRef,
    pub started_ns: u64,
    pub runtime_id: BoundaryId,
    /// Meta sequence of the segment carrying this record.
    pub meta_sequence: u64,
    /// `data_high_water` of that segment: every group of this execution lies
    /// in a data segment with a strictly greater sequence.
    pub data_high_water: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootEnded {
    pub root: ThreadRef,
    pub ended_ns: u64,
    pub status: ExecutionEndStatus,
    pub flags: u8,
    pub data_first_seq: u64,
    pub data_last_seq: u64,
    pub data_segment_count: u64,
    pub health: ExecutionHealthSnapshot,
    pub meta_sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RootIndexEntry {
    pub root: ThreadRef,
    pub started: Option<RootStarted>,
    pub ended: Option<RootEnded>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExecutionStatus {
    Running,
    Abandoned,
    Succeeded,
    Failed,
    Cancelled,
    Panicked,
}

/// Index-plane completeness for one execution (streams spec §6.2).
/// `NoRootStarted` executions are not listed (see `orphan_groups`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IndexState {
    Complete,
    NoRootEnded,
    RootStartedLost,
    IndexCorrupt,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionSummary {
    pub id: ExecutionId,
    pub stream: StreamId,
    pub engine_id: EngineId,
    pub program_id: Option<ProgramId>,
    pub runtime_id: Option<BoundaryId>,
    pub started_ns: Option<u64>,
    pub started_unix_ns: Option<u64>,
    pub ended_ns: Option<u64>,
    pub status: ExecutionStatus,
    pub index_state: IndexState,
    pub health: Option<ExecutionHealthSnapshot>,
    pub data_first_seq: u64,
    pub data_last_seq: u64,
    pub data_segment_count: u64,
}

/// One stream's index plane, fully read at `open`. Listing never opens
/// `data/`.
#[derive(Debug)]
pub struct StreamReader {
    pub stream: StreamId,
    pub header: Option<StreamStarted>,
    pub engines: Vec<EngineStarted>,
    pub roots: Vec<RootIndexEntry>,
    pub high_water: StreamHighWater,
    pub alive: bool,
    /// Missing or corrupt interior meta sequences.
    pub index_gaps: Vec<u64>,
    root: PathBuf,
    lease: Arc<File>,
}

/// Enumerates stream directories (32 lowercase hex chars) under `streams/`.
pub fn list_streams(root: &Path) -> Result<Vec<StreamId>, ReadError> {
    let streams_directory = root.join("streams");
    let entries = match fs::read_dir(&streams_directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(ReadError::Io {
                path: streams_directory,
                source,
            });
        }
    };
    let mut streams = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|source| ReadError::Io {
            path: streams_directory.clone(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.len() != 32 {
            continue;
        }
        let Ok(bytes) = hex::decode(name) else {
            continue;
        };
        let Ok(euid) = <[u8; 16]>::try_from(bytes) else {
            continue;
        };
        streams.push(StreamId(ProcessEuid(euid)));
    }
    streams.sort_by_key(|stream| stream.0.0);
    Ok(streams)
}

/// Every execution across every stream of a store (meta planes only).
pub fn list_executions(root: &Path) -> Result<Vec<ExecutionSummary>, ReadError> {
    let mut executions = Vec::new();
    for stream in list_streams(root)? {
        let reader = StreamReader::open(root, stream)?;
        executions.extend(reader.executions());
    }
    Ok(executions)
}

impl StreamReader {
    pub fn open(root: &Path, stream: StreamId) -> Result<Self, ReadError> {
        let directory = stream_directory(root, stream);
        if !directory.is_dir() {
            return Err(ReadError::StreamNotFound(stream));
        }
        let lease = Arc::new(acquire_shared_lease(root)?);
        let alive = stream_alive(&directory, stream);

        let meta_directory = directory.join("meta");
        let meta_max = max_plane_sequence(&meta_directory, "bamlmeta")?;
        let data_max = max_plane_sequence(&directory.join("data"), "bamldata")?;

        let mut header = None;
        let mut engines = Vec::new();
        let mut roots: Vec<RootIndexEntry> = Vec::new();
        let mut root_lookup: HashMap<ThreadRef, usize> = HashMap::new();
        let mut index_gaps = Vec::new();
        let mut meta_high = 0u64;
        for sequence in 1..=meta_max {
            let path = segment_path(root, stream, Plane::Meta, sequence);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    index_gaps.push(sequence);
                    continue;
                }
                Err(source) => return Err(ReadError::Io { path, source }),
            };
            let decoded = match decode_meta_segment(&bytes, stream.0) {
                Ok(decoded) => decoded,
                Err(_) if sequence == meta_max && alive => {
                    // A torn tail while the stream is alive is ignored;
                    // while dead it is corruption (a gap).
                    continue;
                }
                Err(_) => {
                    index_gaps.push(sequence);
                    continue;
                }
            };
            meta_high = meta_high.max(sequence);
            for record in decoded.records {
                match record {
                    MetaRecord::StreamStarted {
                        pid,
                        zero_unix_ns,
                        baml_version,
                        os_arch,
                    } => {
                        header = Some(StreamStarted {
                            pid,
                            zero_unix_ns,
                            baml_version,
                            os_arch,
                        });
                    }
                    MetaRecord::EngineStarted {
                        engine_id,
                        program_id,
                        function_table_cid,
                        revision_label,
                        source_label,
                    } => engines.push(EngineStarted {
                        engine_id,
                        program_id,
                        function_table_cid,
                        revision_label,
                        source_label,
                    }),
                    MetaRecord::RootStarted {
                        root: thread,
                        started_ns,
                        runtime_id,
                    } => {
                        let entry = entry_for(&mut roots, &mut root_lookup, thread);
                        if entry.started.is_some() {
                            return Err(ReadError::MetaDuplicateRootStarted(thread));
                        }
                        entry.started = Some(RootStarted {
                            root: thread,
                            started_ns,
                            runtime_id,
                            meta_sequence: sequence,
                            data_high_water: decoded.data_high_water,
                        });
                    }
                    MetaRecord::RootEnded {
                        root: thread,
                        ended_ns,
                        status,
                        flags,
                        data_first_seq,
                        data_last_seq,
                        data_segment_count,
                        health,
                    } => {
                        let entry = entry_for(&mut roots, &mut root_lookup, thread);
                        if entry.ended.is_some() {
                            return Err(ReadError::MetaDuplicateRootEnded(thread));
                        }
                        entry.ended = Some(RootEnded {
                            root: thread,
                            ended_ns,
                            status,
                            flags,
                            data_first_seq,
                            data_last_seq,
                            data_segment_count,
                            health,
                            meta_sequence: sequence,
                        });
                    }
                }
            }
        }
        // Gap entries beyond the highest decodable sequence while alive are
        // really an ignored torn tail.
        if alive {
            index_gaps.retain(|gap| *gap < meta_high);
        }
        Ok(Self {
            stream,
            header,
            engines,
            roots,
            high_water: StreamHighWater {
                meta: meta_high,
                data: data_max,
            },
            alive,
            index_gaps,
            root: root.to_owned(),
            lease,
        })
    }

    /// Index-plane summaries of this stream's executions (streams spec §6.2).
    #[must_use]
    pub fn executions(&self) -> Vec<ExecutionSummary> {
        self.roots
            .iter()
            .map(|entry| self.summarize(entry))
            .collect()
    }

    fn summarize(&self, entry: &RootIndexEntry) -> ExecutionSummary {
        let status = match &entry.ended {
            Some(ended) => match ended.status {
                ExecutionEndStatus::Succeeded => ExecutionStatus::Succeeded,
                ExecutionEndStatus::Failed => ExecutionStatus::Failed,
                ExecutionEndStatus::Cancelled => ExecutionStatus::Cancelled,
                ExecutionEndStatus::Panicked => ExecutionStatus::Panicked,
                ExecutionEndStatus::Abandoned => ExecutionStatus::Abandoned,
            },
            None if self.alive => ExecutionStatus::Running,
            None => ExecutionStatus::Abandoned,
        };
        let root_started_lost = entry
            .ended
            .as_ref()
            .is_some_and(|ended| ended.flags & super::ROOT_ENDED_FLAG_ROOT_STARTED_LOST != 0);
        let first_seq = entry
            .started
            .map(|started| started.meta_sequence)
            .or(entry.ended.map(|ended| ended.meta_sequence))
            .unwrap_or(0);
        let ended_seq = entry.ended.map(|ended| ended.meta_sequence);
        let gap_shadows = self
            .index_gaps
            .iter()
            .any(|gap| *gap > first_seq && ended_seq.is_none_or(|ended_seq| *gap < ended_seq));
        let index_state = if gap_shadows
            || (entry.ended.is_some() && entry.started.is_none() && !root_started_lost)
        {
            IndexState::IndexCorrupt
        } else if entry.ended.is_some() && root_started_lost {
            IndexState::RootStartedLost
        } else if entry.ended.is_none() {
            IndexState::NoRootEnded
        } else {
            IndexState::Complete
        };
        ExecutionSummary {
            id: ExecutionId(entry.root),
            stream: self.stream,
            engine_id: entry.root.engine_id,
            program_id: self
                .engines
                .iter()
                .find(|engine| engine.engine_id == entry.root.engine_id)
                .map(|engine| engine.program_id),
            runtime_id: entry.started.map(|started| started.runtime_id),
            started_ns: entry.started.map(|started| started.started_ns),
            started_unix_ns: match (&self.header, &entry.started) {
                (Some(header), Some(started)) => {
                    Some(header.zero_unix_ns.saturating_add(started.started_ns))
                }
                _ => None,
            },
            ended_ns: entry.ended.map(|ended| ended.ended_ns),
            status,
            index_state,
            health: entry.ended.map(|ended| ended.health),
            data_first_seq: entry.ended.map_or(0, |ended| ended.data_first_seq),
            data_last_seq: entry.ended.map_or(0, |ended| ended.data_last_seq),
            data_segment_count: entry.ended.map_or(0, |ended| ended.data_segment_count),
        }
    }

    pub fn execution(&self, id: ExecutionId) -> Result<ExecutionReader, ReadError> {
        let entry = self
            .roots
            .iter()
            .find(|entry| entry.root == id.0)
            .ok_or(ReadError::ExecutionNotFound(id))?;
        Ok(ExecutionReader {
            store_root: self.root.clone(),
            stream: self.stream,
            summary: self.summarize(entry),
            entry: entry.clone(),
            engines: self.engines.clone(),
            data_high_water: self.high_water.data,
            _lease: Arc::clone(&self.lease),
        })
    }

    /// EXPENSIVE: scans every data segment's group headers for roots absent
    /// from the index (a lost meta batch before a crash).
    pub fn orphan_groups(&self) -> Result<Vec<ThreadRef>, ReadError> {
        let mut orphans = Vec::new();
        for sequence in 1..=self.high_water.data {
            let path = segment_path(&self.root, self.stream, Plane::Data, sequence);
            let bytes = match fs::read(&path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(source) => return Err(ReadError::Io { path, source }),
            };
            let Ok(decoded) = decode_data_segment(&bytes, self.stream.0) else {
                continue;
            };
            for group in decoded.groups {
                if !self.roots.iter().any(|entry| entry.root == group.root)
                    && !orphans.contains(&group.root)
                {
                    orphans.push(group.root);
                }
            }
        }
        Ok(orphans)
    }
}

fn entry_for<'a>(
    roots: &'a mut Vec<RootIndexEntry>,
    lookup: &mut HashMap<ThreadRef, usize>,
    thread: ThreadRef,
) -> &'a mut RootIndexEntry {
    let index = *lookup.entry(thread).or_insert_with(|| {
        roots.push(RootIndexEntry {
            root: thread,
            started: None,
            ended: None,
        });
        roots.len() - 1
    });
    &mut roots[index]
}

fn acquire_shared_lease(root: &Path) -> Result<File, ReadError> {
    let parent = root.parent().unwrap_or(root);
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("profiles-v1");
    let path = parent.join(format!("{name}.lock"));
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&path)
        .map_err(|source| ReadError::Io {
            path: path.clone(),
            source,
        })?;
    FileExt::lock_shared(&file).map_err(|source| ReadError::Io { path, source })?;
    Ok(file)
}

/// Liveness (streams spec §6.4): same-process short-circuit through
/// `OPEN_STREAMS`; otherwise a shared-lock probe on `stream.lock`.
fn stream_alive(directory: &Path, stream: StreamId) -> bool {
    if stream_open_in_process(stream) {
        return true;
    }
    let Ok(file) = File::open(directory.join("stream.lock")) else {
        return false;
    };
    match FileExt::try_lock_shared(&file) {
        Ok(()) => {
            let _ = FileExt::unlock(&file);
            false
        }
        Err(error) => error.kind() == io::ErrorKind::WouldBlock,
    }
}

fn max_plane_sequence(directory: &Path, extension: &str) -> Result<u64, ReadError> {
    let suffix = format!(".{extension}");
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(0),
        Err(source) => {
            return Err(ReadError::Io {
                path: directory.to_owned(),
                source,
            });
        }
    };
    let mut max = 0u64;
    for entry in entries {
        let entry = entry.map_err(|source| ReadError::Io {
            path: directory.to_owned(),
            source,
        })?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = name.strip_suffix(&suffix) else {
            continue;
        };
        if stem.len() != 20 || !stem.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        if let Ok(sequence) = stem.parse::<u64>() {
            max = max.max(sequence);
        }
    }
    Ok(max)
}

/// Data-plane completeness (streams spec §6.2/§6.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataState {
    Complete,
    Incomplete(Vec<DataIssue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DataIssue {
    MissingDataSegment(u64),
    CorruptDataSegment(u64),
    GroupCountMismatch { expected: u64, found: u64 },
    NoRootEnded,
    UnresolvedDependency(UnresolvedDependency),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UnresolvedDependency {
    SpanStart(CallRef),
    ContextDefinition(ContextKey),
    ErrorCapture(ErrorCaptureId),
    OverflowBucket {
        reason: OverflowReason,
        edge_kind: EdgeKind,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergedContext {
    pub tuple: Option<ContextTuple>,
    pub counters: CctCounters,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SpanEvidence {
    pub start: Option<SpanStart>,
    pub end: Option<SpanEnd>,
    pub runtime_ids: Vec<SpanRuntimeId>,
    pub input: Option<ValueOccurrence>,
    pub output: Option<ValueOccurrence>,
    pub terminal_error: Option<TerminalErrorRef>,
}

/// Tolerant thread lifecycle evidence (streams spec §4.5): a missing start
/// or parent is counted population loss, not an error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThreadEvidence {
    pub start: Option<ThreadStart>,
    pub end: Option<ThreadEnd>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThreadIssueKind {
    MissingStart,
    MissingParent,
    RootMismatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ThreadIssue {
    pub thread: ThreadRef,
    pub kind: ThreadIssueKind,
}

#[derive(Clone, Debug)]
pub struct ExecutionProfile {
    pub summary: ExecutionSummary,
    pub data_state: DataState,
    pub contexts: HashMap<ContextKey, MergedContext>,
    pub overflow: HashMap<(OverflowReason, EdgeKind), CctCounters>,
    pub cct_health: CounterHealth,
    pub threads: HashMap<ThreadRef, ThreadEvidence>,
    pub thread_issues: Vec<ThreadIssue>,
    pub spans: HashMap<CallRef, SpanEvidence>,
    pub errors: HashMap<ErrorCaptureId, ErrorCapture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ErrorStack {
    Complete(Vec<ContextTuple>),
    StackIncomplete {
        throw_function_id: crate::ids::FunctionId,
        throw_site: Option<ThrowSite>,
    },
}

#[derive(Debug)]
pub struct ExecutionReader {
    store_root: PathBuf,
    stream: StreamId,
    summary: ExecutionSummary,
    entry: RootIndexEntry,
    engines: Vec<EngineStarted>,
    data_high_water: u64,
    /// Keeps the store lease shared while this reader is alive.
    _lease: Arc<File>,
}

impl ExecutionReader {
    #[must_use]
    pub fn summary(&self) -> &ExecutionSummary {
        &self.summary
    }

    /// Folds the execution's data segments (streams spec §6.3). Damage is
    /// tolerated: a missing or corrupt segment strands later deltas, so
    /// dangling references become `DataIssue`s rather than hard errors
    /// whenever the fold is already known to be incomplete.
    pub fn load(&self) -> Result<ExecutionProfile, ReadError> {
        let mut issues: Vec<DataIssue> = Vec::new();
        let (range, expected_count) = match &self.entry.ended {
            Some(ended) => {
                if ended.data_segment_count == 0 || ended.data_first_seq == 0 {
                    (None, Some(0))
                } else {
                    (
                        Some(ended.data_first_seq..=ended.data_last_seq),
                        Some(ended.data_segment_count),
                    )
                }
            }
            None => {
                issues.push(DataIssue::NoRootEnded);
                let lower = self
                    .entry
                    .started
                    .map_or(1, |started| started.data_high_water.saturating_add(1));
                if self.data_high_water >= lower {
                    (Some(lower..=self.data_high_water), None)
                } else {
                    (None, None)
                }
            }
        };

        let mut profile = ExecutionProfile {
            summary: self.summary.clone(),
            data_state: DataState::Complete,
            contexts: HashMap::new(),
            overflow: HashMap::new(),
            cct_health: CounterHealth::default(),
            threads: HashMap::new(),
            thread_issues: Vec::new(),
            spans: HashMap::new(),
            errors: HashMap::new(),
        };
        let mut segments_with_group = 0u64;
        if let Some(range) = range {
            for sequence in range {
                let path = segment_path(&self.store_root, self.stream, Plane::Data, sequence);
                let bytes = match fs::read(&path) {
                    Ok(bytes) => bytes,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {
                        issues.push(DataIssue::MissingDataSegment(sequence));
                        continue;
                    }
                    Err(source) => return Err(ReadError::Io { path, source }),
                };
                let Ok(decoded) = decode_data_segment(&bytes, self.stream.0) else {
                    issues.push(DataIssue::CorruptDataSegment(sequence));
                    continue;
                };
                for group in decoded.groups {
                    if group.root != self.entry.root {
                        continue;
                    }
                    segments_with_group += 1;
                    let cct = group.decode_cct().map_err(|source| ReadError::Decode {
                        path: path.clone(),
                        source,
                    })?;
                    if let Some(cct) = cct {
                        merge_cct(&mut profile, cct)?;
                    }
                    let facts = group
                        .decode_evidence()
                        .map_err(|source| ReadError::Decode {
                            path: path.clone(),
                            source,
                        })?;
                    merge_evidence(&mut profile, facts)?;
                }
            }
        }
        if let Some(expected) = expected_count
            && expected != segments_with_group
        {
            issues.push(DataIssue::GroupCountMismatch {
                expected,
                found: segments_with_group,
            });
        }

        collect_thread_issues(&mut profile, self.entry.root);

        // Dependency validation severity (streams spec §6.3): hard errors
        // only when the fold would otherwise be complete and the writer
        // recorded no publication losses.
        let publication_loss = self.summary.health.is_some_and(|health| {
            health.cct_segment_publish_failed > 0 || health.evidence_segment_publish_failed > 0
        });
        let strict = issues.is_empty() && !publication_loss;
        let dangling = validate_dependencies(&profile);
        if strict {
            if let Some(dependency) = dangling.into_iter().next() {
                return Err(match dependency {
                    UnresolvedDependency::SpanStart(call_ref) => {
                        ReadError::MissingSpanStart(call_ref)
                    }
                    UnresolvedDependency::ContextDefinition(key) => {
                        ReadError::MissingContextDefinition(key)
                    }
                    UnresolvedDependency::ErrorCapture(id) => ReadError::MissingErrorCapture(id),
                    UnresolvedDependency::OverflowBucket { reason, edge_kind } => {
                        ReadError::MissingOverflowBucket { reason, edge_kind }
                    }
                });
            }
        } else {
            issues.extend(dangling.into_iter().map(DataIssue::UnresolvedDependency));
        }

        profile.data_state = if issues.is_empty() && self.entry.ended.is_some() {
            DataState::Complete
        } else {
            DataState::Incomplete(issues)
        };
        Ok(profile)
    }

    pub fn read_value(&self, cid: ValueCid) -> Result<DecodedCasObject, ReadError> {
        read_cas_object(&self.store_root, cid)
    }

    /// The engine's durable function/file tables, via
    /// `EngineStarted.function_table_cid`. `None` when the engine record or
    /// its CAS publication is missing (readers show function labels NULL).
    pub fn function_table(&self) -> Result<Option<FunctionTable>, ReadError> {
        let Some(cid) = self
            .engines
            .iter()
            .find(|engine| engine.engine_id == self.entry.root.engine_id)
            .and_then(|engine| engine.function_table_cid)
        else {
            return Ok(None);
        };
        let object = read_cas_object(&self.store_root, cid)?;
        if object.codec != super::CodecVersion(2) {
            return Err(ReadError::FunctionTableCodecMismatch(cid));
        }
        decode_function_table(&object.body)
            .map(Some)
            .map_err(ReadError::FunctionTableInvalid)
    }
}

fn read_cas_object(store_root: &Path, cid: ValueCid) -> Result<DecodedCasObject, ReadError> {
    let digest = hex::encode(cid.0);
    let path = store_root
        .join("cas/sha256")
        .join(&digest[..2])
        .join(format!("{digest}.bamlvalue"));
    let bytes = fs::read(&path).map_err(|source| ReadError::Io {
        path: path.clone(),
        source,
    })?;
    let object = decode_cas_object(&bytes).map_err(|source| ReadError::Decode { path, source })?;
    if object.cid != cid {
        return Err(ReadError::CasIdentityMismatch(cid));
    }
    Ok(object)
}

fn merge_cct(
    profile: &mut ExecutionProfile,
    segment: super::CctSegmentData,
) -> Result<(), ReadError> {
    profile.cct_health.counter_saturated |= segment.health.counter_saturated;
    profile.cct_health.await_counter_saturated |= segment.health.await_counter_saturated;
    profile.cct_health.self_time_underflow |= segment.health.self_time_underflow;
    for delta in segment.contexts {
        let entry = profile.contexts.entry(delta.key).or_insert(MergedContext {
            tuple: None,
            counters: CctCounters::default(),
        });
        if let Some(tuple) = delta.tuple {
            if entry.tuple.is_some_and(|existing| existing != tuple) {
                return Err(ReadError::ConflictingContextDefinition(delta.key));
            }
            entry.tuple = Some(tuple);
        }
        add_counters(&mut entry.counters, delta.counters, &mut profile.cct_health);
    }
    for delta in segment.overflow {
        let entry = profile
            .overflow
            .entry((delta.reason, delta.edge_kind))
            .or_default();
        add_counters(entry, delta.counters, &mut profile.cct_health);
    }
    Ok(())
}

fn merge_evidence(
    profile: &mut ExecutionProfile,
    facts: Vec<EvidenceFact>,
) -> Result<(), ReadError> {
    for fact in facts {
        match fact {
            EvidenceFact::SpanStart(start) => {
                let call_ref = start.call_ref;
                let span = profile.spans.entry(call_ref).or_default();
                if span.start.replace(start).is_some() {
                    return Err(ReadError::DuplicateSpanStart(call_ref));
                }
            }
            EvidenceFact::SpanEnd(end) => {
                let span = profile.spans.entry(end.call_ref).or_default();
                if span.end.replace(end).is_some() {
                    return Err(ReadError::DuplicateSpanEnd(end.call_ref));
                }
            }
            EvidenceFact::SpanRuntimeId(annotation) => profile
                .spans
                .entry(annotation.call_ref)
                .or_default()
                .runtime_ids
                .push(annotation),
            EvidenceFact::ValueOccurrence(occurrence) => {
                let span = profile.spans.entry(occurrence.call_ref).or_default();
                let target = match occurrence.role {
                    ValueRole::Input => &mut span.input,
                    ValueRole::Output => &mut span.output,
                };
                if target.replace(occurrence).is_some() {
                    return Err(ReadError::DuplicateValueOccurrence {
                        call_ref: occurrence.call_ref,
                        role: occurrence.role,
                    });
                }
            }
            EvidenceFact::ErrorCapture(capture) => {
                if profile.errors.insert(capture.id, capture).is_some() {
                    return Err(ReadError::DuplicateErrorCapture(capture.id));
                }
            }
            EvidenceFact::TerminalErrorRef(terminal) => {
                // One terminal error per call; a second one is corruption,
                // rejected like every other repeated fact.
                if profile
                    .spans
                    .entry(terminal.call_ref)
                    .or_default()
                    .terminal_error
                    .replace(terminal)
                    .is_some()
                {
                    return Err(ReadError::DuplicateTerminalError(terminal.call_ref));
                }
            }
            EvidenceFact::ThreadStart(start) => {
                let thread_ref = start.thread_ref;
                let thread = profile.threads.entry(thread_ref).or_default();
                if thread.start.replace(start).is_some() {
                    return Err(ReadError::DuplicateThreadStart(thread_ref));
                }
            }
            EvidenceFact::ThreadEnd(end) => {
                let thread = profile.threads.entry(end.thread_ref).or_default();
                if thread.end.replace(end).is_some() {
                    return Err(ReadError::DuplicateThreadEnd(end.thread_ref));
                }
            }
        }
    }
    Ok(())
}

/// Tolerant thread-lifecycle validation (streams spec §4.5): population loss
/// already counted by the writer, surfaced as issues, never errors.
fn collect_thread_issues(profile: &mut ExecutionProfile, root: ThreadRef) {
    let mut issues = Vec::new();
    for (thread_ref, evidence) in &profile.threads {
        let Some(start) = &evidence.start else {
            issues.push(ThreadIssue {
                thread: *thread_ref,
                kind: ThreadIssueKind::MissingStart,
            });
            continue;
        };
        if let Some(parent) = start.parent
            && profile
                .threads
                .get(&parent)
                .is_none_or(|parent| parent.start.is_none())
        {
            issues.push(ThreadIssue {
                thread: *thread_ref,
                kind: ThreadIssueKind::MissingParent,
            });
        }
        if matches!(start.kind, super::ThreadStartKind::Root) && start.thread_ref != root {
            issues.push(ThreadIssue {
                thread: *thread_ref,
                kind: ThreadIssueKind::RootMismatch,
            });
        }
    }
    profile.thread_issues = issues;
}

fn validate_dependencies(profile: &ExecutionProfile) -> Vec<UnresolvedDependency> {
    let mut dangling = Vec::new();
    let context_defined =
        |context_ref: &ContextRef, dangling: &mut Vec<UnresolvedDependency>| match context_ref {
            ContextRef::Normal(key) => {
                if profile
                    .contexts
                    .get(key)
                    .is_none_or(|context| context.tuple.is_none())
                {
                    dangling.push(UnresolvedDependency::ContextDefinition(*key));
                }
            }
            ContextRef::Overflow { reason, edge_kind } => {
                if !profile.overflow.contains_key(&(*reason, *edge_kind)) {
                    dangling.push(UnresolvedDependency::OverflowBucket {
                        reason: *reason,
                        edge_kind: *edge_kind,
                    });
                }
            }
        };
    for (call_ref, span) in &profile.spans {
        let Some(start) = &span.start else {
            dangling.push(UnresolvedDependency::SpanStart(*call_ref));
            continue;
        };
        context_defined(&start.context_ref, &mut dangling);
        if let Some(TerminalErrorRef {
            target: TerminalErrorTarget::Capture(id),
            ..
        }) = span.terminal_error
            && !profile.errors.contains_key(&id)
        {
            dangling.push(UnresolvedDependency::ErrorCapture(id));
        }
    }
    for capture in profile.errors.values() {
        context_defined(&capture.throw_context_ref, &mut dangling);
    }
    dangling
}

impl ExecutionProfile {
    pub fn error_stack(&self, id: ErrorCaptureId) -> Result<ErrorStack, ReadError> {
        let capture = self
            .errors
            .get(&id)
            .ok_or(ReadError::MissingErrorCapture(id))?;
        let ContextRef::Normal(key) = capture.throw_context_ref else {
            return Ok(ErrorStack::StackIncomplete {
                throw_function_id: capture.throw_function_id,
                throw_site: capture.throw_site,
            });
        };
        Ok(ErrorStack::Complete(context_chain(&self.contexts, key)?))
    }
}

/// Root-first parent chain of `key`. A well-formed chain visits each context
/// at most once, so walking more than `contexts.len()` steps proves a cycle
/// without a visited set on the read path.
fn context_chain(
    contexts: &HashMap<ContextKey, MergedContext>,
    mut key: ContextKey,
) -> Result<Vec<ContextTuple>, ReadError> {
    let mut stack = Vec::new();
    let max_depth = contexts.len();
    loop {
        let context = contexts
            .get(&key)
            .ok_or(ReadError::MissingContextDefinition(key))?;
        let tuple = context
            .tuple
            .ok_or(ReadError::MissingContextDefinition(key))?;
        stack.push(tuple);
        let Some(parent) = tuple.parent_context_key else {
            break;
        };
        if stack.len() >= max_depth {
            return Err(ReadError::CyclicContextChain(key));
        }
        key = parent;
    }
    stack.reverse();
    Ok(stack)
}

fn add_counters(target: &mut CctCounters, delta: CctCounters, health: &mut CounterHealth) {
    macro_rules! add {
        ($field:ident) => {
            match target.$field.checked_add(delta.$field) {
                Some(value) => target.$field = value,
                None => {
                    target.$field = target.$field.saturating_add(delta.$field);
                    health.counter_saturated = true;
                }
            }
        };
    }
    add!(invocations_started);
    add!(spans_selected);
    add!(completed_ok);
    add!(completed_error);
    add!(completed_cancelled);
    add!(completed_exit);
    add!(inclusive_ns);
    add!(direct_call_child_inclusive_ns);
    match target.await_ns.checked_add(delta.await_ns) {
        Some(value) => target.await_ns = value,
        None => {
            target.await_ns = u128::MAX;
            health.await_counter_saturated = true;
        }
    }
    match target.await_count.checked_add(delta.await_count) {
        Some(value) => target.await_count = value,
        None => {
            target.await_count = u64::MAX;
            health.await_counter_saturated = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::{ContextKey, ContextTuple, MergedContext, ReadError, context_chain};
    use crate::{
        ids::{FunctionId, ProgramId},
        prof::backend::{CctCounters, EdgeKind},
    };

    fn context(parent: Option<ContextKey>, edge_kind: EdgeKind) -> MergedContext {
        MergedContext {
            tuple: Some(ContextTuple {
                program_id: ProgramId([0; 16]),
                parent_context_key: parent,
                function_id: FunctionId(1),
                call_site: None,
                edge_kind,
            }),
            counters: CctCounters::default(),
        }
    }

    #[test]
    fn context_chain_walks_root_first() {
        let root = ContextKey([1; 32]);
        let child = ContextKey([2; 32]);
        let contexts = HashMap::from([
            (root, context(None, EdgeKind::Root)),
            (child, context(Some(root), EdgeKind::Call)),
        ]);
        let chain = context_chain(&contexts, child).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].edge_kind, EdgeKind::Root);
        assert_eq!(chain[1].parent_context_key, Some(root));
    }

    #[test]
    fn context_chain_rejects_cycles_instead_of_hanging() {
        let a = ContextKey([1; 32]);
        let b = ContextKey([2; 32]);
        let contexts = HashMap::from([
            (a, context(Some(b), EdgeKind::Call)),
            (b, context(Some(a), EdgeKind::Call)),
        ]);
        assert!(matches!(
            context_chain(&contexts, a),
            Err(ReadError::CyclicContextChain(_))
        ));
        let contexts = HashMap::from([(a, context(Some(a), EdgeKind::Call))]);
        assert!(matches!(
            context_chain(&contexts, a),
            Err(ReadError::CyclicContextChain(_))
        ));
    }
}
