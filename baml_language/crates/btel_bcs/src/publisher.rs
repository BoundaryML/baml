use std::{
    collections::{HashSet, VecDeque},
    sync::Arc,
    time::Instant,
};

use btel_processor::{AggregateDelta, Publisher};
use btel_recorder::{RecordingBuilder, RecordingConfig, RecordingError, RecordingId, SealedFile};
use btel_records::SpanRecord;
use btel_snapshot::{BlobIndex, CasId, Leaf, Snapshot, Split, Structure};
use btel_types::TelemetryId;

use crate::{
    delivery::{BcsDeliveryHandle, Candidate, DeliveryError, Source, Window},
    metadata::MetadataJournal,
    wire::{ProposedUploadTarget, UploadKind},
};

#[derive(Clone, Debug)]
pub struct CloudPublisherConfig {
    /// Bounded recent-offer deduplication of blob IDs, not authoritative CAS
    /// availability.
    pub max_offered_ids: usize,
    /// Soft per-file targets: blobs waiting for a file, and the retained
    /// memory of the captures they belong to. A single capture may exceed
    /// either.
    pub candidate_target: usize,
    pub retained_bytes_target: usize,
    /// Hard count of captures held across the queue and staged files, not
    /// per batch.
    pub max_pending_snapshots: usize,
    /// Sum of `Snapshot::retained_bytes` across those captures. No capture
    /// is refused for its size: one larger than all of this is held alone.
    /// Recording buffers have separate delivery limits; bookkeeping is
    /// count-bounded.
    pub max_retained_bytes: usize,
    pub inline_target_bytes: usize,
    pub batch_target_bytes: usize,
    pub standalone_threshold_bytes: usize,
}

impl Default for CloudPublisherConfig {
    fn default() -> Self {
        Self {
            max_offered_ids: 65_536,
            candidate_target: 16,
            retained_bytes_target: 4 * 1024 * 1024,
            max_pending_snapshots: 32,
            max_retained_bytes: 8 * 1024 * 1024,
            inline_target_bytes: 16 * 1024,
            batch_target_bytes: 256 * 1024,
            standalone_threshold_bytes: 256 * 1024,
        }
    }
}

/// Owns cloud-specific candidate bookkeeping; HTTP runs on the delivery worker.
///
/// Candidates are blobs. A capture's blobs join a queue, children first, and
/// each sealed file takes the queue's head, so one capture may span several
/// files and plans. A blob that is one string holds its own content and is
/// let go on its own; the rest of the capture is held until the last blob
/// written from it leaves, and a capture that is one string is not held at
/// all. The queue and staged files share capture count/byte budgets. A
/// capture is never refused for its size: before one that does not fit, what
/// is held is sent, waiting on delivery's admission, and one larger than the
/// byte budget is then held alone. Blobs a sealed file left queued are due
/// as events are: they start the next file's deadline, and a due file is
/// followed by files carrying whatever it did not take. A capture queued
/// before the first event (the sources) waits for that event's file. A
/// record can seal
/// a file but cannot deliver it until input chunks are recycled. Staging is
/// additionally bounded by delivery's plan and recording-byte limits. Neither
/// sealing nor staging releases a capture; only delivery or terminal cleanup
/// does. Combined publisher/delivery capture limits leave room in the minimum
/// VM pool for capture to publish its partial chunk.
///
/// Cloud processing detaches at most one chunk, recycling its allocation before
/// any admission wait. The unvisited suffix still owns its captures in the shared,
/// bounded VM snapshot pool; it is not extra delivery capacity. The processor
/// reserves chunk-capacity-sized span and timing buffers, with only one live at
/// a time. Handoffs occur between records and cache deltas, not between chunks.
pub struct CloudPublisher {
    recording: RecordingBuilder,
    recording_target: usize,
    config: CloudPublisherConfig,
    /// Recently offered blob IDs, oldest first.
    offered: HashSet<CasId>,
    offered_order: VecDeque<CasId>,
    /// Blob IDs queued or staged, whatever `offered` retains.
    inflight: HashSet<CasId>,
    metadata: MetadataJournal,
    /// Captures that queued blobs are written from, oldest first;
    /// `first_owner` numbers the front.
    owners: VecDeque<Owner>,
    first_owner: u64,
    next_owner: u64,
    /// Blobs waiting for a file, oldest first. A capture's blobs are
    /// contiguous, so captures complete in order.
    queue: VecDeque<Queued>,
    /// Retained memory of the captures in `owners` and of the queued strings.
    queued_bytes: usize,
    staged: Vec<PendingFile>,
    /// Captures held anywhere in this publisher, and the retained memory of
    /// those and of the strings held.
    retained_snapshots: usize,
    retained_bytes: usize,
    staged_recording_bytes: usize,
    preflight_size: Option<(CasId, usize)>,
    failure: Option<DeliveryError>,
    delivery: BcsDeliveryHandle,
}

/// A capture that blobs still queued are written from.
struct Owner {
    structure: Arc<Structure>,
    size: usize,
    queued: u32,
}

/// A blob waiting for a file.
struct Queued {
    id: CasId,
    encoded_len: u64,
    held: Held,
}

/// Where a queued blob's bytes come from.
enum Held {
    /// A string blob, which holds its own content.
    Leaf(Leaf),
    /// A blob written from the capture numbered `owner`.
    Kept { owner: u64, blob: BlobIndex },
}

struct PendingFile {
    file: SealedFile,
    window: Window,
}

/// Whether servicing seals the open file. Only `End` claims `RecordingEnd`.
#[derive(Clone, Copy)]
enum Seal {
    IfDue,
    Now,
    /// Seal files only to carry the blobs the end file cannot take.
    Carriers,
    /// Seal files only to carry every queued blob.
    Drain,
    End,
}

/// How much of the queue goes in files that carry nothing else.
#[derive(Clone, Copy)]
enum Carry {
    /// While a file's worth of blobs waits, or more than one file takes.
    Pressure,
    /// Until one more file takes the rest.
    Overflow,
    /// All of it.
    Everything,
}

impl CloudPublisher {
    pub fn new(
        id: RecordingId,
        recording: RecordingConfig,
        config: CloudPublisherConfig,
        delivery: BcsDeliveryHandle,
    ) -> Result<Self, RecordingError> {
        let limits = delivery.config();
        if config.max_offered_ids == 0
            || config.candidate_target == 0
            || config.candidate_target > limits.max_candidates
            || config.candidate_target >= limits.max_targets
            || config.max_pending_snapshots == 0
            || config.max_pending_snapshots
                >= btel_settings::snapshot::MIN_SNAPSHOT_SLOTS
                    .saturating_sub(limits.max_pending_snapshots)
            || config.retained_bytes_target == 0
            || config.retained_bytes_target > config.max_retained_bytes
            || config.max_retained_bytes == 0
            || config.batch_target_bytes == 0
            || config.standalone_threshold_bytes == 0
            || recording.target_bytes.get() > limits.max_recording_body_bytes
        {
            return Err(RecordingError::InvalidConfig);
        }
        Ok(Self {
            recording: RecordingBuilder::new(id, recording)?.with_metadata_replay(),
            recording_target: recording.target_bytes.get(),
            config,
            offered: HashSet::new(),
            offered_order: VecDeque::new(),
            inflight: HashSet::new(),
            metadata: MetadataJournal::new(limits.max_recording_body_bytes),
            owners: VecDeque::new(),
            first_owner: 0,
            next_owner: 0,
            queue: VecDeque::new(),
            queued_bytes: 0,
            staged: Vec::new(),
            retained_snapshots: 0,
            retained_bytes: 0,
            staged_recording_bytes: 0,
            preflight_size: None,
            failure: None,
            delivery,
        })
    }

