use std::{
    collections::VecDeque,
    fmt,
    sync::{Arc, Mutex},
};

use crate::{
    ids::{BoundaryId, EngineId, ProcessEuid},
    run::TraceCallKey,
};

pub const DEFAULT_NATIVE_STAGING_BYTES: usize = 32 * 1024 * 1024;
pub const DEFAULT_WASM_STAGING_BYTES: usize = 8 * 1024 * 1024;

/// Hierarchical call identity used for subtree promotion.
///
/// `TraceCallKey` currently names only one call. The host can build this path
/// while frames are open without making the staging ring depend on engine
/// internals.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CallPath {
    pub boundary_id: BoundaryId,
    pub process_euid: ProcessEuid,
    pub engine_id: EngineId,
    pub logical_thread_id: u64,
    pub call_ids: Vec<u64>,
}

impl CallPath {
    #[must_use]
    pub fn single(boundary_id: BoundaryId, call: TraceCallKey) -> Self {
        Self {
            boundary_id,
            process_euid: call.process_euid,
            engine_id: call.engine_id,
            logical_thread_id: call.thread_id.0,
            call_ids: vec![call.call_id.0],
        }
    }

    /// A boundary-wide scope used when the host observes only the terminal
    /// root failure. Empty `call_ids` intentionally acts as a wildcard across
    /// logical threads in the same process/engine boundary.
    #[must_use]
    pub fn boundary(
        boundary_id: BoundaryId,
        process_euid: ProcessEuid,
        engine_id: EngineId,
    ) -> Self {
        Self {
            boundary_id,
            process_euid,
            engine_id,
            logical_thread_id: 0,
            call_ids: Vec::new(),
        }
    }

