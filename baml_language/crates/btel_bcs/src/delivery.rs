//! Bounded, process-local delivery. There is no durable spool or URL renewal.
use std::{
    fmt,
    io::{self, Write},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use btel_recorder::SealedFile;
use btel_snapshot::{BlobIndex, BlobScratch, CasId, Leaf, Snapshot, Split, Structure};
use bytes::Bytes;
use futures::FutureExt;
use reqwest::{Client, StatusCode, Url};
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, mpsc, watch};

use crate::{
    body::{self, Body},
    liveness::{Heartbeat, HeartbeatError},
    plan::{validate_proposal, validate_response, validate_url},
    wire::{
        CasCandidate, CasSizeClass, Disposition, PrepareUploadsRequest, PrepareUploadsResponse,
        ProducerState, ProposedUploadTarget, RecordingDescriptor, UploadKind, UploadTarget,
    },
};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum DeliveryError {
    InvalidConfig,
    Capacity,
    Closed,
    InvalidPlan,
    Expired,
    Encoding,
    Http,
    Unauthorized,
    InitialAuthorization(bcs_api::HttpFailure),
    InitialConnection(Box<DeliveryError>),
    Api(bcs_api::HttpFailure),
    Worker,
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Self::InitialConnection(error) = self {
            return write!(f, "initial cloud telemetry connection failed: {error}");
        }
        if let Self::InitialAuthorization(failure) = self {
            return write!(
                f,
                "initial cloud telemetry authorization failed: {failure}; check Boundary credentials and project permissions"
            );
        }
        if let Self::Api(failure) = self {
            return write!(f, "cloud recording delivery: {failure}");
        }
        write!(f, "cloud recording delivery: {self:?}")
    }
}
impl std::error::Error for DeliveryError {}

/// Limits reserve retained owners and both sides of serialization before admission.
/// No capture or blob is refused for its size: a plan that needs more than the
/// whole CAS reservation is admitted beside it, one such plan at a time, and
/// holds nothing else up. HTTP is opt-in for local tests; production
/// endpoints must use HTTPS.
#[derive(Clone)]
pub struct DeliveryConfig {
    pub prepare_base_url: Url,
    pub bearer_token: Option<String>,
    pub authorization: Option<bcs_api::credentials::RequestAuthorization>,
    /// Start ingestion authorization immediately, before any recording batch is sealed.
    pub initial_recording_id: Option<btel_types::RecordingId>,
    /// Artifact failure policy applies after bounded initial connection retries.
    pub terminal_initial_connection: bool,
    pub max_pending_plans: usize,
    /// At most 32 captures, leaving default snapshot-pool headroom for VM
    /// capture. A capture spanning plans counts once per plan.
    pub max_pending_snapshots: usize,
    pub recording_reserved_bytes: usize,
    /// Memory for what the admitted plans hold: their captures until their
    /// bodies are built, and each CAS body until it is uploaded.
    pub cas_reserved_bytes: usize,
    /// CAS bodies uploaded at once, so that one long upload does not hold
    /// the others back. Recordings upload one at a time.
    pub max_cas_uploads: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    /// Blobs per plan.
    pub max_candidates: usize,
    pub max_targets: usize,
    pub max_recording_body_bytes: usize,
    /// Time for one request, before its body's share: see
    /// [`Self::min_upload_bytes_per_second`].
    pub request_timeout: Duration,
    /// The slowest upload a PUT waits for. A PUT is given `request_timeout`
    /// plus its body's length at this rate, so a large body is not lost for
    /// taking long, only for stalling. Upload URLs must live as long.
    pub min_upload_bytes_per_second: usize,
    pub retry_delay: Duration,
    /// Initial attempt plus retries. Nonretryable HTTP and invalid plans drop immediately.
    pub max_attempts: u32,
}

impl DeliveryConfig {
    /// Configure a parsed BCS endpoint with default delivery limits.
    pub fn new(prepare_base_url: Url) -> Self {
        Self {
            prepare_base_url,
            bearer_token: None,
            authorization: None,
            initial_recording_id: None,
            terminal_initial_connection: false,
            max_pending_plans: 4,
            max_pending_snapshots: 32,
            recording_reserved_bytes: 128 * 1024 * 1024,
            cas_reserved_bytes: 256 * 1024 * 1024,
            max_cas_uploads: 2,
            max_request_bytes: 64 * 1024,
            max_response_bytes: 64 * 1024,
            max_candidates: 32,
            max_targets: 33,
            max_recording_body_bytes: 8 * 1024 * 1024,
            request_timeout: Duration::from_secs(10),
            min_upload_bytes_per_second: 1024 * 1024,
            retry_delay: Duration::from_millis(200),
            max_attempts: 4,
        }
    }
    fn endpoint(&self, recording_id: &str, operation: &str) -> Result<Url, DeliveryError> {
        let mut url = self.prepare_base_url.clone();
        url.path_segments_mut()
            .map_err(|()| DeliveryError::InvalidConfig)?
            .pop_if_empty()
            .extend(["v1", "recordings", recording_id, operation]);
        Ok(url)
    }

    /// The time one PUT of `body_bytes` is given.
    fn put_timeout(&self, body_bytes: usize) -> Duration {
        let millis = (body_bytes as u128).saturating_mul(1000)
            / (self.min_upload_bytes_per_second as u128).max(1);
        self.request_timeout.saturating_add(Duration::from_millis(
            u64::try_from(millis).unwrap_or(u64::MAX),
        ))
    }

    /// The longest a body of `body_bytes` holds its lane: every attempt at
    /// its full timeout, and the delays between them.
    fn put_window(&self, body_bytes: usize) -> Option<Duration> {
        self.put_timeout(body_bytes)
            .checked_mul(self.max_attempts)
            .and_then(|v| {
                self.retry_delay
                    .checked_mul(self.max_attempts.checked_sub(1)?)
                    .and_then(|delay| v.checked_add(delay))
            })
    }

    /// The window of a request without a body to speak of.
    fn retry_window(&self) -> Result<Duration, DeliveryError> {
        self.put_window(0).ok_or(DeliveryError::InvalidConfig)
    }

    fn validate(&self) -> Result<(), DeliveryError> {
        let base = &self.prepare_base_url;
        validate_url(base, true).map_err(|_| DeliveryError::InvalidConfig)?;
        if base.query().is_some() || base.as_str().len() > 8192 {
            return Err(DeliveryError::InvalidConfig);
        }
        if let Some(authorization) = &self.authorization {
            if !authorization
                .authentication
                .accepts_url(&self.endpoint("0", "heartbeat")?)
            {
                return Err(DeliveryError::InvalidConfig);
            }
            reqwest::header::HeaderValue::from_str(&format!(
                "Bearer {}",
                authorization.authentication.bearer().expose()
            ))
            .map_err(|_| DeliveryError::InvalidConfig)?;
        }
        if let Some(token) = &self.bearer_token {
            reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| DeliveryError::InvalidConfig)?;
        }
        if self.max_attempts == 0
            || self.request_timeout.is_zero()
            || self.min_upload_bytes_per_second == 0
            || self.max_pending_plans == 0
            || self.max_pending_plans > 32
            || self.max_pending_snapshots == 0
            || self.max_pending_snapshots > 32
            || self.max_cas_uploads == 0
            || self.max_cas_uploads > 32
            || self.max_candidates == 0
            || self.max_targets == 0
            || self.max_targets > self.max_candidates + 1
            || self.max_request_bytes == 0
            || self.max_response_bytes == 0
            || self.max_recording_body_bytes == 0
        {
            return Err(DeliveryError::InvalidConfig);
        }
        self.retry_window()?;
        Ok(())
    }
}

#[derive(Default)]
struct State {
    sender: Option<mpsc::Sender<Work>>,
    failure: Option<DeliveryError>,
    last_error: Option<DeliveryError>,
    loss_count: u64,
    /// Losses that may have dropped a definition group a capture carried.
    carried_resets: u64,
    metadata_replay_evictions: u64,
    progress: DeliveryProgress,
    recording_bytes: usize,
    cas: Budget,
    snapshots: usize,
    /// Plans whose recording is not yet uploaded.
    plans: usize,
    /// The longest the admitted, unfinished bodies can keep a recording, or
    /// a CAS body, waiting for its turn to upload.
    recording_window: Duration,
    cas_window: Duration,
    last_file: Option<([u8; 16], u64)>,
    finished: bool,
    authorization: InitialAuthorization,
}