    #[must_use]
    pub fn with_source_snapshot(mut self, source_snapshot_id: Option<[u8; 32]>) -> Self {
        self.recording = self.recording.with_source_snapshot(source_snapshot_id);
        self
    }

    /// The process description; its sources capture is offered like any
    /// other capture.
    #[must_use]
    pub fn with_process(mut self, process: btel_recorder::ProcessRecording) -> Self {
        self.recording = self.recording.with_process(process);
        let captures: Vec<_> = self.recording.take_snapshots().collect();
        for capture in captures {
            self.retain(capture);
        }
        self
    }

    /// Owned function definitions copied before execution, shared with the recording builder.
    #[must_use]
    pub fn with_function_metadata(
        mut self,
        functions: std::sync::Arc<btel_types::FunctionMetadataTable>,
    ) -> Self {
        self.recording = self.recording.with_function_metadata(functions);
        self
    }

    fn fail(&mut self, error: DeliveryError) {
        self.failure.get_or_insert(error);
        self.owners.clear();
        self.first_owner = self.next_owner;
        self.queue.clear();
        self.queued_bytes = 0;
        self.staged.clear();
        self.offered.clear();
        self.offered_order.clear();
        self.inflight.clear();
        self.metadata = MetadataJournal::new(self.delivery.config().max_recording_body_bytes);
        self.retained_snapshots = 0;
        self.retained_bytes = 0;
        self.staged_recording_bytes = 0;
        self.preflight_size = None;
        self.recording.discard_pending();
    }

    fn already_offered(&self, id: CasId) -> bool {
        self.inflight.contains(&id) || self.offered.contains(&id)
    }

    fn remember(&mut self, id: CasId) {
        if self.offered.contains(&id) {
            return;
        }
        if self.offered.len() == self.config.max_offered_ids {
            if let Some(oldest) = self.offered_order.pop_front() {
                self.offered.remove(&oldest);
            }
        }
        self.offered.insert(id);
        self.offered_order.push_back(id);
    }

    fn progress(&mut self) {
        let progress = self.delivery.take_progress();
        self.metadata.acknowledge(progress.recording_acked_through);
        if progress.reset_cas {
            // Queued and staged blobs stay protected by `inflight`.
            self.offered.clear();
            self.offered_order.clear();
        }
    }

    /// Whether `size` more may be held, with the storages of `slots`
    /// captures: within the budgets, or alone, however large it is.
    fn has_room(&self, size: usize, slots: usize) -> bool {
        self.retained_snapshots.saturating_add(slots) <= self.config.max_pending_snapshots
            && (self.retained_bytes == 0
                || self
                    .retained_bytes
                    .checked_add(size)
                    .is_some_and(|total| total <= self.config.max_retained_bytes))
    }

    /// Send what is held: the open file with the queue's head, then files
    /// carrying the rest for as long as `more` asks and any can be sent.
    /// Delivery's admission is the wait.
    fn send(&mut self, more: impl Fn(&Self) -> bool) {
        self.service(Instant::now(), Seal::Now);
        while self.failure.is_none() && !self.queue.is_empty() && more(self) {
            let waiting = self.queue.len();
            self.service(Instant::now(), Seal::Drain);
            if self.queue.len() == waiting {
                break;
            }
        }
    }

    /// Seal the open file if it is due and submit what is staged. The
    /// deadline is for everything that waits, so a due file is followed by
    /// files carrying the rest of the queue.
    fn tick(&mut self, now: Instant) {
        if self
            .recording
            .deadline()
            .is_some_and(|deadline| now >= deadline)
        {
            self.send(|_| true);
        } else {
            self.service(now, Seal::IfDue);
        }
    }

    /// Queue the capture's blobs that were not offered recently, whatever
    /// their size, and let go of the rest at once. A capture is lost only
    /// when it has no room and arrived without the chance to make any
    /// (`before_detached_span`).
    fn retain(&mut self, snapshot: Snapshot) {
        let preflight = self.preflight_size.take();
        if self.failure.is_some()
            || !snapshot
                .blobs()
                .any(|blob| !self.already_offered(blob.id()))
        {
            return;
        }
        let whole = match preflight {
            Some((id, size)) if id == snapshot.root_id() => size,
            Some(_) | None => snapshot.retained_bytes(),
        };
        let Split { leaves, structure } = snapshot.split();
        // What the strings took with them is no longer the capture's.
        let given = leaves
            .iter()
            .map(|leaf| leaf.retained_bytes() - size_of::<Leaf>())
            .fold(0, usize::saturating_add);
        let leaves: Vec<_> = leaves
            .into_iter()
            .filter(|leaf| !self.already_offered(leaf.id()))
            .collect();
        let kept: Vec<_> = structure
            .iter()
            .flat_map(Structure::blobs)
            .filter(|blob| !self.already_offered(blob.id()))
            .map(|blob| (blob.index(), blob.id(), blob.encoded_len()))
            .collect();
        // A capture with nothing left to write is let go too.
        let structure = structure
            .filter(|_| !kept.is_empty())
            .map(|structure| (structure, whole.saturating_sub(given)));
        let size = leaves
            .iter()
            .map(Leaf::retained_bytes)
            .chain(structure.iter().map(|(_, size)| *size))
            .fold(0, usize::saturating_add);
        if !self.has_room(size, usize::from(structure.is_some())) {
            self.delivery
                .record_payload_loss(DeliveryError::Capacity, false);
            return;
        }
        for leaf in leaves {
            let (id, bytes) = (leaf.id(), leaf.retained_bytes());
            self.remember(id);
            self.inflight.insert(id);
            self.queue.push_back(Queued {
                id,
                encoded_len: leaf.encoded_len(),
                held: Held::Leaf(leaf),
            });
            self.retained_bytes += bytes;
            self.queued_bytes += bytes;
        }
        let Some((structure, size)) = structure else {
            return;
        };
        let owner = self.next_owner;
        self.next_owner += 1;
        self.owners.push_back(Owner {
            structure: Arc::new(structure),
            size,
            queued: u32::try_from(kept.len()).expect("bounded blob count"),
        });
        for (blob, id, encoded_len) in kept {
            self.remember(id);
            self.inflight.insert(id);
            self.queue.push_back(Queued {
                id,
                encoded_len,
                held: Held::Kept { owner, blob },
            });
        }
        self.retained_snapshots += 1;
        self.retained_bytes += size;
        self.queued_bytes += size;
    }

    /// How many of the queue's blobs the next file takes: a plan's worth,
    /// cut short where the bytes their bodies buffer pass a quarter of
    /// delivery's CAS reservation, which such a blob is charged against in
    /// its body and again in its capture. A string sent from where it is
    /// held buffers next to nothing, so any number of those go together. The
    /// first blob always goes, whatever its size.
    fn window_len(&self) -> usize {
        let config = self.delivery.config();
        let limit = (config.cas_reserved_bytes / 4) as u64;
        let mut bytes = 0_u64;
        self.queue
            .iter()
            .take(config.max_candidates)
            .enumerate()
            .take_while(|(index, queued)| {
                let leaf = match &queued.held {
                    Held::Leaf(leaf) => Some(leaf),
                    Held::Kept { .. } => None,
                };
                bytes = bytes.saturating_add(crate::body::buffered_len(leaf, queued.encoded_len));
                *index == 0 || bytes <= limit
            })
            .count()
    }

