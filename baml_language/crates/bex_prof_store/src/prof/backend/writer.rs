//! Per-session stream writer: batches meta records and per-execution data
//! groups and publishes them on the consumer thread (streams spec §5.2–5.4).
//!
//! Driven ONLY by the consumer thread (`maintain_sessions` after the ready
//! sweep, and the `Flush`/`EngineClosed` control paths); checkpoint readers
//! on other threads only read through the session's writer mutex.

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use rustc_hash::FxHashMap as HashMap;

use super::{
    DataGroup, EvidenceFact, ExecutionEndStatus, ExecutionHandle, ExecutionHealthSnapshot,
    MetaRecord, ProfilerStore, PublishBatchResult, Reservation, ResolveIndeterminateResult,
    SealedCctEpoch, StreamHighWater,
    decoder::{DirectDecoder, EvidenceBatchStats, ExecutionRuntime, with_runtime},
    encode_cct_epoch, encode_evidence_facts,
};
use crate::ids::ThreadRef;

/// Live stream-level counters snapshot (replaces `ProfilerCheckpoint`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StreamCheckpoint {
    pub high_water: StreamHighWater,
    pub pending_groups: u32,
    pub pending_meta: u32,
    pub oldest_pending_age: Option<Duration>,
    pub publication_inflight: bool,
}

/// Per-execution counters snapshot; `None` after the slot is released.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ExecutionCheckpoint {
    pub root: ThreadRef,
    pub health: ExecutionHealthSnapshot,
    pub queued: super::QueueHealthSnapshot,
    pub data_first_seq: u64,
    pub data_last_seq: u64,
}

/// Process-global loss counters (MVP §8.4 additions). Meta-plane loss is
/// tolerated and never latches the store, but it is never silent.
pub mod counters {
    use std::sync::atomic::{AtomicU64, Ordering};

    pub static META_BATCH_LOST: AtomicU64 = AtomicU64::new(0);
    pub static ROOT_ENDED_LOST: AtomicU64 = AtomicU64::new(0);
    pub static FUNCTION_TABLE_PUBLISH_FAILED: AtomicU64 = AtomicU64::new(0);

    pub fn meta_batch_lost() -> u64 {
        META_BATCH_LOST.load(Ordering::Relaxed)
    }

    pub fn root_ended_lost() -> u64 {
        ROOT_ENDED_LOST.load(Ordering::Relaxed)
    }

    pub fn function_table_publish_failed() -> u64 {
        FUNCTION_TABLE_PUBLISH_FAILED.load(Ordering::Relaxed)
    }

    pub(crate) fn bump(counter: &AtomicU64, by: u64) {
        counter.fetch_add(by, Ordering::Relaxed);
    }
}

pub(super) struct WriterEnv<'a> {
    pub publishers: &'a [Mutex<Option<ExecutionRuntime>>],
}

pub(super) struct PendingGroup {
    root: ThreadRef,
    handle: ExecutionHandle,
    cct: Option<SealedCctEpoch>,
    evidence: Vec<EvidenceFact>,
    batch_ids: Vec<u64>,
    stats: EvidenceBatchStats,
    reservations: Vec<Reservation>,
    bytes: u64,
}

impl PendingGroup {
    fn has_cct(&self) -> bool {
        self.cct
            .as_ref()
            .is_some_and(|cct| !cct.contexts.is_empty() || !cct.overflow.is_empty())
    }
}

struct PendingRootEnded {
    root: ThreadRef,
    ended_ns: u64,
    status: ExecutionEndStatus,
    health: ExecutionHealthSnapshot,
}

#[derive(Clone, Copy, Debug, Default)]
struct ExecPublication {
    first: u64,
    last: u64,
    count: u64,
    root_started_lost: bool,
}

enum InflightBatch {
    Meta,
    Data {
        sequence: u64,
        groups: Vec<PendingGroup>,
    },
}

pub(super) struct StreamWriter {
    store: Arc<ProfilerStore>,
    publish_interval: Duration,
    /// `EngineStarted` records currently pending in `pending_meta_pre`; more
    /// than 64 forces an immediate meta-pre publication instead of growing
    /// the vector (streams spec §5.4).
    pending_engines: usize,
    pending_meta_pre: Vec<MetaRecord>,
    pending_meta_post: Vec<PendingRootEnded>,
    pending_groups: BTreeMap<[u8; 32], PendingGroup>,
    pending_bytes: u64,
    oldest_pending: Option<Instant>,
    exec_index: HashMap<ThreadRef, ExecPublication>,
    indeterminate: Option<super::IndeterminateToken>,
    inflight: Option<InflightBatch>,
    segment_target_bytes: u64,
    _meta_queue: Reservation,
}