/// The recording's ingestion admission state, independent of the shared credential cache.
/// Successful query authentication does not establish permission to ingest this recording.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum InitialAuthorization {
    #[default]
    NotRequested,
    Pending,
    Authorized,
    Rejected(bcs_api::HttpFailure),
    Failed(DeliveryError),
}

/// A byte budget that refuses nothing. Plans that fit are admitted together;
/// one whose parts total more than the whole budget is admitted beside it,
/// one such plan at a time. So the most held is the budget and one plan.
#[derive(Default)]
struct Budget {
    used: usize,
    /// Parts still held of the plan admitted beside the budget.
    oversize: usize,
}

/// One part of what a plan was admitted with, given back when the part ends.
#[derive(Clone, Copy)]
enum Share {
    Within(usize),
    Oversize,
}

impl Budget {
    /// Whether a plan whose parts total `total` may be admitted now.
    fn admits(&self, total: usize, limit: usize) -> bool {
        if total > limit {
            self.oversize == 0
        } else {
            self.used
                .checked_add(total)
                .is_some_and(|used| used <= limit)
        }
    }
    /// A part of `bytes` of an admitted plan whose parts total `total`.
    fn take(&mut self, bytes: usize, total: usize, limit: usize) -> Share {
        if total > limit {
            self.oversize += 1;
            Share::Oversize
        } else {
            self.used += bytes;
            Share::Within(bytes)
        }
    }
    fn give(&mut self, share: Share) {
        match share {
            Share::Within(bytes) => self.used -= bytes,
            Share::Oversize => self.oversize -= 1,
        }
    }
}

#[derive(Default)]
pub(crate) struct DeliveryProgress {
    pub reset_cas: bool,
    pub recording_acked_through: u64,
}

struct Shared {
    state: Mutex<State>,
    capacity: Condvar,
    heartbeat: Heartbeat,
    config: DeliveryConfig,
    cancel: watch::Sender<bool>,
    on_failure: Box<dyn Fn(DeliveryError) + Send + Sync>,
}

impl Shared {
    fn authorized(&self) {
        let mut state = self.state.lock().unwrap();
        if state.authorization == InitialAuthorization::Pending {
            state.authorization = InitialAuthorization::Authorized;
        }
    }

    fn authorization_rejected(&self, failure: bcs_api::HttpFailure) -> DeliveryError {
        let mut state = self.state.lock().unwrap();
        if state.authorization == InitialAuthorization::Pending {
            state.authorization = InitialAuthorization::Rejected(failure.clone());
            DeliveryError::InitialAuthorization(failure)
        } else if failure.body.is_some() {
            DeliveryError::Api(failure)
        } else {
            DeliveryError::Unauthorized
        }
    }

    /// A payload is lost. With `reset_cas` it may have held blobs offered
    /// with it: the publisher offers them again, and every stream carries
    /// its definition groups again, since one of those blobs may have been a
    /// group that later captures name by ID only.
    fn lose(&self, error: DeliveryError, reset_cas: bool) {
        self.lose_carrying(error, reset_cas, reset_cas);
    }

    /// [`Self::lose`], where `carried` says whether the payload may have
    /// carried a definition group: only then does every stream carry its
    /// groups again. A loss that cannot drop one (records alone, or blobs
    /// that stay queued) leaves the streams as they are, so that it does not
    /// make every following capture carry, and grow, again.
    fn lose_carrying(&self, error: DeliveryError, reset_cas: bool, carried: bool) {
        let mut state = self.state.lock().unwrap();
        if state.failure.is_some() {
            return;
        }
        state.last_error = Some(error);
        state.loss_count = state.loss_count.saturating_add(1);
        state.progress.reset_cas |= reset_cas;
        if carried {
            state.carried_resets = state.carried_resets.saturating_add(1);
            btel_snapshot::forget_carried();
        }
    }

    fn initial_connection_failed(&self, error: DeliveryError) -> DeliveryError {
        self.state.lock().unwrap().authorization = InitialAuthorization::Failed(error.clone());
        DeliveryError::InitialConnection(Box::new(error))
    }

    fn fail(&self, error: DeliveryError) {
        let (first, sender) = {
            let mut state = self.state.lock().unwrap();
            if state.failure.is_some() {
                (false, None)
            } else {
                state.failure = Some(error.clone());
                (true, state.sender.take())
            }
        };
        drop(sender);
        self.capacity.notify_all();
        if first {
            self.heartbeat.set_state(ProducerState::Disabled);
            self.cancel.send_replace(true);
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (self.on_failure)(error);
            }));
        }
    }
}

/// What a plan holds until its recording is uploaded. Its captures, and the
/// storage slots they hold, are given back sooner: once its bodies are built.
struct Reservation {
    shared: Arc<Shared>,
    recording: usize,
    snapshots: usize,
    captures: Option<Share>,
    /// The strings among the blobs placed with the recording.
    inline: Share,
    recording_window: Duration,
}

impl Reservation {
    fn release_captures(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.snapshots -= std::mem::take(&mut self.snapshots);
        if let Some(share) = self.captures.take() {
            state.cas.give(share);
        }
        self.shared.capacity.notify_all();
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.recording_bytes -= self.recording;
        state.snapshots -= self.snapshots;
        if let Some(share) = self.captures.take() {
            state.cas.give(share);
        }
        state.cas.give(self.inline);
        state.plans -= 1;
        state.recording_window -= self.recording_window;
        self.shared.capacity.notify_all();
    }
}

/// What one CAS body holds until its upload ends: its part of the budget,
/// and the longest it may keep the bodies behind it waiting.
struct BodyHold {
    shared: Arc<Shared>,
    share: Share,
    window: Duration,
}

impl BodyHold {
    /// The body turned out shorter: part of it is stored already.
    fn shorten(&mut self, window: Duration) {
        let window = window.min(self.window);
        let mut state = self.shared.state.lock().unwrap();
        state.cas_window -= self.window.saturating_sub(window);
        self.window = window;
    }
}

impl Drop for BodyHold {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.cas.give(self.share);
        state.cas_window -= self.window;
        self.shared.capacity.notify_all();
    }
}

/// Where a candidate's bytes come from.
pub(crate) enum Source {
    /// A string blob, which holds its own content.
    Leaf(Leaf),
    /// A blob written from one of the window's captures.
    Kept {
        /// Index into the window's owners.
        owner: u32,
        blob: BlobIndex,
    },
}

/// One blob offered with a file.
pub(crate) struct Candidate {
    pub id: CasId,
    pub encoded_len: u64,
    pub source: Source,
}
impl Candidate {
    /// How many of its bytes an upload body holds in a buffer of its own.
    pub(crate) fn buffered_len(&self) -> u64 {
        match &self.source {
            Source::Leaf(leaf) => body::buffered_len(Some(leaf), self.encoded_len),
            Source::Kept { .. } => body::buffered_len(None, self.encoded_len),
        }
    }
}

/// A file's candidates and the captures some of them are written from. A
/// string blob holds its own content and is let go on its own. Several
/// windows may hold one capture; it is freed with the last.
#[derive(Default)]
pub(crate) struct Window {
    pub(crate) owners: Vec<Arc<Structure>>,
    /// Each owner's retained memory, accounted once by the publisher.
    pub(crate) sizes: Vec<usize>,
    /// Per owner: whether this window holds the last of its queued blobs.
    pub(crate) releases: Vec<bool>,
    pub(crate) candidates: Vec<Candidate>,
}

impl Window {
    /// Every blob of every capture, in the captures' order, children first.
    fn whole(snapshots: Vec<Snapshot>) -> Result<Self, DeliveryError> {
        let mut window = Self::default();
        for snapshot in snapshots {
            let Split { leaves, structure } = snapshot.split();
            window
                .candidates
                .extend(leaves.into_iter().map(|leaf| Candidate {
                    id: leaf.id(),
                    encoded_len: leaf.encoded_len(),
                    source: Source::Leaf(leaf),
                }));
            let Some(structure) = structure else {
                continue;
            };
            let owner = u32::try_from(window.owners.len()).map_err(|_| DeliveryError::Capacity)?;
            window
                .candidates
                .extend(structure.blobs().map(|blob| Candidate {
                    id: blob.id(),
                    encoded_len: blob.encoded_len(),
                    source: Source::Kept {
                        owner,
                        blob: blob.index(),
                    },
                }));
            window.sizes.push(structure.retained_bytes());
            window.releases.push(true);
            window.owners.push(Arc::new(structure));
        }
        Ok(window)
    }
}