    /// Take the queue's head for one file.
    fn take_window(&mut self) -> Window {
        let mut window = Window::default();
        let mut current: Option<(u64, u32)> = None;
        for _ in 0..self.window_len() {
            let Some(queued) = self.queue.pop_front() else {
                unreachable!("a window is no longer than the queue")
            };
            let source = match queued.held {
                Held::Leaf(leaf) => {
                    self.queued_bytes -= leaf.retained_bytes();
                    Source::Leaf(leaf)
                }
                Held::Kept { owner, blob } => {
                    let at = usize::try_from(owner - self.first_owner).expect("owner in queue");
                    let index = match current {
                        Some((held, index)) if held == owner => index,
                        Some(_) | None => {
                            let held = &self.owners[at];
                            window.owners.push(Arc::clone(&held.structure));
                            window.sizes.push(held.size);
                            window.releases.push(false);
                            let index =
                                u32::try_from(window.owners.len() - 1).expect("bounded owners");
                            current = Some((owner, index));
                            index
                        }
                    };
                    let held = &mut self.owners[at];
                    held.queued -= 1;
                    if held.queued == 0 {
                        window.releases[index as usize] = true;
                    }
                    Source::Kept { owner: index, blob }
                }
            };
            window.candidates.push(Candidate {
                id: queued.id,
                encoded_len: queued.encoded_len,
                source,
            });
        }
        // Captures complete in queue order.
        while self.owners.front().is_some_and(|owner| owner.queued == 0) {
            let owner = self.owners.pop_front().expect("checked front");
            self.queued_bytes -= owner.size;
            self.first_owner += 1;
        }
        window
    }

    /// The window leaves this publisher, delivered or lost.
    fn release_window(&mut self, window: &Window) {
        for candidate in &window.candidates {
            self.inflight.remove(&candidate.id);
            match &candidate.source {
                Source::Leaf(leaf) => self.retained_bytes -= leaf.retained_bytes(),
                Source::Kept { .. } => {}
            }
        }
        for (index, size) in window.sizes.iter().enumerate() {
            if window.releases[index] {
                self.retained_snapshots -= 1;
                self.retained_bytes -= size;
            }
        }
    }

    /// Stage a sealed file with the queue's head. Sealing a file ended the
    /// open file's deadline; blobs it left queued start the next file's.
    /// Nothing sealed starts nothing: blobs queued before the first event
    /// wait for it.
    fn stage(&mut self, sealed: Result<Option<SealedFile>, RecordingError>) {
        let left = matches!(sealed, Ok(Some(_)));
        self.stage_file(sealed);
        if left && self.failure.is_none() && !self.queue.is_empty() {
            self.start_window();
        }
    }

    fn stage_file(&mut self, sealed: Result<Option<SealedFile>, RecordingError>) {
        let mut file = match sealed {
            Ok(Some(file)) => file,
            Ok(None) => return,
            Err(_) => {
                self.fail(DeliveryError::Encoding);
                return;
            }
        };
        self.progress();
        let limits = self.delivery.config();
        let remaining_bytes = limits
            .recording_reserved_bytes
            .saturating_sub(self.staged_recording_bytes);
        if self.staged.len() >= limits.max_pending_plans
            || file.retained_bytes() > remaining_bytes
            || file.bytes().len() > limits.max_recording_body_bytes
        {
            // The file is lost; queued blobs wait for the next one.
            let evictions = self.metadata.retain(&file);
            self.delivery.record_metadata_replay_evictions(evictions);
            self.delivery
                .record_payload_loss(DeliveryError::Capacity, false);
            self.progress();
            return;
        }
        let body_limit = limits
            .max_recording_body_bytes
            .min(remaining_bytes.saturating_sub(std::mem::size_of::<SealedFile>()));
        let evictions = self.metadata.prepare(&mut file, body_limit);
        self.delivery.record_metadata_replay_evictions(evictions);
        self.staged_recording_bytes += file.retained_bytes();
        let window = self.take_window();
        self.staged.push(PendingFile { file, window });
    }

    /// Seal files with nothing but queued blobs, as far as `carry` asks and
    /// staging has room.
    fn carry(&mut self, carry: Carry) {
        while self.failure.is_none()
            && match carry {
                Carry::Pressure => {
                    self.queue.len() >= self.config.candidate_target
                        || self.window_len() < self.queue.len()
                }
                Carry::Overflow => self.window_len() < self.queue.len(),
                Carry::Everything => !self.queue.is_empty(),
            }
            && self.staged.len() < self.delivery.config().max_pending_plans
        {
            let sealed = self.recording.flush_carrier();
            let staged = self.staged.len();
            self.stage(sealed);
            if self.staged.len() == staged {
                // Staging refused the carrier; delivery must drain first.
                break;
            }
        }
    }

    fn seal_on_pressure(&mut self) {
        if self.failure.is_none()
            && (self.queue.len() >= self.config.candidate_target
                || self.queued_bytes >= self.config.retained_bytes_target
                || self.recording.encoded_size_hint() >= self.recording_target)
        {
            let sealed = self.recording.flush_recording();
            self.stage(sealed);
            self.carry(Carry::Pressure);
        }
    }

    fn start_window(&mut self) {
        if self.recording.deadline().is_none() {
            self.recording.start_deadline(Instant::now());
        }
    }

    fn service(&mut self, now: Instant, seal: Seal) {
        self.progress();
        if self.delivery.is_disabled() {
            self.fail(DeliveryError::Closed);
        }
        if self.failure.is_none() {
            match seal {
                Seal::IfDue => {
                    let sealed = self.recording.flush_if_due(now);
                    self.stage(sealed);
                    self.carry(Carry::Pressure);
                }
                Seal::Now => {
                    let sealed = self.recording.flush_recording();
                    self.stage(sealed);
                    self.carry(Carry::Pressure);
                }
                Seal::Carriers => self.carry(Carry::Overflow),
                Seal::Drain => self.carry(Carry::Everything),
                Seal::End => {
                    let sealed = self.recording.end_recording();
                    self.stage(sealed);
                }
            }
        }
        if self.failure.is_none() {
            let staged = std::mem::take(&mut self.staged);
            self.staged_recording_bytes = 0;
            for PendingFile { file, window } in staged {
                self.release_window(&window);
                let targets = propose(&window.candidates, &self.config);
                if let Err(error) = self.delivery.try_submit_window(file, window, targets) {
                    if self.delivery.is_disabled() {
                        self.fail(error);
                        break;
                    }
                }
            }
        }
        if let Some(error) = self.failure {
            self.delivery.disable(error);
        }
    }
}

impl Publisher<Snapshot, Snapshot> for CloudPublisher {
    const DETACH_RECORDS: bool = true;
    const ERROR_EVIDENCE: bool = true;

    fn before_detached_span(&mut self, record: &SpanRecord<Snapshot, Snapshot>) {
        self.progress();
        self.preflight_size = None;
        if self.failure.is_some() {
            return;
        }
        // What `span` will retain, in its order: the captures with a blob
        // not offered recently.
        let mut slots = 0;
        let mut size = 0_usize;
        for snapshot in record.captures() {
            if !snapshot
                .blobs()
                .any(|blob| !self.already_offered(blob.id()))
            {
                continue;
            }
            let retained = snapshot.retained_bytes();
            if slots == 0 {
                self.preflight_size = Some((snapshot.root_id(), retained));
            }
            slots += 1;
            // Each blob that is one string adds a little when it leaves
            // the capture.
            let leaves = snapshot.blobs().len().saturating_mul(size_of::<Leaf>());
            size = size.saturating_add(retained).saturating_add(leaves);
        }
        if slots > 0 && !self.has_room(size, slots) {
            // Send what is held until these may be.
            self.send(|publisher| !publisher.has_room(size, slots));
        }
    }

    fn after_detached_span(&mut self) {
        if !self.staged.is_empty() || self.failure.is_some() {
            self.tick(Instant::now());
        }
    }

    fn after_detached_aggregate(&mut self) {
        self.after_detached_span();
    }

    fn aggregate(&mut self, delta: AggregateDelta) {
        if self.failure.is_none() {
            self.start_window();
            self.recording.aggregate(delta);
            self.seal_on_pressure();
        }
    }