impl std::fmt::Debug for StreamWriter {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StreamWriter")
            .field("pending_meta_pre", &self.pending_meta_pre.len())
            .field("pending_meta_post", &self.pending_meta_post.len())
            .field("pending_groups", &self.pending_groups.len())
            .field("pending_bytes", &self.pending_bytes)
            .finish_non_exhaustive()
    }
}

impl StreamWriter {
    pub(super) fn new(
        store: Arc<ProfilerStore>,
        publish_interval: Duration,
        segment_target_bytes: u64,
        meta_queue: Reservation,
        stream_started: MetaRecord,
    ) -> Self {
        let mut writer = Self {
            store,
            publish_interval,
            pending_engines: 0,
            pending_meta_pre: Vec::new(),
            pending_meta_post: Vec::new(),
            pending_groups: BTreeMap::new(),
            pending_bytes: 0,
            oldest_pending: None,
            exec_index: HashMap::default(),
            indeterminate: None,
            inflight: None,
            segment_target_bytes,
            _meta_queue: meta_queue,
        };
        // `StreamStarted` only when nothing was ever committed to the meta
        // plane — a re-opened stream never re-emits it.
        if writer.store.high_water().meta == 0 {
            debug_assert!(matches!(stream_started, MetaRecord::StreamStarted { .. }));
            writer.pending_meta_pre.push(stream_started);
            writer.oldest_pending = Some(Instant::now());
        }
        writer
    }

    /// Admission facts from `take_admitted`, enqueued in one call so an
    /// `EngineStarted` and its `RootStarted`s are never split across cycles.
    pub(super) fn enqueue_admitted(&mut self, records: Vec<MetaRecord>, now: Instant) {
        if records.is_empty() {
            return;
        }
        self.pending_engines += records
            .iter()
            .filter(|record| matches!(record, MetaRecord::EngineStarted { .. }))
            .count();
        self.pending_meta_pre.extend(records);
        self.oldest_pending.get_or_insert(now);
    }

    /// Infallible: the `meta_queue` reservation covers 2 × execution slots.
    pub(super) fn enqueue_root_ended(
        &mut self,
        root: ThreadRef,
        ended_ns: u64,
        status: ExecutionEndStatus,
        health: ExecutionHealthSnapshot,
        now: Instant,
    ) {
        self.pending_meta_post.push(PendingRootEnded {
            root,
            ended_ns,
            status,
            health,
        });
        self.oldest_pending.get_or_insert(now);
    }

