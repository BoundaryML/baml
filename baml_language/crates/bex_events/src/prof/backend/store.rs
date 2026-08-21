//! Project-local profiler store and its atomic publication seam.

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU8, Ordering},
    },
};

use fs2::FileExt;
use sha2::{Digest, Sha256};

use super::{
    BoundaryEndStatus, CctCodecError, CctSegmentData, CodecVersion, DiskBudget, EvidenceCodecError,
    EvidenceFact, SealedCctEpoch, ValueCid, decode_cct_payload, decode_evidence_payload,
    encode_cct_epoch, encode_evidence_facts,
};
use crate::ids::{BexThreadId, BoundaryId, EngineId, ProcessEuid, ProgramId, ThreadRef};

const USAGE_MAGIC: &[u8; 8] = b"BAMLUSE1";
const RUN_META_MAGIC: &[u8; 8] = b"BAMLRUN1";
const RUN_END_MAGIC: &[u8; 8] = b"BAMLEND1";
const CCT_MAGIC: &[u8; 8] = b"BAMLCCT1";
const EVIDENCE_MAGIC: &[u8; 8] = b"BAMLSPN1";
const VALUE_MAGIC: &[u8; 8] = b"BAMLVAL1";
const SCHEMA_VERSION: u16 = 1;
const USAGE_STATE_BYTES: u64 = 8 + 8 + 32;
const GATE_OPEN: u8 = 0;
const GATE_DISK: u8 = 1;
const GATE_UNAVAILABLE: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreFileKind {
    RunMeta,
    CctSegment,
    EvidenceSegment,
    CasObject,
    RunEnd,
}

