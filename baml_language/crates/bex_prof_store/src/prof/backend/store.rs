//! Project-local profiler store: one stream of meta/data segments per
//! process, plus the shared CAS, behind one atomic publication seam.
//!
//! Layout (streams spec §3):
//!
//! ```text
//! .baml/profiles-v1/
//!   publish.lock                          project-wide publication/accounting lock
//!   usage.state                           BAMLUSE1 byte-accounting ledger
//!   tmp/
//!   streams/<process_euid hex32>/
//!     stream.lock                         exclusive while the owning store is open
//!     meta/<seq:020>.bamlmeta             index plane (StreamStarted/EngineStarted/Root*)
//!     data/<seq:020>.bamldata             CCT + evidence groups keyed by root ThreadRef
//!   cas/sha256/<2 hex>/<64 hex>.bamlvalue codec 1 = value body, codec 2 = function table
//!   runs/                                 legacy v1 layout: never read, removed by `baml clean`
//! ```

use std::{
    collections::HashSet,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU8, Ordering},
    },
};

use fs2::FileExt;
use sha2::{Digest, Sha256};

use super::{
    CctCodecError, CctSegmentData, CodecVersion, CounterHealth, DiskBudget, EvidenceCodecError,
    EvidenceFact, ExecutionEndStatus, ExecutionHealthSnapshot, ValueCid, cct_codec,
    decode_cct_payload, decode_evidence_payload,
};
use crate::ids::{BexThreadId, BoundaryId, EngineId, ProcessEuid, ProgramId, ThreadRef};

const USAGE_MAGIC: &[u8; 8] = b"BAMLUSE1";
const META_MAGIC: &[u8; 8] = b"BAMLMET1";
const DATA_MAGIC: &[u8; 8] = b"BAMLDAT1";
const VALUE_MAGIC: &[u8; 8] = b"BAMLVAL1";
/// Meta/data segment format version.
pub const SCHEMA_VERSION: u16 = 2;
/// CAS object framing version — split from `SCHEMA_VERSION` so stored value
/// bytes are unchanged by the segment-format bump.
pub const CAS_FORMAT_VERSION: u16 = 1;
const USAGE_STATE_BYTES: u64 = 8 + 8 + 32;
const GATE_OPEN: u8 = 0;
const GATE_DISK: u8 = 1;
const GATE_UNAVAILABLE: u8 = 2;
const MAX_ENCODED_STRING: usize = 64 * 1024;

/// Stream identity: the writing process's effectively-unique id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StreamId(pub ProcessEuid);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    Meta,
    Data,
}

/// Last COMMITTED sequence per plane; 0 = none. An indeterminate candidate
/// is not reflected until its resolution commits.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamHighWater {
    pub meta: u64,
    pub data: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreFileKind {
    MetaSegment,
    DataSegment,
    CasObject,
}

/// The filesystem-dependent operations at the store's fault-injection seam.
pub trait StorePlatform: Send + Sync + 'static {
    fn available_space(&self, path: &Path) -> io::Result<u64>;
    fn sync_dir(&self, path: &Path) -> io::Result<()>;

    /// ALL file fsyncs (segment tmp file AND `usage.state` tmp file) route
    /// here so tests can count and fault-inject them.
    fn sync_file(&self, file: &File) -> io::Result<()> {
        file.sync_all()
    }

    fn before_rename(&self, _kind: StoreFileKind, _temporary: &Path) -> io::Result<()> {
        Ok(())
    }
}

#[derive(Debug, Default)]
pub(crate) struct NativeStorePlatform;

impl StorePlatform for NativeStorePlatform {
    fn available_space(&self, path: &Path) -> io::Result<u64> {
        fs2::available_space(path)
    }

    fn sync_dir(&self, path: &Path) -> io::Result<()> {
        sync_directory(path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreFailureReason {
    DiskGuardExceeded,
    PermissionDenied,
    StoreUnavailable,
    PathConflict,
    SequenceExhausted,
    /// Another open store already owns this stream (`stream.lock`).
    StreamInUse,
}

#[derive(Debug)]
pub struct StoreOpenError {
    pub reason: StoreFailureReason,
    pub source: Option<io::Error>,
}

impl std::fmt::Display for StoreOpenError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "profiling store open failed: {:?}", self.reason)
    }
}

impl std::error::Error for StoreOpenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|source| source as &(dyn std::error::Error + 'static))
    }
}

#[derive(Debug)]
pub enum CleanProfilesError {
    InUse,
    InvalidRoot,
    Io(io::Error),
}

impl std::fmt::Display for CleanProfilesError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "profiling cleanup failed: {self:?}")
    }
}

impl std::error::Error for CleanProfilesError {}

/// Removes only one configured segmented profiler root while holding the
/// stable sibling lease. Non-profile history is outside this root and is
/// never inspected or removed. The legacy `runs/` layout inside the root is
/// removed with everything else.
pub fn clean_profiles_v1(root: &Path) -> Result<bool, CleanProfilesError> {
    let parent = root.parent().ok_or(CleanProfilesError::InvalidRoot)?;
    let root_name = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or(CleanProfilesError::InvalidRoot)?;
    if root_name != "profiles-v1" || parent.file_name().is_none_or(|name| name != ".baml") {
        return Err(CleanProfilesError::InvalidRoot);
    }
    fs::create_dir_all(parent).map_err(CleanProfilesError::Io)?;
    let parent_metadata = fs::symlink_metadata(parent).map_err(CleanProfilesError::Io)?;
    if !parent_metadata.file_type().is_dir() || parent_metadata.file_type().is_symlink() {
        return Err(CleanProfilesError::InvalidRoot);
    }
    let store_lock = read_write_file(&parent.join(format!("{root_name}.lock")))
        .map_err(CleanProfilesError::Io)?;
    FileExt::try_lock_exclusive(&store_lock).map_err(|error| {
        // Contention is `WouldBlock` on Unix but `ERROR_LOCK_VIOLATION` on
        // Windows; `fs2` names the platform's contended error.
        let contended = fs2::lock_contended_error();
        if error.kind() == io::ErrorKind::WouldBlock
            || (error.raw_os_error().is_some() && error.raw_os_error() == contended.raw_os_error())
        {
            CleanProfilesError::InUse
        } else {
            CleanProfilesError::Io(error)
        }
    })?;
    if !root.exists() {
        return Ok(false);
    }
    let metadata = fs::symlink_metadata(root).map_err(CleanProfilesError::Io)?;
    if !metadata.file_type().is_dir() || metadata.file_type().is_symlink() {
        return Err(CleanProfilesError::InvalidRoot);
    }
    let publish_path = root.join("publish.lock");
    if publish_path.exists() {
        let publish_lock = read_write_file(&publish_path).map_err(CleanProfilesError::Io)?;
        FileExt::lock_exclusive(&publish_lock).map_err(CleanProfilesError::Io)?;
        FileExt::unlock(&publish_lock).map_err(CleanProfilesError::Io)?;
    }
    fs::remove_dir_all(root).map_err(CleanProfilesError::Io)?;
    Ok(true)
}

/// One index-plane record (streams spec §4.3).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MetaRecord {
    /// Record 1 of meta segment 1, nowhere else.
    StreamStarted {
        pid: u32,
        zero_unix_ns: u64,
        baml_version: String,
        os_arch: String,
    },
    /// Once per engine, before any `RootStarted` of that engine.
    EngineStarted {
        engine_id: EngineId,
        program_id: ProgramId,
        function_table_cid: Option<ValueCid>,
        revision_label: Option<String>,
        source_label: Option<String>,
    },
    RootStarted {
        root: ThreadRef,
        started_ns: u64,
        /// Host runtime token (`baml_id_1_…`), opaque to the profiler.
        runtime_id: BoundaryId,
    },
    RootEnded {
        root: ThreadRef,
        ended_ns: u64,
        status: ExecutionEndStatus,
        /// Bit 0 = `root_started_lost`; bits 1–7 reserved zero.
        flags: u8,
        data_first_seq: u64,
        data_last_seq: u64,
        data_segment_count: u64,
        health: ExecutionHealthSnapshot,
    },
}

/// Flag bit 0 of `MetaRecord::RootEnded`: the meta batch carrying this
/// execution's `RootStarted` was lost.
pub const ROOT_ENDED_FLAG_ROOT_STARTED_LOST: u8 = 1;

/// One execution's encoded contribution to a data segment (streams spec §4.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataGroup {
    pub root: ThreadRef,
    pub cct_health: CounterHealth,
    pub cct_record_count: u64,
    /// Encoded CCT payload (`cct_codec`), empty when no CCT delta.
    pub cct: Vec<u8>,
    pub evidence_record_count: u64,
    /// Encoded evidence payload (`evidence_codec`), empty when no facts.
    pub evidence: Vec<u8>,
}

impl DataGroup {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cct.is_empty() && self.evidence.is_empty()
    }

    fn root_bytes(&self) -> [u8; 32] {
        thread_ref_bytes(self.root)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedMetaSegment {
    pub sequence: u64,
    pub data_high_water: u64,
    pub records: Vec<MetaRecord>,
}

/// A structurally validated data-segment group whose payloads are decoded on
/// demand — readers skip foreign groups by slice arithmetic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawDataGroup<'a> {
    pub root: ThreadRef,
    pub cct_health: CounterHealth,
    pub cct_record_count: u64,
    pub cct: &'a [u8],
    pub evidence_record_count: u64,
    pub evidence: &'a [u8],
}