struct Work {
    file: SealedFile,
    window: Window,
    request: PrepareUploadsRequest,
    reservation: Reservation,
    /// One per proposed CAS target, by its client target ID.
    holds: Vec<(u32, BodyHold)>,
}

/// Cloneable producer handle. Admission waits for capacity without performing I/O.
/// Callers must recycle processor input chunks before submitting.
#[derive(Clone)]
pub struct BcsDeliveryHandle {
    shared: Arc<Shared>,
}

impl BcsDeliveryHandle {
    pub fn disabled(error: DeliveryError, config: DeliveryConfig) -> Self {
        Self {
            shared: Arc::new(Shared {
                state: Mutex::new(State {
                    failure: Some(error),
                    finished: true,
                    ..State::default()
                }),
                config,
                capacity: Condvar::new(),
                cancel: watch::channel(true).0,
                heartbeat: Heartbeat::new(),
                on_failure: Box::new(|_| {}),
            }),
        }
    }

    pub fn is_disabled(&self) -> bool {
        self.shared.state.lock().unwrap().failure.is_some()
    }

    /// Saturating count of dropped plans and physical targets, not HTTP attempts.
    pub fn loss_count(&self) -> u64 {
        self.shared.state.lock().unwrap().loss_count
    }

    /// Saturating count of losses that may have dropped a definition group,
    /// each of which made every stream carry its groups again.
    pub fn carried_resets(&self) -> u64 {
        self.shared.state.lock().unwrap().carried_resets
    }

    /// Saturating count of definition segments evicted from bounded replay retention.
    pub fn metadata_replay_evictions(&self) -> u64 {
        self.shared.state.lock().unwrap().metadata_replay_evictions
    }

    pub(crate) fn record_metadata_replay_evictions(&self, count: u64) {
        if count == 0 {
            return;
        }
        let mut state = self.shared.state.lock().unwrap();
        state.metadata_replay_evictions = state.metadata_replay_evictions.saturating_add(count);
    }

    pub(crate) fn record_payload_loss(&self, error: DeliveryError, reset_cas: bool) {
        self.shared.lose(error, reset_cas);
    }

    /// A capture dropped for room before any of its blobs was offered. A
    /// later capture offers what it shared with this one, so nothing needs
    /// offering again; but with `carried`, it carried a definition group no
    /// other offer holds, which its stream will not carry again unless every
    /// stream forgets what it carried.
    pub(crate) fn record_capture_loss(&self, carried: bool) {
        self.shared
            .lose_carrying(DeliveryError::Capacity, false, carried);
    }

    /// Most recent loss, or the fatal lifecycle error if delivery was disabled.
    pub fn last_error(&self) -> Option<DeliveryError> {
        let state = self.shared.state.lock().unwrap();
        state.failure.clone().or_else(|| state.last_error.clone())
    }

    pub(crate) fn take_progress(&self) -> DeliveryProgress {
        let mut state = self.shared.state.lock().unwrap();
        DeliveryProgress {
            reset_cas: std::mem::take(&mut state.progress.reset_cas),
            recording_acked_through: state.progress.recording_acked_through,
        }
    }

    pub fn disable(&self, error: DeliveryError) {
        self.shared.fail(error);
    }

    pub(crate) fn config(&self) -> &DeliveryConfig {
        &self.shared.config
    }

    /// Offer every blob of every capture with the file; `proposed_uploads`
    /// index the blobs in the captures' order, children first.
    pub fn try_submit(
        &self,
        file: SealedFile,
        snapshots: Vec<Snapshot>,
        proposed_uploads: Vec<ProposedUploadTarget>,
    ) -> Result<(), DeliveryError> {
        let window = match Window::whole(snapshots) {
            Ok(window) => window,
            Err(error) => {
                self.shared.lose(error.clone(), true);
                return Err(error);
            }
        };
        self.try_submit_window(file, window, proposed_uploads)
    }

    /// The publisher accounts each capture once while building its window.
    pub(crate) fn try_submit_window(
        &self,
        file: SealedFile,
        window: Window,
        proposed_uploads: Vec<ProposedUploadTarget>,
    ) -> Result<(), DeliveryError> {
        let result = self.submit(file, window, proposed_uploads);
        if let Err(error) = &result {
            if *error != DeliveryError::Closed {
                self.shared.lose(error.clone(), true);
            }
        }
        result
    }