    #[must_use]
    pub fn contains(&self, candidate: &Self) -> bool {
        self.boundary_id == candidate.boundary_id
            && self.process_euid == candidate.process_euid
            && self.engine_id == candidate.engine_id
            && (self.call_ids.is_empty()
                || (self.logical_thread_id == candidate.logical_thread_id
                    && candidate.call_ids.starts_with(&self.call_ids)))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct TriggerId(pub String);

impl fmt::Display for TriggerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// One owned draft retained speculatively.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StagedDraft {
    pub call: CallPath,
    pub body: Vec<u8>,
    /// Opaque caller metadata (capture role, value id, and similar).
    pub metadata: Vec<u8>,
}

impl StagedDraft {
    #[must_use]
    pub fn retained_bytes(&self) -> usize {
        self.body.len().saturating_add(self.metadata.len())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CaptureLossReason {
    StagingEvicted,
    StagingValueTooLarge,
    EvictionHistoryOverflow,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaptureLoss {
    pub reason: CaptureLossReason,
    pub call: CallPath,
    pub skipped_count: u64,
    pub skipped_bytes: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionAudit {
    pub trigger: TriggerId,
    pub scope: CallPath,
    pub records: u64,
    pub staged_evicted: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PromotionReport {
    pub promoted: Vec<StagedDraft>,
    pub losses: Vec<CaptureLoss>,
    pub audit: PromotionAudit,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ReleaseReport {
    pub released_records: usize,
    pub released_bytes: usize,
    pub cleared_eviction_tombstones: usize,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StageReport {
    pub retained: bool,
    pub evicted_records: usize,
    pub evicted_bytes: usize,
    /// Present when this draft could not be staged at all.
    pub loss: Option<CaptureLoss>,
}

#[derive(Clone, Debug)]
pub struct StagingRing {
    inner: Arc<Mutex<StagingInner>>,
}

#[derive(Clone, Debug)]
struct EvictedDraft {
    call: CallPath,
    bytes: usize,
}

#[derive(Debug)]
struct StagingInner {
    max_bytes: usize,
    max_eviction_tombstones: usize,
    current_bytes: usize,
    drafts: VecDeque<StagedDraft>,
    evicted: VecDeque<EvictedDraft>,
    unattributed_eviction_records: u64,
    unattributed_eviction_bytes: usize,
}

impl StagingRing {
    /// The eviction history is metadata-only and independently bounded.
    #[must_use]
    pub fn new(max_bytes: usize, max_eviction_tombstones: usize) -> Self {
        Self {
            inner: Arc::new(Mutex::new(StagingInner {
                max_bytes,
                max_eviction_tombstones,
                current_bytes: 0,
                drafts: VecDeque::new(),
                evicted: VecDeque::new(),
                unattributed_eviction_records: 0,
                unattributed_eviction_bytes: 0,
            })),
        }
    }

    #[must_use]
    pub fn native_default() -> Self {
        Self::new(DEFAULT_NATIVE_STAGING_BYTES, 4096)
    }

    #[must_use]
    pub fn wasm_default() -> Self {
        Self::new(DEFAULT_WASM_STAGING_BYTES, 4096)
    }

    #[must_use]
    pub fn current_bytes(&self) -> usize {
        self.lock().current_bytes
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.lock().drafts.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Reserve byte capacity before the expensive deep copy.
    ///
    /// `copy` is not called when `retained_bytes` exceeds the ring budget.
    /// Callers should ensure the returned draft reports the reserved size;
    /// a mismatch is converted into an explicit loss rather than overshooting
    /// the byte cap.
    pub fn stage_with(
        &self,
        call: CallPath,
        retained_bytes: usize,
        copy: impl FnOnce() -> StagedDraft,
    ) -> StageReport {
        let mut inner = self.lock();
        if retained_bytes > inner.max_bytes {
            return StageReport {
                retained: false,
                evicted_records: 0,
                evicted_bytes: 0,
                loss: Some(CaptureLoss {
                    reason: CaptureLossReason::StagingValueTooLarge,
                    call,
                    skipped_count: 1,
                    skipped_bytes: retained_bytes,
                }),
            };
        }

        let (evicted_records, evicted_bytes) = inner.evict_for_reservation(retained_bytes);
        let draft = copy();
        let actual_bytes = draft.retained_bytes();
        if draft.call != call || actual_bytes != retained_bytes {
            return StageReport {
                retained: false,
                evicted_records,
                evicted_bytes,
                loss: Some(CaptureLoss {
                    reason: CaptureLossReason::StagingValueTooLarge,
                    call,
                    skipped_count: 1,
                    skipped_bytes: actual_bytes,
                }),
            };
        }
        inner.current_bytes = inner.current_bytes.saturating_add(actual_bytes);
        inner.drafts.push_back(draft);
        StageReport {
            retained: true,
            evicted_records,
            evicted_bytes,
            loss: None,
        }
    }

    /// Move all retained drafts in `scope` to the caller's durable queue and
    /// report every still-attributable staging eviction.
    pub fn promote(&self, scope: &CallPath, trigger: TriggerId) -> PromotionReport {
        let mut inner = self.lock();
        let mut promoted = Vec::new();
        let mut retained = VecDeque::with_capacity(inner.drafts.len());
        while let Some(draft) = inner.drafts.pop_front() {
            if scope.contains(&draft.call) {
                inner.current_bytes = inner.current_bytes.saturating_sub(draft.retained_bytes());
                promoted.push(draft);
            } else {
                retained.push_back(draft);
            }
        }
        inner.drafts = retained;

        let mut losses = Vec::new();
        let mut unrelated_evictions = VecDeque::with_capacity(inner.evicted.len());
        let mut staged_evicted = 0_u64;
        while let Some(evicted) = inner.evicted.pop_front() {
            if scope.contains(&evicted.call) {
                staged_evicted = staged_evicted.saturating_add(1);
                losses.push(CaptureLoss {
                    reason: CaptureLossReason::StagingEvicted,
                    call: evicted.call,
                    skipped_count: 1,
                    skipped_bytes: evicted.bytes,
                });
            } else {
                unrelated_evictions.push_back(evicted);
            }
        }
        inner.evicted = unrelated_evictions;
        if inner.unattributed_eviction_records > 0 {
            let skipped_count = std::mem::take(&mut inner.unattributed_eviction_records);
            let skipped_bytes = std::mem::take(&mut inner.unattributed_eviction_bytes);
            staged_evicted = staged_evicted.saturating_add(skipped_count);
            losses.push(CaptureLoss {
                reason: CaptureLossReason::EvictionHistoryOverflow,
                // Once bounded tombstone history overflows, exact subtree
                // attribution is impossible. Conservatively attach the loss
                // to the firing scope instead of silently discarding it.
                call: scope.clone(),
                skipped_count,
                skipped_bytes,
            });
        }
        let records = u64::try_from(promoted.len()).unwrap_or(u64::MAX);
        PromotionReport {
            promoted,
            losses,
            audit: PromotionAudit {
                trigger,
                scope: scope.clone(),
                records,
                staged_evicted,
            },
        }
    }

    /// Drop speculative drafts when a frame/subtree completes normally.
    pub fn release(&self, scope: &CallPath) -> ReleaseReport {
        let mut inner = self.lock();
        let mut report = ReleaseReport::default();
        let mut retained = VecDeque::with_capacity(inner.drafts.len());
        while let Some(draft) = inner.drafts.pop_front() {
            if scope.contains(&draft.call) {
                let bytes = draft.retained_bytes();
                inner.current_bytes = inner.current_bytes.saturating_sub(bytes);
                report.released_records = report.released_records.saturating_add(1);
                report.released_bytes = report.released_bytes.saturating_add(bytes);
            } else {
                retained.push_back(draft);
            }
        }
        inner.drafts = retained;
        let before = inner.evicted.len();
        inner
            .evicted
            .retain(|evicted| !scope.contains(&evicted.call));
        report.cleared_eviction_tombstones = before.saturating_sub(inner.evicted.len());
        report
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, StagingInner> {
        self.inner
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl StagingInner {
    fn evict_for_reservation(&mut self, reserved_bytes: usize) -> (usize, usize) {
        let mut records = 0_usize;
        let mut bytes = 0_usize;
        while self.current_bytes.saturating_add(reserved_bytes) > self.max_bytes {
            let Some(evicted) = self.drafts.pop_front() else {
                break;
            };
            let evicted_bytes = evicted.retained_bytes();
            self.current_bytes = self.current_bytes.saturating_sub(evicted_bytes);
            records = records.saturating_add(1);
            bytes = bytes.saturating_add(evicted_bytes);
            self.evicted.push_back(EvictedDraft {
                call: evicted.call,
                bytes: evicted_bytes,
            });
        }
        while self.evicted.len() > self.max_eviction_tombstones {
            if let Some(evicted) = self.evicted.pop_front() {
                self.unattributed_eviction_records =
                    self.unattributed_eviction_records.saturating_add(1);
                self.unattributed_eviction_bytes = self
                    .unattributed_eviction_bytes
                    .saturating_add(evicted.bytes);
            }
        }
        (records, bytes)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use crate::ids::{BoundaryId, EngineId, ProcessEuid};

    use super::{CallPath, CaptureLossReason, StagedDraft, StagingRing, TriggerId};

    fn call(ids: &[u64]) -> CallPath {
        CallPath {
            boundary_id: BoundaryId::from_bytes([3; 16]),
            process_euid: ProcessEuid([4; 16]),
            engine_id: EngineId(5),
            logical_thread_id: 6,
            call_ids: ids.to_vec(),
        }
    }

    fn draft(call: CallPath, bytes: usize) -> StagedDraft {
        StagedDraft {
            call,
            body: vec![7; bytes],
            metadata: Vec::new(),
        }
    }

    #[test]
    fn failed_reservation_does_zero_copy_work() {
        let ring = StagingRing::new(4, 8);
        let copies = Arc::new(AtomicUsize::new(0));
        let copy_count = Arc::clone(&copies);
        let report = ring.stage_with(call(&[1]), 5, move || {
            copy_count.fetch_add(1, Ordering::Relaxed);
            draft(call(&[1]), 5)
        });
        assert!(!report.retained);
        assert_eq!(
            report.loss.unwrap().reason,
            CaptureLossReason::StagingValueTooLarge
        );
        assert_eq!(copies.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn promotion_is_subtree_scoped_and_reports_evictions() {
        let ring = StagingRing::new(6, 8);
        for ids in [&[1, 1][..], &[2][..], &[1, 2][..]] {
            let path = call(ids);
            let staged = path.clone();
            ring.stage_with(path, 3, move || draft(staged, 3));
        }
        let report = ring.promote(&call(&[1]), TriggerId("error-7".to_string()));
        assert_eq!(report.promoted.len(), 1);
        assert_eq!(report.promoted[0].call, call(&[1, 2]));
        assert_eq!(report.audit.staged_evicted, 1);
        assert_eq!(report.losses.len(), 1);
        assert_eq!(report.losses[0].call, call(&[1, 1]));
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn boundary_scope_crosses_logical_threads_but_not_engines() {
        let scope = CallPath::boundary(
            BoundaryId::from_bytes([3; 16]),
            ProcessEuid([4; 16]),
            EngineId(5),
        );
        assert!(scope.contains(&call(&[1, 2, 3])));
        let mut another_thread = call(&[9]);
        another_thread.logical_thread_id = 42;
        assert!(scope.contains(&another_thread));
        another_thread.engine_id = EngineId(7);
        assert!(!scope.contains(&another_thread));
    }

    #[test]
    fn normal_close_releases_only_matching_subtree() {
        let ring = StagingRing::new(10, 8);
        for ids in [&[1, 1][..], &[2][..]] {
            let path = call(ids);
            let staged = path.clone();
            ring.stage_with(path, 3, move || draft(staged, 3));
        }
        let report = ring.release(&call(&[1]));
        assert_eq!(report.released_records, 1);
        assert_eq!(ring.len(), 1);
    }

    #[test]
    fn bounded_eviction_history_degrades_to_explicit_loss() {
        let ring = StagingRing::new(1, 0);
        for id in 1..=2 {
            let path = call(&[1, id]);
            let staged = path.clone();
            ring.stage_with(path, 1, move || draft(staged, 1));
        }
        let report = ring.promote(&call(&[1]), TriggerId("manual".to_string()));
        assert_eq!(report.audit.staged_evicted, 1);
        assert_eq!(report.losses.len(), 1);
        assert_eq!(
            report.losses[0].reason,
            CaptureLossReason::EvictionHistoryOverflow
        );
    }
}