impl RawDataGroup<'_> {
    pub fn decode_cct(&self) -> Result<Option<CctSegmentData>, SegmentReadError> {
        if self.cct.is_empty() && self.cct_record_count == 0 {
            return Ok(None);
        }
        decode_cct_payload(
            self.cct,
            self.cct_record_count,
            &[cct_codec::encode_health(self.cct_health)],
        )
        .map(Some)
        .map_err(SegmentReadError::InvalidCct)
    }

    pub fn decode_evidence(&self) -> Result<Vec<EvidenceFact>, SegmentReadError> {
        if self.evidence.is_empty() && self.evidence_record_count == 0 {
            return Ok(Vec::new());
        }
        decode_evidence_payload(self.evidence, self.evidence_record_count)
            .map_err(SegmentReadError::InvalidEvidence)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedDataSegment<'a> {
    pub sequence: u64,
    pub groups: Vec<RawDataGroup<'a>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedCasObject {
    pub cid: ValueCid,
    pub codec: CodecVersion,
    pub body: Vec<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentReadError {
    Truncated,
    InvalidChecksum,
    InvalidMagic,
    UnsupportedVersion(u16),
    InvalidKind,
    InvalidUtf8,
    InvalidEvidence(EvidenceCodecError),
    InvalidCct(CctCodecError),
    TrailingBytes,
    /// The segment's embedded `process_euid` differs from its stream
    /// directory.
    EuidMismatch,
    MetaUnknownTag(u8),
    MetaInvalidStatus,
    MetaInvalidHealth,
    /// `StreamStarted` anywhere except record 1 of meta segment 1.
    MetaStreamStartedMisplaced,
    DataDuplicateGroup,
    DataEmptyGroup,
    DataGroupOrder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IndeterminateToken([u8; 16]);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishBatchResult {
    Committed {
        sequence: u64,
    },
    Lost(StoreFailureReason),
    /// The store was already indeterminate (another publication's post-rename
    /// state): this batch was NOT written. Keep it pending and retry after
    /// `resolve_indeterminate` succeeds.
    Blocked(IndeterminateToken),
    /// This batch IS the post-rename candidate at `sequence`; when the token
    /// resolves `Committed` it counts as `Committed { sequence }`.
    Indeterminate {
        token: IndeterminateToken,
        sequence: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishCasResult {
    Published,
    Reused,
    Lost(StoreFailureReason),
    Conflict,
    Indeterminate(IndeterminateToken),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResolveIndeterminateResult {
    Committed,
    StillIndeterminate,
    TokenMismatch,
}

#[derive(Debug)]
struct IndeterminateState {
    token: IndeterminateToken,
    final_directory: PathBuf,
    /// Set for segment publications: the plane/sequence whose high-water
    /// advances when this token resolves `Committed`.
    plane: Option<(Plane, u64)>,
}

pub struct ProfilerStore {
    root: PathBuf,
    disk: DiskBudget,
    platform: Arc<dyn StorePlatform>,
    stream: StreamId,
    _store_lock: File,
    _stream_lock: File,
    publish_lock: File,
    process_publish: Mutex<()>,
    indeterminate: Mutex<Option<IndeterminateState>>,
    indeterminate_flag: AtomicBool,
    admission_gate: AtomicU8,
    high_water: Mutex<StreamHighWater>,
}

impl std::fmt::Debug for ProfilerStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfilerStore")
            .field("root", &self.root)
            .field("stream", &self.stream)
            .field("disk", &self.disk)
            .field(
                "admission_gate",
                &self.admission_gate.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

fn open_streams() -> &'static Mutex<HashSet<[u8; 16]>> {
    static OPEN_STREAMS: OnceLock<Mutex<HashSet<[u8; 16]>>> = OnceLock::new();
    OPEN_STREAMS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Same-process liveness short-circuit for readers: whether a
/// `ProfilerStore` for this stream is open in this process. Lock probing
/// would misreport the process's own stream on NFS `flock` emulation.
#[must_use]
pub fn stream_open_in_process(stream: StreamId) -> bool {
    open_streams()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .contains(&stream.0.0)
}

impl Drop for ProfilerStore {
    fn drop(&mut self) {
        open_streams()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.stream.0.0);
    }
}

enum GuardedPublish {
    Committed,
    Lost(StoreFailureReason),
    /// The store was already indeterminate; nothing was written.
    AlreadyIndeterminate(IndeterminateToken),
    /// This publication renamed its file and then went indeterminate.
    WentIndeterminate(IndeterminateToken),
}

impl ProfilerStore {
    #[must_use]
    pub fn cas_publication_allocation_bound(&self, encoded_body_bytes: u64) -> u64 {
        let root_bytes =
            u64::try_from(self.root.as_os_str().as_encoded_bytes().len()).unwrap_or(u64::MAX);
        encoded_body_bytes
            .saturating_add(84)
            .saturating_add(1024)
            .saturating_add(root_bytes.saturating_mul(3))
    }

    pub fn open_native(
        root: PathBuf,
        disk: DiskBudget,
        stream: StreamId,
    ) -> Result<Arc<Self>, StoreOpenError> {
        Self::open(root, disk, Arc::new(NativeStorePlatform), stream)
    }

    pub fn open(
        root: PathBuf,
        disk: DiskBudget,
        platform: Arc<dyn StorePlatform>,
        stream: StreamId,
    ) -> Result<Arc<Self>, StoreOpenError> {
        let parent = root.parent().ok_or(StoreOpenError {
            reason: StoreFailureReason::StoreUnavailable,
            source: None,
        })?;
        fs::create_dir_all(parent).map_err(open_error)?;
        // Best effort: a project that cannot take the ignore marker still
        // profiles. Failure here is never a reason to disable the store.
        let _ = ensure_baml_dir_ignored(&root);
        let root_name = root
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or(StoreOpenError {
                reason: StoreFailureReason::StoreUnavailable,
                source: None,
            })?;
        let store_lock_path = parent.join(format!("{root_name}.lock"));
        let store_lock = read_write_file(&store_lock_path).map_err(open_error)?;
        FileExt::lock_shared(&store_lock).map_err(open_error)?;

        let stream_directory = stream_directory(&root, stream);
        let meta_directory = stream_directory.join("meta");
        let data_directory = stream_directory.join("data");
        fs::create_dir_all(&meta_directory).map_err(open_error)?;
        fs::create_dir_all(&data_directory).map_err(open_error)?;
        fs::create_dir_all(root.join("cas/sha256")).map_err(open_error)?;
        fs::create_dir_all(root.join("tmp")).map_err(open_error)?;

        // Exclusive stream ownership. The process-global set is the primary
        // same-process check (lock re-acquisition semantics differ per
        // platform); the flock is the cross-process check.
        {
            let mut streams = open_streams()
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if !streams.insert(stream.0.0) {
                return Err(StoreOpenError {
                    reason: StoreFailureReason::StreamInUse,
                    source: None,
                });
            }
        }
        let release_on_error = ReleaseStreamOnError(stream);
        let stream_lock =
            read_write_file(&stream_directory.join("stream.lock")).map_err(open_error)?;
        if let Err(error) = FileExt::try_lock_exclusive(&stream_lock) {
            return Err(if error.kind() == io::ErrorKind::WouldBlock {
                StoreOpenError {
                    reason: StoreFailureReason::StreamInUse,
                    source: Some(error),
                }
            } else {
                open_error(error)
            });
        }

        let publish_lock = read_write_file(&root.join("publish.lock")).map_err(open_error)?;
        FileExt::lock_exclusive(&publish_lock).map_err(open_error)?;
        let physical_usage = scan_physical_usage(&root).map_err(open_error)?;
        write_usage_state(&root, physical_usage, platform.as_ref()).map_err(open_error)?;
        FileExt::unlock(&publish_lock).map_err(open_error)?;

        // Open-scan: highest well-formed sequence per plane; a corrupt final
        // path fails closed (MVP §9.1). The directory fsync resolves a
        // publisher that crashed after rename but before its dir sync.
        let meta_high = scan_plane_high_water(&meta_directory, "bamlmeta").map_err(open_error)?;
        let data_high = scan_plane_high_water(&data_directory, "bamldata").map_err(open_error)?;
        platform.sync_dir(&meta_directory).map_err(open_error)?;
        platform.sync_dir(&data_directory).map_err(open_error)?;

        let gate = if physical_usage > disk.max_project_bytes
            || platform.available_space(&root).map_err(open_error)? < disk.minimum_free_bytes
        {
            GATE_DISK
        } else {
            GATE_OPEN
        };

        std::mem::forget(release_on_error);
        Ok(Arc::new(Self {
            root,
            disk,
            platform,
            stream,
            _store_lock: store_lock,
            _stream_lock: stream_lock,
            publish_lock,
            process_publish: Mutex::new(()),
            indeterminate: Mutex::new(None),
            indeterminate_flag: AtomicBool::new(false),
            admission_gate: AtomicU8::new(gate),
            high_water: Mutex::new(StreamHighWater {
                meta: meta_high,
                data: data_high,
            }),
        }))
    }

    #[must_use]
    pub fn stream(&self) -> StreamId {
        self.stream
    }

    #[must_use]
    pub fn high_water(&self) -> StreamHighWater {
        *self
            .high_water
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[must_use]
    pub fn is_normal_admission_open(&self) -> bool {
        self.admission_gate.load(Ordering::Acquire) == GATE_OPEN
    }

    /// Whether a post-rename publication ambiguity is pending. Root admission
    /// consults this (with the gate) so `pending_meta_*` stays bounded while
    /// the store cannot publish.
    #[must_use]
    pub fn is_indeterminate(&self) -> bool {
        self.indeterminate_flag.load(Ordering::Acquire)
    }

    /// The parked ambiguity token, if any — lets the stream writer pick up
    /// and resolve an indeterminate CAS publication it did not initiate.
    #[must_use]
    pub fn pending_indeterminate_token(&self) -> Option<IndeterminateToken> {
        self.indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(|pending| pending.token)
    }

    /// Publish one meta segment. `terminal` = "contains at least one
    /// `RootEnded`": one attempt under a latched disk gate.
    pub fn publish_meta(&self, records: &[MetaRecord], terminal: bool) -> PublishBatchResult {
        self.publish_segment(Plane::Meta, terminal, |sequence, high_water| {
            encode_meta_segment(self.stream.0, sequence, high_water.data, records)
        })
    }

    /// Publish one data segment. Groups must be sorted by root `ThreadRef`
    /// bytes ascending, distinct, and non-empty.
    pub fn publish_data(&self, groups: &[DataGroup]) -> PublishBatchResult {
        debug_assert!(
            groups
                .windows(2)
                .all(|pair| pair[0].root_bytes() < pair[1].root_bytes()),
            "data groups must be sorted and distinct by root"
        );
        debug_assert!(groups.iter().all(|group| !group.is_empty()));
        self.publish_segment(Plane::Data, false, |sequence, _| {
            Ok(encode_data_segment(self.stream.0, sequence, groups))
        })
    }

    fn publish_segment(
        &self,
        plane: Plane,
        terminal: bool,
        encode: impl FnOnce(u64, StreamHighWater) -> Result<Vec<u8>, EncodeError>,
    ) -> PublishBatchResult {
        if !terminal && let Some(reason) = self.gate_reason() {
            return PublishBatchResult::Lost(reason);
        }
        let _process_guard = self
            .process_publish
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let high_water = self.high_water();
        let last = match plane {
            Plane::Meta => high_water.meta,
            Plane::Data => high_water.data,
        };
        let Some(sequence) = last.checked_add(1) else {
            return PublishBatchResult::Lost(StoreFailureReason::SequenceExhausted);
        };
        let Ok(bytes) = encode(sequence, high_water) else {
            return PublishBatchResult::Lost(StoreFailureReason::StoreUnavailable);
        };
        let kind = match plane {
            Plane::Meta => StoreFileKind::MetaSegment,
            Plane::Data => StoreFileKind::DataSegment,
        };
        let final_path = self.segment_path(plane, sequence);
        match self.publish_bytes_guarded(
            kind,
            &final_path,
            &bytes,
            terminal,
            Some((plane, sequence)),
        ) {
            GuardedPublish::Committed => {
                let mut high_water = self
                    .high_water
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                match plane {
                    Plane::Meta => high_water.meta = sequence,
                    Plane::Data => high_water.data = sequence,
                }
                PublishBatchResult::Committed { sequence }
            }
            GuardedPublish::Lost(reason) => PublishBatchResult::Lost(reason),
            GuardedPublish::AlreadyIndeterminate(token) => PublishBatchResult::Blocked(token),
            GuardedPublish::WentIndeterminate(token) => {
                PublishBatchResult::Indeterminate { token, sequence }
            }
        }
    }

    /// Publish or fully verify one project-shared canonical value object.
    /// Existing objects are never trusted from their path alone: the complete
    /// framing and body are checked and the CID is recomputed before reuse.
    pub fn publish_cas_object(
        &self,
        codec: CodecVersion,
        encoded_body: &[u8],
    ) -> (ValueCid, PublishCasResult) {
        let cid = ValueCid::for_encoded(codec, encoded_body);
        let digest = hex::encode(cid.0);
        let final_path = self
            .root
            .join("cas/sha256")
            .join(&digest[..2])
            .join(format!("{digest}.bamlvalue"));
        if final_path.exists() {
            return (
                cid,
                verify_cas_object(&final_path, cid, codec, encoded_body),
            );
        }
        let bytes = match encode_cas_object(cid, codec, encoded_body) {
            Ok(bytes) => bytes,
            Err(reason) => return (cid, PublishCasResult::Lost(reason)),
        };
        if let Some(reason) = self.gate_reason() {
            return (cid, PublishCasResult::Lost(reason));
        }
        let _process_guard = self
            .process_publish
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let result = match self.publish_bytes_guarded(
            StoreFileKind::CasObject,
            &final_path,
            &bytes,
            false,
            None,
        ) {
            GuardedPublish::Committed => PublishCasResult::Published,
            GuardedPublish::Lost(StoreFailureReason::PathConflict) => {
                verify_cas_object(&final_path, cid, codec, encoded_body)
            }
            GuardedPublish::Lost(reason) => PublishCasResult::Lost(reason),
            GuardedPublish::AlreadyIndeterminate(token)
            | GuardedPublish::WentIndeterminate(token) => PublishCasResult::Indeterminate(token),
        };
        (cid, result)
    }

    pub fn resolve_indeterminate(&self, token: IndeterminateToken) -> ResolveIndeterminateResult {
        let _process_guard = self
            .process_publish
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut state = self
            .indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(pending) = state.as_ref() else {
            return ResolveIndeterminateResult::TokenMismatch;
        };
        if pending.token != token {
            return ResolveIndeterminateResult::TokenMismatch;
        }
        let resolved = self
            .platform
            .sync_dir(&pending.final_directory)
            .and_then(|()| scan_physical_usage(&self.root))
            .and_then(|usage| write_usage_state(&self.root, usage, self.platform.as_ref()));
        if resolved.is_err() {
            return ResolveIndeterminateResult::StillIndeterminate;
        }
        if let Some((plane, sequence)) = pending.plane {
            let mut high_water = self
                .high_water
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match plane {
                Plane::Meta => high_water.meta = high_water.meta.max(sequence),
                Plane::Data => high_water.data = high_water.data.max(sequence),
            }
        }
        *state = None;
        self.indeterminate_flag.store(false, Ordering::Release);
        if FileExt::unlock(&self.publish_lock).is_err() {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
        }
        ResolveIndeterminateResult::Committed
    }

    /// Assumes `process_publish` is held by the caller.
    fn publish_bytes_guarded(
        &self,
        kind: StoreFileKind,
        final_path: &Path,
        bytes: &[u8],
        terminal: bool,
        plane: Option<(Plane, u64)>,
    ) -> GuardedPublish {
        if let Some(pending) = self
            .indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            return GuardedPublish::AlreadyIndeterminate(pending.token);
        }
        if !terminal && let Some(reason) = self.gate_reason() {
            return GuardedPublish::Lost(reason);
        }
        if FileExt::lock_exclusive(&self.publish_lock).is_err() {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return GuardedPublish::Lost(StoreFailureReason::StoreUnavailable);
        }

        let result = self.publish_bytes_locked(kind, final_path, bytes, plane);
        if !matches!(result, GuardedPublish::WentIndeterminate(_))
            && FileExt::unlock(&self.publish_lock).is_err()
        {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return GuardedPublish::Lost(StoreFailureReason::StoreUnavailable);
        }
        result
    }

    fn publish_bytes_locked(
        &self,
        kind: StoreFileKind,
        final_path: &Path,
        bytes: &[u8],
        plane: Option<(Plane, u64)>,
    ) -> GuardedPublish {
        if final_path.exists() {
            if kind != StoreFileKind::CasObject {
                self.admission_gate
                    .store(GATE_UNAVAILABLE, Ordering::Release);
            }
            return GuardedPublish::Lost(StoreFailureReason::PathConflict);
        }
        let Ok(current_usage) = read_usage_state(&self.root) else {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return GuardedPublish::Lost(StoreFailureReason::StoreUnavailable);
        };
        let Ok(bytes_len) = u64::try_from(bytes.len()) else {
            return self.disk_guard_failure();
        };
        let Some(peak_usage) = current_usage
            .checked_add(bytes_len)
            .and_then(|usage| usage.checked_add(USAGE_STATE_BYTES))
        else {
            return self.disk_guard_failure();
        };
        let Ok(free) = self.platform.available_space(&self.root) else {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return GuardedPublish::Lost(StoreFailureReason::StoreUnavailable);
        };
        if peak_usage > self.disk.max_project_bytes
            || free.saturating_sub(bytes_len.saturating_add(USAGE_STATE_BYTES))
                < self.disk.minimum_free_bytes
        {
            return self.disk_guard_failure();
        }

        let Some(final_directory) = final_path.parent() else {
            return GuardedPublish::Lost(StoreFailureReason::StoreUnavailable);
        };
        if let Err(error) = fs::create_dir_all(final_directory) {
            return self.io_failure(&error);
        }
        let temporary = self
            .root
            .join("tmp")
            .join(format!("{}.pending", uuid::Uuid::new_v4().simple()));
        let write_result = write_synced_file(&temporary, bytes, self.platform.as_ref())
            .and_then(|()| self.platform.before_rename(kind, &temporary));
        if let Err(error) = write_result {
            self.account_pre_rename_orphan(&temporary, current_usage);
            return self.io_failure(&error);
        }
        if final_path.exists() {
            if kind == StoreFileKind::CasObject {
                if fs::remove_file(&temporary).is_err() {
                    self.account_pre_rename_orphan(&temporary, current_usage);
                }
            } else {
                self.account_pre_rename_orphan(&temporary, current_usage);
                self.admission_gate
                    .store(GATE_UNAVAILABLE, Ordering::Release);
            }
            return GuardedPublish::Lost(StoreFailureReason::PathConflict);
        }
        if let Err(error) = fs::rename(&temporary, final_path) {
            self.account_pre_rename_orphan(&temporary, current_usage);
            return self.io_failure(&error);
        }

        if self.platform.sync_dir(final_directory).is_err() {
            return self.retain_indeterminate(final_directory, plane);
        }
        let new_usage = current_usage + bytes_len;
        if write_usage_state(&self.root, new_usage, self.platform.as_ref()).is_err() {
            return self.retain_indeterminate(final_directory, plane);
        }
        GuardedPublish::Committed
    }

    fn account_pre_rename_orphan(&self, temporary: &Path, current_usage: u64) {
        let orphan_bytes = fs::metadata(temporary).map_or(0, |metadata| metadata.len());
        let Some(usage) = current_usage.checked_add(orphan_bytes) else {
            self.admission_gate.store(GATE_DISK, Ordering::Release);
            return;
        };
        if write_usage_state(&self.root, usage, self.platform.as_ref()).is_err() {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
        }
    }

    fn retain_indeterminate(
        &self,
        final_directory: &Path,
        plane: Option<(Plane, u64)>,
    ) -> GuardedPublish {
        let token = IndeterminateToken(*uuid::Uuid::new_v4().as_bytes());
        *self
            .indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(IndeterminateState {
            token,
            final_directory: final_directory.to_owned(),
            plane,
        });
        self.indeterminate_flag.store(true, Ordering::Release);
        GuardedPublish::WentIndeterminate(token)
    }

    fn io_failure(&self, error: &io::Error) -> GuardedPublish {
        if is_out_of_space(error) {
            self.disk_guard_failure()
        } else if error.kind() == io::ErrorKind::PermissionDenied {
            GuardedPublish::Lost(StoreFailureReason::PermissionDenied)
        } else {
            GuardedPublish::Lost(StoreFailureReason::StoreUnavailable)
        }
    }

    fn disk_guard_failure(&self) -> GuardedPublish {
        self.admission_gate.store(GATE_DISK, Ordering::Release);
        GuardedPublish::Lost(StoreFailureReason::DiskGuardExceeded)
    }

    fn gate_reason(&self) -> Option<StoreFailureReason> {
        match self.admission_gate.load(Ordering::Acquire) {
            GATE_OPEN => None,
            GATE_DISK => Some(StoreFailureReason::DiskGuardExceeded),
            _ => Some(StoreFailureReason::StoreUnavailable),
        }
    }

    fn segment_path(&self, plane: Plane, sequence: u64) -> PathBuf {
        segment_path(&self.root, self.stream, plane, sequence)
    }
}

/// Releases the `OPEN_STREAMS` entry if `open` fails after inserting it.
struct ReleaseStreamOnError(StreamId);

impl Drop for ReleaseStreamOnError {
    fn drop(&mut self) {
        open_streams()
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(&self.0.0.0);
    }
}

#[must_use]
pub fn stream_directory(root: &Path, stream: StreamId) -> PathBuf {
    root.join("streams").join(hex::encode(stream.0.0))
}

#[must_use]
pub fn segment_path(root: &Path, stream: StreamId, plane: Plane, sequence: u64) -> PathBuf {
    let (directory, extension) = match plane {
        Plane::Meta => ("meta", "bamlmeta"),
        Plane::Data => ("data", "bamldata"),
    };
    stream_directory(root, stream)
        .join(directory)
        .join(format!("{sequence:020}.{extension}"))
}

/// Highest well-formed (checksummed) sequence in one plane directory. A
/// corrupt highest final path fails closed; lower sequences are the reader's
/// concern.
fn scan_plane_high_water(directory: &Path, extension: &str) -> io::Result<u64> {
    let suffix = format!(".{extension}");
    let mut highest: Option<(u64, PathBuf)> = None;
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        let Some(stem) = name.strip_suffix(&suffix) else {
            continue;
        };
        if stem.len() != 20 || !stem.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let Ok(sequence) = stem.parse::<u64>() else {
            continue;
        };
        if highest.as_ref().is_none_or(|(high, _)| sequence > *high) {
            highest = Some((sequence, entry.path()));
        }
    }
    let Some((sequence, path)) = highest else {
        return Ok(0);
    };
    let bytes = fs::read(&path)?;
    validate_checksum(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "corrupt tail segment in profiling stream",
        )
    })?;
    Ok(sequence)
}

pub(crate) fn thread_ref_bytes(thread: ThreadRef) -> [u8; 32] {
    let mut bytes = [0u8; 32];
    bytes[..16].copy_from_slice(&thread.process_euid.0);
    bytes[16..24].copy_from_slice(&thread.engine_id.0.to_be_bytes());
    bytes[24..32].copy_from_slice(&thread.thread_id.0.to_be_bytes());
    bytes
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EncodeError {
    StringTooLong,
}

fn encode_meta_segment(
    euid: ProcessEuid,
    sequence: u64,
    data_high_water: u64,
    records: &[MetaRecord],
) -> Result<Vec<u8>, EncodeError> {
    let mut payload = Vec::with_capacity(records.len().saturating_mul(320));
    for record in records {
        let (tag, body) = encode_meta_record(record)?;
        payload.push(tag);
        payload.extend_from_slice(&u32::try_from(body.len()).unwrap_or(u32::MAX).to_be_bytes());
        payload.extend_from_slice(&body);
    }
    let mut bytes = Vec::with_capacity(64 + payload.len());
    bytes.extend_from_slice(META_MAGIC);
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&euid.0);
    bytes.extend_from_slice(&data_high_water.to_be_bytes());
    bytes.extend_from_slice(
        &u64::try_from(records.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(
        &u64::try_from(payload.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&payload);
    Ok(with_checksum(bytes))
}

fn encode_meta_record(record: &MetaRecord) -> Result<(u8, Vec<u8>), EncodeError> {
    let mut body = Vec::with_capacity(320);
    let tag = match record {
        MetaRecord::StreamStarted {
            pid,
            zero_unix_ns,
            baml_version,
            os_arch,
        } => {
            body.extend_from_slice(&pid.to_be_bytes());
            body.extend_from_slice(&zero_unix_ns.to_be_bytes());
            encode_string(&mut body, baml_version)?;
            encode_string(&mut body, os_arch)?;
            0
        }
        MetaRecord::EngineStarted {
            engine_id,
            program_id,
            function_table_cid,
            revision_label,
            source_label,
        } => {
            body.extend_from_slice(&engine_id.0.to_be_bytes());
            body.extend_from_slice(&program_id.0);
            match function_table_cid {
                None => body.push(0),
                Some(cid) => {
                    body.push(1);
                    body.extend_from_slice(&cid.0);
                }
            }
            encode_optional_string(&mut body, revision_label.as_deref())?;
            encode_optional_string(&mut body, source_label.as_deref())?;
            1
        }
        MetaRecord::RootStarted {
            root,
            started_ns,
            runtime_id,
        } => {
            body.extend_from_slice(&thread_ref_bytes(*root));
            body.extend_from_slice(&started_ns.to_be_bytes());
            body.extend_from_slice(&runtime_id.as_bytes());
            2
        }
        MetaRecord::RootEnded {
            root,
            ended_ns,
            status,
            flags,
            data_first_seq,
            data_last_seq,
            data_segment_count,
            health,
        } => {
            body.extend_from_slice(&thread_ref_bytes(*root));
            body.extend_from_slice(&ended_ns.to_be_bytes());
            body.push(*status as u8);
            body.push(*flags);
            body.extend_from_slice(&data_first_seq.to_be_bytes());
            body.extend_from_slice(&data_last_seq.to_be_bytes());
            body.extend_from_slice(&data_segment_count.to_be_bytes());
            let health = health.encode();
            body.extend_from_slice(
                &u32::try_from(health.len())
                    .unwrap_or(u32::MAX)
                    .to_be_bytes(),
            );
            body.extend_from_slice(&health);
            3
        }
    };
    Ok((tag, body))
}

pub fn decode_meta_segment(
    bytes: &[u8],
    expected_euid: ProcessEuid,
) -> Result<DecodedMetaSegment, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != META_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
    let sequence = cursor.u64()?;
    if ProcessEuid(cursor.array::<16>()?) != expected_euid {
        return Err(SegmentReadError::EuidMismatch);
    }
    let data_high_water = cursor.u64()?;
    let record_count = cursor.usize_u64()?;
    let payload_len = cursor.usize_u64()?;
    let payload = cursor.take(payload_len)?;
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    let mut records = Vec::with_capacity(record_count.min(1024));
    let mut payload_cursor = SegmentCursor::new(payload);
    for index in 0..record_count {
        let tag = payload_cursor.u8()?;
        let body_len =
            usize::try_from(payload_cursor.u32()?).map_err(|_| SegmentReadError::Truncated)?;
        let body = payload_cursor.take(body_len)?;
        let record = decode_meta_record(tag, body)?;
        if matches!(record, MetaRecord::StreamStarted { .. }) && (sequence != 1 || index != 0) {
            return Err(SegmentReadError::MetaStreamStartedMisplaced);
        }
        records.push(record);
    }
    if !payload_cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    Ok(DecodedMetaSegment {
        sequence,
        data_high_water,
        records,
    })
}

fn decode_meta_record(tag: u8, body: &[u8]) -> Result<MetaRecord, SegmentReadError> {
    let mut cursor = SegmentCursor::new(body);
    let record = match tag {
        0 => MetaRecord::StreamStarted {
            pid: cursor.u32()?,
            zero_unix_ns: cursor.u64()?,
            baml_version: cursor.string()?,
            os_arch: cursor.string()?,
        },
        1 => MetaRecord::EngineStarted {
            engine_id: EngineId(cursor.u64()?),
            program_id: ProgramId(cursor.array::<16>()?),
            function_table_cid: match cursor.u8()? {
                0 => None,
                1 => Some(ValueCid(cursor.array::<32>()?)),
                _ => return Err(SegmentReadError::InvalidKind),
            },
            revision_label: cursor.optional_string()?,
            source_label: cursor.optional_string()?,
        },
        2 => MetaRecord::RootStarted {
            root: decode_thread_ref(&mut cursor)?,
            started_ns: cursor.u64()?,
            runtime_id: BoundaryId::from_bytes(cursor.array::<16>()?),
        },
        3 => {
            let root = decode_thread_ref(&mut cursor)?;
            let ended_ns = cursor.u64()?;
            let status = decode_status(cursor.u8()?)?;
            let flags = cursor.u8()?;
            let data_first_seq = cursor.u64()?;
            let data_last_seq = cursor.u64()?;
            let data_segment_count = cursor.u64()?;
            let health_len =
                usize::try_from(cursor.u32()?).map_err(|_| SegmentReadError::Truncated)?;
            let health = ExecutionHealthSnapshot::decode(cursor.take(health_len)?)
                .ok_or(SegmentReadError::MetaInvalidHealth)?;
            MetaRecord::RootEnded {
                root,
                ended_ns,
                status,
                flags,
                data_first_seq,
                data_last_seq,
                data_segment_count,
                health,
            }
        }
        unknown => return Err(SegmentReadError::MetaUnknownTag(unknown)),
    };
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    Ok(record)
}

fn encode_data_segment(euid: ProcessEuid, sequence: u64, groups: &[DataGroup]) -> Vec<u8> {
    let payload_len: usize = groups
        .iter()
        .map(|group| 49 + 16 + group.cct.len() + group.evidence.len())
        .sum();
    let mut bytes = Vec::with_capacity(64 + payload_len);
    bytes.extend_from_slice(DATA_MAGIC);
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    bytes.extend_from_slice(&sequence.to_be_bytes());
    bytes.extend_from_slice(&euid.0);
    bytes.extend_from_slice(
        &u64::try_from(groups.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    let mut payload = Vec::with_capacity(payload_len);
    for group in groups {
        payload.extend_from_slice(&thread_ref_bytes(group.root));
        payload.push(cct_codec::encode_health(group.cct_health));
        payload.extend_from_slice(&group.cct_record_count.to_be_bytes());
        payload.extend_from_slice(
            &u64::try_from(group.cct.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        payload.extend_from_slice(&group.cct);
        payload.extend_from_slice(&group.evidence_record_count.to_be_bytes());
        payload.extend_from_slice(
            &u64::try_from(group.evidence.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        payload.extend_from_slice(&group.evidence);
    }
    bytes.extend_from_slice(
        &u64::try_from(payload.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&payload);
    with_checksum(bytes)
}

pub fn decode_data_segment(
    bytes: &[u8],
    expected_euid: ProcessEuid,
) -> Result<DecodedDataSegment<'_>, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != DATA_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
    let sequence = cursor.u64()?;
    if ProcessEuid(cursor.array::<16>()?) != expected_euid {
        return Err(SegmentReadError::EuidMismatch);
    }
    let group_count = cursor.usize_u64()?;
    let payload_len = cursor.usize_u64()?;
    let payload = cursor.take(payload_len)?;
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    let mut groups = Vec::with_capacity(group_count.min(1024));
    let mut payload_cursor = SegmentCursor::new(payload);
    let mut previous_root: Option<[u8; 32]> = None;
    for _ in 0..group_count {
        let root_bytes = payload_cursor.array::<32>()?;
        match previous_root {
            Some(previous) if previous == root_bytes => {
                return Err(SegmentReadError::DataDuplicateGroup);
            }
            Some(previous) if previous > root_bytes => {
                return Err(SegmentReadError::DataGroupOrder);
            }
            _ => {}
        }
        previous_root = Some(root_bytes);
        let cct_health = cct_codec::decode_health(&[payload_cursor.u8()?])
            .map_err(SegmentReadError::InvalidCct)?;
        let cct_record_count = payload_cursor.u64()?;
        let cct_len = payload_cursor.usize_u64()?;
        let cct = payload_cursor.take(cct_len)?;
        let evidence_record_count = payload_cursor.u64()?;
        let evidence_len = payload_cursor.usize_u64()?;
        let evidence = payload_cursor.take(evidence_len)?;
        if cct.is_empty() && evidence.is_empty() {
            return Err(SegmentReadError::DataEmptyGroup);
        }
        groups.push(RawDataGroup {
            root: decode_thread_ref_bytes(root_bytes),
            cct_health,
            cct_record_count,
            cct,
            evidence_record_count,
            evidence,
        });
    }
    if !payload_cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    Ok(DecodedDataSegment { sequence, groups })
}

pub fn decode_cas_object(bytes: &[u8]) -> Result<DecodedCasObject, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != VALUE_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    let version = cursor.u16()?;
    if version != CAS_FORMAT_VERSION {
        return Err(SegmentReadError::UnsupportedVersion(version));
    }
    let codec = CodecVersion(cursor.u16()?);
    let body_len = cursor.usize_u64()?;
    let cid = ValueCid(cursor.array::<32>()?);
    let body = cursor.take(body_len)?.to_vec();
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    if ValueCid::for_encoded(codec, &body) != cid {
        return Err(SegmentReadError::InvalidChecksum);
    }
    Ok(DecodedCasObject { cid, codec, body })
}

fn decode_status(tag: u8) -> Result<ExecutionEndStatus, SegmentReadError> {
    match tag {
        0 => Ok(ExecutionEndStatus::Succeeded),
        1 => Ok(ExecutionEndStatus::Failed),
        2 => Ok(ExecutionEndStatus::Cancelled),
        3 => Ok(ExecutionEndStatus::Panicked),
        4 => Ok(ExecutionEndStatus::Abandoned),
        _ => Err(SegmentReadError::MetaInvalidStatus),
    }
}

fn decode_thread_ref(cursor: &mut SegmentCursor<'_>) -> Result<ThreadRef, SegmentReadError> {
    Ok(decode_thread_ref_bytes(cursor.array::<32>()?))
}

pub(crate) fn decode_thread_ref_bytes(bytes: [u8; 32]) -> ThreadRef {
    ThreadRef {
        process_euid: ProcessEuid(bytes[..16].try_into().expect("fixed-width slice")),
        engine_id: EngineId(u64::from_be_bytes(
            bytes[16..24].try_into().expect("fixed-width slice"),
        )),
        thread_id: BexThreadId(u64::from_be_bytes(
            bytes[24..32].try_into().expect("fixed-width slice"),
        )),
    }
}

pub(crate) fn validate_checksum(bytes: &[u8]) -> Result<usize, SegmentReadError> {
    let checksum_start = bytes
        .len()
        .checked_sub(32)
        .ok_or(SegmentReadError::Truncated)?;
    let expected: [u8; 32] = Sha256::digest(&bytes[..checksum_start]).into();
    if bytes[checksum_start..] != expected {
        return Err(SegmentReadError::InvalidChecksum);
    }
    Ok(checksum_start)
}

fn validate_version(cursor: &mut SegmentCursor<'_>) -> Result<(), SegmentReadError> {
    let version = cursor.u16()?;
    if version != SCHEMA_VERSION {
        return Err(SegmentReadError::UnsupportedVersion(version));
    }
    Ok(())
}

struct SegmentCursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> SegmentCursor<'a> {
    const fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Result<&'a [u8], SegmentReadError> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or(SegmentReadError::Truncated)?;
        let slice = self
            .bytes
            .get(self.offset..end)
            .ok_or(SegmentReadError::Truncated)?;
        self.offset = end;
        Ok(slice)
    }

    fn array<const N: usize>(&mut self) -> Result<[u8; N], SegmentReadError> {
        self.take(N)?
            .try_into()
            .map_err(|_| SegmentReadError::Truncated)
    }

    fn u8(&mut self) -> Result<u8, SegmentReadError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, SegmentReadError> {
        Ok(u16::from_be_bytes(self.array()?))
    }

    fn u32(&mut self) -> Result<u32, SegmentReadError> {
        Ok(u32::from_be_bytes(self.array()?))
    }

    fn u64(&mut self) -> Result<u64, SegmentReadError> {
        Ok(u64::from_be_bytes(self.array()?))
    }

    fn usize_u64(&mut self) -> Result<usize, SegmentReadError> {
        usize::try_from(self.u64()?).map_err(|_| SegmentReadError::Truncated)
    }

    fn string(&mut self) -> Result<String, SegmentReadError> {
        let length = usize::try_from(self.u32()?).map_err(|_| SegmentReadError::Truncated)?;
        let value =
            std::str::from_utf8(self.take(length)?).map_err(|_| SegmentReadError::InvalidUtf8)?;
        Ok(value.to_owned())
    }

    fn optional_string(&mut self) -> Result<Option<String>, SegmentReadError> {
        match self.u8()? {
            0 => Ok(None),
            1 => self.string().map(Some),
            _ => Err(SegmentReadError::InvalidKind),
        }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn encode_string(output: &mut Vec<u8>, value: &str) -> Result<(), EncodeError> {
    if value.len() > MAX_ENCODED_STRING {
        return Err(EncodeError::StringTooLong);
    }
    output.extend_from_slice(
        &u32::try_from(value.len())
            .map_err(|_| EncodeError::StringTooLong)?
            .to_be_bytes(),
    );
    output.extend_from_slice(value.as_bytes());
    Ok(())
}

fn encode_optional_string(output: &mut Vec<u8>, value: Option<&str>) -> Result<(), EncodeError> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            encode_string(output, value)?;
        }
    }
    Ok(())
}

fn encode_cas_object(
    cid: ValueCid,
    codec: CodecVersion,
    encoded_body: &[u8],
) -> Result<Vec<u8>, StoreFailureReason> {
    let capacity = 84usize
        .checked_add(encoded_body.len())
        .ok_or(StoreFailureReason::StoreUnavailable)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| StoreFailureReason::StoreUnavailable)?;
    bytes.extend_from_slice(VALUE_MAGIC);
    bytes.extend_from_slice(&CAS_FORMAT_VERSION.to_be_bytes());
    bytes.extend_from_slice(&codec.0.to_be_bytes());
    bytes.extend_from_slice(
        &u64::try_from(encoded_body.len())
            .map_err(|_| StoreFailureReason::StoreUnavailable)?
            .to_be_bytes(),
    );
    bytes.extend_from_slice(&cid.0);
    bytes.extend_from_slice(encoded_body);
    Ok(with_checksum(bytes))
}

fn verify_cas_object(
    path: &Path,
    cid: ValueCid,
    codec: CodecVersion,
    encoded_body: &[u8],
) -> PublishCasResult {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => metadata,
        _ => return PublishCasResult::Conflict,
    };
    let expected_len = 84u64.saturating_add(u64::try_from(encoded_body.len()).unwrap_or(u64::MAX));
    if metadata.len() != expected_len {
        return PublishCasResult::Conflict;
    }
    let Ok(bytes) = fs::read(path) else {
        return PublishCasResult::Lost(StoreFailureReason::StoreUnavailable);
    };
    if u64::try_from(bytes.len()) != Ok(expected_len) {
        return PublishCasResult::Conflict;
    }
    let checksum_start = bytes.len() - 32;
    let expected_checksum: [u8; 32] = Sha256::digest(&bytes[..checksum_start]).into();
    if bytes[checksum_start..] != expected_checksum
        || &bytes[..8] != VALUE_MAGIC
        || bytes[8..10] != CAS_FORMAT_VERSION.to_be_bytes()
        || bytes[10..12] != codec.0.to_be_bytes()
        || bytes[12..20]
            != u64::try_from(encoded_body.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes()
        || bytes[20..52] != cid.0
        || bytes[52..checksum_start] != *encoded_body
        || ValueCid::for_encoded(codec, &bytes[52..checksum_start]) != cid
    {
        return PublishCasResult::Conflict;
    }
    PublishCasResult::Reused
}

fn with_checksum(mut body: Vec<u8>) -> Vec<u8> {
    let checksum: [u8; 32] = Sha256::digest(&body).into();
    body.extend_from_slice(&checksum);
    body
}

fn read_write_file(path: &Path) -> io::Result<File> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
}

fn write_synced_file(path: &Path, bytes: &[u8], platform: &dyn StorePlatform) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    platform.sync_file(&file)
}

fn scan_physical_usage(root: &Path) -> io::Result<u64> {
    fn scan(directory: &Path, root: &Path, total: &mut u64) -> io::Result<()> {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "profiling store contains a symlink",
                ));
            }
            if file_type.is_dir() {
                scan(&path, root, total)?;
            } else if file_type.is_file() {
                let relative = path.strip_prefix(root).map_err(io::Error::other)?;
                if relative == Path::new("publish.lock")
                    || relative == Path::new("usage.state")
                    || relative == Path::new("tmp/usage-state.pending")
                {
                    continue;
                }
                *total = total
                    .checked_add(entry.metadata()?.len())
                    .ok_or_else(|| io::Error::other("profiling usage overflow"))?;
            } else {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "profiling store contains an unsupported file type",
                ));
            }
        }
        Ok(())
    }

    let mut total = USAGE_STATE_BYTES;
    scan(root, root, &mut total)?;
    Ok(total)
}