    fn submit(
        &self,
        file: SealedFile,
        window: Window,
        proposed_uploads: Vec<ProposedUploadTarget>,
    ) -> Result<(), DeliveryError> {
        let config = &self.shared.config;
        let candidates = &window.candidates;
        if window.sizes.len() != window.owners.len()
            || candidates.iter().any(|candidate| match candidate.source {
                Source::Leaf(_) => false,
                Source::Kept { owner, .. } => owner as usize >= window.owners.len(),
            })
        {
            return Err(DeliveryError::InvalidPlan);
        }
        if file.bytes().len() > config.max_recording_body_bytes
            || candidates.len() > config.max_candidates
            || window.owners.len() > config.max_pending_snapshots
            || proposed_uploads.len() > config.max_targets
            || proposed_uploads
                .iter()
                .any(|p| p.candidate_indices.len() > config.max_candidates)
        {
            return Err(DeliveryError::Capacity);
        }
        let mut metadata = Vec::with_capacity(candidates.len());
        // The captures, held until the plan's bodies are built.
        let mut captures = window
            .owners
            .capacity()
            .checked_mul(std::mem::size_of::<Arc<Structure>>())
            .and_then(|v| {
                candidates
                    .capacity()
                    .checked_mul(std::mem::size_of::<Candidate>())
                    .and_then(|c| v.checked_add(c))
            })
            .ok_or(DeliveryError::Capacity)?;
        for size in &window.sizes {
            captures = captures.checked_add(*size).ok_or(DeliveryError::Capacity)?;
        }
        for (index, candidate) in candidates.iter().enumerate() {
            let index = u32::try_from(index).map_err(|_| DeliveryError::Capacity)?;
            let standalone = proposed_uploads
                .iter()
                .any(|p| p.kind == UploadKind::CasObject && p.candidate_indices.contains(&index));
            metadata.push(CasCandidate {
                snapshot_id: hex::encode(candidate.id.as_bytes()),
                snapshot_format_version: btel_settings::snapshot::BLOB_VERSION,
                size_class: standalone.then_some(CasSizeClass::Large),
            });
        }
        let request = PrepareUploadsRequest {
            recording: RecordingDescriptor {
                recording_id: hex::encode(file.recording_id().as_bytes()),
                recording_file_sequence: file.sequence().get(),
                encoded_length: u64::try_from(file.bytes().len())
                    .map_err(|_| DeliveryError::Capacity)?,
                sha256: hex::encode(Sha256::digest(file.bytes())),
            },
            candidates: metadata,
            proposed_uploads,
            producer_session_id: None,
            liveness_sequence: None,
            state: None,
        };
        validate_proposal(&request)?;
        // Response JSON allocations and parsed strings are bounded
        // conservatively as control bytes.
        let control = config
            .max_response_bytes
            .checked_mul(64)
            .and_then(|v| {
                config
                    .max_request_bytes
                    .checked_mul(4)
                    .and_then(|r| v.checked_add(r))
            })
            .ok_or(DeliveryError::Capacity)?;
        let proposal_bytes = request.proposed_uploads.iter().try_fold(
            request
                .proposed_uploads
                .capacity()
                .checked_mul(std::mem::size_of::<ProposedUploadTarget>())
                .ok_or(DeliveryError::Capacity)?,
            |sum, p| {
                p.candidate_indices
                    .capacity()
                    .checked_mul(4)
                    .and_then(|n| sum.checked_add(n))
                    .ok_or(DeliveryError::Capacity)
            },
        )?;
        let metadata_bytes = request.candidates.iter().try_fold(
            request
                .candidates
                .capacity()
                .checked_mul(std::mem::size_of::<CasCandidate>())
                .and_then(|v| v.checked_add(std::mem::size_of::<Work>()))
                .and_then(|v| v.checked_add(request.recording.recording_id.capacity()))
                .and_then(|v| v.checked_add(request.recording.sha256.capacity()))
                .ok_or(DeliveryError::Capacity)?,
            |sum, candidate| {
                sum.checked_add(candidate.snapshot_id.capacity())
                    .ok_or(DeliveryError::Capacity)
            },
        )?;
        // The file is held until its body, which copies it once, is built.
        let recording = config
            .max_recording_body_bytes
            .checked_add(file.retained_bytes())
            .and_then(|v| v.checked_add(control))
            .and_then(|v| v.checked_add(proposal_bytes))
            .and_then(|v| v.checked_add(metadata_bytes))
            .ok_or(DeliveryError::Capacity)?;
        // A body holds what it buffers (its blobs, once, but for the strings
        // it sends from where they are held) and those strings, until its
        // upload ends. Its whole length decides how long that may take.
        let mut bodies = Vec::new();
        let mut recording_window = Duration::ZERO;
        // What a blob placed with the recording buffers is within the
        // recording body, reserved above. A string among them is held like
        // any other, and a string may hold far more than its own length (a
        // part of a long one holds all of it): that is the CAS budget's to
        // count, which refuses nothing.
        let mut inline = 0;
        for target in &request.proposed_uploads {
            let window = blob_bytes(candidates, &target.candidate_indices);
            let leaves = leaf_bytes(candidates, &target.candidate_indices);
            match target.kind {
                UploadKind::Recording => {
                    recording_window = window
                        .and_then(|blobs| blobs.checked_add(file.bytes().len()))
                        .and_then(|body| config.put_window(body))
                        .ok_or(DeliveryError::Capacity)?;
                    inline = leaves.ok_or(DeliveryError::Capacity)?;
                }
                UploadKind::CasBatch | UploadKind::CasObject => {
                    let window = window
                        .and_then(|blobs| config.put_window(blobs))
                        .ok_or(DeliveryError::Capacity)?;
                    let held = buffered_bytes(candidates, &target.candidate_indices)
                        .zip(leaves)
                        .and_then(|(buffered, leaves)| buffered.checked_add(leaves))
                        .ok_or(DeliveryError::Capacity)?;
                    bodies.push((target.client_target_id, held, window));
                }
            }
        }
        let cas = bodies
            .iter()
            .try_fold(captures, |sum, (_, held, _)| sum.checked_add(*held))
            .and_then(|sum| sum.checked_add(inline))
            .ok_or(DeliveryError::Capacity)?;
        let cas_window = bodies
            .iter()
            .try_fold(Duration::ZERO, |sum, (_, _, window)| {
                sum.checked_add(*window)
            })
            .ok_or(DeliveryError::Capacity)?;
        if recording > config.recording_reserved_bytes {
            return Err(DeliveryError::Capacity);
        }
        let mut state = self.shared.state.lock().unwrap();
        let identity = (*file.recording_id().as_bytes(), file.sequence().get());
        loop {
            if let Some(error) = state.failure.clone() {
                return Err(error);
            }
            if state.sender.is_none() {
                return Err(DeliveryError::Closed);
            }
            if state
                .last_file
                .is_some_and(|(id, seq)| id != identity.0 || seq >= identity.1)
            {
                return Err(DeliveryError::InvalidPlan);
            }
            let fits = |used: usize, add: usize, limit: usize| {
                used.checked_add(add).is_some_and(|v| v <= limit)
            };
            if state.plans >= config.max_pending_plans
                || !fits(
                    state.snapshots,
                    window.owners.len(),
                    config.max_pending_snapshots,
                )
                || !fits(
                    state.recording_bytes,
                    recording,
                    config.recording_reserved_bytes,
                )
                || !state.cas.admits(cas, config.cas_reserved_bytes)
            {
                state = self.shared.capacity.wait(state).unwrap();
                continue;
            }
            break;
        }
        let limit = config.cas_reserved_bytes;
        state.recording_bytes += recording;
        state.snapshots += window.owners.len();
        state.plans += 1;
        state.recording_window += recording_window;
        state.cas_window += cas_window;
        state.last_file = Some(identity);
        let reservation = Reservation {
            shared: self.shared.clone(),
            recording,
            snapshots: window.owners.len(),
            captures: Some(state.cas.take(captures, cas, limit)),
            inline: state.cas.take(inline, cas, limit),
            recording_window,
        };
        let holds = bodies
            .into_iter()
            .map(|(target, held, window)| {
                let hold = BodyHold {
                    shared: self.shared.clone(),
                    share: state.cas.take(held, cas, limit),
                    window,
                };
                (target, hold)
            })
            .collect();
        let work = Work {
            file,
            window,
            request,
            reservation,
            holds,
        };
        let result = state.sender.as_ref().unwrap().try_send(work);
        drop(state);
        result.map_err(|_| DeliveryError::Closed)
    }
}

/// Owns the worker. Finish after the processor seals its last file.
pub struct BcsDelivery {
    handle: BcsDeliveryHandle,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl BcsDelivery {
    pub fn new(
        config: DeliveryConfig,
        on_failure: impl Fn(DeliveryError) + Send + Sync + 'static,
    ) -> Result<Self, DeliveryError> {
        config.validate()?;
        #[cfg(feature = "ring-crypto")]
        if rustls::crypto::CryptoProvider::get_default().is_none() {
            let _ = rustls::crypto::ring::default_provider().install_default();
        }
        let (sender, receiver) = mpsc::channel(config.max_pending_plans);
        let shared = Arc::new(Shared {
            state: Mutex::new(State {
                sender: Some(sender),
                authorization: if config.initial_recording_id.is_some() {
                    InitialAuthorization::Pending
                } else {
                    InitialAuthorization::NotRequested
                },
                ..State::default()
            }),
            config,
            capacity: Condvar::new(),
            cancel: watch::channel(false).0,
            heartbeat: Heartbeat::new(),
            on_failure: Box::new(on_failure),
        });
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("btel-bcs".into())
            .spawn(move || {
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .map_err(|_| DeliveryError::Worker)?;
                    let client = Client::builder()
                        .redirect(reqwest::redirect::Policy::none())
                        .retry(reqwest::retry::never())
                        .no_proxy()
                        .timeout(worker_shared.config.request_timeout)
                        .build()
                        .map_err(|_| DeliveryError::Http)?;
                    runtime.block_on(run_cancellable(client, worker_shared.clone(), receiver));
                    Ok::<(), DeliveryError>(())
                }));
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(error)) => worker_shared.fail(error),
                    Err(_) => worker_shared.fail(DeliveryError::Worker),
                }
            })
            .map_err(|_| DeliveryError::Worker)?;
        Ok(Self {
            handle: BcsDeliveryHandle { shared },
            worker: Mutex::new(Some(worker)),
        })
    }

    pub fn handle(&self) -> BcsDeliveryHandle {
        self.handle.clone()
    }

    pub fn finish(&self) -> Result<(), DeliveryError> {
        let mut worker = self.worker.lock().unwrap();
        let sender = {
            let mut state = self.handle.shared.state.lock().unwrap();
            if state.failure.is_none() {
                self.handle
                    .shared
                    .heartbeat
                    .set_state(ProducerState::Draining);
            }
            state.sender.take()
        };
        drop(sender);
        self.handle.shared.capacity.notify_all();
        if worker.take().is_some_and(|worker| worker.join().is_err()) {
            self.handle.shared.fail(DeliveryError::Worker);
        }
        let mut state = self.handle.shared.state.lock().unwrap();
        state.finished = true;
        state
            .failure
            .clone()
            .or_else(|| state.last_error.clone())
            .map_or(Ok(()), Err)
    }

    pub fn result(&self) -> Option<Result<(), DeliveryError>> {
        let state = self.handle.shared.state.lock().unwrap();
        if let Some(error) = state.failure.clone().or_else(|| state.last_error.clone()) {
            Some(Err(error))
        } else {
            state.finished.then_some(Ok(()))
        }
    }

    pub fn heartbeat_error(&self) -> Option<HeartbeatError> {
        self.handle.shared.heartbeat.last_error()
    }

    pub fn heartbeat_failure_count(&self) -> u64 {
        self.handle.shared.heartbeat.failure_count()
    }

    pub fn initial_authorization(&self) -> InitialAuthorization {
        self.handle
            .shared
            .state
            .lock()
            .unwrap()
            .authorization
            .clone()
    }
}

