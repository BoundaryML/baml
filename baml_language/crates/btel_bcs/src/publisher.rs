use std::{
    collections::{HashSet, VecDeque},
    time::Instant,
};

use btel_processor::{AggregateDelta, Publisher};
use btel_publisher::{RecordingBuilder, RecordingConfig, RecordingError, RecordingId, SealedFile};
use btel_records::SpanRecord;
use btel_snapshot::{Snapshot, SnapshotId};
use btel_types::TelemetryId;

use crate::{
    delivery::{DeliveryError, DeliveryHandle},
    metadata::MetadataJournal,
    wire::{ProposedUploadTarget, UploadKind},
};

#[derive(Clone, Debug)]
pub struct PublisherConfig {
    /// Bounded recent-offer deduplication, not authoritative CAS availability.
    pub max_offered_snapshots: usize,
    /// Soft per-file targets. A single capture may exceed the byte target.
    pub snapshot_target: usize,
    pub retained_bytes_target: usize,
    /// Hard owner count across the open file and staged files, not per batch.
    pub max_pending_snapshots: usize,
    /// Hard sum of `Snapshot::retained_bytes` across those owners. Recording
    /// buffers have separate delivery limits; bookkeeping is count-bounded.
    pub max_retained_bytes: usize,
    pub inline_target_bytes: usize,
    pub batch_target_bytes: usize,
    pub standalone_threshold_bytes: usize,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            max_offered_snapshots: 65_536,
            snapshot_target: 16,
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
/// The open file and staged files share hard snapshot count/byte budgets. A
/// record can seal a file but cannot deliver it until input chunks are recycled.
/// Staging is additionally bounded by delivery's plan and recording-byte limits.
/// Neither sealing nor staging releases snapshot owners; only delivery or
/// terminal cleanup does. Combined publisher/delivery owner limits leave room
/// in the minimum VM pool for capture to publish its partial chunk.
///
/// Cloud processing detaches at most one chunk, recycling its allocation before
/// any admission wait. The unvisited suffix still owns its captures in the shared,
/// bounded VM snapshot pool; it is not extra delivery capacity. The processor
/// reserves chunk-capacity-sized span and timing buffers, with only one live at
/// a time. Handoffs occur between records and cache deltas, not between chunks.
pub struct CloudPublisher {
    recording: RecordingBuilder,
    recording_target: usize,
    config: PublisherConfig,
    offered: HashSet<SnapshotId>,
    offered_order: VecDeque<SnapshotId>,
    metadata: MetadataJournal,
    pending: PendingSnapshots,
    staged: Vec<PendingFile>,
    retained_snapshots: usize,
    retained_bytes: usize,
    staged_recording_bytes: usize,
    preflight_size: Option<(SnapshotId, Option<usize>)>,
    failure: Option<DeliveryError>,
    delivery: DeliveryHandle,
}

#[derive(Default)]
struct PendingSnapshots {
    owners: Vec<Snapshot>,
    sizes: Vec<usize>,
    bytes: usize,
}

struct PendingFile {
    file: SealedFile,
    snapshots: PendingSnapshots,
}

impl CloudPublisher {
    pub fn new(
        id: RecordingId,
        recording: RecordingConfig,
        config: PublisherConfig,
        delivery: DeliveryHandle,
    ) -> Result<Self, RecordingError> {
        let limits = delivery.config();
        if config.max_offered_snapshots == 0
            || config.snapshot_target == 0
            || config.snapshot_target > config.max_pending_snapshots
            || config.snapshot_target > limits.max_candidates
            || config.snapshot_target >= limits.max_targets
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
            metadata: MetadataJournal::new(limits.max_recording_body_bytes),
            pending: PendingSnapshots::default(),
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

    fn fail(&mut self, error: DeliveryError) {
        self.failure.get_or_insert(error);
        self.pending = PendingSnapshots::default();
        self.staged.clear();
        self.offered.clear();
        self.offered_order.clear();
        self.metadata = MetadataJournal::new(self.delivery.config().max_recording_body_bytes);
        self.retained_snapshots = 0;
        self.retained_bytes = 0;
        self.staged_recording_bytes = 0;
        self.preflight_size = None;
        self.recording.discard_pending();
    }

    fn already_offered(&self, id: SnapshotId) -> bool {
        self.offered.contains(&id)
            || self.pending.owners.iter().any(|owner| owner.id() == id)
            || self
                .staged
                .iter()
                .any(|file| file.snapshots.owners.iter().any(|owner| owner.id() == id))
    }

    fn remember(&mut self, id: SnapshotId) {
        if self.offered.contains(&id) {
            return;
        }
        if self.offered.len() == self.config.max_offered_snapshots {
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
            self.offered.clear();
            self.offered_order.clear();
            // Pending owners remain protected by `already_offered` even if
            // there are more of them than the recent-offer cache can retain.
            let ids: Vec<_> = self
                .pending
                .owners
                .iter()
                .chain(self.staged.iter().flat_map(|file| &file.snapshots.owners))
                .map(Snapshot::id)
                .collect();
            for id in ids {
                self.remember(id);
            }
        }
    }

    fn retain(&mut self, snapshot: Snapshot) {
        let preflight = self.preflight_size.take();
        if self.already_offered(snapshot.id()) {
            return;
        }
        let size = match preflight {
            Some((id, size)) if id == snapshot.id() => size,
            _ => snapshot.retained_bytes(),
        };
        let Some(size) = size else {
            self.delivery
                .record_payload_loss(DeliveryError::Capacity, false);
            return;
        };
        let Some(total) = self.retained_bytes.checked_add(size) else {
            self.delivery
                .record_payload_loss(DeliveryError::Capacity, false);
            return;
        };
        if total > self.config.max_retained_bytes
            || self.retained_snapshots >= self.config.max_pending_snapshots
        {
            self.delivery
                .record_payload_loss(DeliveryError::Capacity, false);
            return;
        }
        self.remember(snapshot.id());
        self.retained_snapshots += 1;
        self.retained_bytes = total;
        self.pending.bytes += size;
        self.pending.sizes.push(size);
        self.pending.owners.push(snapshot);
    }

    fn stage(&mut self, sealed: Result<Option<SealedFile>, RecordingError>) {
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
            let evictions = self.metadata.retain(&file);
            for _ in 0..evictions {
                self.delivery.record_metadata_replay_eviction();
            }
            self.drop_pending_payload();
            return;
        }
        let body_limit = limits
            .max_recording_body_bytes
            .min(remaining_bytes.saturating_sub(std::mem::size_of::<SealedFile>()));
        let evictions = self.metadata.prepare(&mut file, body_limit);
        for _ in 0..evictions {
            self.delivery.record_metadata_replay_eviction();
        }
        self.staged_recording_bytes += file.retained_bytes();
        self.staged.push(PendingFile {
            file,
            snapshots: std::mem::take(&mut self.pending),
        });
    }