fn read_usage_state(root: &Path) -> io::Result<u64> {
    let usage_state_len = usize::try_from(USAGE_STATE_BYTES).expect("fixed usage state fits usize");
    let mut bytes = Vec::with_capacity(usage_state_len);
    File::open(root.join("usage.state"))?.read_to_end(&mut bytes)?;
    if bytes.len() != usage_state_len || &bytes[..8] != USAGE_MAGIC {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid profiling usage ledger",
        ));
    }
    let expected: [u8; 32] = Sha256::digest(&bytes[..16]).into();
    if bytes[16..] != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "profiling usage ledger checksum mismatch",
        ));
    }
    Ok(u64::from_be_bytes(
        bytes[8..16].try_into().expect("fixed-width usage"),
    ))
}

fn write_usage_state(root: &Path, usage: u64, platform: &dyn StorePlatform) -> io::Result<()> {
    let usage_state_len = usize::try_from(USAGE_STATE_BYTES).expect("fixed usage state fits usize");
    let mut bytes = Vec::with_capacity(usage_state_len);
    bytes.extend_from_slice(USAGE_MAGIC);
    bytes.extend_from_slice(&usage.to_be_bytes());
    let checksum: [u8; 32] = Sha256::digest(&bytes).into();
    bytes.extend_from_slice(&checksum);
    let temporary = root.join("tmp/usage-state.pending");
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(&temporary)?;
    file.write_all(&bytes)?;
    file.flush()?;
    platform.sync_file(&file)?;
    fs::rename(&temporary, root.join("usage.state"))?;
    platform.sync_dir(root)
}