    /// One hand-off of an execution's sealed CCT epoch and/or evidence batch.
    /// A hand-off with no contexts, no overflow, and no evidence is a no-op.
    /// Reservations transfer to the writer; released at `Committed`/`Lost`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn hand_off(
        &mut self,
        handle: ExecutionHandle,
        root: ThreadRef,
        cct: Option<SealedCctEpoch>,
        evidence: Vec<EvidenceFact>,
        batch_id: Option<u64>,
        stats: EvidenceBatchStats,
        reservations: Vec<Reservation>,
        now: Instant,
    ) {
        let cct = cct.filter(|cct| !cct.contexts.is_empty() || !cct.overflow.is_empty());
        if cct.is_none() && evidence.is_empty() {
            return;
        }
        let bytes = cct
            .as_ref()
            .map_or(0, SealedCctEpoch::accounted_bytes)
            .saturating_add(
                reservations
                    .iter()
                    .map(Reservation::accounted_bytes)
                    .fold(0u64, u64::saturating_add),
            );
        let key = super::store::thread_ref_bytes(root);
        match self.pending_groups.entry(key) {
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                let group = entry.get_mut();
                debug_assert_eq!(group.handle, handle, "one execution, one live handle");
                match (&mut group.cct, cct) {
                    (Some(existing), Some(new)) => existing.merge(new),
                    (slot @ None, Some(new)) => *slot = Some(new),
                    (_, None) => {}
                }
                group.evidence.extend(evidence);
                group.batch_ids.extend(batch_id);
                group.stats.add(stats);
                group.reservations.extend(reservations);
                group.bytes = group.bytes.saturating_add(bytes);
            }
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(PendingGroup {
                    root,
                    handle,
                    cct,
                    evidence,
                    batch_ids: batch_id.into_iter().collect(),
                    stats,
                    reservations,
                    bytes,
                });
            }
        }
        self.pending_bytes = self.pending_bytes.saturating_add(bytes);
        self.oldest_pending.get_or_insert(now);
    }

    pub(super) fn checkpoint(&self, now: Instant) -> StreamCheckpoint {
        StreamCheckpoint {
            high_water: self.store.high_water(),
            pending_groups: u32::try_from(self.pending_groups.len()).unwrap_or(u32::MAX),
            pending_meta: u32::try_from(self.pending_meta_pre.len() + self.pending_meta_post.len())
                .unwrap_or(u32::MAX),
            oldest_pending_age: self
                .oldest_pending
                .map(|oldest| now.saturating_duration_since(oldest)),
            publication_inflight: self.indeterminate.is_some(),
        }
    }

    pub(super) fn exec_publication_range(&self, root: ThreadRef) -> (u64, u64) {
        self.exec_index
            .get(&root)
            .map_or((0, 0), |publication| (publication.first, publication.last))
    }

    /// The publication cycle (streams spec §5.3): meta-pre → data →
    /// meta-post, with the `RootEnded` eligibility rule.
    pub(super) fn publish_if_due(
        &mut self,
        now: Instant,
        force: bool,
        decoder: &mut DirectDecoder,
        env: &WriterEnv<'_>,
    ) {
        // Step 1: resolve a parked ambiguity before anything publishes. The
        // token may be the writer's own or a foreign one (a CAS publication
        // that went indeterminate parks its token in the store).
        let token = self
            .indeterminate
            .or_else(|| self.store.pending_indeterminate_token());
        if let Some(token) = token {
            match self.store.resolve_indeterminate(token) {
                ResolveIndeterminateResult::StillIndeterminate => return,
                ResolveIndeterminateResult::Committed
                | ResolveIndeterminateResult::TokenMismatch => {
                    if let Some(inflight) = self.inflight.take() {
                        self.apply_inflight_committed(inflight, decoder, env);
                    }
                    self.indeterminate = None;
                }
            }
        }

        // Step 2: due?
        let age_due = self
            .oldest_pending
            .is_some_and(|oldest| now.saturating_duration_since(oldest) >= self.publish_interval);
        let due = force
            || self.pending_bytes >= self.segment_target_bytes
            || self.pending_engines > 64
            || age_due;
        if !due {
            return;
        }

        let gate_open = self.store.is_normal_admission_open();

        // Step 3: meta-pre (skipped under a latched gate — 3′ folds it into
        // the terminal batch below).
        if gate_open && !self.pending_meta_pre.is_empty() {
            let batch = std::mem::take(&mut self.pending_meta_pre);
            self.pending_engines = 0;
            match self.store.publish_meta(&batch, false) {
                PublishBatchResult::Committed { .. } => {}
                PublishBatchResult::Lost(_) => self.record_meta_pre_lost(&batch),
                PublishBatchResult::Blocked(token) => {
                    self.pending_engines = batch
                        .iter()
                        .filter(|record| matches!(record, MetaRecord::EngineStarted { .. }))
                        .count();
                    self.pending_meta_pre = batch;
                    self.indeterminate = Some(token);
                    return;
                }
                PublishBatchResult::Indeterminate { token, sequence: _ } => {
                    self.inflight = Some(InflightBatch::Meta);
                    self.indeterminate = Some(token);
                    return;
                }
            }
        }

        // Step 4: data, in ThreadRef order, packed to the segment target;
        // once a cycle starts it drains.
        while !self.pending_groups.is_empty() {
            let mut batch: Vec<PendingGroup> = Vec::new();
            let mut batch_bytes = 0u64;
            let keys: Vec<[u8; 32]> = self.pending_groups.keys().copied().collect();
            for key in keys {
                let bytes = self.pending_groups[&key].bytes;
                if !batch.is_empty()
                    && batch_bytes.saturating_add(bytes) > self.segment_target_bytes
                {
                    break;
                }
                batch_bytes = batch_bytes.saturating_add(bytes);
                batch.push(self.pending_groups.remove(&key).expect("key just listed"));
                if batch_bytes > self.segment_target_bytes {
                    break; // a single oversize group goes alone
                }
            }
            self.pending_bytes = self.pending_bytes.saturating_sub(batch_bytes);

            let encoded: Vec<DataGroup> = batch.iter().map(encode_group).collect();
            match self.store.publish_data(&encoded) {
                PublishBatchResult::Committed { sequence } => {
                    for group in batch {
                        self.group_committed(group, sequence, decoder, env);
                    }
                }
                PublishBatchResult::Lost(_) => {
                    for group in batch {
                        self.group_lost(group, decoder, env);
                    }
                }
                PublishBatchResult::Blocked(token) => {
                    self.restore_groups(batch, batch_bytes);
                    self.indeterminate = Some(token);
                    return;
                }
                PublishBatchResult::Indeterminate { token, sequence } => {
                    self.inflight = Some(InflightBatch::Data {
                        sequence,
                        groups: batch,
                    });
                    self.indeterminate = Some(token);
                    return;
                }
            }
        }

        // Step 5: meta-post — a RootEnded(x) is eligible iff no group of x is
        // pending or in flight. 3′: under a latched gate the pre records ride
        // ahead of the RootEnded records in one terminal batch.
        let mut eligible = Vec::new();
        let mut still_pending = Vec::new();
        for pending in self.pending_meta_post.drain(..) {
            let key = super::store::thread_ref_bytes(pending.root);
            let inflight_holds = match &self.inflight {
                Some(InflightBatch::Data { groups, .. }) => {
                    groups.iter().any(|group| group.root == pending.root)
                }
                _ => false,
            };
            if self.pending_groups.contains_key(&key) || inflight_holds {
                still_pending.push(pending);
            } else {
                eligible.push(pending);
            }
        }
        self.pending_meta_post = still_pending;
        if !eligible.is_empty() || (!gate_open && !self.pending_meta_pre.is_empty()) {
            let mut records = if gate_open {
                Vec::with_capacity(eligible.len())
            } else {
                // 3′ prefix order: StreamStarted, EngineStarted, RootStarted
                // (enqueue order already satisfies it).
                self.pending_engines = 0;
                std::mem::take(&mut self.pending_meta_pre)
            };
            let pre_len = records.len();
            for pending in &eligible {
                let publication = self.exec_index.remove(&pending.root).unwrap_or_default();
                let flags = if publication.root_started_lost {
                    super::ROOT_ENDED_FLAG_ROOT_STARTED_LOST
                } else {
                    0
                };
                records.push(MetaRecord::RootEnded {
                    root: pending.root,
                    ended_ns: pending.ended_ns,
                    status: pending.status,
                    flags,
                    data_first_seq: publication.first,
                    data_last_seq: publication.last,
                    data_segment_count: publication.count,
                    health: pending.health,
                });
            }
            match self.store.publish_meta(&records, true) {
                PublishBatchResult::Committed { .. } => {}
                PublishBatchResult::Lost(_) => {
                    if pre_len > 0 {
                        self.record_meta_pre_lost(&records[..pre_len]);
                    }
                    counters::bump(
                        &counters::ROOT_ENDED_LOST,
                        u64::try_from(records.len() - pre_len).unwrap_or(u64::MAX),
                    );
                }
                PublishBatchResult::Blocked(token) => {
                    // Keep the batch pending: restore the un-encoded form.
                    self.restore_meta_post(records, pre_len, eligible);
                    self.indeterminate = Some(token);
                    return;
                }
                PublishBatchResult::Indeterminate { token, sequence: _ } => {
                    self.inflight = Some(InflightBatch::Meta);
                    self.indeterminate = Some(token);
                    return;
                }
            }
        }

        // Step 6: reset the age trigger.
        self.oldest_pending = if self.pending_meta_pre.is_empty()
            && self.pending_meta_post.is_empty()
            && self.pending_groups.is_empty()
        {
            None
        } else {
            Some(now)
        };
    }

    fn restore_groups(&mut self, batch: Vec<PendingGroup>, batch_bytes: u64) {
        for group in batch {
            let key = super::store::thread_ref_bytes(group.root);
            debug_assert!(!self.pending_groups.contains_key(&key));
            self.pending_groups.insert(key, group);
        }
        self.pending_bytes = self.pending_bytes.saturating_add(batch_bytes);
    }

    /// Restores a blocked meta-post batch to its pending form. Encoded
    /// `RootEnded` records are discarded in favour of the retained
    /// `PendingRootEnded`s (their `exec_index` entries were consumed at
    /// encode time and are re-created empty on the retry — acceptable only
    /// because a `Blocked` batch was never written and the entries are
    /// restored below).
    fn restore_meta_post(
        &mut self,
        records: Vec<MetaRecord>,
        pre_len: usize,
        eligible: Vec<PendingRootEnded>,
    ) {
        // Re-create the exec_index entries consumed at encode time.
        for record in &records[pre_len..] {
            let MetaRecord::RootEnded {
                root,
                flags,
                data_first_seq,
                data_last_seq,
                data_segment_count,
                ..
            } = record
            else {
                continue;
            };
            self.exec_index.insert(
                *root,
                ExecPublication {
                    first: *data_first_seq,
                    last: *data_last_seq,
                    count: *data_segment_count,
                    root_started_lost: flags & super::ROOT_ENDED_FLAG_ROOT_STARTED_LOST != 0,
                },
            );
        }
        if pre_len > 0 {
            let mut pre: Vec<MetaRecord> = records;
            pre.truncate(pre_len);
            debug_assert!(self.pending_meta_pre.is_empty());
            self.pending_engines = pre
                .iter()
                .filter(|record| matches!(record, MetaRecord::EngineStarted { .. }))
                .count();
            self.pending_meta_pre = pre;
        }
        self.pending_meta_post.extend(eligible);
    }

    fn record_meta_pre_lost(&mut self, batch: &[MetaRecord]) {
        counters::bump(&counters::META_BATCH_LOST, 1);
        for record in batch {
            if let MetaRecord::RootStarted { root, .. } = record {
                self.exec_index.entry(*root).or_default().root_started_lost = true;
            }
        }
    }

    fn apply_inflight_committed(
        &mut self,
        inflight: InflightBatch,
        decoder: &mut DirectDecoder,
        env: &WriterEnv<'_>,
    ) {
        match inflight {
            InflightBatch::Meta => {}
            InflightBatch::Data { sequence, groups } => {
                for group in groups {
                    self.group_committed(group, sequence, decoder, env);
                }
            }
        }
    }

    #[allow(clippy::needless_pass_by_value)] // consuming releases the group's reservations
    fn group_committed(
        &mut self,
        group: PendingGroup,
        sequence: u64,
        decoder: &mut DirectDecoder,
        env: &WriterEnv<'_>,
    ) {
        let publication = self.exec_index.entry(group.root).or_default();
        if publication.first == 0 {
            publication.first = sequence;
        }
        publication.last = sequence;
        publication.count = publication.count.saturating_add(1);
        decoder.apply_batch_outcomes(group.handle, &group.batch_ids, true);
        let stats = group.stats;
        self.record_health(env, group.handle, group.root, |health| {
            health.record_evidence_committed(stats);
        });
        // Reservations release on drop.
    }

    #[allow(clippy::needless_pass_by_value)] // consuming releases the group's reservations
    fn group_lost(
        &mut self,
        group: PendingGroup,
        decoder: &mut DirectDecoder,
        env: &WriterEnv<'_>,
    ) {
        decoder.apply_batch_outcomes(group.handle, &group.batch_ids, false);
        let stats = group.stats;
        let had_cct = group.has_cct();
        let had_evidence = !group.evidence.is_empty();
        self.record_health(env, group.handle, group.root, |health| {
            health.record_evidence_publish_failed(stats);
            if had_cct {
                health.cct_segment_publish_failed =
                    health.cct_segment_publish_failed.saturating_add(1);
            }
            if had_evidence {
                health.evidence_segment_publish_failed =
                    health.evidence_segment_publish_failed.saturating_add(1);
            }
        });
    }

    /// The health sink (streams spec §5.3): the live `ExecutionRuntime` while
    /// the slot is valid, else the still-pending `RootEnded`'s snapshot.
    fn record_health(
        &mut self,
        env: &WriterEnv<'_>,
        handle: ExecutionHandle,
        root: ThreadRef,
        fold: impl FnOnce(&mut ExecutionHealthSnapshot),
    ) {
        let mut fold = Some(fold);
        with_runtime(env.publishers, handle, |runtime| {
            if let Some(fold) = fold.take() {
                fold(&mut runtime.health);
            }
        });
        let Some(fold) = fold.take() else { return };
        if let Some(pending) = self
            .pending_meta_post
            .iter_mut()
            .find(|pending| pending.root == root)
        {
            fold(&mut pending.health);
        } else {
            debug_assert!(
                false,
                "a group outcome must find a live slot or a pending RootEnded"
            );
        }
    }
}

