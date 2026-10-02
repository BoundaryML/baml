//! Bounded, process-local delivery. There is no durable spool or URL renewal.
use std::{
    fmt,
    io::{self, Write},
    sync::{Arc, Condvar, Mutex},
    thread,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use btel_recorder::SealedFile;
use btel_snapshot::{BlobIndex, BlobScratch, CasId, Snapshot};
use bytes::Bytes;
use futures::FutureExt;
use prost::Message;
use reqwest::{Client, StatusCode, Url};
use sha2::{Digest, Sha256};
use tokio::sync::{Semaphore, mpsc, watch};

use crate::{
    liveness::{Heartbeat, HeartbeatError},
    plan::{validate_proposal, validate_response, validate_url},
    proto::{CasObject, CloudUploadEnvelope},
    wire::{
        CasCandidate, CasSizeClass, Disposition, PrepareUploadsRequest, PrepareUploadsResponse,
        ProducerState, ProposedUploadTarget, RecordingDescriptor, UploadKind, UploadTarget,
    },
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeliveryError {
    InvalidConfig,
    Capacity,
    Closed,
    InvalidPlan,
    Expired,
    Encoding,
    Http,
    Worker,
}

impl fmt::Display for DeliveryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "cloud recording delivery: {self:?}")
    }
}
impl std::error::Error for DeliveryError {}

/// Limits reserve retained owners and both sides of serialization before admission.
/// No capture or blob is refused for its size: one plan that needs more than
/// the whole CAS reservation waits until nothing else holds any, then goes
/// alone. HTTP is opt-in for local tests; production endpoints must use HTTPS.
#[derive(Clone)]
pub struct DeliveryConfig {
    pub prepare_base_url: Url,
    pub bearer_token: Option<String>,
    pub max_pending_plans: usize,
    /// At most 32 captures, leaving default snapshot-pool headroom for VM
    /// capture. A capture spanning plans counts once per plan.
    pub max_pending_snapshots: usize,
    pub recording_reserved_bytes: usize,
    /// Memory for the captures and CAS bodies of the plans admitted together.
    pub cas_reserved_bytes: usize,
    pub max_request_bytes: usize,
    pub max_response_bytes: usize,
    /// Blobs per plan.
    pub max_candidates: usize,
    pub max_targets: usize,
    pub max_recording_body_bytes: usize,
    pub request_timeout: Duration,
    pub retry_delay: Duration,
    /// Initial attempt plus retries. Nonretryable HTTP and invalid plans drop immediately.
    pub max_attempts: u32,
    pub allow_http: bool,
}