impl Drop for BcsDelivery {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

/// A blob's share of an upload envelope beyond its bytes: the object's
/// identity, version, digest and lengths, and the envelope's plan and upload
/// identifiers.
const CAS_OBJECT_OVERHEAD_BYTES: usize = 512;

/// The most the blobs at `indices` add to an upload body. `None` when that
/// does not fit a `usize`.
fn blob_bytes(candidates: &[Candidate], indices: &[u32]) -> Option<usize> {
    indices.iter().try_fold(0_usize, |sum, &index| {
        usize::try_from(candidates[index as usize].encoded_len)
            .ok()?
            .checked_add(CAS_OBJECT_OVERHEAD_BYTES)?
            .checked_add(sum)
    })
}

/// As [`blob_bytes`], counting only what a body buffers.
fn buffered_bytes(candidates: &[Candidate], indices: &[u32]) -> Option<usize> {
    indices.iter().try_fold(0_usize, |sum, &index| {
        usize::try_from(candidates[index as usize].buffered_len())
            .ok()?
            .checked_add(CAS_OBJECT_OVERHEAD_BYTES)?
            .checked_add(sum)
    })
}

/// What the strings among the blobs at `indices` retain: a body holds them
/// until its upload ends.
fn leaf_bytes(candidates: &[Candidate], indices: &[u32]) -> Option<usize> {
    indices.iter().try_fold(0_usize, |sum, &index| {
        match &candidates[index as usize].source {
            Source::Leaf(leaf) => sum.checked_add(leaf.retained_bytes()),
            Source::Kept { .. } => Some(sum),
        }
    })
}

struct LimitedWriter {
    bytes: Vec<u8>,
    limit: usize,
}

impl LimitedWriter {
    fn new(limit: usize) -> Self {
        Self {
            bytes: Vec::new(),
            limit,
        }
    }
}

impl Write for LimitedWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        let len = self
            .bytes
            .len()
            .checked_add(bytes.len())
            .filter(|&len| len <= self.limit)
            .ok_or_else(|| io::Error::other("cloud body byte limit"))?;
        if len > self.bytes.capacity() {
            let capacity = self
                .bytes
                .capacity()
                .checked_mul(2)
                .unwrap_or(self.limit)
                .max(len)
                .min(self.limit);
            self.bytes
                .try_reserve_exact(capacity - self.bytes.len())
                .map_err(|_| io::Error::other("cloud body allocation"))?;
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn now_ms() -> Result<u64, DeliveryError> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| u64::try_from(d.as_millis()).ok())
        .ok_or(DeliveryError::Expired)
}

fn retryable(status: StatusCode) -> bool {
    status.is_server_error()
        || status == StatusCode::TOO_MANY_REQUESTS
        || status == StatusCode::REQUEST_TIMEOUT
}

async fn prepare(
    client: &Client,
    config: &DeliveryConfig,
    request: &PrepareUploadsRequest,
    shared: &Shared,
) -> Result<PrepareUploadsResponse, DeliveryError> {
    let heartbeat = &shared.heartbeat;
    let url = config.endpoint(&request.recording.recording_id, "uploads:prepare")?;
    let key = format!(
        "{}:{}",
        request.recording.recording_id, request.recording.recording_file_sequence
    );
    let mut last_failure = None;
    for attempt in 0..config.max_attempts {
        let decorated = heartbeat.decorate_prepare(request);
        let mut writer = LimitedWriter::new(config.max_request_bytes);
        serde_json::to_writer(
            &mut writer,
            &bcs_api::credentials::Targeted {
                build_id: config
                    .authorization
                    .as_ref()
                    .and_then(|auth| auth.authentication.build_id()),
                target: config
                    .authorization
                    .as_ref()
                    .and_then(|auth| auth.target.as_ref()),
                payload: decorated.as_ref().unwrap_or(request),
            },
        )
        .map_err(|_| DeliveryError::Capacity)?;
        let body = Bytes::from(writer.bytes);
        match bcs_api::telemetry::prepare(
            client,
            &url,
            config.bearer_token.as_deref(),
            config.authorization.as_ref(),
            &key,
            body,
        )
        .await
        {
            Ok(response) => {
                shared.authorized();
                match read_response(response, config.max_response_bytes).await {
                    Ok(bytes) => {
                        heartbeat.mark_control_success();
                        let response: PrepareUploadsResponse = serde_json::from_slice(&bytes)
                            .map_err(|_| DeliveryError::InvalidPlan)?;
                        let _ = heartbeat.configure(response.heartbeat);
                        return Ok(response);
                    }
                    Err(DeliveryError::Http) => {}
                    Err(error) => return Err(error),
                }
            }
            Err(error)
                if matches!(
                    error.status(),
                    Some(reqwest::StatusCode::UNAUTHORIZED | reqwest::StatusCode::FORBIDDEN)
                ) =>
            {
                let failure = error
                    .http_failure()
                    .cloned()
                    .unwrap_or_else(|| bcs_api::HttpFailure::new(error.status().unwrap()));
                return Err(shared.authorization_rejected(failure));
            }
            Err(error)
                if error
                    .http_failure()
                    .is_some_and(|failure| !retryable(failure.status)) =>
            {
                return Err(error
                    .http_failure()
                    .cloned()
                    .filter(|failure| failure.body.is_some())
                    .map_or(DeliveryError::Http, DeliveryError::Api));
            }
            Err(error) => last_failure = error.http_failure().cloned(),
        }
        if attempt + 1 < config.max_attempts {
            tokio::time::sleep(config.retry_delay).await;
        }
    }
    Err(last_failure
        .filter(|failure| failure.body.is_some())
        .map_or(DeliveryError::Http, DeliveryError::Api))
}

async fn read_response(
    mut response: reqwest::Response,
    limit: usize,
) -> Result<Vec<u8>, DeliveryError> {
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err(DeliveryError::Capacity);
    }
    let mut writer = LimitedWriter::new(limit);
    while let Some(chunk) = response.chunk().await.map_err(|_| DeliveryError::Http)? {
        writer
            .write_all(&chunk)
            .map_err(|_| DeliveryError::Capacity)?;
    }
    Ok(writer.bytes)
}

async fn put(
    client: &Client,
    config: &DeliveryConfig,
    target: UploadTarget,
    body: Body,
    plan_expiry: u64,
) -> Result<(), DeliveryError> {
    let expiry = plan_expiry.min(target.expires_at_unix_ms);
    let mut headers = reqwest::header::HeaderMap::new();
    for (name, value) in &target.required_headers {
        headers.insert(
            reqwest::header::HeaderName::from_bytes(name.as_bytes())
                .map_err(|_| DeliveryError::InvalidPlan)?,
            reqwest::header::HeaderValue::from_str(value)
                .map_err(|_| DeliveryError::InvalidPlan)?,
        );
    }
    for attempt in 0..config.max_attempts {
        let remaining = expiry
            .checked_sub(now_ms()?)
            .filter(|&ms| ms > 0)
            .ok_or(DeliveryError::Expired)?;
        let request_body = reqwest::Body::wrap_stream(futures::stream::iter(
            body.chunks()
                .into_iter()
                .map(Ok::<Bytes, std::convert::Infallible>),
        ));
        match bcs_api::telemetry::upload(
            client,
            &target.presigned_put_url,
            headers.clone(),
            request_body,
            body.len(),
            config
                .put_timeout(body.len())
                .min(Duration::from_millis(remaining)),
        )
        .await
        {
            Ok(response) if response.status().is_success() => return Ok(()),
            Ok(response) if !retryable(response.status()) => return Err(DeliveryError::Http),
            _ => {}
        }
        if attempt + 1 < config.max_attempts {
            if now_ms()?
                .checked_add(
                    u64::try_from(config.retry_delay.as_millis())
                        .map_err(|_| DeliveryError::Expired)?,
                )
                .is_none_or(|t| t >= expiry)
            {
                return Err(DeliveryError::Expired);
            }
            tokio::time::sleep(config.retry_delay).await;
        }
    }
    Err(DeliveryError::Http)
}

struct Prepared {
    recording: Option<(UploadTarget, Body)>,
    recording_sequence: u64,
    cas: Vec<(UploadTarget, Body, BodyHold)>,
    expiry: u64,
    reservation: Reservation,
}