/// The filesystem-dependent operations at the store's fault-injection seam.
pub trait StorePlatform: Send + Sync + 'static {
    fn available_space(&self, path: &Path) -> io::Result<u64>;
    fn sync_dir(&self, path: &Path) -> io::Result<()>;

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
    BoundaryFinished,
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
/// never inspected or removed.
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
        if error.kind() == io::ErrorKind::WouldBlock {
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoundaryRunMeta {
    pub boundary_id: BoundaryId,
    pub program_id: ProgramId,
    pub root_thread_ref: ThreadRef,
    pub revision_label: Option<String>,
    pub source_label: Option<String>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SegmentHighWater {
    pub last_sequence: u64,
    pub segment_count: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunEndSegmentFence {
    pub cct: SegmentHighWater,
    pub evidence: SegmentHighWater,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunEnd {
    pub status: BoundaryEndStatus,
    pub terminal_health: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedEvidenceSegment {
    pub sequence: u64,
    pub boundary_id: BoundaryId,
    pub program_id: ProgramId,
    pub revision_label: Option<String>,
    pub source_label: Option<String>,
    pub terminal_health: Vec<u8>,
    pub facts: Vec<EvidenceFact>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedCctSegment {
    pub sequence: u64,
    pub boundary_id: BoundaryId,
    pub program_id: ProgramId,
    pub revision_label: Option<String>,
    pub source_label: Option<String>,
    pub data: CctSegmentData,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedRunEnd {
    pub end: RunEnd,
    pub fence: RunEndSegmentFence,
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
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SegmentKind {
    Cct,
    Evidence,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct IndeterminateToken([u8; 16]);

#[derive(Debug)]
pub enum BeginBoundaryResult {
    Admitted(AdmittedBoundary),
    Rejected(StoreFailureReason),
    Indeterminate(IndeterminateToken),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PublishBatchResult {
    Committed { sequence: u64 },
    Lost(StoreFailureReason),
    Indeterminate(IndeterminateToken),
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
pub enum FinishBoundaryResult {
    Sealed,
    ReleasedIncomplete(StoreFailureReason),
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
    kind: StoreFileKind,
    final_directory: PathBuf,
}

#[derive(Debug)]
struct PublicationState {
    fence: RunEndSegmentFence,
    pending: Option<PendingPublication>,
    finished: bool,
}

#[derive(Clone, Copy, Debug)]
struct PendingPublication {
    token: IndeterminateToken,
    kind: PendingKind,
}

#[derive(Clone, Copy, Debug)]
enum PendingKind {
    Segment { kind: SegmentKind, sequence: u64 },
    Finish,
}

pub struct ProfilerStore {
    root: PathBuf,
    disk: DiskBudget,
    platform: Arc<dyn StorePlatform>,
    _store_lock: File,
    publish_lock: File,
    process_publish: Mutex<()>,
    indeterminate: Mutex<Option<IndeterminateState>>,
    admission_gate: AtomicU8,
}

pub struct AdmittedBoundary {
    store: Arc<ProfilerStore>,
    meta: BoundaryRunMeta,
    publication: Mutex<PublicationState>,
}

impl std::fmt::Debug for ProfilerStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ProfilerStore")
            .field("root", &self.root)
            .field("disk", &self.disk)
            .field(
                "admission_gate",
                &self.admission_gate.load(Ordering::Relaxed),
            )
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for AdmittedBoundary {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AdmittedBoundary")
            .field("meta", &self.meta)
            .finish_non_exhaustive()
    }
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

    pub fn open_native(root: PathBuf, disk: DiskBudget) -> Result<Arc<Self>, StoreOpenError> {
        Self::open(root, disk, Arc::new(NativeStorePlatform))
    }

    pub fn open(
        root: PathBuf,
        disk: DiskBudget,
        platform: Arc<dyn StorePlatform>,
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

        fs::create_dir_all(root.join("runs")).map_err(open_error)?;
        fs::create_dir_all(root.join("cas/sha256")).map_err(open_error)?;
        fs::create_dir_all(root.join("tmp")).map_err(open_error)?;
        let publish_lock = read_write_file(&root.join("publish.lock")).map_err(open_error)?;
        FileExt::lock_exclusive(&publish_lock).map_err(open_error)?;

        let physical_usage = scan_physical_usage(&root).map_err(open_error)?;
        write_usage_state(&root, physical_usage, platform.as_ref()).map_err(open_error)?;
        FileExt::unlock(&publish_lock).map_err(open_error)?;

        let gate = if physical_usage > disk.max_project_bytes
            || platform.available_space(&root).map_err(open_error)? < disk.minimum_free_bytes
        {
            GATE_DISK
        } else {
            GATE_OPEN
        };

        Ok(Arc::new(Self {
            root,
            disk,
            platform,
            _store_lock: store_lock,
            publish_lock,
            process_publish: Mutex::new(()),
            indeterminate: Mutex::new(None),
            admission_gate: AtomicU8::new(gate),
        }))
    }

    #[must_use]
    pub fn is_normal_admission_open(&self) -> bool {
        self.admission_gate.load(Ordering::Acquire) == GATE_OPEN
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
        let result = match self.publish_bytes(StoreFileKind::CasObject, &final_path, &bytes, false)
        {
            AtomicPublishResult::Committed => PublishCasResult::Published,
            AtomicPublishResult::Lost(StoreFailureReason::PathConflict) => {
                verify_cas_object(&final_path, cid, codec, encoded_body)
            }
            AtomicPublishResult::Lost(reason) => PublishCasResult::Lost(reason),
            AtomicPublishResult::Indeterminate(token) => PublishCasResult::Indeterminate(token),
        };
        (cid, result)
    }

    pub fn begin_boundary(self: &Arc<Self>, meta: BoundaryRunMeta) -> BeginBoundaryResult {
        // A post-rename `run.meta` ambiguity has no boundary publisher to
        // drive its retry. Resolve that store-owned state before admitting a
        // later root; segment, CAS, and terminal ambiguities remain owned by
        // their original caller/publisher and must not be stolen here.
        loop {
            let token = self
                .indeterminate
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .as_ref()
                .filter(|pending| pending.kind == StoreFileKind::RunMeta)
                .map(|pending| pending.token);
            let Some(token) = token else { break };
            match self.resolve_indeterminate(token) {
                ResolveIndeterminateResult::Committed
                | ResolveIndeterminateResult::TokenMismatch => {}
                ResolveIndeterminateResult::StillIndeterminate => {
                    return BeginBoundaryResult::Indeterminate(token);
                }
            }
        }
        if let Some(reason) = self.gate_reason() {
            return BeginBoundaryResult::Rejected(reason);
        }
        let bytes = match encode_run_meta(&meta) {
            Ok(bytes) => bytes,
            Err(reason) => return BeginBoundaryResult::Rejected(reason),
        };
        let run_directory = self.run_directory(meta.boundary_id);
        let final_path = run_directory.join("run.meta");
        match self.publish_bytes(StoreFileKind::RunMeta, &final_path, &bytes, false) {
            AtomicPublishResult::Committed => BeginBoundaryResult::Admitted(AdmittedBoundary {
                store: Arc::clone(self),
                meta,
                publication: Mutex::new(PublicationState {
                    fence: RunEndSegmentFence::default(),
                    pending: None,
                    finished: false,
                }),
            }),
            AtomicPublishResult::Lost(reason) => BeginBoundaryResult::Rejected(reason),
            AtomicPublishResult::Indeterminate(token) => BeginBoundaryResult::Indeterminate(token),
        }
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
        *state = None;
        if FileExt::unlock(&self.publish_lock).is_err() {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
        }
        ResolveIndeterminateResult::Committed
    }

    fn publish_bytes(
        &self,
        kind: StoreFileKind,
        final_path: &Path,
        bytes: &[u8],
        terminal: bool,
    ) -> AtomicPublishResult {
        if !terminal && let Some(reason) = self.gate_reason() {
            return AtomicPublishResult::Lost(reason);
        }
        let _process_guard = self
            .process_publish
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(pending) = self
            .indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            return AtomicPublishResult::Indeterminate(pending.token);
        }
        if !terminal && let Some(reason) = self.gate_reason() {
            return AtomicPublishResult::Lost(reason);
        }
        if FileExt::lock_exclusive(&self.publish_lock).is_err() {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable);
        }

        let result = self.publish_bytes_locked(kind, final_path, bytes);
        if !matches!(result, AtomicPublishResult::Indeterminate(_))
            && FileExt::unlock(&self.publish_lock).is_err()
        {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable);
        }
        result
    }

    fn publish_bytes_locked(
        &self,
        kind: StoreFileKind,
        final_path: &Path,
        bytes: &[u8],
    ) -> AtomicPublishResult {
        if final_path.exists() {
            if kind != StoreFileKind::CasObject {
                self.admission_gate
                    .store(GATE_UNAVAILABLE, Ordering::Release);
            }
            return AtomicPublishResult::Lost(StoreFailureReason::PathConflict);
        }
        let Ok(current_usage) = read_usage_state(&self.root) else {
            self.admission_gate
                .store(GATE_UNAVAILABLE, Ordering::Release);
            return AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable);
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
            return AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable);
        };
        if peak_usage > self.disk.max_project_bytes
            || free.saturating_sub(bytes_len.saturating_add(USAGE_STATE_BYTES))
                < self.disk.minimum_free_bytes
        {
            return self.disk_guard_failure();
        }

        let Some(final_directory) = final_path.parent() else {
            return AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable);
        };
        if let Err(error) = fs::create_dir_all(final_directory) {
            return self.io_failure(&error);
        }
        let temporary = self
            .root
            .join("tmp")
            .join(format!("{}.pending", uuid::Uuid::new_v4().simple()));
        let write_result = write_synced_file(&temporary, bytes)
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
            return AtomicPublishResult::Lost(StoreFailureReason::PathConflict);
        }
        if let Err(error) = fs::rename(&temporary, final_path) {
            self.account_pre_rename_orphan(&temporary, current_usage);
            return self.io_failure(&error);
        }

        if self.platform.sync_dir(final_directory).is_err() {
            return self.retain_indeterminate(kind, final_directory);
        }
        let new_usage = current_usage + bytes_len;
        if write_usage_state(&self.root, new_usage, self.platform.as_ref()).is_err() {
            return self.retain_indeterminate(kind, final_directory);
        }
        AtomicPublishResult::Committed
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
        kind: StoreFileKind,
        final_directory: &Path,
    ) -> AtomicPublishResult {
        let token = IndeterminateToken(*uuid::Uuid::new_v4().as_bytes());
        *self
            .indeterminate
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(IndeterminateState {
            token,
            kind,
            final_directory: final_directory.to_owned(),
        });
        AtomicPublishResult::Indeterminate(token)
    }

    fn io_failure(&self, error: &io::Error) -> AtomicPublishResult {
        if is_out_of_space(error) {
            self.disk_guard_failure()
        } else if error.kind() == io::ErrorKind::PermissionDenied {
            AtomicPublishResult::Lost(StoreFailureReason::PermissionDenied)
        } else {
            AtomicPublishResult::Lost(StoreFailureReason::StoreUnavailable)
        }
    }

    fn disk_guard_failure(&self) -> AtomicPublishResult {
        self.admission_gate.store(GATE_DISK, Ordering::Release);
        AtomicPublishResult::Lost(StoreFailureReason::DiskGuardExceeded)
    }

    fn gate_reason(&self) -> Option<StoreFailureReason> {
        match self.admission_gate.load(Ordering::Acquire) {
            GATE_OPEN => None,
            GATE_DISK => Some(StoreFailureReason::DiskGuardExceeded),
            _ => Some(StoreFailureReason::StoreUnavailable),
        }
    }

    fn run_directory(&self, boundary_id: BoundaryId) -> PathBuf {
        self.root
            .join("runs")
            .join(hex::encode(boundary_id.as_bytes()))
    }
}

impl AdmittedBoundary {
    #[must_use]
    pub fn meta(&self) -> &BoundaryRunMeta {
        &self.meta
    }

    #[must_use]
    pub fn fence(&self) -> RunEndSegmentFence {
        self.publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .fence
    }

    pub fn reserve_and_publish(
        &self,
        kind: SegmentKind,
        record_count: u64,
        terminal_health: &[u8],
        payload: &[u8],
    ) -> PublishBatchResult {
        let mut state = self
            .publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.finished {
            return PublishBatchResult::Lost(StoreFailureReason::BoundaryFinished);
        }
        if let Some(pending) = state.pending {
            return PublishBatchResult::Indeterminate(pending.token);
        }
        let high_water = high_water_mut(&mut state.fence, kind);
        let Some(sequence) = high_water.last_sequence.checked_add(1) else {
            return PublishBatchResult::Lost(StoreFailureReason::SequenceExhausted);
        };
        let bytes = match encode_segment(
            &self.meta,
            kind,
            sequence,
            record_count,
            terminal_health,
            payload,
        ) {
            Ok(bytes) => bytes,
            Err(reason) => return PublishBatchResult::Lost(reason),
        };
        let extension = match kind {
            SegmentKind::Cct => "bamlcct",
            SegmentKind::Evidence => "bamlspans",
        };
        let file_kind = match kind {
            SegmentKind::Cct => StoreFileKind::CctSegment,
            SegmentKind::Evidence => StoreFileKind::EvidenceSegment,
        };
        let final_path = self
            .store
            .run_directory(self.meta.boundary_id)
            .join(match kind {
                SegmentKind::Cct => "cct",
                SegmentKind::Evidence => "evidence",
            })
            .join(format!("{sequence:020}.{extension}"));
        match self
            .store
            .publish_bytes(file_kind, &final_path, &bytes, false)
        {
            AtomicPublishResult::Committed => {
                high_water.last_sequence = sequence;
                high_water.segment_count = sequence;
                PublishBatchResult::Committed { sequence }
            }
            AtomicPublishResult::Lost(reason) => PublishBatchResult::Lost(reason),
            AtomicPublishResult::Indeterminate(token) => {
                state.pending = Some(PendingPublication {
                    token,
                    kind: PendingKind::Segment { kind, sequence },
                });
                PublishBatchResult::Indeterminate(token)
            }
        }
    }

    pub fn publish_cct_epoch(&self, epoch: &SealedCctEpoch) -> PublishBatchResult {
        let encoded = encode_cct_epoch(epoch);
        self.reserve_and_publish(
            SegmentKind::Cct,
            encoded.record_count,
            &encoded.terminal_health,
            &encoded.payload,
        )
    }

    pub fn publish_evidence_facts(&self, facts: &[EvidenceFact]) -> PublishBatchResult {
        let encoded = encode_evidence_facts(facts);
        self.reserve_and_publish(
            SegmentKind::Evidence,
            encoded.record_count,
            &[],
            &encoded.payload,
        )
    }

    pub fn finish_boundary(&self, end: &RunEnd) -> FinishBoundaryResult {
        let mut state = self
            .publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.finished {
            return FinishBoundaryResult::ReleasedIncomplete(StoreFailureReason::BoundaryFinished);
        }
        if let Some(pending) = state.pending {
            return FinishBoundaryResult::Indeterminate(pending.token);
        }
        let bytes = match encode_run_end(end, state.fence) {
            Ok(bytes) => bytes,
            Err(reason) => {
                state.finished = true;
                return FinishBoundaryResult::ReleasedIncomplete(reason);
            }
        };
        let final_path = self
            .store
            .run_directory(self.meta.boundary_id)
            .join("run.end");
        match self
            .store
            .publish_bytes(StoreFileKind::RunEnd, &final_path, &bytes, true)
        {
            AtomicPublishResult::Committed => {
                state.finished = true;
                FinishBoundaryResult::Sealed
            }
            AtomicPublishResult::Lost(reason) => {
                state.finished = true;
                FinishBoundaryResult::ReleasedIncomplete(reason)
            }
            AtomicPublishResult::Indeterminate(token) => {
                state.pending = Some(PendingPublication {
                    token,
                    kind: PendingKind::Finish,
                });
                FinishBoundaryResult::Indeterminate(token)
            }
        }
    }

    pub fn resolve_pending(&self) -> ResolveIndeterminateResult {
        let mut state = self
            .publication
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let Some(pending) = state.pending else {
            return ResolveIndeterminateResult::TokenMismatch;
        };
        let result = self.store.resolve_indeterminate(pending.token);
        if result != ResolveIndeterminateResult::Committed {
            return result;
        }
        match pending.kind {
            PendingKind::Segment { kind, sequence } => {
                let high_water = high_water_mut(&mut state.fence, kind);
                high_water.last_sequence = sequence;
                high_water.segment_count = sequence;
            }
            PendingKind::Finish => state.finished = true,
        }
        state.pending = None;
        ResolveIndeterminateResult::Committed
    }
}

#[derive(Clone, Copy, Debug)]
enum AtomicPublishResult {
    Committed,
    Lost(StoreFailureReason),
    Indeterminate(IndeterminateToken),
}

fn high_water_mut(fence: &mut RunEndSegmentFence, kind: SegmentKind) -> &mut SegmentHighWater {
    match kind {
        SegmentKind::Cct => &mut fence.cct,
        SegmentKind::Evidence => &mut fence.evidence,
    }
}

fn encode_run_meta(meta: &BoundaryRunMeta) -> Result<Vec<u8>, StoreFailureReason> {
    let mut body = Vec::with_capacity(160);
    body.extend_from_slice(RUN_META_MAGIC);
    body.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    body.extend_from_slice(&meta.boundary_id.as_bytes());
    body.extend_from_slice(&meta.program_id.0);
    encode_thread_ref(&mut body, meta.root_thread_ref);
    encode_optional_string(&mut body, meta.revision_label.as_deref())?;
    encode_optional_string(&mut body, meta.source_label.as_deref())?;
    Ok(with_checksum(body))
}

fn encode_segment(
    meta: &BoundaryRunMeta,
    kind: SegmentKind,
    sequence: u64,
    record_count: u64,
    terminal_health: &[u8],
    payload: &[u8],
) -> Result<Vec<u8>, StoreFailureReason> {
    let mut body = Vec::with_capacity(
        192usize
            .saturating_add(terminal_health.len())
            .saturating_add(payload.len()),
    );
    body.extend_from_slice(match kind {
        SegmentKind::Cct => CCT_MAGIC,
        SegmentKind::Evidence => EVIDENCE_MAGIC,
    });
    body.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    body.push(match kind {
        SegmentKind::Cct => 0,
        SegmentKind::Evidence => 1,
    });
    body.extend_from_slice(&sequence.to_be_bytes());
    body.extend_from_slice(&meta.boundary_id.as_bytes());
    body.extend_from_slice(&meta.program_id.0);
    encode_optional_string(&mut body, meta.revision_label.as_deref())?;
    encode_optional_string(&mut body, meta.source_label.as_deref())?;
    body.extend_from_slice(&record_count.to_be_bytes());
    body.extend_from_slice(
        &u64::try_from(payload.len())
            .map_err(|_| StoreFailureReason::StoreUnavailable)?
            .to_be_bytes(),
    );
    body.extend_from_slice(
        &u32::try_from(terminal_health.len())
            .map_err(|_| StoreFailureReason::StoreUnavailable)?
            .to_be_bytes(),
    );
    body.extend_from_slice(terminal_health);
    body.extend_from_slice(payload);
    Ok(with_checksum(body))
}

pub fn decode_evidence_segment(bytes: &[u8]) -> Result<DecodedEvidenceSegment, SegmentReadError> {
    let checksum_start = bytes
        .len()
        .checked_sub(32)
        .ok_or(SegmentReadError::Truncated)?;
    let expected: [u8; 32] = Sha256::digest(&bytes[..checksum_start]).into();
    if bytes[checksum_start..] != expected {
        return Err(SegmentReadError::InvalidChecksum);
    }
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != EVIDENCE_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    let version = cursor.u16()?;
    if version != SCHEMA_VERSION {
        return Err(SegmentReadError::UnsupportedVersion(version));
    }
    if cursor.u8()? != SegmentKind::Evidence as u8 {
        return Err(SegmentReadError::InvalidKind);
    }
    let sequence = cursor.u64()?;
    let boundary_id = BoundaryId::from_bytes(cursor.array::<16>()?);
    let program_id = ProgramId(cursor.array::<16>()?);
    let revision_label = cursor.optional_string()?;
    let source_label = cursor.optional_string()?;
    let record_count = cursor.u64()?;
    let payload_len = cursor.usize_u64()?;
    let health_len = usize::try_from(cursor.u32()?).map_err(|_| SegmentReadError::Truncated)?;
    let terminal_health = cursor.take(health_len)?.to_vec();
    let payload = cursor.take(payload_len)?;
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    let facts = decode_evidence_payload(payload, record_count)
        .map_err(SegmentReadError::InvalidEvidence)?;
    Ok(DecodedEvidenceSegment {
        sequence,
        boundary_id,
        program_id,
        revision_label,
        source_label,
        terminal_health,
        facts,
    })
}

pub(crate) fn decode_cct_segment(bytes: &[u8]) -> Result<DecodedCctSegment, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != CCT_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
    if cursor.u8()? != SegmentKind::Cct as u8 {
        return Err(SegmentReadError::InvalidKind);
    }
    let sequence = cursor.u64()?;
    let boundary_id = BoundaryId::from_bytes(cursor.array::<16>()?);
    let program_id = ProgramId(cursor.array::<16>()?);
    let revision_label = cursor.optional_string()?;
    let source_label = cursor.optional_string()?;
    let record_count = cursor.u64()?;
    let payload_len = cursor.usize_u64()?;
    let health_len = usize::try_from(cursor.u32()?).map_err(|_| SegmentReadError::Truncated)?;
    let terminal_health = cursor.take(health_len)?;
    let payload = cursor.take(payload_len)?;
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    let data = decode_cct_payload(payload, record_count, terminal_health)
        .map_err(SegmentReadError::InvalidCct)?;
    Ok(DecodedCctSegment {
        sequence,
        boundary_id,
        program_id,
        revision_label,
        source_label,
        data,
    })
}

pub(crate) fn decode_run_meta(bytes: &[u8]) -> Result<BoundaryRunMeta, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != RUN_META_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
    let boundary_id = BoundaryId::from_bytes(cursor.array::<16>()?);
    let program_id = ProgramId(cursor.array::<16>()?);
    let root_thread_ref = ThreadRef {
        process_euid: ProcessEuid(cursor.array::<16>()?),
        engine_id: EngineId(cursor.u64()?),
        thread_id: BexThreadId(cursor.u64()?),
    };
    let revision_label = cursor.optional_string()?;
    let source_label = cursor.optional_string()?;
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    Ok(BoundaryRunMeta {
        boundary_id,
        program_id,
        root_thread_ref,
        revision_label,
        source_label,
    })
}

pub(crate) fn decode_run_end(bytes: &[u8]) -> Result<DecodedRunEnd, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != RUN_END_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
    let status = match cursor.u8()? {
        0 => BoundaryEndStatus::Succeeded,
        1 => BoundaryEndStatus::Failed,
        2 => BoundaryEndStatus::Cancelled,
        3 => BoundaryEndStatus::Panicked,
        4 => BoundaryEndStatus::Abandoned,
        _ => return Err(SegmentReadError::InvalidKind),
    };
    let mut high_water = || -> Result<SegmentHighWater, SegmentReadError> {
        Ok(SegmentHighWater {
            last_sequence: cursor.u64()?,
            segment_count: cursor.u64()?,
        })
    };
    let fence = RunEndSegmentFence {
        cct: high_water()?,
        evidence: high_water()?,
    };
    let health_len = usize::try_from(cursor.u32()?).map_err(|_| SegmentReadError::Truncated)?;
    let terminal_health = cursor.take(health_len)?.to_vec();
    if !cursor.is_empty() {
        return Err(SegmentReadError::TrailingBytes);
    }
    Ok(DecodedRunEnd {
        end: RunEnd {
            status,
            terminal_health,
        },
        fence,
    })
}