impl DeliveryConfig {
    /// Configure a parsed BCS endpoint with default delivery limits.
    pub fn new(prepare_base_url: Url) -> Self {
        Self {
            prepare_base_url,
            bearer_token: None,
            max_pending_plans: 4,
            max_pending_snapshots: 32,
            recording_reserved_bytes: 128 * 1024 * 1024,
            cas_reserved_bytes: 256 * 1024 * 1024,
            max_request_bytes: 64 * 1024,
            max_response_bytes: 64 * 1024,
            max_candidates: 32,
            max_targets: 33,
            max_recording_body_bytes: 8 * 1024 * 1024,
            request_timeout: Duration::from_secs(10),
            retry_delay: Duration::from_millis(200),
            max_attempts: 4,
            allow_http: false,
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

    fn retry_window(&self) -> Result<Duration, DeliveryError> {
        self.request_timeout
            .checked_mul(self.max_attempts)
            .and_then(|v| {
                self.retry_delay
                    .checked_mul(self.max_attempts - 1)
                    .and_then(|delay| v.checked_add(delay))
            })
            .ok_or(DeliveryError::InvalidConfig)
    }

    fn validate(&self) -> Result<(), DeliveryError> {
        let base = &self.prepare_base_url;
        validate_url(base, self.allow_http).map_err(|_| DeliveryError::InvalidConfig)?;
        if base.query().is_some() || base.as_str().len() > 8192 {
            return Err(DeliveryError::InvalidConfig);
        }
        if let Some(token) = &self.bearer_token {
            reqwest::header::HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| DeliveryError::InvalidConfig)?;
        }
        if self.max_attempts == 0
            || self.request_timeout.is_zero()
            || self.max_pending_plans == 0
            || self.max_pending_plans > 32
            || self.max_pending_snapshots == 0
            || self.max_pending_snapshots > 32
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
    metadata_replay_evictions: u64,
    progress: DeliveryProgress,
    recording_bytes: usize,
    cas_bytes: usize,
    snapshots: usize,
    plans: usize,
    cas_targets: usize,
    last_file: Option<([u8; 16], u64)>,
    finished: bool,
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
    fn lose(&self, error: DeliveryError, reset_cas: bool) {
        let mut state = self.state.lock().unwrap();
        if state.failure.is_some() {
            return;
        }
        state.last_error = Some(error);
        state.loss_count = state.loss_count.saturating_add(1);
        state.progress.reset_cas |= reset_cas;
    }

    fn fail(&self, error: DeliveryError) {
        let (first, sender) = {
            let mut state = self.state.lock().unwrap();
            if state.failure.is_some() {
                (false, None)
            } else {
                state.failure = Some(error);
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

struct Reservation {
    shared: Arc<Shared>,
    recording: usize,
    cas: usize,
    snapshots: usize,
    cas_targets: usize,
}

impl Reservation {
    fn release_snapshot_slots(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.snapshots -= std::mem::take(&mut self.snapshots);
        self.shared.capacity.notify_all();
    }
}

impl Drop for Reservation {
    fn drop(&mut self) {
        let mut state = self.shared.state.lock().unwrap();
        state.recording_bytes -= self.recording;
        state.cas_bytes -= self.cas;
        state.snapshots -= self.snapshots;
        state.plans -= 1;
        state.cas_targets -= self.cas_targets;
        self.shared.capacity.notify_all();
    }
}

/// One blob offered with a file: which capture holds it, and where.
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    /// Index into the window's owners.
    pub owner: u32,
    pub blob: BlobIndex,
    pub id: CasId,
    pub encoded_len: u64,
}

/// A file's candidates and the captures that hold them. Several windows may
/// hold one capture; it is freed with the last.
#[derive(Default)]
pub(crate) struct Window {
    pub(crate) owners: Vec<Arc<Snapshot>>,
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
            let owner = u32::try_from(window.owners.len()).map_err(|_| DeliveryError::Capacity)?;
            for blob in snapshot.blobs() {
                window.candidates.push(Candidate {
                    owner,
                    blob: blob.index(),
                    id: blob.id(),
                    encoded_len: blob.encoded_len(),
                });
            }
            window.sizes.push(snapshot.retained_bytes());
            window.releases.push(true);
            window.owners.push(Arc::new(snapshot));
        }
        Ok(window)
    }
}

struct Work {
    file: SealedFile,
    window: Window,
    request: PrepareUploadsRequest,
    reservation: Reservation,
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

    /// Most recent loss, or the fatal lifecycle error if delivery was disabled.
    pub fn last_error(&self) -> Option<DeliveryError> {
        let state = self.shared.state.lock().unwrap();
        state.failure.or(state.last_error)
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
                self.shared.lose(error, true);
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
        if let Err(error) = result {
            if error != DeliveryError::Closed {
                self.shared.lose(error, true);
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
            || candidates
                .iter()
                .any(|c| c.owner as usize >= window.owners.len())
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
        let mut retained = window
            .owners
            .capacity()
            .checked_mul(std::mem::size_of::<Arc<Snapshot>>())
            .and_then(|v| {
                candidates
                    .capacity()
                    .checked_mul(std::mem::size_of::<Candidate>())
                    .and_then(|c| v.checked_add(c))
            })
            .ok_or(DeliveryError::Capacity)?;
        for size in &window.sizes {
            retained = retained.checked_add(*size).ok_or(DeliveryError::Capacity)?;
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
        let cas_targets = request
            .proposed_uploads
            .iter()
            .filter(|p| p.kind != UploadKind::Recording)
            .count();
        // Envelopes coexist with their encoded body during prost encoding. Response
        // JSON allocations and parsed strings are bounded conservatively as control bytes.
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
        let recording = config
            .max_recording_body_bytes
            .checked_mul(2)
            .and_then(|v| v.checked_add(file.retained_bytes()))
            .and_then(|v| v.checked_add(control))
            .and_then(|v| v.checked_add(proposal_bytes))
            .and_then(|v| v.checked_add(metadata_bytes))
            .ok_or(DeliveryError::Capacity)?;
        // A CAS body is as long as its blobs, and coexists with its envelope
        // while that is encoded.
        let bodies = request
            .proposed_uploads
            .iter()
            .filter(|p| p.kind != UploadKind::Recording)
            .flat_map(|p| &p.candidate_indices)
            .try_fold(0_usize, |sum, &index| {
                usize::try_from(candidates[index as usize].encoded_len)
                    .ok()
                    .and_then(|len| len.checked_add(CAS_OBJECT_OVERHEAD_BYTES))
                    .and_then(|len| sum.checked_add(len))
            })
            .ok_or(DeliveryError::Capacity)?;
        let cas = bodies
            .checked_mul(2)
            .and_then(|v| v.checked_add(retained))
            .ok_or(DeliveryError::Capacity)?;
        if recording > config.recording_reserved_bytes {
            return Err(DeliveryError::Capacity);
        }
        let mut state = self.shared.state.lock().unwrap();
        let identity = (*file.recording_id().as_bytes(), file.sequence().get());
        loop {
            if let Some(error) = state.failure {
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
                // What exceeds the whole reservation goes alone.
                || !(state.cas_bytes == 0
                    || fits(state.cas_bytes, cas, config.cas_reserved_bytes))
            {
                state = self.shared.capacity.wait(state).unwrap();
                continue;
            }
            break;
        }
        state.recording_bytes += recording;
        state.cas_bytes += cas;
        state.snapshots += window.owners.len();
        state.plans += 1;
        state.cas_targets += cas_targets;
        state.last_file = Some(identity);
        let reservation = Reservation {
            shared: self.shared.clone(),
            recording,
            cas,
            snapshots: window.owners.len(),
            cas_targets,
        };
        let work = Work {
            file,
            window,
            request,
            reservation,
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
        state.failure.or(state.last_error).map_or(Ok(()), Err)
    }

    pub fn result(&self) -> Option<Result<(), DeliveryError>> {
        let state = self.handle.shared.state.lock().unwrap();
        if let Some(error) = state.failure.or(state.last_error) {
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
    /// For exactly `len` bytes, allocated at once. A length that cannot be
    /// allocated is a loss, not an abort.
    fn exact(len: usize) -> Result<Self, DeliveryError> {
        let mut bytes = Vec::new();
        bytes
            .try_reserve_exact(len)
            .map_err(|_| DeliveryError::Capacity)?;
        Ok(Self { bytes, limit: len })
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
    heartbeat: &Heartbeat,
) -> Result<PrepareUploadsResponse, DeliveryError> {
    let url = config.endpoint(&request.recording.recording_id, "uploads:prepare")?;
    let key = format!(
        "{}:{}",
        request.recording.recording_id, request.recording.recording_file_sequence
    );
    for attempt in 0..config.max_attempts {
        let decorated = heartbeat.decorate_prepare(request);
        let mut writer = LimitedWriter::new(config.max_request_bytes);
        serde_json::to_writer(&mut writer, decorated.as_ref().unwrap_or(request))
            .map_err(|_| DeliveryError::Capacity)?;
        let body = Bytes::from(writer.bytes);
        let mut post = client
            .post(url.clone())
            .header("content-type", "application/json")
            .header("idempotency-key", &key)
            .body(body.clone());
        if let Some(token) = &config.bearer_token {
            post = post.bearer_auth(token);
        }
        match post.send().await {
            Ok(response) if response.status().is_success() => {
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
            Ok(response) if !retryable(response.status()) => return Err(DeliveryError::Http),
            _ => {}
        }
        if attempt + 1 < config.max_attempts {
            tokio::time::sleep(config.retry_delay).await;
        }
    }
    Err(DeliveryError::Http)
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
    body: Bytes,
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
    headers.entry(reqwest::header::CONTENT_TYPE).or_insert(
        reqwest::header::HeaderValue::from_static("application/x-protobuf"),
    );
    for attempt in 0..config.max_attempts {
        let remaining = expiry
            .checked_sub(now_ms()?)
            .filter(|&ms| ms > 0)
            .ok_or(DeliveryError::Expired)?;
        let request = client
            .put(&target.presigned_put_url)
            .timeout(config.request_timeout.min(Duration::from_millis(remaining)))
            .headers(headers.clone())
            .body(body.clone());
        match request.send().await {
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
    recording: Option<(UploadTarget, Bytes)>,
    recording_sequence: u64,
    cas: Vec<(UploadTarget, Bytes)>,
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
    } = work;
    let response = prepare(client, config, &request, &reservation.shared.heartbeat).await?;
    let retry_window = config.retry_window()?;
    let now = now_ms()?;
    let horizon = |windows: usize| {
        u32::try_from(windows)
            .ok()
            .and_then(|n| retry_window.checked_mul(n.max(1)))
            .and_then(|d| u64::try_from(d.as_millis()).ok())
            .and_then(|ms| now.checked_add(ms))
            .ok_or(DeliveryError::Expired)
    };
    validate_response(&request, &response, config.allow_http)?;
    if response.expires_at_unix_ms <= horizon(1)? {
        return Err(DeliveryError::Expired);
    }
    let actual_cas = response
        .uploads
        .iter()
        .filter(|u| u.kind != UploadKind::Recording)
        .count();
    let (recording_horizon, cas_horizon) = {
        let mut state = reservation.shared.state.lock().unwrap();
        state.cas_targets -= reservation.cas_targets - actual_cas;
        reservation.cas_targets = actual_cas;
        (horizon(state.plans)?, horizon(state.cas_targets)?)
    };
    // A candidate serializes at most once; an available one never does.
    let mut taken = vec![false; window.candidates.len()];
    for disposition in &response.cas {
        if matches!(disposition.disposition, Disposition::AlreadyAvailable {}) {
            taken[disposition.candidate_index as usize] = true;
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
            let mut envelope = CloudUploadEnvelope {
                format_version: 1,
                plan_id: response.plan_id.clone(),
                upload_id: target.upload_id.clone(),
                recording_file: (target.kind == UploadKind::Recording)
                    .then(|| file.bytes().to_vec()),
                cas_objects: Vec::new(),
            };
            if envelope.encoded_len() > limit {
                return Err(DeliveryError::Capacity);
            }
            for &index in &target.candidate_indices {
                let candidate = &window.candidates[index as usize];
                if std::mem::replace(&mut taken[index as usize], true) {
                    return Err(DeliveryError::InvalidPlan);
                }
                let len = usize::try_from(candidate.encoded_len)
                    .ok()
                    .filter(|&len| len <= limit - envelope.encoded_len())
                    .ok_or(DeliveryError::Capacity)?;
                let mut writer = LimitedWriter::exact(len)?;
                window.owners[candidate.owner as usize]
                    .blob(candidate.blob)
                    .write(&mut scratch, &mut writer)
                    .map_err(|_| DeliveryError::Encoding)?;
                let object = CasObject {
                    snapshot_id: candidate.id.as_bytes().to_vec(),
                    snapshot_format_version: btel_settings::snapshot::BLOB_VERSION,
                    blob_sha256: Sha256::digest(&writer.bytes).to_vec(),
                    blob: writer.bytes,
                };
                envelope.cas_objects.push(object);
                if envelope.encoded_len() > limit {
                    return Err(DeliveryError::Capacity);
                }
            }
            let mut body = Vec::new();
            body.try_reserve_exact(envelope.encoded_len())
                .map_err(|_| DeliveryError::Capacity)?;
            envelope
                .encode(&mut body)
                .map_err(|_| DeliveryError::Encoding)?;
            drop(envelope);
            Ok(Bytes::from(body))
        })();
        let body = match body {
            Ok(body) => body,
            Err(error) => {
                reservation
                    .shared
                    .lose(error, !target.candidate_indices.is_empty());
                for &index in &target.candidate_indices {
                    taken[index as usize] = true;
                }
                continue;
            }
        };
        if target.kind == UploadKind::Recording {
            recording = Some((target, body));
        } else {
            cas.push((target, body));
        }
    }
    // Every candidate was serialized or skipped: the captures can go.
    drop(window);
    let recording_sequence = file.sequence().get();
    drop(file);
    reservation.release_snapshot_slots();
    Ok(Prepared {
        recording,
        recording_sequence,
        cas,
        expiry: response.expires_at_unix_ms,
        reservation,
    })
}

async fn upload(
    client: &Client,
    config: &DeliveryConfig,
    prepared: Prepared,
    recording_slots: Arc<Semaphore>,
    cas_slots: Arc<Semaphore>,
) {
    let Prepared {
        recording,
        recording_sequence,
        cas,
        expiry,
        reservation,
    } = prepared;
    let shared = &reservation.shared;
    let recording_lane = async {
        if let Some((target, body)) = recording {
            let _slot = recording_slots
                .acquire()
                .await
                .expect("recording lane open");
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
    };
    let cas_lane = async {
        if cas.is_empty() {
            return;
        }
        let _slot = cas_slots.acquire().await.expect("CAS lane open");
        for (target, body) in cas {
            if let Err(error) = put(client, config, target, body, expiry).await {
                shared.lose(error, true);
            }
        }
    };
    tokio::join!(recording_lane, cas_lane);
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
    let recording_slots = Arc::new(Semaphore::new(1));
    let cas_slots = Arc::new(Semaphore::new(1));
    let mut uploads = tokio::task::JoinSet::new();
    let mut heartbeats = tokio::task::JoinSet::new();
    let mut heartbeat_started = false;
    loop {
        if shared.state.lock().unwrap().failure.is_some() {
            break;
        }
        tokio::select! {
            completed = uploads.join_next(), if !uploads.is_empty() => {
                if completed.is_some_and(|result| result.is_err()) {
                    shared.fail(DeliveryError::Worker);
                }
            }
            work = receiver.recv(), if uploads.len() < shared.config.max_pending_plans => {
                let Some(work) = work else { break };
                if !heartbeat_started {
                    heartbeat_started = true;
                    match shared.config.endpoint(&work.request.recording.recording_id, "heartbeat") {
                        Ok(endpoint) => {
                            let heartbeat = shared.heartbeat.clone();
                            let client = client.clone();
                            let token = shared.config.bearer_token.clone();
                            let timeout = shared.config.request_timeout;
                            heartbeats.spawn(async move {
                                if std::panic::AssertUnwindSafe(
                                    heartbeat.run(client, endpoint, token, timeout),
                                ).catch_unwind().await.is_err() {
                                    heartbeat.observe_error(HeartbeatError::Worker);
                                }
                            });
                        }
                        Err(_) => shared.heartbeat.observe_error(HeartbeatError::InvalidEndpoint),
                    }
                }
                match assemble(&client, &shared.config, work).await {
                    Ok(prepared) => {
                        let client = client.clone();
                        let shared = shared.clone();
                        let recording_slots = recording_slots.clone();
                        let cas_slots = cas_slots.clone();
                        uploads.spawn(async move {
                            upload(
                                &client, &shared.config, prepared, recording_slots, cas_slots,
                            ).await;
                        });
                    }
                    Err(error) => {
                        shared.lose(error, true);
                    }
                }
            }
        }
    }
    drop(receiver);
    if shared.state.lock().unwrap().failure.is_some() {
        uploads.abort_all();
    }
    while let Some(result) = uploads.join_next().await {
        if result.is_err()
            && !result
                .as_ref()
                .is_err_and(tokio::task::JoinError::is_cancelled)
        {
            shared.fail(DeliveryError::Worker);
        }
        if shared.state.lock().unwrap().failure.is_some() {
            uploads.abort_all();
        }
    }
    shared.heartbeat.stop();
    while heartbeats.join_next().await.is_some() {}
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mut local = DeliveryConfig::new("http://localhost:8080".parse().unwrap());
        local.allow_http = true;
        assert_eq!(local.validate(), Ok(()));
        local.bearer_token = Some("invalid\ntoken".into());
        assert_eq!(local.validate(), Err(DeliveryError::InvalidConfig));
        let oversized = format!("https://bcs.example/{}", "x".repeat(8192));
        assert_eq!(
            DeliveryConfig::new(oversized.parse().unwrap()).validate(),
            Err(DeliveryError::InvalidConfig)
        );
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