async fn assemble(
    client: &Client,
    config: &DeliveryConfig,
    work: Work,
) -> Result<Prepared, DeliveryError> {
    let Work {
        file,
        window,
        request,
        mut reservation,
        mut holds,
    } = work;
    let response = prepare(client, config, &request, &reservation.shared).await?;
    let now = now_ms()?;
    // When a lane that may be held for `held` is certainly free.
    let after = |held: Duration| {
        u64::try_from(held.as_millis())
            .ok()
            .and_then(|ms| now.checked_add(ms))
            .ok_or(DeliveryError::Expired)
    };
    validate_response(&request, &response, &config.prepare_base_url)?;
    if response.expires_at_unix_ms <= after(config.retry_window()?)? {
        return Err(DeliveryError::Expired);
    }
    // What the server already has is not uploaded: a body is as long as
    // what is left of it, and one with nothing left holds nothing.
    let mut wanted = Vec::with_capacity(holds.len());
    for upload in &response.uploads {
        if upload.kind == UploadKind::Recording {
            continue;
        }
        let at = holds
            .iter()
            .position(|(target, _)| *target == upload.client_target_id)
            .ok_or(DeliveryError::InvalidPlan)?;
        let (target, mut hold) = holds.swap_remove(at);
        hold.shorten(
            blob_bytes(&window.candidates, &upload.candidate_indices)
                .and_then(|blobs| config.put_window(blobs))
                .ok_or(DeliveryError::Capacity)?,
        );
        wanted.push((target, hold));
    }
    drop(holds);
    // A URL must outlive every body that may upload before its own.
    let (recording_horizon, cas_horizon) = {
        let state = reservation.shared.state.lock().unwrap();
        (after(state.recording_window)?, after(state.cas_window)?)
    };
    let Window {
        owners, candidates, ..
    } = window;
    // A candidate goes into at most one body; an available one into none,
    // and a string the server has is let go here.
    let mut sources: Vec<Option<Source>> = Vec::with_capacity(candidates.len());
    let mut heads = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        heads.push((candidate.id, candidate.encoded_len));
        sources.push(Some(candidate.source));
    }
    for disposition in &response.cas {
        if matches!(disposition.disposition, Disposition::AlreadyAvailable {}) {
            sources[disposition.candidate_index as usize] = None;
        }
    }
    let mut scratch = BlobScratch::default();
    let mut recording = None;
    let mut cas = Vec::new();
    for target in response.uploads {
        let body = (|| {
            let horizon = if target.kind == UploadKind::Recording {
                recording_horizon
            } else {
                cas_horizon
            };
            if target.expires_at_unix_ms <= horizon || response.expires_at_unix_ms <= horizon {
                return Err(DeliveryError::Expired);
            }
            // A CAS body is as long as its blobs, which admission reserved.
            let limit = match target.kind {
                UploadKind::Recording => config.max_recording_body_bytes,
                UploadKind::CasBatch | UploadKind::CasObject => usize::MAX,
            };
            let head = body::Head {
                plan_id: &response.plan_id,
                upload_id: &target.upload_id,
                recording_file: (target.kind == UploadKind::Recording).then(|| file.bytes()),
                limit,
            };
            let blobs = target
                .candidate_indices
                .iter()
                .map(|&index| {
                    let (id, encoded_len) = heads[index as usize];
                    sources[index as usize]
                        .take()
                        .map(|source| Candidate {
                            id,
                            encoded_len,
                            source,
                        })
                        .ok_or(DeliveryError::InvalidPlan)
                })
                .collect::<Result<Vec<_>, _>>()?;
            body::build(&head, blobs, &owners, &mut scratch)
        })();
        let body = match body {
            Ok(body) => body,
            Err(error) => {
                reservation
                    .shared
                    .lose(error, !target.candidate_indices.is_empty());
                for &index in &target.candidate_indices {
                    sources[index as usize] = None;
                }
                continue;
            }
        };
        match target.kind {
            UploadKind::Recording => recording = Some((target, body)),
            UploadKind::CasBatch | UploadKind::CasObject => {
                let at = wanted
                    .iter()
                    .position(|(id, _)| *id == target.client_target_id)
                    .unwrap_or_else(|| unreachable!("every CAS upload was given its hold"));
                cas.push((target, body, wanted.swap_remove(at).1));
            }
        }
    }
    // A body that was not built holds nothing.
    drop(wanted);
    // Every candidate is in a body or skipped: the captures can go. A long
    // string stays with the body that sends it.
    drop(sources);
    drop(owners);
    let recording_sequence = file.sequence().get();
    drop(file);
    reservation.release_captures();
    Ok(Prepared {
        recording,
        recording_sequence,
        cas,
        expiry: response.expires_at_unix_ms,
        reservation,
    })
}

/// Upload a plan's recording, in its turn. The plan is pending until then.
async fn upload_recording(
    client: &Client,
    config: &DeliveryConfig,
    recording: Option<(UploadTarget, Body)>,
    recording_sequence: u64,
    expiry: u64,
    reservation: Reservation,
    slots: Arc<Semaphore>,
) {
    let shared = &reservation.shared;
    if let Some((target, body)) = recording {
        let _slot = slots.acquire().await.expect("recording lane open");
        let reset_cas = !target.candidate_indices.is_empty();
        match put(client, config, target, body, expiry).await {
            Ok(()) => {
                let mut state = shared.state.lock().unwrap();
                state.progress.recording_acked_through = state
                    .progress
                    .recording_acked_through
                    .max(recording_sequence);
            }
            Err(error) => shared.lose(error, reset_cas),
        }
    }
}

/// Upload one CAS body, when one of the upload slots is free. It answers for
/// itself: the plan it came with may be long done.
async fn upload_body(
    client: &Client,
    config: &DeliveryConfig,
    target: UploadTarget,
    body: Body,
    hold: BodyHold,
    expiry: u64,
    slots: Arc<Semaphore>,
) {
    let _slot = slots.acquire().await.expect("CAS lane open");
    if let Err(error) = put(client, config, target, body, expiry).await {
        hold.shared.lose(error, true);
    }
}

/// This runs on the delivery worker, independently of VM execution and file sealing.
/// Transient failures retain the ordinary retry/loss policy; only an admission refusal
/// before the first successful control request is an initial authorization failure.
async fn authorize_initial(
    client: &Client,
    shared: &Shared,
    id: &str,
) -> Result<(), DeliveryError> {
    let config = &shared.config;
    let endpoint = config.endpoint(id, "heartbeat")?;
    let started = tokio::time::Instant::now();
    let window = config.retry_window()?;
    let mut last_failure = None;
    for attempt in 0..config.max_attempts {
        let remaining = window.saturating_sub(started.elapsed());
        if remaining.is_zero() {
            break;
        }
        let mut retry_delay = config.retry_delay;
        let body = shared
            .heartbeat
            .liveness()
            .map_err(|_| DeliveryError::Http)?;
        let result = tokio::time::timeout(
            config.request_timeout.min(remaining),
            crate::liveness::post(
                client,
                &endpoint,
                config.bearer_token.as_deref(),
                config.authorization.as_ref(),
                body,
            ),
        )
        .await;
        match result {
            Ok(Ok(())) => {
                shared.heartbeat.mark_control_success();
                return Ok(());
            }
            Ok(Err(error))
                if error
                    .http_failure()
                    .is_some_and(|failure| matches!(failure.status.as_u16(), 401 | 403)) =>
            {
                return Err(shared.authorization_rejected(error.http_failure().unwrap()));
            }
            Ok(Err(error))
                if error
                    .http_failure()
                    .is_some_and(|failure| !retryable(failure.status)) =>
            {
                return Err(error
                    .http_failure()
                    .map_or(DeliveryError::Http, DeliveryError::Api));
            }
            Ok(Err(error)) => {
                last_failure = error.http_failure();
                if let Some(after) = last_failure
                    .as_ref()
                    .and_then(|failure| failure.retry_after)
                {
                    retry_delay = retry_delay.max(after);
                }
            }
            _ => {}
        }
        if attempt + 1 < config.max_attempts {
            // A server delay is a minimum, not permission to exceed the existing window.
            // If no retry fits, retain the failure instead of sending another request early.
            if retry_delay >= window.saturating_sub(started.elapsed()) {
                break;
            }
            tokio::time::sleep(retry_delay).await;
        }
    }
    Err(last_failure.map_or(DeliveryError::Http, DeliveryError::Api))
}

async fn run_cancellable(client: Client, shared: Arc<Shared>, receiver: mpsc::Receiver<Work>) {
    let mut cancel = shared.cancel.subscribe();
    if *cancel.borrow() {
        return;
    }
    tokio::select! {
        _ = cancel.changed() => {}
        result = std::panic::AssertUnwindSafe(run_worker(client, shared.clone(), receiver)).catch_unwind() => {
            if result.is_err() {
                shared.fail(DeliveryError::Worker);
            }
        }
    }
    shared.heartbeat.stop();
}