fn encode_group(group: &PendingGroup) -> DataGroup {
    let (cct_health, cct_record_count, cct) = match &group.cct {
        Some(sealed) if !sealed.contexts.is_empty() || !sealed.overflow.is_empty() => {
            let encoded = encode_cct_epoch(sealed);
            (sealed.health, encoded.record_count, encoded.payload)
        }
        _ => (super::CounterHealth::default(), 0, Vec::new()),
    };
    let (evidence_record_count, evidence) = if group.evidence.is_empty() {
        (0, Vec::new())
    } else {
        let encoded = encode_evidence_facts(&group.evidence);
        (encoded.record_count, encoded.payload)
    };
    DataGroup {
        root: group.root,
        cct_health,
        cct_record_count,
        cct,
        evidence_record_count,
        evidence,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use tempfile::TempDir;

    use super::{
        super::{
            ActiveCctEpoch, DiskBudget, ExecutionHandle, MeasuredLayouts, ParentContextRef, Plane,
            ProfilerMemoryGovernor, ProfilerSizingPolicy, ROOT_ENDED_FLAG_ROOT_STARTED_LOST,
            StorePlatform, decode_data_segment, decode_meta_segment, segment_path,
        },
        *,
    };
    use crate::{
        ids::{BexThreadId, BoundaryId, EngineId, FunctionId, ProcessEuid, ProgramId},
        prof::record::FunctionEndStatus,
    };

    #[derive(Debug, Default)]
    struct TestPlatform {
        fail_next_rename: AtomicBool,
        fail_next_dir_sync: AtomicBool,
    }

    impl StorePlatform for TestPlatform {
        fn available_space(&self, _path: &std::path::Path) -> std::io::Result<u64> {
            Ok(u64::MAX)
        }

        fn sync_dir(&self, _path: &std::path::Path) -> std::io::Result<()> {
            if self.fail_next_dir_sync.swap(false, Ordering::Relaxed) {
                return Err(std::io::Error::other("injected directory sync failure"));
            }
            Ok(())
        }

        fn before_rename(
            &self,
            _kind: super::super::StoreFileKind,
            _temporary: &std::path::Path,
        ) -> std::io::Result<()> {
            if self.fail_next_rename.swap(false, Ordering::Relaxed) {
                return Err(std::io::Error::other("injected pre-rename failure"));
            }
            Ok(())
        }
    }

    struct Harness {
        _temp: TempDir,
        root: std::path::PathBuf,
        stream: super::super::StreamId,
        platform: Arc<TestPlatform>,
        memory: ProfilerMemoryGovernor,
        writer: StreamWriter,
        publishers: Vec<Mutex<Option<ExecutionRuntime>>>,
        decoder: DirectDecoder,
    }

    fn harness(euid_byte: u8) -> Harness {
        let temp = TempDir::new().unwrap();
        let root = temp.path().join(".baml/profiles-v1");
        let stream = super::super::StreamId(ProcessEuid([euid_byte; 16]));
        let platform = Arc::new(TestPlatform::default());
        let store = ProfilerStore::open(
            root.clone(),
            DiskBudget {
                max_project_bytes: 1024 * 1024 * 1024,
                minimum_free_bytes: 0,
            },
            Arc::clone(&platform) as Arc<dyn StorePlatform>,
            stream,
        )
        .unwrap();
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        let memory = ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1);
        let meta_queue = memory
            .try_reserve(
                super::super::ReservationClass::General,
                super::super::Owner::Writer,
                64 * 1024,
            )
            .unwrap();
        let writer = StreamWriter::new(
            store,
            Duration::MAX,
            sizing.segment_target_bytes,
            meta_queue,
            MetaRecord::StreamStarted {
                pid: 1,
                zero_unix_ns: 0,
                baml_version: "test".to_string(),
                os_arch: "test".to_string(),
            },
        );
        Harness {
            _temp: temp,
            root,
            stream,
            platform,
            memory,
            writer,
            publishers: Vec::new(),
            decoder: DirectDecoder::default(),
        }
    }

    /// Installs a live `ExecutionRuntime` for `handle(slot)` so the writer's
    /// health sink finds it (the production invariant: a group can only be
    /// in flight while its slot is live or its `RootEnded` is pending).
    fn install_runtime(harness: &mut Harness, slot: u32, root: ThreadRef) {
        while harness.publishers.len() <= slot as usize {
            harness.publishers.push(Mutex::new(None));
        }
        *harness.publishers[slot as usize].lock().unwrap() = Some(ExecutionRuntime::new(
            1,
            root,
            BoundaryId::from_bytes([7; 16]),
            ProgramId([9; 16]),
        ));
    }

    fn thread(stream: super::super::StreamId, thread_id: u64) -> ThreadRef {
        ThreadRef {
            process_euid: stream.0,
            engine_id: EngineId(1),
            thread_id: BexThreadId(thread_id),
        }
    }

    fn handle(slot: u32) -> ExecutionHandle {
        ExecutionHandle {
            slot,
            generation: 1,
        }
    }

    fn sealed_epoch(memory: &ProfilerMemoryGovernor, calls: u64) -> super::super::SealedCctEpoch {
        let mut epoch = ActiveCctEpoch::new(
            ProgramId([9; 16]),
            MeasuredLayouts::V1.population_item_min_bytes,
        );
        for _ in 0..calls {
            let admission = epoch.record_start(
                ParentContextRef::Root,
                FunctionId(7),
                None,
                super::super::EdgeKind::Root,
                false,
                memory,
            );
            epoch.record_end(admission, FunctionEndStatus::Ok, 10, 0, 0);
        }
        epoch.seal()
    }

    fn publish(harness: &mut Harness, force: bool) {
        let env = WriterEnv {
            publishers: &harness.publishers,
        };
        harness
            .writer
            .publish_if_due(Instant::now(), force, &mut harness.decoder, &env);
    }

    /// Two hand-offs of one execution in one cycle merge into one group with
    /// summed counters (streams spec §9).
    #[test]
    fn two_hand_offs_of_one_execution_merge_into_one_group() {
        let mut harness = harness(51);
        let root = thread(harness.stream, 3);
        install_runtime(&mut harness, 0, root);
        for _ in 0..2 {
            let sealed = sealed_epoch(&harness.memory, 5);
            harness.writer.hand_off(
                handle(0),
                root,
                Some(sealed),
                Vec::new(),
                None,
                EvidenceBatchStats::default(),
                Vec::new(),
                Instant::now(),
            );
        }
        publish(&mut harness, true);
        let bytes =
            std::fs::read(segment_path(&harness.root, harness.stream, Plane::Data, 1)).unwrap();
        let decoded = decode_data_segment(&bytes, harness.stream.0).unwrap();
        assert_eq!(decoded.groups.len(), 1, "one execution, one merged group");
        let cct = decoded.groups[0].decode_cct().unwrap().unwrap();
        assert_eq!(cct.contexts.len(), 1);
        assert_eq!(cct.contexts[0].counters.invocations_started, 10);
        assert!(
            !segment_path(&harness.root, harness.stream, Plane::Data, 2).exists(),
            "one cycle, one data segment"
        );
    }

    /// A `RootEnded` enqueued while its group is pending is not in the next
    /// meta-pre; it lands in meta-post with the final range (pin a
    /// 3-data-segment execution: first=1, last=3, count=3).
    #[test]
    fn root_ended_waits_for_its_groups_and_records_the_final_range() {
        let mut harness = harness(52);
        let root = thread(harness.stream, 3);
        install_runtime(&mut harness, 0, root);
        for cycle in 0..3 {
            let sealed = sealed_epoch(&harness.memory, 1);
            harness.writer.hand_off(
                handle(0),
                root,
                Some(sealed),
                Vec::new(),
                None,
                EvidenceBatchStats::default(),
                Vec::new(),
                Instant::now(),
            );
            if cycle == 2 {
                harness.writer.enqueue_root_ended(
                    root,
                    100,
                    ExecutionEndStatus::Succeeded,
                    ExecutionHealthSnapshot::default(),
                    Instant::now(),
                );
            }
            publish(&mut harness, true);
        }
        // Data plane: three segments, one group each.
        for sequence in 1..=3 {
            assert!(
                segment_path(&harness.root, harness.stream, Plane::Data, sequence).is_file(),
                "data segment {sequence}"
            );
        }
        // Meta plane: pre (StreamStarted) then the final RootEnded.
        let last_meta = (1..=8)
            .rev()
            .find(|sequence| {
                segment_path(&harness.root, harness.stream, Plane::Meta, *sequence).is_file()
            })
            .unwrap();
        let bytes = std::fs::read(segment_path(
            &harness.root,
            harness.stream,
            Plane::Meta,
            last_meta,
        ))
        .unwrap();
        let decoded = decode_meta_segment(&bytes, harness.stream.0).unwrap();
        assert!(matches!(
            decoded.records.as_slice(),
            [MetaRecord::RootEnded {
                data_first_seq: 1,
                data_last_seq: 3,
                data_segment_count: 3,
                flags: 0,
                ..
            }]
        ));
    }

    /// Meta-pre `Lost` drops the batch (never retried), counts
    /// `meta_batch_lost`, and sets the execution's `root_started_lost` flag
    /// on its eventual `RootEnded` (streams spec §5.3 step 3).
    #[test]
    fn lost_meta_pre_sets_root_started_lost_on_the_root_ended() {
        let mut harness = harness(53);
        let root = thread(harness.stream, 3);
        harness.writer.enqueue_admitted(
            vec![MetaRecord::RootStarted {
                root,
                started_ns: 1,
                runtime_id: BoundaryId::from_bytes([7; 16]),
            }],
            Instant::now(),
        );
        let lost_before = counters::meta_batch_lost();
        harness
            .platform
            .fail_next_rename
            .store(true, Ordering::Relaxed);
        publish(&mut harness, true);
        assert_eq!(counters::meta_batch_lost(), lost_before + 1);

        harness.writer.enqueue_root_ended(
            root,
            9,
            ExecutionEndStatus::Succeeded,
            ExecutionHealthSnapshot::default(),
            Instant::now(),
        );
        publish(&mut harness, true);
        let bytes =
            std::fs::read(segment_path(&harness.root, harness.stream, Plane::Meta, 1)).unwrap();
        let decoded = decode_meta_segment(&bytes, harness.stream.0).unwrap();
        assert!(matches!(
            decoded.records.as_slice(),
            [MetaRecord::RootEnded { flags, .. }]
                if flags & ROOT_ENDED_FLAG_ROOT_STARTED_LOST != 0
        ));
        // Reader-facing: the execution is listed with `RootStartedLost`.
        let executions = super::super::list_executions(&harness.root).unwrap();
        assert_eq!(executions.len(), 1);
        assert_eq!(
            executions[0].index_state,
            super::super::IndexState::RootStartedLost
        );
    }

    /// A post-rename indeterminate data batch is applied exactly once as
    /// committed on resolution; a `Blocked` batch is published once at the
    /// next sequence — no duplicate groups either way (streams spec §9).
    #[test]
    fn indeterminate_then_blocked_batches_resolve_without_duplicates() {
        let mut harness = harness(54);
        let root_a = thread(harness.stream, 3);
        let root_b = thread(harness.stream, 4);
        install_runtime(&mut harness, 0, root_a);
        install_runtime(&mut harness, 1, root_b);
        // Drain the pending StreamStarted header so the injected fault hits
        // the data batch, not the meta-pre batch.
        publish(&mut harness, true);

        // First batch goes post-rename indeterminate.
        harness.writer.hand_off(
            handle(0),
            root_a,
            Some(sealed_epoch(&harness.memory, 2)),
            Vec::new(),
            None,
            EvidenceBatchStats::default(),
            Vec::new(),
            Instant::now(),
        );
        harness
            .platform
            .fail_next_dir_sync
            .store(true, Ordering::Relaxed);
        publish(&mut harness, true);

        // Second batch arrives while indeterminate: Blocked, stays pending.
        harness.writer.hand_off(
            handle(1),
            root_b,
            Some(sealed_epoch(&harness.memory, 1)),
            Vec::new(),
            None,
            EvidenceBatchStats::default(),
            Vec::new(),
            Instant::now(),
        );
        publish(&mut harness, true); // step 1 resolves, then publishes pending

        harness.writer.enqueue_root_ended(
            root_a,
            5,
            ExecutionEndStatus::Succeeded,
            ExecutionHealthSnapshot::default(),
            Instant::now(),
        );
        harness.writer.enqueue_root_ended(
            root_b,
            6,
            ExecutionEndStatus::Succeeded,
            ExecutionHealthSnapshot::default(),
            Instant::now(),
        );
        publish(&mut harness, true);

        let one =
            std::fs::read(segment_path(&harness.root, harness.stream, Plane::Data, 1)).unwrap();
        let one = decode_data_segment(&one, harness.stream.0).unwrap();
        assert_eq!(one.groups.len(), 1);
        assert_eq!(one.groups[0].root, root_a);
        let two =
            std::fs::read(segment_path(&harness.root, harness.stream, Plane::Data, 2)).unwrap();
        let two = decode_data_segment(&two, harness.stream.0).unwrap();
        assert_eq!(two.groups.len(), 1);
        assert_eq!(two.groups[0].root, root_b);
        assert!(!segment_path(&harness.root, harness.stream, Plane::Data, 3).exists());

        // Ranges reflect exactly-once application.
        let last_meta = (1..=8)
            .rev()
            .find(|sequence| {
                segment_path(&harness.root, harness.stream, Plane::Meta, *sequence).is_file()
            })
            .unwrap();
        let meta = std::fs::read(segment_path(
            &harness.root,
            harness.stream,
            Plane::Meta,
            last_meta,
        ))
        .unwrap();
        let meta = decode_meta_segment(&meta, harness.stream.0).unwrap();
        let ranges: Vec<(u64, u64, u64)> = meta
            .records
            .iter()
            .filter_map(|record| match record {
                MetaRecord::RootEnded {
                    data_first_seq,
                    data_last_seq,
                    data_segment_count,
                    ..
                } => Some((*data_first_seq, *data_last_seq, *data_segment_count)),
                _ => None,
            })
            .collect();
        assert_eq!(ranges, vec![(1, 1, 1), (2, 2, 1)]);
    }
}
