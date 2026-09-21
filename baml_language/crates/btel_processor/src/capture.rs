//! Whole-snapshot combining on the processor worker. Hashes are already frozen
//! by VM capture; this stage never traverses payloads or claims persistence.
use btel_snapshot::{Snapshot, SnapshotId};
use rustc_hash::FxHashSet;

/// Bounded recent-ID cache. Clearing at capacity permits duplicate publication,
/// not lost data; downstream CAS may deduplicate those repeats again.
#[derive(Default)]
pub struct CaptureProcessor {
    seen: FxHashSet<SnapshotId>,
}
impl CaptureProcessor {
    /// Move a first-seen owner downstream; recycle a recent duplicate immediately.
    /// An ID here means offered to the receiver, never durably delivered.
    pub fn retain(&mut self, snapshot: Snapshot) -> Option<Snapshot> {
        if self.seen.contains(&snapshot.id()) {
            return None;
        }
        if self.seen.len() == btel_settings::snapshot::RECENT_CAPTURE_IDS {
            self.seen.clear();
        }
        self.seen.insert(snapshot.id());
        Some(snapshot)
    }
}

#[cfg(test)]
mod tests {
    use btel_snapshot::{Limits, SnapshotPool, SnapshotValue};

    use super::*;

    #[test]
    fn combining_is_bounded_and_evicted_ids_can_be_offered_again() {
        let pool = SnapshotPool::new(1, Limits::default());
        let snapshot = |n| {
            pool.try_acquire()
                .unwrap()
                .finish_value(SnapshotValue::Int(n))
        };
        let mut processor = CaptureProcessor::default();
        for n in 0..btel_settings::snapshot::RECENT_CAPTURE_IDS {
            drop(
                processor
                    .retain(snapshot(i64::try_from(n).unwrap()))
                    .unwrap(),
            );
        }
        assert!(processor.retain(snapshot(0)).is_none());
        drop(processor.retain(snapshot(-1)).unwrap());
        assert_eq!(processor.seen.len(), 1);
        drop(processor.retain(snapshot(0)).unwrap());
        assert_eq!(pool.stats().in_use, 0);
        assert_eq!(pool.stats().allocation_misses, 1);
    }
}