    fn sysop_time(&mut self, path: btel_types::CallPathId, elapsed: btel_types::ClockDuration) {
        if self.failure.is_none() {
            self.start_window();
            self.recording.sysop_time(path, elapsed);
            self.seal_on_pressure();
        }
    }

    fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        self.progress();
        if self.failure.is_none() {
            self.start_window();
            self.recording.span_reference(thread, record);
            while let Some(snapshot) = record.take_capture() {
                self.retain(snapshot);
            }
            self.seal_on_pressure();
        }
    }

    fn before_batch(&mut self, records: usize) {
        self.progress();
        if self.delivery.is_disabled() {
            self.fail(DeliveryError::Closed);
        }
        if self.failure.is_none() {
            self.recording.before_batch(records);
        }
    }

    fn before_span_chunk(&mut self, records: usize) {
        if self.failure.is_none() {
            self.recording.before_span_chunk(records);
        }
    }

    fn after_batch(&mut self, _records: usize) {
        self.tick(Instant::now());
    }

    fn max_chunks_per_batch(&self) -> usize {
        1
    }

    fn manages_flush_deadline(&self) -> bool {
        true
    }

    fn deadline(&self) -> Option<Instant> {
        if self.failure.is_some() {
            None
        } else {
            self.recording.deadline()
        }
    }

    /// Everything consumed is sent, queued blobs included.
    fn flush(&mut self) {
        self.send(|_| true);
    }

    /// Input is exhausted. Queued blobs beyond what one file takes, by count
    /// or by bytes, ship in carrier files first; then the last file takes the
    /// rest and carries
    /// `RecordingEnd` once every observed clock epoch settled (see
    /// `RecordingBuilder::end_recording`). It is submitted after every earlier
    /// file, but uploads run concurrently and earlier files or blobs may still
    /// be lost: the end declares the file count, not their arrival. A failed
    /// recording submits nothing more.
    fn finish(&mut self) {
        while self.failure.is_none() && self.window_len() < self.queue.len() {
            let waiting = self.queue.len();
            self.service(Instant::now(), Seal::Carriers);
            if self.queue.len() == waiting {
                break;
            }
        }
        self.service(Instant::now(), Seal::End);
    }
}

