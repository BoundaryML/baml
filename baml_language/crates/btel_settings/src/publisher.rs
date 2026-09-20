//! Recording configuration, allocation accounting and soft-size estimates.
use std::{num::NonZeroUsize, time::Duration};

use crate::{encoding, processor};
/// **Tune first.** Larger files amortize sealing/delivery overhead but retain more data
/// before publication. This soft target can be exceeded by a complete batch.
pub const TARGET_BYTES: NonZeroUsize = NonZeroUsize::new(1024 * 1024).unwrap();
/// **Tune first.** Time-based file sealing: shorter means fresher output and more small
/// files; longer means more batching. Independent of the processor cache-flush timer.
pub const FILE_FLUSH_INTERVAL_DURATION: Duration = Duration::from_secs(1);
/// **Tune first with processor/transport limits.** Currently the effective batch cap. Larger
/// batches increase preflight headroom and time between deadline checks.
pub const MAX_BATCH_CHUNKS: usize = 1;
/// **Sensitive capacity limit.** Raise only with an explicit memory allowance. Includes
/// retained sealed files; exhaustion is terminal. It bounds charged memory, not process RSS.
pub const MAX_RESIDENT_BYTES: NonZeroUsize = NonZeroUsize::new(64 * 1024 * 1024).unwrap();
#[derive(Clone, Copy, Debug)]
pub struct RecordingConfig {
    /// Soft serialized size; see [`TARGET_BYTES`] for tuning guidance.
    pub target_bytes: NonZeroUsize,
    /// Charged budget; see [`MAX_RESIDENT_BYTES`] before changing.
    pub max_resident_bytes: NonZeroUsize,
    /// Typed file-sealing duration; see [`FILE_FLUSH_INTERVAL_DURATION`].
    pub flush_interval_duration: Duration,
}
impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            target_bytes: TARGET_BYTES,
            max_resident_bytes: MAX_RESIDENT_BYTES,
            flush_interval_duration: FILE_FLUSH_INTERVAL_DURATION,
        }
    }
}
/// **Sensitive accounting.** Conservative per-item charge, not actual encoded bytes. Reducing
/// it changes admission accounting, not allocation or encoding cost.
pub const ITEM_CHARGE: usize = 4096;
/// **Sensitive accounting.** Must cover the derived metadata/conversion work before consuming
/// input. Re-prove this bound when record shapes change.
pub const CHUNK_HEADROOM_PER_RECORD: usize = 6 * ITEM_CHARGE;
/// **Sensitive accounting.** Covers baseline structures/allocation overhead outside per-record
/// charges. Lowering it does not make those structures smaller.
pub const BASE_CHARGE: usize = 64 * 1024;
/// **Sensitive accounting.** Additional retained-file bookkeeping charge; change from
/// ownership/allocation evidence.
pub const FILE_OVERHEAD: usize = 512;
/// **Derived.** Extra admission headroom for flushing every combining-cache slot; change the
/// cache setting rather than this relationship.
pub const BATCH_EXTRA_RECORDS: usize = processor::COMBINING_SLOTS;
/// **Measure first.** Larger growth reduces reallocations but retains more capacity and
/// increases old/new allocation overlap.
pub const BUFFER_GROWTH_FACTOR: usize = 2;
/// **Measure first on tiny recordings.** Lower values can reduce retained capacity but
/// increase growth frequency.
pub const MIN_BUFFER_CAPACITY: usize = 256;
/// **Sensitive accounting.** Derived objects charged for a thread announcement/completion;
/// review when conversion changes.
pub const THREAD_ITEMS: usize = 4;
/// **Sensitive accounting.** Base objects charged for a call-path definition; review
/// alongside metadata conversion.
pub const CALL_PATH_ITEMS: usize = 2;
/// **Sensitive accounting.** Additional objects charged when a call-path definition includes
/// its visible caller.
pub const VISIBLE_CALLER_ITEMS: usize = 1;
/// **Sensitive accounting.** Baseline charge for other converted records.
pub const OTHER_ITEMS: usize = 1;
/// **Size estimate.** Drives soft sealing, not encoding. Lowering this postpones publication
/// without making an event smaller.
pub const SPAN_ESTIMATE_BYTES: usize = 128;
/// **Size estimate.** Includes thread/clock definitions; keep aligned with representative
/// encoded output.
pub const THREAD_ESTIMATE_BYTES: usize = 512;
/// **Size estimate.** Charged to the soft-size target for each new aggregate contribution;
/// not an encoder allocation bound.
pub const AGGREGATE_ESTIMATE_BYTES: usize = 37;
/// **Size estimate.** Call-path and associated metadata contribution to the soft-size target.
pub const CALL_PATH_ESTIMATE_BYTES: usize = 192;
/// **Sensitive accounting.** Conservative hash-table load/spare-capacity factor; does not
/// control the map allocator.
pub const MAP_CHARGE_FACTOR: usize = 2;
/// **Sensitive accounting.** Control-byte estimate per capacity unit; review if the map
/// representation changes.
pub const MAP_CONTROL_BYTES: usize = 1;
/// **Measure first on bursty cardinality.** Shrinking sooner saves retained memory but can
/// cause allocation churn in the next window.
pub const MAP_SHRINK_THRESHOLD: usize = 8192;
/// **Measure first with shrink threshold.** Target retained capacity after a burst; actual
/// map capacity may be rounded or constrained by live entries.
pub const MAP_RETAINED_CAPACITY: usize = 4096;