async fn run_worker(client: Client, shared: Arc<Shared>, mut receiver: mpsc::Receiver<Work>) {
    if let Some(id) = shared.config.initial_recording_id {
        match authorize_initial(&client, &shared, &hex::encode(id.as_bytes())).await {
            Ok(()) => shared.authorized(),
            Err(error @ DeliveryError::InitialAuthorization(_)) => {
                shared.fail(error);
                return;
            }
            Err(error) if shared.config.terminal_initial_connection => {
                shared.fail(shared.initial_connection_failed(error));
                return;
            }
            Err(error) => shared.lose(error, false),
        }
    }
    let recording_slots = Arc::new(Semaphore::new(1));
    let cas_slots = Arc::new(Semaphore::new(shared.config.max_cas_uploads));
    // One task per plan, for its recording, and one per CAS body.
    let mut recordings = tokio::task::JoinSet::new();
    let mut bodies = tokio::task::JoinSet::new();
    let mut heartbeats = tokio::task::JoinSet::new();
    let mut heartbeat_started = false;
    loop {
        if shared.state.lock().unwrap().failure.is_some() {
            break;
        }
        tokio::select! {
            completed = recordings.join_next(), if !recordings.is_empty() => {
                if completed.is_some_and(|result| result.is_err()) {
                    shared.fail(DeliveryError::Worker);
                }
            }
            completed = bodies.join_next(), if !bodies.is_empty() => {
                if completed.is_some_and(|result| result.is_err()) {
                    shared.fail(DeliveryError::Worker);
                }
            }
            work = receiver.recv(), if recordings.len() < shared.config.max_pending_plans => {
                let Some(work) = work else { break };
                if !heartbeat_started {
                    heartbeat_started = true;
                    match shared.config.endpoint(&work.request.recording.recording_id, "heartbeat") {
                        Ok(endpoint) => {
                            let heartbeat = shared.heartbeat.clone();
                            let client = client.clone();
                            let token = shared.config.bearer_token.clone();
                            let authorization = shared.config.authorization.clone();
                            let timeout = shared.config.request_timeout;
                            let heartbeat_owner = shared.clone();
                            heartbeats.spawn(async move {
                                if std::panic::AssertUnwindSafe(
                                    heartbeat.run_authorized(client, endpoint, token, authorization, timeout),
                                ).catch_unwind().await.is_err() {
                                    heartbeat.observe_error(HeartbeatError::Worker);
                                }
                                if let Some(failure) = heartbeat.last_error().and_then(|error| error.http_failure()).filter(|failure| matches!(failure.status.as_u16(), 401 | 403)) {
                                    heartbeat_owner.fail(heartbeat_owner.authorization_rejected(failure));
                                }
                            });
                        }
                        Err(_) => shared.heartbeat.observe_error(HeartbeatError::InvalidEndpoint),
                    }
                }
                match assemble(&client, &shared.config, work).await {
                    Ok(prepared) => {
                        let Prepared { recording, recording_sequence, cas, expiry, reservation } = prepared;
                        for (target, body, hold) in cas {
                            let client = client.clone();
                            let shared = shared.clone();
                            let slots = cas_slots.clone();
                            bodies.spawn(async move {
                                upload_body(
                                    &client, &shared.config, target, body, hold, expiry, slots,
                                ).await;
                            });
                        }
                        let client = client.clone();
                        let shared = shared.clone();
                        let slots = recording_slots.clone();
                        recordings.spawn(async move {
                            upload_recording(
                                &client, &shared.config, recording, recording_sequence, expiry,
                                reservation, slots,
                            ).await;
                        });
                    }
                    Err(error @ (DeliveryError::Unauthorized | DeliveryError::InitialAuthorization(_))) => shared.fail(error),
                    Err(DeliveryError::Api(failure)) if matches!(failure.status.as_u16(), 401 | 403) => shared.fail(DeliveryError::Api(failure)),
                    Err(error) => {
                        shared.lose(error, true);
                    }
                }
            }
        }
    }
    drop(receiver);
    // Drain what was admitted, unless delivery failed.
    loop {
        if shared.state.lock().unwrap().failure.is_some() {
            recordings.abort_all();
            bodies.abort_all();
        }
        let result = tokio::select! {
            Some(result) = recordings.join_next() => result,
            Some(result) = bodies.join_next() => result,
            else => break,
        };
        if result.is_err_and(|error| !error.is_cancelled()) {
            shared.fail(DeliveryError::Worker);
        }
    }
    shared.heartbeat.stop();
    while heartbeats.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn initial_authorization_runs_without_a_recording_batch_and_is_joined_at_finish() {
        use wiremock::{
            Mock, MockServer, ResponseTemplate,
            matchers::{method, path_regex},
        };
        for status in [204, 401, 403] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(r"/heartbeat$"))
                .respond_with(ResponseTemplate::new(status).set_delay(Duration::from_millis(100)))
                .expect(1)
                .mount(&server)
                .await;
            let delivery = BcsDelivery::new(
                DeliveryConfig {
                    initial_recording_id: Some(btel_types::RecordingId::generate()),
                    ..DeliveryConfig::new(server.uri().parse().unwrap())
                },
                |_| {},
            )
            .unwrap();
            assert_eq!(
                delivery.initial_authorization(),
                InitialAuthorization::Pending
            );
            // No files were submitted. Finishing must still wait for authorization.
            let delivery = tokio::task::spawn_blocking(move || {
                let result = delivery.finish();
                (delivery.initial_authorization(), result)
            })
            .await
            .unwrap();
            if status == 204 {
                assert_eq!(delivery, (InitialAuthorization::Authorized, Ok(())));
            } else {
                assert_eq!(
                    delivery,
                    (
                        InitialAuthorization::Rejected(bcs_api::HttpFailure::new(
                            StatusCode::from_u16(status).unwrap()
                        )),
                        Err(DeliveryError::InitialAuthorization(
                            bcs_api::HttpFailure::new(StatusCode::from_u16(status).unwrap())
                        ))
                    )
                );
            }
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn initial_transient_failure_is_retried_without_canceling_execution() {
        use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        let count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let responses = count.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &Request| {
                ResponseTemplate::new(
                    if responses.fetch_add(1, std::sync::atomic::Ordering::Relaxed) == 0 {
                        503
                    } else {
                        204
                    },
                )
            })
            .expect(2)
            .mount(&server)
            .await;
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                initial_recording_id: Some(btel_types::RecordingId::generate()),
                retry_delay: Duration::ZERO,
                ..DeliveryConfig::new(server.uri().parse().unwrap())
            },
            |_| panic!("transient failure must not cancel execution"),
        )
        .unwrap();
        tokio::task::spawn_blocking(move || {
            assert_eq!(delivery.finish(), Ok(()));
            assert_eq!(
                delivery.initial_authorization(),
                InitialAuthorization::Authorized
            );
        })
        .await
        .unwrap();
    }

    async fn initial_authorization_after_pending(status: u16) {
        use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        let first = Arc::new(Mutex::new(None::<std::time::Instant>));
        let requested = first.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &Request| {
                let mut first = requested.lock().unwrap();
                let first = first.get_or_insert_with(std::time::Instant::now);
                if first.elapsed() < Duration::from_secs(5) {
                    let pending = ResponseTemplate::new(503).insert_header("Retry-After", "5");
                    if status == 204 {
                        pending.set_body_json(serde_json::json!({
                            "code": "AUTHORIZATION_STATE_PENDING", "retryable": true,
                        }))
                    } else {
                        // Proxies may return no structured body: the header still applies.
                        pending
                    }
                } else {
                    ResponseTemplate::new(status)
                }
            })
            .expect(2)
            .mount(&server)
            .await;
        let failures = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let failed = failures.clone();
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                initial_recording_id: Some(btel_types::RecordingId::generate()),
                ..DeliveryConfig::new(server.uri().parse().unwrap())
            },
            move |_| {
                failed.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            },
        )
        .unwrap();
        let (authorization, result, losses) = tokio::task::spawn_blocking(move || {
            let result = delivery.finish();
            (
                delivery.initial_authorization(),
                result,
                delivery.handle().loss_count(),
            )
        })
        .await
        .unwrap();
        assert!(first.lock().unwrap().unwrap().elapsed() >= Duration::from_secs(5));
        assert_eq!(losses, 0);
        if status == 204 {
            assert_eq!(authorization, InitialAuthorization::Authorized);
            assert_eq!(result, Ok(()));
            assert_eq!(failures.load(std::sync::atomic::Ordering::Relaxed), 0);
        } else {
            let failure = bcs_api::HttpFailure::new(StatusCode::from_u16(status).unwrap());
            assert_eq!(
                authorization,
                InitialAuthorization::Rejected(failure.clone())
            );
            assert_eq!(result, Err(DeliveryError::InitialAuthorization(failure)));
            assert_eq!(failures.load(std::sync::atomic::Ordering::Relaxed), 1);
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn initial_retry_after_waits_out_authorization_pending() {
        initial_authorization_after_pending(204).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn initial_retry_after_eventually_refuses_the_key() {
        initial_authorization_after_pending(401).await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn initial_retry_after_cannot_exceed_the_existing_retry_window() {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(
                ResponseTemplate::new(503).insert_header("Retry-After", "18446744073709551615"),
            )
            .expect(1)
            .mount(&server)
            .await;
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                initial_recording_id: Some(btel_types::RecordingId::generate()),
                ..DeliveryConfig::new(server.uri().parse().unwrap())
            },
            |_| panic!("ordinary connection failure must retain its loss policy"),
        )
        .unwrap();
        let (result, losses) = tokio::time::timeout(
            Duration::from_secs(5),
            tokio::task::spawn_blocking(move || {
                (delivery.finish(), delivery.handle().loss_count())
            }),
        )
        .await
        .unwrap()
        .unwrap();
        let Err(DeliveryError::Api(failure)) = result else {
            panic!("expected the HTTP failure: {result:?}");
        };
        assert_eq!(failure.status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(failure.retry_after, Some(Duration::from_secs(u64::MAX)));
        assert_eq!(losses, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn fatal_cancellation_interrupts_initial_retry_after() {
        use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};
        let server = MockServer::start().await;
        let requested = Arc::new(tokio::sync::Notify::new());
        let seen = requested.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &Request| {
                seen.notify_one();
                ResponseTemplate::new(503).insert_header("Retry-After", "5")
            })
            .expect(1)
            .mount(&server)
            .await;
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                initial_recording_id: Some(btel_types::RecordingId::generate()),
                ..DeliveryConfig::new(server.uri().parse().unwrap())
            },
            |_| {},
        )
        .unwrap();
        tokio::time::timeout(Duration::from_secs(5), requested.notified())
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        delivery.handle().disable(DeliveryError::Worker);
        let result = tokio::time::timeout(
            Duration::from_secs(1),
            tokio::task::spawn_blocking(move || delivery.finish()),
        )
        .await
        .unwrap()
        .unwrap();
        assert_eq!(result, Err(DeliveryError::Worker));
    }

    #[test]
    fn parsed_endpoints_still_require_http_policy_validation() {
        let valid = DeliveryConfig::new("https://bcs.example/proxy".parse().unwrap());
        assert_eq!(valid.validate(), Ok(()));
        for endpoint in [
            "http://bcs.example",
            "ftp://bcs.example",
            "file:///tmp",
            "mailto:test@example.com",
            "https://user@bcs.example",
            "https://user:password@bcs.example",
            "https://bcs.example?query=value",
            "https://bcs.example#fragment",
        ] {
            let config = DeliveryConfig::new(endpoint.parse().unwrap());
            assert_eq!(config.validate(), Err(DeliveryError::InvalidConfig));
        }
        for endpoint in [
            "http://localhost:8080",
            "http://127.0.0.1:8080/publisher",
            "http://[::1]:8080",
        ] {
            assert_eq!(
                DeliveryConfig::new(endpoint.parse().unwrap()).validate(),
                Ok(())
            );
        }
        let mut local = DeliveryConfig::new("http://localhost:8080".parse().unwrap());
        local.bearer_token = Some("invalid\ntoken".into());
        assert_eq!(local.validate(), Err(DeliveryError::InvalidConfig));
        let oversized = format!("https://bcs.example/{}", "x".repeat(8192));
        assert_eq!(
            DeliveryConfig::new(oversized.parse().unwrap()).validate(),
            Err(DeliveryError::InvalidConfig)
        );
    }

    #[test]
    fn a_put_is_given_time_for_its_body_at_the_slowest_rate_waited_for() {
        let config = DeliveryConfig::new("https://bcs.example".parse().unwrap());
        assert_eq!(config.min_upload_bytes_per_second, 1 << 20);
        assert_eq!(config.put_timeout(0), Duration::from_secs(10));
        assert_eq!(config.put_timeout(8 << 20), Duration::from_secs(18));
        assert_eq!(config.put_timeout(1 << 30), Duration::from_secs(10 + 1024));
        // Four attempts and the three delays between them.
        assert_eq!(config.retry_window(), Ok(Duration::from_millis(40_600)));
        assert_eq!(
            config.put_window(8 << 20),
            Some(Duration::from_millis(4 * 18_000 + 600))
        );
        // A time too long to count saturates; it does not wrap.
        let slow = DeliveryConfig {
            min_upload_bytes_per_second: 1,
            ..config.clone()
        };
        assert_eq!(slow.put_timeout(3), Duration::from_secs(13));
        assert!(slow.put_timeout(usize::MAX) >= Duration::from_millis(u64::MAX));
        let stalled = DeliveryConfig {
            min_upload_bytes_per_second: 0,
            ..config
        };
        assert_eq!(stalled.validate(), Err(DeliveryError::InvalidConfig));
    }

    #[test]
    fn control_endpoints_preserve_base_paths_and_encoded_segments() {
        for (base, prefix) in [
            ("https://bcs.example", "https://bcs.example"),
            ("https://bcs.example/", "https://bcs.example"),
            ("https://bcs.example/proxy", "https://bcs.example/proxy"),
            ("https://bcs.example/proxy/", "https://bcs.example/proxy"),
            ("https://bcs.example/a%2Fb/", "https://bcs.example/a%2Fb"),
        ] {
            let config = DeliveryConfig::new(base.parse().unwrap());
            for operation in ["uploads:prepare", "heartbeat"] {
                assert_eq!(
                    config.endpoint("0123", operation).unwrap().as_str(),
                    format!("{prefix}/v1/recordings/0123/{operation}")
                );
            }
            assert_eq!(
                config.prepare_base_url.as_str(),
                Url::parse(base).unwrap().as_str()
            );
        }
    }

    #[test]
    fn fatal_cancellation_does_not_reset_cas_or_record_payload_loss() {
        let handle = BcsDeliveryHandle::disabled(
            DeliveryError::Worker,
            DeliveryConfig::new("https://localhost".parse().unwrap()),
        );
        handle.record_payload_loss(DeliveryError::Worker, true);
        assert_eq!(handle.loss_count(), 0);
        assert_eq!(handle.shared.state.lock().unwrap().last_error, None);
        let progress = handle.take_progress();
        assert!(!progress.reset_cas);
        assert_eq!(progress.recording_acked_through, 0);
    }

    #[test]
    fn replay_evictions_and_payload_losses_are_separate_bounded_counters() {
        let delivery = BcsDelivery::new(
            DeliveryConfig::new("https://localhost".parse().unwrap()),
            |_| panic!("ordinary loss must not disable delivery"),
        )
        .unwrap();
        let handle = delivery.handle();
        {
            let mut state = handle.shared.state.lock().unwrap();
            state.loss_count = u64::MAX - 1;
            state.metadata_replay_evictions = u64::MAX - 1;
            state.progress.recording_acked_through = 7;
        }
        handle.record_metadata_replay_evictions(0);
        assert_eq!(handle.metadata_replay_evictions(), u64::MAX - 1);
        handle.record_metadata_replay_evictions(2);
        assert_eq!(handle.metadata_replay_evictions(), u64::MAX);
        assert_eq!(handle.loss_count(), u64::MAX - 1);
        assert_eq!(handle.last_error(), None);
        handle.record_payload_loss(DeliveryError::Encoding, true);
        handle.record_payload_loss(DeliveryError::Capacity, false);
        // Only the loss that may have dropped offered blobs makes the
        // streams carry their definition groups again.
        assert_eq!(handle.carried_resets(), 1);
        handle.record_capture_loss(false);
        assert_eq!(handle.carried_resets(), 1);
        handle.record_capture_loss(true);
        assert_eq!(handle.carried_resets(), 2);
        assert_eq!(handle.loss_count(), u64::MAX);
        assert_eq!(handle.last_error(), Some(DeliveryError::Capacity));
        assert!(!handle.is_disabled());
        let progress = handle.take_progress();
        assert!(progress.reset_cas);
        assert_eq!(progress.recording_acked_through, 7);
        let progress = handle.take_progress();
        assert!(!progress.reset_cas);
        assert_eq!(progress.recording_acked_through, 7);
        assert_eq!(delivery.finish(), Err(DeliveryError::Capacity));
    }
}