fn propose(candidates: &[Candidate], config: &CloudPublisherConfig) -> Vec<ProposedUploadTarget> {
    let mut targets = vec![ProposedUploadTarget {
        client_target_id: 0,
        kind: UploadKind::Recording,
        candidate_indices: Vec::new(),
    }];
    let mut inline_bytes = 0_u64;
    let mut batch = None;
    let mut batch_bytes = 0_u64;
    for (index, candidate) in candidates.iter().enumerate() {
        let index = u32::try_from(index).expect("publisher bounds candidate count");
        let size = candidate.encoded_len;
        if size >= config.standalone_threshold_bytes as u64 {
            targets.push(ProposedUploadTarget {
                client_target_id: u32::try_from(targets.len()).expect("bounded target count"),
                kind: UploadKind::CasObject,
                candidate_indices: vec![index],
            });
        } else if size <= (config.inline_target_bytes as u64).saturating_sub(inline_bytes) {
            targets[0].candidate_indices.push(index);
            inline_bytes += size;
        } else {
            if batch.is_none()
                || size > (config.batch_target_bytes as u64).saturating_sub(batch_bytes)
            {
                batch = Some(targets.len());
                batch_bytes = 0;
                targets.push(ProposedUploadTarget {
                    client_target_id: u32::try_from(targets.len()).expect("bounded target count"),
                    kind: UploadKind::CasBatch,
                    candidate_indices: Vec::new(),
                });
            }
            targets[batch.expect("created batch")]
                .candidate_indices
                .push(index);
            batch_bytes += size;
        }
    }
    targets
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroUsize, time::Duration};

    use btel_snapshot::{Limits, ShapePolicy, Shaper, SnapshotPool, SnapshotValue};
    use btel_types::{CallPathId, ClockInstant, allocate_telemetry_id};

    use super::*;
    use crate::delivery::{BcsDelivery, DeliveryConfig};

    fn publisher(config: CloudPublisherConfig) -> (BcsDelivery, CloudPublisher) {
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                ..DeliveryConfig::new("http://127.0.0.1:1".parse().unwrap())
            },
            |_| {},
        )
        .unwrap();
        let publisher = CloudPublisher::new(
            RecordingId::generate(),
            RecordingConfig::default(),
            config,
            delivery.handle(),
        )
        .unwrap();
        (delivery, publisher)
    }

    fn offer(publisher: &mut CloudPublisher, snapshot: Snapshot) {
        let thread = allocate_telemetry_id();
        publisher.span(
            thread,
            &mut SpanRecord::FunctionSpanAnnouncement {
                id: allocate_telemetry_id(),
                parent_id: thread,
                call_path: CallPathId::ROOT,
                entered_at: ClockInstant::from_ticks(1),
                captured_inputs: Some(snapshot),
                captured_type_args: None,
            },
        );
    }

    fn capture(publisher: &mut CloudPublisher, pool: &SnapshotPool, value: i64) {
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish(SnapshotValue::Int(value), &mut Shaper::default());
        offer(publisher, snapshot);
    }

    /// A list of `count` distinct strings, each in a blob of its own.
    fn cut_list(pool: &SnapshotPool, seed: i64, count: usize) -> Snapshot {
        let mut b = pool.try_acquire().unwrap();
        let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
        let list = b.list(element_type, 0..count, |leaves, index| {
            let text = format!("{seed}-{index}-{}", "x".repeat(100));
            leaves.string_value(&text.as_str().into())
        });
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        b.finish(SnapshotValue::Object(list), &mut shaper)
    }

    #[test]
    fn detached_preflight_size_is_consumed_and_failure_skips_preflight() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(2, Limits::default());
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish(SnapshotValue::Int(1), &mut Shaper::default());
        let expected = snapshot.retained_bytes();
        let id = snapshot.root_id();
        let thread = allocate_telemetry_id();
        let mut record = SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
            captured_type_args: None,
        };
        publisher.before_detached_span(&record);
        assert_eq!(publisher.preflight_size, Some((id, expected)));
        publisher.span(thread, &mut record);
        assert_eq!(publisher.preflight_size, None);
        assert_eq!(publisher.retained_bytes, expected);
        publisher.fail(DeliveryError::Capacity);
        let next = pool
            .try_acquire()
            .unwrap()
            .finish(SnapshotValue::Int(2), &mut Shaper::default());
        let SpanRecord::FunctionSpanAnnouncement {
            captured_inputs, ..
        } = &mut record
        else {
            unreachable!();
        };
        *captured_inputs = Some(next);
        publisher.before_detached_span(&record);
        assert_eq!(publisher.preflight_size, None);
        drop(record);
        assert_eq!(pool.stats().in_use, 0);
    }

    #[test]
    fn snapshots_accumulate_across_processor_batches_without_serialization() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(4, Limits::default());
        for value in 0..3 {
            publisher.before_batch(1);
            capture(&mut publisher, &pool, value);
            publisher.after_batch(1);
        }
        assert_eq!(publisher.queue.len(), 3);
        assert_eq!(publisher.owners.len(), 3);
        assert!(publisher.staged.is_empty());
        assert_eq!(publisher.retained_snapshots, 3);
        assert_eq!(
            publisher.retained_bytes,
            publisher
                .owners
                .iter()
                .map(|owner| owner.size)
                .sum::<usize>()
        );
        assert_eq!(publisher.queued_bytes, publisher.retained_bytes);
        assert_eq!(pool.stats().in_use, 3);
    }

    #[test]
    fn first_event_starts_non_sliding_deadline_and_idle_check_seals() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        assert!(publisher.deadline().is_none());
        let start = Instant::now();
        publisher.aggregate(AggregateDelta {
            count: 1,
            ..AggregateDelta::default()
        });
        let deadline = publisher.deadline().unwrap();
        let interval = RecordingConfig::default().flush_interval_duration;
        assert!(deadline >= start + interval);
        assert!(deadline <= Instant::now() + interval);
        publisher
            .recording
            .start_deadline(start + Duration::from_millis(500));
        publisher.after_batch(1);
        assert_eq!(publisher.deadline(), Some(deadline));
        assert!(
            publisher
                .recording
                .flush_if_due(deadline.checked_sub(Duration::from_nanos(1)).unwrap())
                .unwrap()
                .is_none()
        );
        let sealed = publisher.recording.flush_if_due(deadline);
        publisher.stage(sealed);
        assert_eq!(publisher.staged.len(), 1);
        assert!(publisher.deadline().is_none());
    }

    #[test]
    fn slot_pressure_seals_inside_a_multi_capture_chunk_without_delivery() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 2,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(8, Limits::default());
        publisher.before_batch(6);
        publisher.before_span_chunk(6);
        for value in 0..6 {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.staged.len(), 3);
        assert!(publisher.queue.is_empty());
        assert!(publisher.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 6);
        assert!(publisher.failure.is_none());
        assert_eq!(delivery.result(), None);
        assert!(
            publisher
                .staged
                .iter()
                .all(|file| file.window.candidates.len() == 2 && file.window.owners.len() == 2)
        );
    }

    #[test]
    fn a_capture_with_more_blobs_than_a_plan_spans_carrier_files() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 4,
            ..CloudPublisherConfig::default()
        });
        let max = delivery.handle().config().max_candidates;
        let pool = SnapshotPool::new(4, Limits::default());
        let blobs = 2 * max + 3;
        let snapshot = cut_list(&pool, 1, blobs - 1);
        assert_eq!(snapshot.blobs().len(), blobs);
        let ids: Vec<_> = snapshot.blobs().map(|blob| blob.id()).collect();
        publisher.before_batch(1);
        publisher.before_span_chunk(1);
        offer(&mut publisher, snapshot);
        // The sealed file and one carrier took a plan's worth each; the
        // rest waits for the next file.
        assert_eq!(publisher.staged.len(), 2);
        assert_eq!(publisher.queue.len(), 3);
        assert_eq!(publisher.owners.len(), 1);
        assert_eq!(publisher.retained_snapshots, 1);
        let mut delivered = Vec::new();
        for file in &publisher.staged {
            assert_eq!(file.window.candidates.len(), max);
            // Strings hold themselves: these files hold no capture, which
            // waits with its root.
            assert!(file.window.owners.is_empty());
            assert!(
                file.window
                    .candidates
                    .iter()
                    .all(|candidate| matches!(candidate.source, Source::Leaf(_)))
            );
            delivered.extend(file.window.candidates.iter().map(|c| c.id));
        }
        publisher.finish();
        // The end file took the rest; the capture is released.
        assert!(publisher.queue.is_empty());
        assert!(publisher.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 0);
        assert_eq!(publisher.retained_bytes, 0);
        assert!(publisher.inflight.is_empty());
        assert_eq!(delivered.len(), 2 * max);
        assert_eq!(delivered, ids[..2 * max]);
        assert!(publisher.failure.is_none());
    }

    /// A capture of more blobs than the files sealed for it take, offered
    /// as the only record of its chunk. Returns how many it left queued.
    fn offer_with_a_tail(publisher: &mut CloudPublisher, pool: &SnapshotPool) -> usize {
        let max = publisher.delivery.config().max_candidates;
        let snapshot = cut_list(pool, 1, 2 * max + 2);
        publisher.before_batch(1);
        publisher.before_span_chunk(1);
        offer(publisher, snapshot);
        publisher.after_detached_span();
        assert!(publisher.staged.is_empty());
        assert_eq!(publisher.owners.len(), 1);
        publisher.queue.len()
    }

    #[test]
    fn blobs_left_queued_by_a_seal_are_sent_when_the_next_deadline_passes() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 4,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(4, Limits::default());
        // The root among them: the capture is held while they wait.
        assert_eq!(offer_with_a_tail(&mut publisher, &pool), 3);
        assert_eq!(pool.stats().in_use, 1);
        // No event is pending, yet a deadline runs for them.
        assert_eq!(publisher.recording.encoded_size_hint(), 0);
        let deadline = publisher.deadline().unwrap();
        publisher.tick(deadline.checked_sub(Duration::from_nanos(1)).unwrap());
        assert_eq!(publisher.queue.len(), 3);
        assert_eq!(publisher.deadline(), Some(deadline));
        publisher.tick(deadline);
        assert!(publisher.queue.is_empty());
        assert!(publisher.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 0);
        assert_eq!(publisher.retained_bytes, 0);
        assert!(publisher.inflight.is_empty());
        assert!(publisher.deadline().is_none());
        assert!(publisher.failure.is_none());
    }

    #[test]
    fn a_flush_sends_the_blobs_still_queued() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 4,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(4, Limits::default());
        assert_eq!(offer_with_a_tail(&mut publisher, &pool), 3);
        publisher.flush();
        assert!(publisher.queue.is_empty());
        assert!(publisher.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 0);
        assert!(publisher.deadline().is_none());
        assert!(publisher.failure.is_none());
    }

    #[test]
    fn room_is_made_before_every_kind_of_record_that_carries_a_capture() {
        let thread = allocate_telemetry_id();
        let function = btel_types::FunctionIdAllocator::default()
            .allocate()
            .unwrap();
        for kind in [
            "log",
            "context",
            "inputs",
            "type arguments",
            "output",
            "request",
            "event",
            "network error",
        ] {
            let (delivery, mut publisher) = publisher(CloudPublisherConfig::default());
            let pool = SnapshotPool::new(4, Limits::default());
            capture(&mut publisher, &pool, 1);
            // No room for a second capture beside the first.
            publisher.config.max_retained_bytes = publisher.retained_bytes;
            let snapshot = pool
                .try_acquire()
                .unwrap()
                .finish(SnapshotValue::Int(2), &mut Shaper::default());
            let id = snapshot.root_id();
            let mut record = match kind {
                "log" => SpanRecord::Log {
                    parent_id: thread,
                    function,
                    pc: 0,
                    at: ClockInstant::from_ticks(1),
                    level: btel_records::LogLevel::Info,
                    event_name: None,
                    captured_data: Some(snapshot),
                },
                "context" => SpanRecord::ContextSelected {
                    captured_context: Some(snapshot),
                },
                "inputs" => SpanRecord::FunctionSpanAnnouncement {
                    id: allocate_telemetry_id(),
                    parent_id: thread,
                    call_path: CallPathId::ROOT,
                    entered_at: ClockInstant::from_ticks(1),
                    captured_inputs: Some(snapshot),
                    captured_type_args: None,
                },
                "type arguments" => SpanRecord::FunctionSpanAnnouncement {
                    id: allocate_telemetry_id(),
                    parent_id: thread,
                    call_path: CallPathId::ROOT,
                    entered_at: ClockInstant::from_ticks(1),
                    captured_inputs: None,
                    captured_type_args: Some(snapshot),
                },
                "request" => SpanRecord::NetworkSpanAnnouncement(Box::new(
                    btel_records::NetworkAnnouncement {
                        id: allocate_telemetry_id(),
                        parent_id: thread,
                        call_path: CallPathId::ROOT,
                        started_at: ClockInstant::from_ticks(1),
                        method: "GET".into(),
                        url: "https://example.test/".into(),
                        request: Some(snapshot),
                    },
                )),
                "event" => SpanRecord::NetworkEvent(Box::new(btel_records::NetworkEventRecord {
                    span: allocate_telemetry_id(),
                    name: btel_records::NetworkEventName::Data,
                    at: ClockInstant::from_ticks(1),
                    payload: Some(snapshot),
                })),
                "network error" => {
                    SpanRecord::NetworkSpanCompletion(Box::new(btel_records::NetworkCompletion {
                        span: allocate_telemetry_id(),
                        completed_at: ClockInstant::from_ticks(2),
                        outcome: btel_types::InvocationOutcome::Errored,
                        error: Some(snapshot),
                    }))
                }
                "output" => SpanRecord::FunctionSpanCompletionOk {
                    id: allocate_telemetry_id(),
                    parent_id: thread,
                    call_path: CallPathId::ROOT,
                    entered_at: ClockInstant::from_ticks(1),
                    exited_at: ClockInstant::from_ticks(2),
                    await_time: btel_types::AwaitDuration::ZERO,
                    captured_value: Some(snapshot),
                },
                other => unreachable!("{other}"),
            };
            publisher.before_detached_span(&record);
            publisher.span(thread, &mut record);
            // The first capture was sent to make room; this one is held.
            assert_eq!(delivery.handle().loss_count(), 0, "{kind}");
            assert_eq!(
                publisher
                    .queue
                    .iter()
                    .map(|queued| queued.id)
                    .collect::<Vec<_>>(),
                [id],
                "{kind}"
            );
            assert_eq!(publisher.retained_snapshots, 1, "{kind}");
        }
    }

    #[test]
    fn room_is_made_for_both_captures_of_an_announcement() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(4, Limits::default());
        capture(&mut publisher, &pool, 1);
        // One capture more fits, not two.
        publisher.config.max_pending_snapshots = 2;
        let snapshot = |value| {
            pool.try_acquire()
                .unwrap()
                .finish(SnapshotValue::Int(value), &mut Shaper::default())
        };
        let (inputs, type_args) = (snapshot(2), snapshot(3));
        let ids = [inputs.root_id(), type_args.root_id()];
        let thread = allocate_telemetry_id();
        let mut record = SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(inputs),
            captured_type_args: Some(type_args),
        };
        publisher.before_detached_span(&record);
        publisher.span(thread, &mut record);
        assert_eq!(delivery.handle().loss_count(), 0);
        assert_eq!(
            publisher
                .queue
                .iter()
                .map(|queued| queued.id)
                .collect::<Vec<_>>(),
            ids
        );
        assert_eq!(publisher.retained_snapshots, 2);
    }

    #[test]
    fn blobs_offered_before_are_not_queued_again_and_empty_captures_are_dropped() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(4, Limits::default());
        offer(&mut publisher, cut_list(&pool, 1, 3));
        assert_eq!(publisher.queue.len(), 4);
        // The same list again: nothing new, so the capture is not held.
        offer(&mut publisher, cut_list(&pool, 1, 3));
        assert_eq!(publisher.queue.len(), 4);
        assert_eq!(publisher.owners.len(), 1);
        assert_eq!(pool.stats().in_use, 1);
        // A list sharing two strings: only the third and the list itself.
        let mut b = pool.try_acquire().unwrap();
        let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
        let list = b.list(element_type, [0, 1, 7].into_iter(), |leaves, index| {
            let text = format!("1-{index}-{}", "x".repeat(100));
            leaves.string_value(&text.as_str().into())
        });
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        offer(
            &mut publisher,
            b.finish(SnapshotValue::Object(list), &mut shaper),
        );
        assert_eq!(publisher.queue.len(), 6);
        assert_eq!(publisher.owners.len(), 2);
        assert!(matches!(publisher.queue[4].held, Held::Leaf(_)));
        assert!(matches!(publisher.queue[5].held, Held::Kept { .. }));
        assert_eq!(publisher.owners[1].queued, 1);
    }

    #[test]
    fn a_string_blob_is_let_go_alone_and_a_string_capture_holds_no_storage() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(2, Limits::default());
        // A capture that is one string: nothing of it is held but the string.
        let mut b = pool.try_acquire().unwrap();
        let output = b
            .leaves()
            .string_value(&"output".repeat(1000).as_str().into());
        offer(&mut publisher, b.finish(output, &mut Shaper::default()));
        assert_eq!(pool.stats().in_use, 0);
        assert_eq!(publisher.queue.len(), 1);
        assert!(publisher.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 0);
        assert!(publisher.retained_bytes > 6000);
        let window = publisher.take_window();
        publisher.release_window(&window);
        assert_eq!(publisher.retained_bytes, 0);
        drop(window);

        // A list of three strings: each is let go when its own blob is.
        let texts = ["a", "b", "c"].map(|letter| btel_snapshot::BexStr::from(letter.repeat(200)));
        let backing = texts.each_ref().map(|text| match text {
            btel_snapshot::BexStr::Flat(flat) => Arc::downgrade(flat),
            other => panic!("expected a heap-backed string, got {other:?}"),
        });
        let mut b = pool.try_acquire().unwrap();
        let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
        let list = b.list(element_type, texts.iter(), |leaves, text| {
            leaves.string_value(text)
        });
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        offer(
            &mut publisher,
            b.finish(SnapshotValue::Object(list), &mut shaper),
        );
        drop(texts);
        let held = publisher.retained_bytes;
        let mut window = publisher.take_window();
        assert_eq!(window.candidates.len(), 4);
        assert_eq!(window.owners.len(), 1);
        publisher.release_window(&window);
        assert_eq!(publisher.retained_bytes, 0);
        assert!(held > 600);
        for backing in &backing {
            assert!(backing.upgrade().is_some());
            drop(window.candidates.remove(0));
            assert!(backing.upgrade().is_none());
        }
        // The root is still written from the capture.
        assert_eq!(pool.stats().in_use, 1);
        drop(window);
        assert_eq!(pool.stats().in_use, 0);
    }

    /// A list of strings of these lengths, each in a blob of its own.
    fn cut_strings(pool: &SnapshotPool, lengths: &[usize]) -> Snapshot {
        let mut b = pool.try_acquire().unwrap();
        let element_type = b.leaves().ty(btel_snapshot::OwnedType::string());
        let list = b.list(
            element_type,
            lengths.iter().enumerate(),
            |leaves, (index, length)| {
                let text = format!("{index}-{}", "x".repeat(*length));
                leaves.string_value(&text.as_str().into())
            },
        );
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        b.finish(SnapshotValue::Object(list), &mut shaper)
    }

    #[test]
    fn a_capture_larger_than_the_byte_budget_is_held_alone_with_every_blob() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            retained_bytes_target: 1,
            max_retained_bytes: 1,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(2, Limits::default());
        let snapshot = cut_strings(&pool, &[100, 10_000]);
        assert!(snapshot.retained_bytes() > publisher.config.max_retained_bytes);
        offer(&mut publisher, snapshot);
        assert_eq!(delivery.handle().loss_count(), 0);
        assert_eq!(publisher.retained_snapshots, 1);
        // Byte pressure sealed a file at once; it took all three blobs.
        assert_eq!(publisher.staged.len(), 1);
        assert_eq!(publisher.staged[0].window.candidates.len(), 3);
        assert!(publisher.queue.is_empty());
        // While it is held and cannot be sent, the next capture has no room.
        offer(&mut publisher, cut_strings(&pool, &[200]));
        assert_eq!(delivery.handle().loss_count(), 1);
        assert_eq!(publisher.retained_snapshots, 1);
        assert!(!delivery.handle().is_disabled());
    }

    #[test]
    fn a_window_is_cut_by_the_bytes_its_bodies_buffer_and_always_takes_a_blob() {
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                // A window takes a quarter of this.
                cas_reserved_bytes: 4 * 1024,
                ..DeliveryConfig::new("http://127.0.0.1:1".parse().unwrap())
            },
            |_| {},
        )
        .unwrap();
        let mut publisher = CloudPublisher::new(
            RecordingId::generate(),
            RecordingConfig::default(),
            CloudPublisherConfig::default(),
            delivery.handle(),
        )
        .unwrap();
        let pool = SnapshotPool::new(2, Limits::default());
        // Two long strings, then byte arrays, each a blob of its own.
        let mut b = pool.try_acquire().unwrap();
        let ty = b.leaves().ty(btel_snapshot::OwnedType::unknown());
        let mut values: Vec<_> = ["a", "b"]
            .into_iter()
            .map(|letter| {
                b.leaves()
                    .string_value(&letter.repeat(10_000).as_str().into())
            })
            .collect();
        for (fill, length) in [600, 600, 3000, 100].into_iter().enumerate() {
            let bytes = b.bytes(&vec![u8::try_from(fill).unwrap(); length]);
            values.push(SnapshotValue::Object(b.leaves().object(bytes).unwrap()));
        }
        let list = b.list(ty, values.into_iter(), |_, value| value);
        let list = b.leaves().object(list).unwrap();
        let mut shaper = Shaper::new(ShapePolicy::Split {
            unit_bytes: 1 << 20,
            leaf_bytes: 64,
        });
        let snapshot = b.finish(SnapshotValue::Object(list), &mut shaper);
        let lengths: Vec<_> = snapshot.blobs().map(|blob| blob.encoded_len()).collect();
        assert_eq!(lengths.len(), 7);
        offer(&mut publisher, snapshot);
        assert!(publisher.staged.is_empty());
        let mut windows = Vec::new();
        while !publisher.queue.is_empty() {
            let window = publisher.take_window();
            windows.push(
                window
                    .candidates
                    .iter()
                    .map(|candidate| candidate.encoded_len)
                    .collect::<Vec<_>>(),
            );
        }
        // The strings are sent from where they are held, so both go with
        // the first array, far past the limit in length. Then arrays that do
        // not fit together, one too large for any window, and the rest.
        assert_eq!(
            windows.iter().map(Vec::len).collect::<Vec<_>>(),
            [3, 1, 1, 2]
        );
        assert_eq!(windows.concat(), lengths);
        assert!(windows[0][..2].iter().all(|length| *length > 10_000));
        assert!(windows[0][2] + windows[1][0] > 1024);
        assert!(windows[2][0] > 1024);
        assert!(windows[3].iter().sum::<u64>() <= 1024);
        assert!(publisher.owners.is_empty());
    }

    #[test]
    fn the_sources_capture_is_offered_like_any_other() {
        let (_delivery, publisher) = publisher(CloudPublisherConfig::default());
        let pool = SnapshotPool::new(2, Limits::default());
        let sources = btel_snapshot::string_map(
            &pool,
            &[("main.baml".to_owned(), "function main() {}".to_owned())],
            usize::MAX,
        )
        .unwrap();
        let id = sources.root_id();
        let mut publisher = publisher.with_process(btel_recorder::ProcessRecording {
            info: btel_types::ProcessInfo {
                process_id: [0; 16],
                baml_version: "test".into(),
                host: "test".into(),
                command: Vec::new(),
                started_at_unix_ns: 0,
            },
            sources: Some(sources),
            context: None,
            exit: Arc::default(),
        });
        assert_eq!(publisher.queue.len(), 1);
        assert_eq!(publisher.queue[0].id, id);
        assert_eq!(publisher.retained_snapshots, 1);
        assert_eq!(pool.stats().in_use, 1);
        // It goes with the first event's file: no file is sealed for it
        // alone, so the first plan is never the sources by themselves.
        assert!(publisher.deadline().is_none());
        publisher.after_batch(0);
        assert!(publisher.deadline().is_none());
        assert_eq!(publisher.queue.len(), 1);
        capture(&mut publisher, &pool, 1);
        let deadline = publisher.deadline().unwrap();
        publisher.tick(deadline);
        assert!(publisher.queue.is_empty());
        assert!(publisher.failure.is_none());
    }

    #[test]
    fn byte_pressure_drops_only_the_owner_that_cannot_fit() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            retained_bytes_target: 1,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(4, Limits::default());
        capture(&mut publisher, &pool, 1);
        assert_eq!(publisher.staged.len(), 1);
        let bytes = publisher.retained_bytes;
        assert!(bytes > publisher.config.retained_bytes_target);
        publisher.config.max_retained_bytes = bytes;
        capture(&mut publisher, &pool, 2);
        assert_eq!(publisher.failure, None);
        assert_eq!(publisher.staged.len(), 1);
        assert!(publisher.queue.is_empty());
        assert_eq!(publisher.offered.len(), 1);
        assert_eq!(publisher.retained_bytes, bytes);
        assert_eq!(publisher.retained_snapshots, 1);
        assert_eq!(delivery.handle().loss_count(), 1);
        assert!(!delivery.handle().is_disabled());
        assert_eq!(pool.stats().in_use, 1);
    }

    #[test]
    fn duplicate_ids_are_remembered_across_sealed_windows() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 1,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(4, Limits::default());
        capture(&mut publisher, &pool, 1);
        capture(&mut publisher, &pool, 1);
        capture(&mut publisher, &pool, 2);
        assert_eq!(publisher.staged.len(), 2);
        assert_eq!(publisher.retained_snapshots, 2);
        assert_eq!(publisher.offered.len(), 2);
        assert_eq!(pool.stats().in_use, 2);
    }

    #[test]
    fn recent_id_eviction_and_loss_reset_preserve_pending_candidate_uniqueness() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 2,
            max_offered_ids: 1,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(8, Limits::default());
        for value in [1, 2, 3, 1, 2, 3] {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.staged.len(), 1);
        assert_eq!(publisher.queue.len(), 1);
        assert_eq!(publisher.offered.len(), 1);
        delivery
            .handle()
            .record_payload_loss(DeliveryError::Http, true);
        for value in [1, 2, 3] {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.retained_snapshots, 3);
        assert_eq!(publisher.staged[0].window.candidates.len(), 2);
        assert_eq!(publisher.queue.len(), 1);
        assert_eq!(publisher.failure, None);
        assert_eq!(pool.stats().in_use, 3);
    }

    #[test]
    fn owner_count_pressure_drops_only_the_unadmitted_capture() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig {
            candidate_target: 1,
            max_pending_snapshots: 2,
            ..CloudPublisherConfig::default()
        });
        let pool = SnapshotPool::new(4, Limits::default());
        for value in 0..3 {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.failure, None);
        assert_eq!(publisher.retained_snapshots, 2);
        assert_eq!(pool.stats().in_use, 2);
        assert_eq!(delivery.handle().loss_count(), 1);
        assert!(!delivery.handle().is_disabled());
    }

    #[test]
    fn oversized_recording_is_lost_and_a_carrier_takes_its_blobs() {
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                max_recording_body_bytes: 128,
                ..DeliveryConfig::new("http://127.0.0.1:1".parse().unwrap())
            },
            |_| panic!("oversized payload must not disable delivery"),
        )
        .unwrap();
        let mut publisher = CloudPublisher::new(
            RecordingId::generate(),
            RecordingConfig {
                target_bytes: NonZeroUsize::new(128).unwrap(),
                ..RecordingConfig::default()
            },
            CloudPublisherConfig::default(),
            delivery.handle(),
        )
        .unwrap();
        publisher.recording_target = usize::MAX;
        let pool = SnapshotPool::new(20, Limits::default());
        for value in 0..16 {
            capture(&mut publisher, &pool, value);
        }
        // The file with the announcements is lost; a carrier, small enough
        // for the body limit, takes every queued blob at once.
        assert_eq!(publisher.failure, None);
        assert_eq!(delivery.handle().loss_count(), 1);
        assert_eq!(publisher.staged.len(), 1);
        assert_eq!(publisher.staged[0].window.candidates.len(), 16);
        assert!(publisher.staged[0].file.bytes().len() <= 128);
        assert!(publisher.queue.is_empty());
        assert_eq!(publisher.retained_snapshots, 16);
        assert_eq!(pool.stats().in_use, 16);
        publisher.aggregate(AggregateDelta {
            count: 1,
            ..AggregateDelta::default()
        });
        let sealed = publisher.recording.flush_recording();
        publisher.stage(sealed);
        assert_eq!(publisher.staged.len(), 2);
        assert!(publisher.staged[1].window.candidates.is_empty());
        assert!(!delivery.handle().is_disabled());
    }

    #[test]
    fn metadata_pressure_is_observable_without_disabling_recordings() {
        let (delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        publisher.metadata = MetadataJournal::new(1);
        publisher.span(
            allocate_telemetry_id(),
            &mut SpanRecord::CallPathDefined {
                call_path: CallPathId::new_non_root(1).unwrap(),
                parent_call_path: CallPathId::ROOT,
                visible_caller: None,
                caller_pc: 0,
                callee: btel_types::FunctionIdAllocator::default()
                    .allocate()
                    .unwrap(),
                edge: btel_types::CallPathEdge::Synchronous,
            },
        );
        let sealed = publisher.recording.flush_recording();
        publisher.stage(sealed);
        assert_eq!(publisher.failure, None);
        assert_eq!(publisher.staged.len(), 1);
        assert_eq!(delivery.handle().metadata_replay_evictions(), 1);
        assert_eq!(delivery.handle().loss_count(), 0);
        assert!(!delivery.handle().is_disabled());
    }

    #[test]
    fn rejected_recordings_preserve_prior_metadata_for_a_later_small_file() {
        use prost::Message;

        for rejection in ["body", "plans", "bytes"] {
            let delivery = BcsDelivery::new(
                DeliveryConfig {
                    max_recording_body_bytes: 512,
                    ..DeliveryConfig::new("http://127.0.0.1:1".parse().unwrap())
                },
                |_| panic!("payload rejection must not disable delivery"),
            )
            .unwrap();
            let mut publisher = CloudPublisher::new(
                RecordingId::generate(),
                RecordingConfig {
                    target_bytes: NonZeroUsize::new(512).unwrap(),
                    ..RecordingConfig::default()
                },
                CloudPublisherConfig::default(),
                delivery.handle(),
            )
            .unwrap();
            publisher.recording_target = usize::MAX;
            let thread = allocate_telemetry_id();
            let define = |publisher: &mut CloudPublisher, path| {
                publisher.span(
                    thread,
                    &mut SpanRecord::CallPathDefined {
                        call_path: CallPathId::new_non_root(path).unwrap(),
                        parent_call_path: CallPathId::ROOT,
                        visible_caller: None,
                        caller_pc: 0,
                        callee: btel_types::FunctionIdAllocator::default()
                            .allocate()
                            .unwrap(),
                        edge: btel_types::CallPathEdge::Synchronous,
                    },
                );
            };
            define(&mut publisher, 1);
            let first = publisher.recording.flush_recording();
            publisher.stage(first);
            assert_eq!(publisher.staged.len(), 1);
            if rejection == "plans" {
                while publisher.staged.len() < delivery.handle().config().max_pending_plans {
                    publisher.aggregate(AggregateDelta {
                        count: 1,
                        ..AggregateDelta::default()
                    });
                    let sealed = publisher.recording.flush_recording();
                    publisher.stage(sealed);
                }
            } else if rejection == "bytes" {
                publisher.staged_recording_bytes =
                    delivery.handle().config().recording_reserved_bytes;
            }
            define(&mut publisher, 2);
            let pool = SnapshotPool::new(20, Limits::default());
            if rejection == "body" {
                for value in 0..16 {
                    capture(&mut publisher, &pool, value);
                }
            } else {
                let sealed = publisher.recording.flush_recording();
                publisher.stage(sealed);
            }
            assert_eq!(delivery.handle().loss_count(), 1, "{rejection}");
            assert_eq!(
                delivery.handle().metadata_replay_evictions(),
                0,
                "{rejection}"
            );

            // Discard the earlier staged payloads without acknowledging them.
            publisher.staged.clear();
            publisher.staged_recording_bytes = 0;
            publisher.aggregate(AggregateDelta {
                count: 1,
                ..AggregateDelta::default()
            });
            let sealed = publisher.recording.flush_recording();
            publisher.stage(sealed);
            assert_eq!(publisher.staged.len(), 1);
            let decoded =
                btel_recorder::proto::RecordingFile::decode(publisher.staged[0].file.bytes())
                    .unwrap();
            let paths: Vec<_> = decoded
                .definitions
                .unwrap()
                .call_paths
                .into_iter()
                .map(|path| path.call_path_id)
                .collect();
            assert_eq!(paths, [1, 2], "{rejection}");
            assert!(!delivery.handle().is_disabled());
        }
    }

    #[test]
    fn recording_target_and_staged_plan_cap_are_enforced_at_record_boundaries() {
        let (_delivery, mut publisher) = publisher(CloudPublisherConfig::default());
        publisher.recording_target = NonZeroUsize::MIN.get();
        for _ in 0..publisher.delivery.config().max_pending_plans {
            publisher.aggregate(AggregateDelta {
                count: 1,
                ..AggregateDelta::default()
            });
        }
        assert!(publisher.failure.is_none());
        publisher.aggregate(AggregateDelta {
            count: 1,
            ..AggregateDelta::default()
        });
        assert_eq!(publisher.failure, None);
        assert_eq!(
            publisher.staged.len(),
            publisher.delivery.config().max_pending_plans
        );
        assert_eq!(publisher.delivery.loss_count(), 1);
    }

    #[test]
    fn publisher_owner_limit_preserves_minimum_pool_headroom() {
        let (delivery, _) = publisher(CloudPublisherConfig::default());
        let make = |max_pending_snapshots| {
            CloudPublisher::new(
                RecordingId::generate(),
                RecordingConfig::default(),
                CloudPublisherConfig {
                    max_pending_snapshots,
                    ..CloudPublisherConfig::default()
                },
                delivery.handle(),
            )
        };
        let remaining = btel_settings::snapshot::MIN_SNAPSHOT_SLOTS
            - delivery.handle().config().max_pending_snapshots;
        assert!(make(remaining).is_err());
        assert!(make(remaining - 1).is_ok());
    }

    #[test]
    fn placement_preserves_candidate_order_and_uses_all_target_kinds() {
        let config = CloudPublisherConfig {
            inline_target_bytes: 10,
            batch_target_bytes: 20,
            standalone_threshold_bytes: 100,
            ..CloudPublisherConfig::default()
        };
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish(SnapshotValue::Int(0), &mut Shaper::default());
        let candidates: Vec<_> = [5_u8, 12, 8, 100, 15]
            .into_iter()
            .map(|encoded_len| Candidate {
                id: CasId::from_bytes([encoded_len; 16]),
                encoded_len: u64::from(encoded_len),
                source: Source::Kept {
                    owner: 0,
                    blob: snapshot.root_blob().index(),
                },
            })
            .collect();
        let targets = propose(&candidates, &config);
        assert_eq!(targets[0].candidate_indices, [0]);
        assert_eq!(targets[1].kind, UploadKind::CasBatch);
        assert_eq!(targets[1].candidate_indices, [1, 2]);
        assert_eq!(targets[2].kind, UploadKind::CasObject);
        assert_eq!(targets[2].candidate_indices, [3]);
        assert_eq!(targets[3].candidate_indices, [4]);
    }

    #[test]
    fn recording_target_exists_without_snapshots() {
        let targets = propose(&[], &CloudPublisherConfig::default());
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, UploadKind::Recording);
        assert!(targets[0].candidate_indices.is_empty());
    }
}