pub(crate) fn decode_cas_object(bytes: &[u8]) -> Result<DecodedCasObject, SegmentReadError> {
    let checksum_start = validate_checksum(bytes)?;
    let mut cursor = SegmentCursor::new(&bytes[..checksum_start]);
    if cursor.take(8)? != VALUE_MAGIC {
        return Err(SegmentReadError::InvalidMagic);
    }
    validate_version(&mut cursor)?;
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

fn validate_checksum(bytes: &[u8]) -> Result<usize, SegmentReadError> {
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

    fn optional_string(&mut self) -> Result<Option<String>, SegmentReadError> {
        match self.u8()? {
            0 => Ok(None),
            1 => {
                let length =
                    usize::try_from(self.u32()?).map_err(|_| SegmentReadError::Truncated)?;
                let value = std::str::from_utf8(self.take(length)?)
                    .map_err(|_| SegmentReadError::InvalidUtf8)?;
                Ok(Some(value.to_owned()))
            }
            _ => Err(SegmentReadError::InvalidKind),
        }
    }

    fn is_empty(&self) -> bool {
        self.offset == self.bytes.len()
    }
}

fn encode_run_end(end: &RunEnd, fence: RunEndSegmentFence) -> Result<Vec<u8>, StoreFailureReason> {
    let mut body = Vec::with_capacity(128usize.saturating_add(end.terminal_health.len()));
    body.extend_from_slice(RUN_END_MAGIC);
    body.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
    body.push(end.status as u8);
    for high_water in [fence.cct, fence.evidence] {
        body.extend_from_slice(&high_water.last_sequence.to_be_bytes());
        body.extend_from_slice(&high_water.segment_count.to_be_bytes());
    }
    body.extend_from_slice(
        &u32::try_from(end.terminal_health.len())
            .map_err(|_| StoreFailureReason::StoreUnavailable)?
            .to_be_bytes(),
    );
    body.extend_from_slice(&end.terminal_health);
    Ok(with_checksum(body))
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
    bytes.extend_from_slice(&SCHEMA_VERSION.to_be_bytes());
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
        || bytes[8..10] != SCHEMA_VERSION.to_be_bytes()
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

fn encode_thread_ref(output: &mut Vec<u8>, thread: ThreadRef) {
    output.extend_from_slice(&thread.process_euid.0);
    output.extend_from_slice(&thread.engine_id.0.to_be_bytes());
    output.extend_from_slice(&thread.thread_id.0.to_be_bytes());
}

fn encode_optional_string(
    output: &mut Vec<u8>,
    value: Option<&str>,
) -> Result<(), StoreFailureReason> {
    match value {
        None => output.push(0),
        Some(value) => {
            output.push(1);
            output.extend_from_slice(
                &u32::try_from(value.len())
                    .map_err(|_| StoreFailureReason::StoreUnavailable)?
                    .to_be_bytes(),
            );
            output.extend_from_slice(value.as_bytes());
        }
    }
    Ok(())
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

fn write_synced_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    file.write_all(bytes)?;
    file.flush()?;
    file.sync_all()
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
    file.sync_all()?;
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

#[cfg(windows)]
fn sync_directory(path: &Path) -> io::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?
        .sync_all()
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, AtomicU64};

    use tempfile::TempDir;

    use super::*;
    use crate::ids::{BexThreadId, EngineId, ProcessEuid};

    #[test]
    fn store_open_writes_idempotent_baml_gitignore() {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join("project/.baml/profiles-v1");
        let ignore = temp.path().join("project/.baml/.gitignore");
        let disk = DiskBudget {
            max_project_bytes: 16 * 1024 * 1024,
            minimum_free_bytes: 0,
        };

        let store = ProfilerStore::open_native(root.clone(), disk).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"*\n");
        drop(store);

        // Re-opening never duplicates the marker.
        let store = ProfilerStore::open_native(root.clone(), disk).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"*\n");
        drop(store);

        // Existing user rules are preserved and the marker is appended once.
        fs::write(&ignore, b"# keep this\n!.keep").unwrap();
        let store = ProfilerStore::open_native(root.clone(), disk).unwrap();
        assert_eq!(fs::read(&ignore).unwrap(), b"# keep this\n!.keep\n*\n");
        drop(store);
        let _ = ProfilerStore::open_native(root, disk).unwrap();
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

    fn meta(byte: u8) -> BoundaryRunMeta {
        BoundaryRunMeta {
            boundary_id: BoundaryId::from_bytes([byte; 16]),
            program_id: ProgramId([byte.wrapping_add(1); 16]),
            root_thread_ref: ThreadRef {
                process_euid: ProcessEuid([byte.wrapping_add(2); 16]),
                engine_id: EngineId(3),
                thread_id: BexThreadId(4),
            },
            revision_label: Some("revision".to_string()),
            source_label: None,
        }
    }

    fn store(temp: &TempDir, platform: Arc<TestPlatform>, max_bytes: u64) -> Arc<ProfilerStore> {
        ProfilerStore::open(
            temp.path().join(".baml/profiles-v1"),
            DiskBudget {
                max_project_bytes: max_bytes,
                minimum_free_bytes: 100,
            },
            platform,
        )
        .unwrap()
    }

    fn admitted(store: &Arc<ProfilerStore>, byte: u8) -> AdmittedBoundary {
        match store.begin_boundary(meta(byte)) {
            BeginBoundaryResult::Admitted(boundary) => boundary,
            other => panic!("expected admission, got {other:?}"),
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
        let store = store(&temp, Arc::new(TestPlatform::new(u64::MAX)), 1024 * 1024);
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
    fn metadata_segments_and_terminal_are_atomic_and_contiguous() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp, Arc::new(TestPlatform::new(u64::MAX)), 1024 * 1024);
        let boundary = admitted(&store, 1);
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Cct, 2, b"health", b"cct"),
            PublishBatchResult::Committed { sequence: 1 }
        );
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Cct, 1, b"", b"more"),
            PublishBatchResult::Committed { sequence: 2 }
        );
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Evidence, 1, b"", b"span"),
            PublishBatchResult::Committed { sequence: 1 }
        );
        assert_eq!(
            boundary.finish_boundary(&RunEnd {
                status: BoundaryEndStatus::Succeeded,
                terminal_health: Vec::new(),
            }),
            FinishBoundaryResult::Sealed
        );
        assert_eq!(
            boundary.fence(),
            RunEndSegmentFence {
                cct: SegmentHighWater {
                    last_sequence: 2,
                    segment_count: 2,
                },
                evidence: SegmentHighWater {
                    last_sequence: 1,
                    segment_count: 1,
                },
            }
        );
        let run = store.run_directory(meta(1).boundary_id);
        assert!(run.join("run.meta").is_file());
        assert!(run.join("cct/00000000000000000001.bamlcct").is_file());
        assert!(run.join("cct/00000000000000000002.bamlcct").is_file());
        assert!(
            run.join("evidence/00000000000000000001.bamlspans")
                .is_file()
        );
        assert!(run.join("run.end").is_file());
    }

    #[test]
    fn cas_miss_hit_conflict_and_indeterminate_resolution_are_explicit() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), 1024 * 1024);
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
        assert_eq!(
            store.resolve_indeterminate(token),
            ResolveIndeterminateResult::Committed
        );
        assert_eq!(
            store.publish_cas_object(codec, second_body),
            (second_cid, PublishCasResult::Reused)
        );
    }

    #[test]
    fn cas_publish_race_does_not_close_store_admission() {
        let temp = TempDir::new().unwrap();
        let store = store(&temp, Arc::new(TestPlatform::new(u64::MAX)), 1024 * 1024);
        let codec = CodecVersion(1);
        let body = b"shared-value";
        let (cid, result) = store.publish_cas_object(codec, body);
        assert_eq!(result, PublishCasResult::Published);

        // This is the locked half of the race where another process creates
        // the CID path after our optimistic existence check.
        let digest = hex::encode(cid.0);
        let path = store
            .root
            .join("cas/sha256")
            .join(&digest[..2])
            .join(format!("{digest}.bamlvalue"));
        assert!(matches!(
            store.publish_bytes(StoreFileKind::CasObject, &path, b"discarded", false),
            AtomicPublishResult::Lost(StoreFailureReason::PathConflict)
        ));
        assert!(store.is_normal_admission_open());
        assert!(matches!(
            store.begin_boundary(meta(12)),
            BeginBoundaryResult::Admitted(_)
        ));
    }

    #[test]
    fn later_boundary_resolves_store_owned_metadata_ambiguity() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), 1024 * 1024);
        platform.fail_next_dir_sync.store(true, Ordering::Relaxed);
        assert!(matches!(
            store.begin_boundary(meta(13)),
            BeginBoundaryResult::Indeterminate(_)
        ));

        assert!(matches!(
            store.begin_boundary(meta(14)),
            BeginBoundaryResult::Admitted(_)
        ));
        assert!(
            store
                .run_directory(meta(13).boundary_id)
                .join("run.meta")
                .is_file()
        );
        assert!(
            store
                .run_directory(meta(14).boundary_id)
                .join("run.meta")
                .is_file()
        );
    }

    #[test]
    fn exact_disk_guard_latches_normal_admission_but_terminal_gets_one_attempt() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), 1024 * 1024);
        let boundary = admitted(&store, 2);
        platform.free.store(100, Ordering::Relaxed);
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Cct, 1, b"", b"x"),
            PublishBatchResult::Lost(StoreFailureReason::DiskGuardExceeded)
        );
        assert!(!store.is_normal_admission_open());
        assert!(matches!(
            store.begin_boundary(meta(3)),
            BeginBoundaryResult::Rejected(StoreFailureReason::DiskGuardExceeded)
        ));
        assert_eq!(
            boundary.finish_boundary(&RunEnd {
                status: BoundaryEndStatus::Succeeded,
                terminal_health: Vec::new(),
            }),
            FinishBoundaryResult::ReleasedIncomplete(StoreFailureReason::DiskGuardExceeded)
        );
    }

    #[test]
    fn pre_rename_loss_does_not_consume_a_sequence() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), 1024 * 1024);
        let boundary = admitted(&store, 4);
        platform.fail_next_rename.store(true, Ordering::Relaxed);
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Cct, 1, b"", b"lost"),
            PublishBatchResult::Lost(StoreFailureReason::StoreUnavailable)
        );
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Cct, 1, b"", b"committed"),
            PublishBatchResult::Committed { sequence: 1 }
        );
    }

    #[test]
    fn renamed_dir_sync_failure_blocks_publication_until_exact_resolution() {
        let temp = TempDir::new().unwrap();
        let platform = Arc::new(TestPlatform::new(u64::MAX));
        let store = store(&temp, Arc::clone(&platform), 1024 * 1024);
        let boundary = admitted(&store, 5);
        platform.fail_next_dir_sync.store(true, Ordering::Relaxed);
        let PublishBatchResult::Indeterminate(token) =
            boundary.reserve_and_publish(SegmentKind::Cct, 1, b"", b"visible")
        else {
            panic!("expected indeterminate publication")
        };
        assert_eq!(boundary.fence().cct.last_sequence, 0);
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Evidence, 1, b"", b"blocked"),
            PublishBatchResult::Indeterminate(token)
        );
        assert_eq!(
            boundary.resolve_pending(),
            ResolveIndeterminateResult::Committed
        );
        assert_eq!(boundary.fence().cct.last_sequence, 1);
        assert_eq!(
            boundary.reserve_and_publish(SegmentKind::Evidence, 1, b"", b"after"),
            PublishBatchResult::Committed { sequence: 1 }
        );
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
                platform,
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
        )
        .unwrap();
        assert!(root.join("tmp/orphan").is_file());
        assert_eq!(read_usage_state(&root).unwrap(), USAGE_STATE_BYTES + 6);
        drop(reopened);
    }
}