/// Keeps profiler output out of user repositories: the nearest `.baml/`
/// ancestor of `root` gets a `.gitignore` containing a standalone `*` line,
/// created if missing and appended if present (existing rules are preserved;
/// an existing standalone `*` is left alone). Returns `Ok(false)` when `root`
/// is not under a `.baml/` directory (custom store roots stay user-managed).
fn ensure_baml_dir_ignored(root: &Path) -> io::Result<bool> {
    let Some(baml_dir) = root
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == ".baml"))
    else {
        return Ok(false);
    };
    fs::create_dir_all(baml_dir)?;
    let ignore_path = baml_dir.join(".gitignore");
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&ignore_path)
    {
        Ok(mut file) => {
            file.write_all(b"*\n")?;
            Ok(true)
        }
        Err(err) if err.kind() == io::ErrorKind::AlreadyExists => {
            let contents = fs::read(&ignore_path)?;
            if has_standalone_star_line(&contents) {
                return Ok(true);
            }
            let mut file = fs::OpenOptions::new().append(true).open(&ignore_path)?;
            if contents.is_empty() || contents.ends_with(b"\n") {
                file.write_all(b"*\n")?;
            } else {
                file.write_all(b"\n*\n")?;
            }
            Ok(true)
        }
        Err(err) => Err(err),
    }
}