#[inline]
pub fn encoding_capacity(current: usize, required: usize) -> usize {
    required
        .max(current.saturating_mul(BUFFER_GROWTH_FACTOR))
        .max(MIN_BUFFER_CAPACITY)
}
#[inline]
pub fn batch_record_charge(records: usize) -> usize {
    records
        .saturating_add(BATCH_EXTRA_RECORDS)
        .saturating_mul(CHUNK_HEADROOM_PER_RECORD)
}
/// Empty-publisher charge to admit a batch, before any input is consumed.
pub fn initial_batch_charge(records: usize) -> usize {
    BASE_CHARGE
        .saturating_add(batch_record_charge(records))
        .saturating_add(encoding_capacity(
            0,
            records.saturating_mul(encoding::MAX_EVENT_BYTES),
        ))
        .saturating_add(FILE_OVERHEAD)
}
const _: () = assert!(MAP_RETAINED_CAPACITY <= MAP_SHRINK_THRESHOLD);

impl RecordingConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.flush_interval_duration.is_zero()
            || std::time::Instant::now()
                .checked_add(self.flush_interval_duration)
                .is_none()
            || self.target_bytes.get() >= self.max_resident_bytes.get()
            || self.max_resident_bytes.get() <= BASE_CHARGE
        {
            return Err("invalid telemetry recording limits");
        }
        Ok(())
    }
    /// Reject a configuration that cannot admit even its first transport batch.
    /// This does not promise arbitrary retained sink data will fit the budget.
    pub fn validate_transport(
        &self,
        transport: &crate::transport::ChunkConfig,
    ) -> Result<(), &'static str> {
        self.validate()?;
        let records = transport
            .chunk_capacity
            .get()
            .checked_mul(
                processor::REQUESTED_BATCH_CHUNKS
                    .get()
                    .min(MAX_BATCH_CHUNKS),
            )
            .ok_or("telemetry batch size overflow")?;
        if records.saturating_mul(encoding::MAX_EVENT_BYTES) > encoding::MAX_BUFFER_BYTES {
            return Err("telemetry batch exceeds encoder capacity");
        }
        let charge = initial_batch_charge(records);
        if charge == usize::MAX || charge > self.max_resident_bytes.get() {
            return Err("telemetry memory budget cannot admit a transport batch");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_budget_is_validated_against_transport_batch() {
        let mut transport =
            crate::transport::ChunkConfig::for_producers(NonZeroUsize::new(2).unwrap());
        let mut recording = RecordingConfig::default();
        assert!(recording.validate_transport(&transport).is_ok());
        recording.target_bytes = NonZeroUsize::new(1).unwrap();
        let minimum = initial_batch_charge(transport.chunk_capacity.get() * MAX_BATCH_CHUNKS);
        recording.max_resident_bytes = NonZeroUsize::new(minimum - 1).unwrap();
        assert!(recording.validate().is_ok());
        assert!(recording.validate_transport(&transport).is_err());
        recording.max_resident_bytes = NonZeroUsize::new(minimum).unwrap();
        assert!(recording.validate_transport(&transport).is_ok());
        transport.chunk_capacity = NonZeroUsize::new(transport.chunk_capacity.get() * 2).unwrap();
        assert!(recording.validate_transport(&transport).is_err());
        transport.chunk_capacity = NonZeroUsize::new(usize::MAX).unwrap();
        recording.max_resident_bytes = NonZeroUsize::new(usize::MAX).unwrap();
        assert!(recording.validate_transport(&transport).is_err());
    }
}