    fn drop_pending_payload(&mut self) {
        let reset_cas = !self.pending.owners.is_empty();
        self.retained_snapshots -= self.pending.owners.len();
        self.retained_bytes -= self.pending.bytes;
        self.pending = PendingSnapshots::default();
        self.delivery
            .record_payload_loss(DeliveryError::Capacity, reset_cas);
        self.progress();
    }

    fn seal_on_pressure(&mut self) {
        if self.failure.is_none()
            && (self.pending.owners.len() >= self.config.snapshot_target
                || self.pending.bytes >= self.config.retained_bytes_target
                || self.recording.encoded_size_hint() >= self.recording_target)
        {
            let sealed = self.recording.finish_recording();
            self.stage(sealed);
        }
    }

    fn start_window(&mut self) {
        if self.recording.deadline().is_none() {
            self.recording.start_deadline(Instant::now());
        }
    }

    fn service(&mut self, now: Instant, force: bool) {
        self.progress();
        if self.delivery.is_disabled() {
            self.fail(DeliveryError::Closed);
        }
        if self.failure.is_none() {
            let sealed = if force {
                self.recording.finish_recording()
            } else {
                self.recording.flush_if_due(now)
            };
            self.stage(sealed);
        }
        if self.failure.is_none() {
            let staged = std::mem::take(&mut self.staged);
            self.staged_recording_bytes = 0;
            for PendingFile { file, snapshots } in staged {
                self.retained_snapshots -= snapshots.owners.len();
                self.retained_bytes -= snapshots.bytes;
                let targets = propose(&snapshots.sizes, &self.config);
                if let Err(error) = self.delivery.try_submit_accounted(
                    file,
                    snapshots.owners,
                    targets,
                    &snapshots.sizes,
                ) {
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

    fn before_detached_span(&mut self, record: &SpanRecord<Snapshot, Snapshot>) {
        self.progress();
        self.preflight_size = None;
        if self.failure.is_some() {
            return;
        }
        let snapshot = match record {
            SpanRecord::FunctionSpanAnnouncement {
                captured_inputs, ..
            } => captured_inputs.as_ref(),
            _ => record
                .completion()
                .and_then(|completion| completion.captured_value),
        };
        if let Some(snapshot) = snapshot {
            if !self.already_offered(snapshot.id()) {
                let size = snapshot.retained_bytes();
                self.preflight_size = Some((snapshot.id(), size));
                if self.retained_snapshots >= self.config.max_pending_snapshots
                    || size.is_some_and(|bytes| {
                        self.retained_bytes.saturating_add(bytes) > self.config.max_retained_bytes
                    })
                {
                    self.service(Instant::now(), true);
                }
            }
        }
    }

    fn after_detached_span(&mut self) {
        if !self.staged.is_empty() || self.failure.is_some() {
            self.service(Instant::now(), false);
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

    fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        self.progress();
        if self.failure.is_none() {
            self.start_window();
            self.recording.span_reference(thread, record);
            if let Some(snapshot) = record.take_capture() {
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
        self.service(Instant::now(), false);
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

    fn flush(&mut self) {
        self.service(Instant::now(), true);
    }

    fn finish(&mut self) {
        self.flush();
    }
}

fn propose(sizes: &[usize], config: &PublisherConfig) -> Vec<ProposedUploadTarget> {
    let mut targets = vec![ProposedUploadTarget {
        client_target_id: 0,
        kind: UploadKind::Recording,
        candidate_indices: Vec::new(),
    }];
    let mut inline_bytes = 0_usize;
    let mut batch = None;
    let mut batch_bytes = 0_usize;
    for (index, &size) in sizes.iter().enumerate() {
        let index = u32::try_from(index).expect("publisher bounds candidate count");
        if size >= config.standalone_threshold_bytes {
            targets.push(ProposedUploadTarget {
                client_target_id: u32::try_from(targets.len()).expect("bounded target count"),
                kind: UploadKind::CasObject,
                candidate_indices: vec![index],
            });
        } else if size <= config.inline_target_bytes.saturating_sub(inline_bytes) {
            targets[0].candidate_indices.push(index);
            inline_bytes += size;
        } else {
            if batch.is_none() || size > config.batch_target_bytes.saturating_sub(batch_bytes) {
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

    use btel_snapshot::{Limits, SnapshotPool, SnapshotValue};
    use btel_types::{CallPathId, ClockInstant, allocate_telemetry_id};

    use super::*;
    use crate::delivery::{BcsDelivery, DeliveryConfig};

    fn publisher(config: PublisherConfig) -> (BcsDelivery, CloudPublisher) {
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                prepare_base_url: "http://127.0.0.1:1".into(),
                allow_http: true,
                ..DeliveryConfig::default()
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

    fn capture(publisher: &mut CloudPublisher, pool: &SnapshotPool, value: i64) {
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(value));
        let thread = allocate_telemetry_id();
        publisher.span(
            thread,
            &mut SpanRecord::FunctionSpanAnnouncement {
                id: allocate_telemetry_id(),
                parent_id: thread,
                call_path: CallPathId::ROOT,
                entered_at: ClockInstant::from_ticks(1),
                captured_inputs: Some(snapshot),
            },
        );
    }

    #[test]
    fn detached_preflight_size_is_consumed_and_failure_skips_preflight() {
        let (_delivery, mut publisher) = publisher(PublisherConfig::default());
        let pool = SnapshotPool::new(2, Limits::default());
        let snapshot = pool
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(1));
        let expected = snapshot.retained_bytes().unwrap();
        let id = snapshot.id();
        let thread = allocate_telemetry_id();
        let mut record = SpanRecord::FunctionSpanAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: CallPathId::ROOT,
            entered_at: ClockInstant::from_ticks(1),
            captured_inputs: Some(snapshot),
        };
        publisher.before_detached_span(&record);
        assert_eq!(publisher.preflight_size, Some((id, Some(expected))));
        publisher.span(thread, &mut record);
        assert_eq!(publisher.preflight_size, None);
        assert_eq!(publisher.retained_bytes, expected);
        publisher.fail(DeliveryError::Capacity);
        let next = pool
            .try_acquire()
            .unwrap()
            .finish_value(SnapshotValue::Int(2));
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
        let (_delivery, mut publisher) = publisher(PublisherConfig::default());
        let pool = SnapshotPool::new(4, Limits::default());
        for value in 0..3 {
            publisher.before_batch(1);
            capture(&mut publisher, &pool, value);
            publisher.after_batch(1);
        }
        assert_eq!(publisher.pending.owners.len(), 3);
        assert!(publisher.staged.is_empty());
        assert_eq!(publisher.retained_snapshots, 3);
        assert_eq!(
            publisher.retained_bytes,
            publisher.pending.sizes.iter().sum::<usize>()
        );
        assert_eq!(pool.stats().in_use, 3);
    }

    #[test]
    fn first_event_starts_non_sliding_deadline_and_idle_check_seals() {
        let (_delivery, mut publisher) = publisher(PublisherConfig::default());
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
        let (delivery, mut publisher) = publisher(PublisherConfig {
            snapshot_target: 2,
            ..PublisherConfig::default()
        });
        let pool = SnapshotPool::new(8, Limits::default());
        publisher.before_batch(6);
        publisher.before_span_chunk(6);
        for value in 0..6 {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.staged.len(), 3);
        assert!(publisher.pending.owners.is_empty());
        assert_eq!(publisher.retained_snapshots, 6);
        assert!(publisher.failure.is_none());
        assert_eq!(delivery.result(), None);
        assert!(
            publisher
                .staged
                .iter()
                .all(|file| file.snapshots.owners.len() == 2)
        );
    }

    #[test]
    fn byte_pressure_drops_only_the_owner_that_cannot_fit() {
        let (delivery, mut publisher) = publisher(PublisherConfig {
            retained_bytes_target: 1,
            ..PublisherConfig::default()
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
        assert!(publisher.pending.owners.is_empty());
        assert_eq!(publisher.offered.len(), 1);
        assert_eq!(publisher.retained_bytes, bytes);
        assert_eq!(publisher.retained_snapshots, 1);
        assert_eq!(delivery.handle().loss_count(), 1);
        assert!(!delivery.handle().is_disabled());
        assert_eq!(pool.stats().in_use, 1);
    }

    #[test]
    fn duplicate_ids_are_remembered_across_sealed_windows() {
        let (_delivery, mut publisher) = publisher(PublisherConfig {
            snapshot_target: 1,
            ..PublisherConfig::default()
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
        let (delivery, mut publisher) = publisher(PublisherConfig {
            snapshot_target: 2,
            max_offered_snapshots: 1,
            ..PublisherConfig::default()
        });
        let pool = SnapshotPool::new(8, Limits::default());
        for value in [1, 2, 3, 1, 2, 3] {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.staged.len(), 1);
        assert_eq!(publisher.pending.owners.len(), 1);
        assert_eq!(publisher.offered.len(), 1);
        delivery
            .handle()
            .record_payload_loss(DeliveryError::Http, true);
        for value in [1, 2, 3] {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.retained_snapshots, 3);
        assert_eq!(publisher.staged[0].snapshots.owners.len(), 2);
        assert_eq!(publisher.pending.owners.len(), 1);
        assert_eq!(publisher.failure, None);
        assert_eq!(pool.stats().in_use, 3);
    }

    #[test]
    fn owner_count_pressure_drops_only_the_unadmitted_capture() {
        let (delivery, mut publisher) = publisher(PublisherConfig {
            snapshot_target: 1,
            max_pending_snapshots: 2,
            ..PublisherConfig::default()
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
    fn oversized_recording_releases_owners_and_later_small_file_is_staged() {
        let delivery = BcsDelivery::new(
            DeliveryConfig {
                prepare_base_url: "http://127.0.0.1:1".into(),
                allow_http: true,
                max_recording_body_bytes: 128,
                ..DeliveryConfig::default()
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
            PublisherConfig::default(),
            delivery.handle(),
        )
        .unwrap();
        publisher.recording_target = usize::MAX;
        let pool = SnapshotPool::new(20, Limits::default());
        for value in 0..16 {
            capture(&mut publisher, &pool, value);
        }
        assert_eq!(publisher.failure, None);
        assert_eq!(publisher.retained_snapshots, 0);
        assert_eq!(publisher.retained_bytes, 0);
        assert!(publisher.staged.is_empty());
        assert!(publisher.offered.is_empty());
        assert_eq!(pool.stats().in_use, 0);
        assert_eq!(delivery.handle().loss_count(), 1);
        publisher.aggregate(AggregateDelta {
            count: 1,
            ..AggregateDelta::default()
        });
        let sealed = publisher.recording.finish_recording();
        publisher.stage(sealed);
        assert_eq!(publisher.staged.len(), 1);
        assert!(!delivery.handle().is_disabled());
    }

    #[test]
    fn metadata_pressure_is_observable_without_disabling_recordings() {
        let (delivery, mut publisher) = publisher(PublisherConfig::default());
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
        let sealed = publisher.recording.finish_recording();
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
                    prepare_base_url: "http://127.0.0.1:1".into(),
                    allow_http: true,
                    max_recording_body_bytes: 512,
                    ..DeliveryConfig::default()
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
                PublisherConfig::default(),
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
            let first = publisher.recording.finish_recording();
            publisher.stage(first);
            assert_eq!(publisher.staged.len(), 1);
            if rejection == "plans" {
                while publisher.staged.len() < delivery.handle().config().max_pending_plans {
                    publisher.aggregate(AggregateDelta {
                        count: 1,
                        ..AggregateDelta::default()
                    });
                    let sealed = publisher.recording.finish_recording();
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
                let sealed = publisher.recording.finish_recording();
                publisher.stage(sealed);
            }
            assert_eq!(delivery.handle().loss_count(), 1, "{rejection}");
            assert_eq!(
                delivery.handle().metadata_replay_evictions(),
                0,
                "{rejection}"
            );
            assert_eq!(pool.stats().in_use, 0);
            assert_eq!(publisher.retained_snapshots, 0);

            // Discard the earlier staged payloads without acknowledging them.
            publisher.staged.clear();
            publisher.staged_recording_bytes = 0;
            publisher.aggregate(AggregateDelta {
                count: 1,
                ..AggregateDelta::default()
            });
            let sealed = publisher.recording.finish_recording();
            publisher.stage(sealed);
            assert_eq!(publisher.staged.len(), 1);
            let decoded =
                btel_publisher::proto::RecordingFile::decode(publisher.staged[0].file.bytes())
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
        let (_delivery, mut publisher) = publisher(PublisherConfig::default());
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
        let (delivery, _) = publisher(PublisherConfig::default());
        let make = |max_pending_snapshots| {
            CloudPublisher::new(
                RecordingId::generate(),
                RecordingConfig::default(),
                PublisherConfig {
                    max_pending_snapshots,
                    ..PublisherConfig::default()
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
        let config = PublisherConfig {
            inline_target_bytes: 10,
            batch_target_bytes: 20,
            standalone_threshold_bytes: 100,
            ..PublisherConfig::default()
        };
        let targets = propose(&[5, 12, 8, 100, 15], &config);
        assert_eq!(targets[0].candidate_indices, [0]);
        assert_eq!(targets[1].kind, UploadKind::CasBatch);
        assert_eq!(targets[1].candidate_indices, [1, 2]);
        assert_eq!(targets[2].kind, UploadKind::CasObject);
        assert_eq!(targets[2].candidate_indices, [3]);
        assert_eq!(targets[3].candidate_indices, [4]);
    }

    #[test]
    fn recording_target_exists_without_snapshots() {
        let targets = propose(&[], &PublisherConfig::default());
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].kind, UploadKind::Recording);
        assert!(targets[0].candidate_indices.is_empty());
    }
}