fn has_standalone_star_line(contents: &[u8]) -> bool {
    contents
        .split(|byte| *byte == b'\n')
        .any(|line| line.trim_ascii() == b"*")
}

fn open_error(source: io::Error) -> StoreOpenError {
    let reason = if source.kind() == io::ErrorKind::PermissionDenied {
        StoreFailureReason::PermissionDenied
    } else {
        StoreFailureReason::StoreUnavailable
    };
    StoreOpenError {
        reason,
        source: Some(source),
    }
}

fn is_out_of_space(error: &io::Error) -> bool {
    matches!(error.raw_os_error(), Some(28 | 112))
}

#[cfg(unix)]
fn sync_directory(path: &Path) -> io::Result<()> {
    File::open(path)?.sync_all()
}

/// Windows has no directory fsync: NTFS journals directory entries itself,
/// and `FlushFileBuffers` on a directory handle is denied unless the handle
/// was opened for writing (a read-only backup-semantics handle fails with
/// `PermissionDenied`, which took the whole store down at open). File data is
/// still synced by the file handles; the directory step is a no-op here.
#[cfg(windows)]
fn sync_directory(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn store_open_writes_idempotent_baml_gitignore() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("project/.baml/profiles-v1");
        let ignore = temp.path().join("project/.baml/.gitignore");
        let disk = DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        };

        let store = ProfilerStore::open_native(root.clone(), disk, stream_id(40)).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"*\n");
        drop(store);

        // Re-opening never duplicates the marker.
        let store = ProfilerStore::open_native(root.clone(), disk, stream_id(41)).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"*\n");
        drop(store);

        // Existing user rules are preserved and the marker is appended once.
        fs::write(&ignore, b"# keep this\n!.keep").unwrap();
        let store = ProfilerStore::open_native(root.clone(), disk, stream_id(42)).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"# keep this\n!.keep\n*\n");
        drop(store);
        let _ = ProfilerStore::open_native(root, disk, stream_id(43)).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"# keep this\n!.keep\n*\n");
    }

    #[test]
    fn baml_gitignore_marker_recognizes_padded_star_and_skips_custom_roots() {
        let temp = TempDir::new().unwrap();
        let baml_dir = temp.path().join("project/.baml");
        fs::create_dir_all(&baml_dir).unwrap();
        let original = b"# keep this\n  * \r\n!.keep\n";
        fs::write(baml_dir.join(".gitignore"), original).unwrap();
        assert!(ensure_baml_dir_ignored(&baml_dir.join("profiles-v1")).unwrap());
        assert_eq!(fs::read(baml_dir.join(".gitignore")).unwrap(), original);

        let custom = temp.path().join("custom-profiles");
        assert!(!ensure_baml_dir_ignored(&custom).unwrap());
        assert!(
            !custom.exists(),
            "custom roots outside .baml stay user-managed"
        );
    }

    #[derive(Debug)]
    struct TestPlatform {
        free: AtomicU64,
        fail_next_rename: AtomicBool,
        fail_next_dir_sync: AtomicBool,
    }

    impl TestPlatform {
        fn new(free: u64) -> Self {
            Self {
                free: AtomicU64::new(free),
                fail_next_rename: AtomicBool::new(false),
                fail_next_dir_sync: AtomicBool::new(false),
            }
        }
    }

    impl StorePlatform for TestPlatform {
        fn available_space(&self, _path: &Path) -> io::Result<u64> {
            Ok(self.free.load(Ordering::Relaxed))
        }

        fn sync_dir(&self, path: &Path) -> io::Result<()> {
            if self.fail_next_dir_sync.swap(false, Ordering::Relaxed) {
                return Err(io::Error::other("injected directory sync failure"));
            }
            sync_directory(path)
        }

        fn before_rename(&self, _kind: StoreFileKind, _temporary: &Path) -> io::Result<()> {
            if self.fail_next_rename.swap(false, Ordering::Relaxed) {
                return Err(io::Error::other("injected pre-rename failure"));
            }
            Ok(())
        }
    }

    fn stream_id(byte: u8) -> StreamId {
        StreamId(ProcessEuid([byte; 16]))
    }

    fn thread(stream: StreamId, thread_id: u64) -> ThreadRef {
        ThreadRef {
            process_euid: stream.0,
            engine_id: EngineId(1),
            thread_id: BexThreadId(thread_id),
        }
    }

    fn store(temp: &TempDir, platform: Arc<TestPlatform>, stream: StreamId) -> Arc<ProfilerStore> {
        ProfilerStore::open(
            temp.path().join(".baml/profiles-v1"),
            DiskBudget {
                max_project_bytes: 1024 * 1024,
                minimum_free_bytes: 100,
            },
            platform,
            stream,
        )
        .unwrap()
    }

    fn root_started(stream: StreamId, thread_id: u64) -> MetaRecord {
        MetaRecord::RootStarted {
            root: thread(stream, thread_id),
            started_ns: 10,
            runtime_id: BoundaryId::from_bytes([9; 16]),
        }
    }

    fn root_ended(stream: StreamId, thread_id: u64) -> MetaRecord {
        MetaRecord::RootEnded {
            root: thread(stream, thread_id),
            ended_ns: 20,
            status: ExecutionEndStatus::Succeeded,
            flags: 0,
            data_first_seq: 1,
            data_last_seq: 1,
            data_segment_count: 1,
            health: ExecutionHealthSnapshot::default(),
        }
    }

    fn data_group(stream: StreamId, thread_id: u64) -> DataGroup {
        DataGroup {
            root: thread(stream, thread_id),
            cct_health: CounterHealth::default(),
            cct_record_count: 0,
            cct: Vec::new(),
            evidence_record_count: 1,
            evidence: b"fake-evidence".to_vec(),
        }
    }

    #[test]
    fn cleanup_is_exclusively_leased_and_scoped_to_profiles_v1() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let legacy = temp.path().join(".baml/history/keep");
        let unrelated = temp.path().join(".baml/not-profiles");
        fs::create_dir_all(&legacy).unwrap();
        fs::create_dir_all(&unrelated).unwrap();
        assert!(matches!(
            clean_profiles_v1(&unrelated),
            Err(CleanProfilesError::InvalidRoot)
        ));
        assert!(unrelated.is_dir());
        let store = store(&temp, Arc::new(TestPlatform::new(u64::MAX)), stream_id(30));
        assert!(matches!(
            clean_profiles_v1(&root),
            Err(CleanProfilesError::InUse)
        ));
        drop(store);

        assert!(clean_profiles_v1(&root).unwrap());
        assert!(!root.exists());
        assert!(temp.path().join(".baml/profiles-v1.lock").is_file());
        assert!(
            legacy.is_dir(),
            "legacy history must be outside cleanup scope"
        );
        assert!(!clean_profiles_v1(&root).unwrap());
    }

    #[test]
    fn meta_and_data_planes_have_independent_contiguous_sequences() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(31);
        let store = store(&temp, Arc::new(TestPlatform::new(u64::MAX)), stream);
        assert_eq!(
            store.publish_meta(&[root_started(stream, 3)], false),
            PublishBatchResult::Committed { sequence: 1 }
        );
        assert_eq!(
            store.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Committed { sequence: 1 }
        );
        assert_eq!(
            store.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Committed { sequence: 2 }
        );
        assert_eq!(
            store.publish_meta(&[root_ended(stream, 3)], true),
            PublishBatchResult::Committed { sequence: 2 }
        );
        assert_eq!(store.high_water(), StreamHighWater { meta: 2, data: 2 });
        let directory = stream_directory(&temp.path().join(".baml/profiles-v1"), stream);
        assert!(
            directory
                .join("meta/00000000000000000001.bamlmeta")
                .is_file()
        );
        assert!(
            directory
                .join("meta/00000000000000000002.bamlmeta")
                .is_file()
        );
        assert!(
            directory
                .join("data/00000000000000000001.bamldata")
                .is_file()
        );
        assert!(
            directory
                .join("data/00000000000000000002.bamldata")
                .is_file()
        );

        let bytes = fs::read(directory.join("meta/00000000000000000002.bamlmeta")).unwrap();
        let decoded = decode_meta_segment(&bytes, stream.0).unwrap();
        assert_eq!(decoded.sequence, 2);
        assert_eq!(decoded.data_high_water, 2);
        assert_eq!(decoded.records, vec![root_ended(stream, 3)]);
    }

    #[test]
    fn second_open_of_one_stream_is_rejected_and_distinct_streams_coexist() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(32);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let first = store(&temp, Arc::clone(&platform), stream);
        let conflict = ProfilerStore::open(
            temp.path().join(".baml/profiles-v1"),
            DiskBudget {
                max_project_bytes: 1024 * 1024,
                minimum_free_bytes: 100,
            },
            Arc::clone(&platform) as Arc<dyn StorePlatform>,
            stream,
        );
        assert!(matches!(
            conflict,
            Err(StoreOpenError {
                reason: StoreFailureReason::StreamInUse,
                ..
            })
        ));
        assert!(stream_open_in_process(stream));

        let other = store(&temp, Arc::clone(&platform), stream_id(33));
        assert_eq!(
            first.publish_meta(&[root_started(stream, 3)], false),
            PublishBatchResult::Committed { sequence: 1 }
        );
        assert_eq!(
            other.publish_meta(&[root_started(stream_id(33), 3)], false),
            PublishBatchResult::Committed { sequence: 1 }
        );
        drop(first);
        assert!(!stream_open_in_process(stream));
    }

    #[test]
    fn sequential_reopen_resumes_sequences() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(34);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        {
            let store = store(&temp, Arc::clone(&platform), stream);
            assert_eq!(
                store.publish_meta(&[root_started(stream, 3)], false),
                PublishBatchResult::Committed { sequence: 1 }
            );
            assert_eq!(
                store.publish_data(&[data_group(stream, 3)]),
                PublishBatchResult::Committed { sequence: 1 }
            );
        }
        let reopened = store(&temp, platform, stream);
        assert_eq!(reopened.high_water(), StreamHighWater { meta: 1, data: 1 });
        assert_eq!(
            reopened.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Committed { sequence: 2 }
        );
    }

    #[test]
    fn cas_miss_hit_conflict_and_indeterminate_resolution_are_explicit() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), stream_id(35));
        let codec = CodecVersion(1);
        let body = b"canonical-value";
        let (cid, first) = store.publish_cas_object(codec, body);
        assert_eq!(first, PublishCasResult::Published);
        assert_eq!(
            store.publish_cas_object(codec, body),
            (cid, PublishCasResult::Reused)
        );

        let digest = hex::encode(cid.0);
        let path = temp
            .path()
            .join(".baml/profiles-v1/cas/sha256")
            .join(&digest[..2])
            .join(format!("{digest}.bamlvalue"));
        let mut corrupt = fs::read(&path).unwrap();
        corrupt[52] ^= 1;
        fs::write(&path, corrupt).unwrap();
        assert_eq!(
            store.publish_cas_object(codec, body),
            (cid, PublishCasResult::Conflict)
        );

        let second_body = b"second-value";
        platform.fail_next_dir_sync.store(true, Ordering::Relaxed);
        let (second_cid, result) = store.publish_cas_object(codec, second_body);
        let PublishCasResult::Indeterminate(token) = result else {
            panic!("expected post-rename ambiguity, got {result:?}");
        };
        assert!(store.is_indeterminate());
        assert_eq!(
            store.resolve_indeterminate(token),
            ResolveIndeterminateResult::Committed
        );
        assert!(!store.is_indeterminate());
        assert_eq!(
            store.publish_cas_object(codec, second_body),
            (second_cid, PublishCasResult::Reused)
        );
    }

    #[test]
    fn exact_disk_guard_latches_normal_admission_but_terminal_gets_one_attempt() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(2);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), stream);
        platform.free.store(100, Ordering::Relaxed);
        assert_eq!(
            store.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Lost(StoreFailureReason::DiskGuardExceeded)
        );
        assert!(!store.is_normal_admission_open());
        assert_eq!(
            store.publish_meta(&[root_started(stream, 4)], false),
            PublishBatchResult::Lost(StoreFailureReason::DiskGuardExceeded)
        );
        // The terminal batch gets exactly one attempt: the disk guard inside
        // the locked path still bounds the write.
        assert_eq!(
            store.publish_meta(&[root_ended(stream, 3)], true),
            PublishBatchResult::Lost(StoreFailureReason::DiskGuardExceeded)
        );
    }

    #[test]
    fn pre_rename_loss_does_not_consume_a_sequence() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(4);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), stream);
        platform.fail_next_rename.store(true, Ordering::Relaxed);
        assert_eq!(
            store.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Lost(StoreFailureReason::StoreUnavailable)
        );
        assert_eq!(
            store.publish_data(&[data_group(stream, 3)]),
            PublishBatchResult::Committed { sequence: 1 }
        );
    }

    #[test]
    fn renamed_dir_sync_failure_blocks_both_planes_until_exact_resolution() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(5);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), stream);
        platform.fail_next_dir_sync.store(true, Ordering::Relaxed);
        let result = store.publish_data(&[data_group(stream, 3)]);
        let PublishBatchResult::Indeterminate { token, sequence } = result else {
            panic!("expected indeterminate publication, got {result:?}");
        };
        assert_eq!(sequence, 1);
        // The written batch is not reflected until resolution.
        assert_eq!(store.high_water(), StreamHighWater::default());
        // Both planes and CAS are blocked.
        assert_eq!(
            store.publish_data(&[data_group(stream, 4)]),
            PublishBatchResult::Blocked(token)
        );
        assert_eq!(
            store.publish_meta(&[root_started(stream, 3)], false),
            PublishBatchResult::Blocked(token)
        );
        assert!(matches!(
            store.publish_cas_object(CodecVersion(1), b"blocked").1,
            PublishCasResult::Indeterminate(blocked) if blocked == token
        ));
        assert_eq!(
            store.resolve_indeterminate(token),
            ResolveIndeterminateResult::Committed
        );
        // Resolution commits the written candidate: high water advances and
        // the next publication takes the next sequence.
        assert_eq!(store.high_water(), StreamHighWater { meta: 0, data: 1 });
        assert_eq!(
            store.publish_data(&[data_group(stream, 4)]),
            PublishBatchResult::Committed { sequence: 2 }
        );
    }

    #[test]
    fn data_groups_reject_structural_violations_at_decode() {
        let stream = stream_id(6);
        let group_a = data_group(stream, 1);
        let group_b = data_group(stream, 2);
        let encoded = encode_data_segment(stream.0, 1, &[group_a.clone(), group_b.clone()]);
        let decoded = decode_data_segment(&encoded, stream.0).unwrap();
        assert_eq!(decoded.sequence, 1);
        assert_eq!(decoded.groups.len(), 2);
        assert_eq!(decoded.groups[0].root, group_a.root);
        assert_eq!(decoded.groups[1].evidence, b"fake-evidence");

        // Duplicate group.
        let duplicate = encode_data_segment(stream.0, 1, &[group_a.clone(), group_a.clone()]);
        assert_eq!(
            decode_data_segment(&duplicate, stream.0),
            Err(SegmentReadError::DataDuplicateGroup)
        );
        // Order violation.
        let unordered = encode_data_segment(stream.0, 1, &[group_b, group_a.clone()]);
        assert_eq!(
            decode_data_segment(&unordered, stream.0),
            Err(SegmentReadError::DataGroupOrder)
        );
        // Empty group.
        let empty = encode_data_segment(
            stream.0,
            1,
            &[DataGroup {
                cct: Vec::new(),
                evidence: Vec::new(),
                cct_record_count: 0,
                evidence_record_count: 0,
                ..group_a
            }],
        );
        assert_eq!(
            decode_data_segment(&empty, stream.0),
            Err(SegmentReadError::DataEmptyGroup)
        );
        // Foreign euid.
        assert_eq!(
            decode_data_segment(&encoded, ProcessEuid([9; 16])),
            Err(SegmentReadError::EuidMismatch)
        );
        // Truncation is typed at every cut.
        for cut in 0..encoded.len() {
            assert!(decode_data_segment(&encoded[..cut], stream.0).is_err());
        }
    }

    #[test]
    fn meta_segment_decode_checks_every_frame_rule() {
        let stream = stream_id(7);
        let records = vec![
            MetaRecord::StreamStarted {
                pid: 42,
                zero_unix_ns: 1_000,
                baml_version: "0.17.0".to_string(),
                os_arch: "macos-aarch64".to_string(),
            },
            MetaRecord::EngineStarted {
                engine_id: EngineId(1),
                program_id: ProgramId([2; 16]),
                function_table_cid: Some(ValueCid([3; 32])),
                revision_label: Some("rev".to_string()),
                source_label: None,
            },
            root_started(stream, 3),
            root_ended(stream, 3),
        ];
        let encoded = encode_meta_segment(stream.0, 1, 0, &records).unwrap();
        let decoded = decode_meta_segment(&encoded, stream.0).unwrap();
        assert_eq!(decoded.records, records);

        // StreamStarted outside segment 1 / record 0 is misplaced.
        let misplaced = encode_meta_segment(stream.0, 2, 0, &records).unwrap();
        assert_eq!(
            decode_meta_segment(&misplaced, stream.0),
            Err(SegmentReadError::MetaStreamStartedMisplaced)
        );
        let reordered =
            encode_meta_segment(stream.0, 1, 0, &[records[2].clone(), records[0].clone()]).unwrap();
        assert_eq!(
            decode_meta_segment(&reordered, stream.0),
            Err(SegmentReadError::MetaStreamStartedMisplaced)
        );
        // Foreign euid.
        assert_eq!(
            decode_meta_segment(&encoded, ProcessEuid([9; 16])),
            Err(SegmentReadError::EuidMismatch)
        );
        // Truncation is typed at every cut.
        for cut in 0..encoded.len() {
            assert!(decode_meta_segment(&encoded[..cut], stream.0).is_err());
        }
        let mut trailing = encoded;
        trailing.push(0);
        assert!(decode_meta_segment(&trailing, stream.0).is_err());
    }

    #[test]
    fn corrupt_tail_segment_fails_reopen_closed() {
        let temp = TempDir::new().unwrap();
        let stream = stream_id(8);
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        {
            let store = store(&temp, Arc::clone(&platform), stream);
            assert!(matches!(
                store.publish_meta(&[root_started(stream, 3)], false),
                PublishBatchResult::Committed { .. }
            ));
        }
        let path = segment_path(
            &temp.path().join(".baml/profiles-v1"),
            stream,
            Plane::Meta,
            1,
        );
        let mut bytes = fs::read(&path).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        fs::write(&path, bytes).unwrap();
        assert!(matches!(
            ProfilerStore::open(
                temp.path().join(".baml/profiles-v1"),
                DiskBudget {
                    max_project_bytes: 1024 * 1024,
                    minimum_free_bytes: 100,
                },
                platform as Arc<dyn StorePlatform>,
                stream,
            ),
            Err(StoreOpenError {
                reason: StoreFailureReason::StoreUnavailable,
                ..
            })
        ));
    }

    #[test]
    fn reopen_reconciles_orphan_bytes_without_deleting_them() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        {
            let store = ProfilerStore::open(
                root.clone(),
                DiskBudget {
                    max_project_bytes: 1024 * 1024,
                    minimum_free_bytes: 100,
                },
                platform as Arc<dyn StorePlatform>,
                stream_id(36),
            )
            .unwrap();
            fs::write(root.join("tmp/orphan"), b"orphan").unwrap();
            drop(store);
        }
        let reopened = ProfilerStore::open_native(
            root.clone(),
            DiskBudget {
                max_project_bytes: 1024 * 1024,
                minimum_free_bytes: 0,
            },
            stream_id(36),
        )
        .unwrap();
        assert!(root.join("tmp/orphan").is_file());
        // stream.lock is zero bytes, so only the orphan contributes.
        assert_eq!(read_usage_state(&root).unwrap(), USAGE_STATE_BYTES + 6);
        drop(reopened);
    }

    /// Golden byte fixtures (streams spec §9): stable cross-platform
    /// SHA-256 of a meta segment with all four record kinds and a data
    /// segment with two groups (CCT-only and evidence-only).
    #[test]
    fn segment_encodings_have_cross_platform_goldens() {
        use sha2::Digest as _;

        let stream = stream_id(9);
        let records = vec![
            MetaRecord::StreamStarted {
                pid: 42,
                zero_unix_ns: 1_000,
                baml_version: "golden".to_string(),
                os_arch: "test-arch".to_string(),
            },
            MetaRecord::EngineStarted {
                engine_id: EngineId(1),
                program_id: ProgramId([2; 16]),
                function_table_cid: Some(ValueCid([3; 32])),
                revision_label: Some("rev".to_string()),
                source_label: None,
            },
            root_started(stream, 3),
            root_ended(stream, 3),
        ];
        let meta = encode_meta_segment(stream.0, 1, 7, &records).unwrap();
        assert_eq!(
            hex::encode(Sha256::digest(&meta)),
            "ccb40cc7dcccdd402a626c422db29fce4dbf36e789f839dcf492dc4309f57b4d"
        );

        let cct_only = DataGroup {
            root: thread(stream, 1),
            cct_health: CounterHealth::default(),
            cct_record_count: 1,
            cct: b"stand-in-cct-payload".to_vec(),
            evidence_record_count: 0,
            evidence: Vec::new(),
        };
        let evidence_only = DataGroup {
            root: thread(stream, 2),
            cct_health: CounterHealth::default(),
            cct_record_count: 0,
            cct: Vec::new(),
            evidence_record_count: 2,
            evidence: b"stand-in-evidence-payload".to_vec(),
        };
        let data = encode_data_segment(stream.0, 1, &[cct_only, evidence_only]);
        assert_eq!(
            hex::encode(Sha256::digest(&data)),
            "e67d72a47ffeb03c6f1f6a0be573a952fea0f0603a2c7c94b8f598195388e8bd"
        );
    }
}
