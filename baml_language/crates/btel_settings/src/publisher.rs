//! Recording size/time triggers and buffer reuse settings.
use std::{num::NonZeroUsize, time::Duration};

use crate::{encoding, processor};
/// **Tune first.** Larger files amortize sealing/delivery overhead but retain more data
/// before publication. This soft target can be exceeded by a complete batch.
pub const TARGET_BYTES: NonZeroUsize = NonZeroUsize::new(1024 * 1024).unwrap();
/// **Tune first.** Time-based file sealing: shorter means fresher output and more small
/// files; longer means more batching. Independent of the processor cache-flush timer.
pub const FILE_FLUSH_INTERVAL_DURATION: Duration = Duration::from_secs(1);
/// **Tune first with processor/transport limits.** Currently the effective batch cap. Larger
/// batches increase possible size overshoot and time between deadline checks.
pub const MAX_BATCH_CHUNKS: usize = 1;
#[derive(Clone, Copy, Debug)]
pub struct RecordingConfig {
    /// Soft serialized size; see [`TARGET_BYTES`] for tuning guidance.
    pub target_bytes: NonZeroUsize,
    /// Target age from the first processed batch; see [`FILE_FLUSH_INTERVAL_DURATION`].
    /// Checked between batches. Sink backpressure can delay sealing/delivery.
    pub flush_interval_duration: Duration,
}
impl Default for RecordingConfig {
    fn default() -> Self {
        Self {
            target_bytes: TARGET_BYTES,
            flush_interval_duration: FILE_FLUSH_INTERVAL_DURATION,
        }
    }
}
/// **Format overhead.** Upper bound for the file header, sequence and outer
/// protobuf containers. Record bodies use their actual encoded lengths. This
/// adds at most 128 bytes of conservatism per file, not a charge per record.
pub const FILE_ENVELOPE_BYTES: usize = 128;
/// **Measure first.** Larger growth reduces reallocations but retains more capacity and
/// increases old/new allocation overlap.
pub const BUFFER_GROWTH_FACTOR: usize = 2;
/// **Measure first on tiny recordings.** Lower values can reduce retained capacity but
/// increase growth frequency.
pub const MIN_BUFFER_CAPACITY: usize = 256;
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
const _: () = assert!(MAP_RETAINED_CAPACITY <= MAP_SHRINK_THRESHOLD);

impl RecordingConfig {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.flush_interval_duration.is_zero() || u32::try_from(self.target_bytes.get()).is_err()
        {
            return Err("invalid telemetry recording limits");
        }
        Ok(())
    }
    /// Check the encoder's length representation against a transport batch.
    /// This is a format limit, not an accounting budget.
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
        records
            .checked_mul(encoding::MAX_EVENT_BYTES)
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or("telemetry batch exceeds encoder capacity")?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recording_validates_duration_and_encoder_limits() {
        let mut transport =
            crate::transport::ChunkConfig::for_producers(NonZeroUsize::new(2).unwrap());
        let mut recording = RecordingConfig::default();
        assert!(recording.validate_transport(&transport).is_ok());
        recording.flush_interval_duration = Duration::ZERO;
        assert!(recording.validate().is_err());
        recording = RecordingConfig::default();
        transport.chunk_capacity = NonZeroUsize::new(usize::MAX).unwrap();
        assert!(recording.validate_transport(&transport).is_err());
    }

    #[test]
    fn recording_target_accepts_u32_limit() {
        let mut recording = RecordingConfig {
            target_bytes: NonZeroUsize::new(encoding::MAX_BUFFER_BYTES).unwrap(),
            ..RecordingConfig::default()
        };
        assert!(recording.validate().is_ok());
        if let Some(above_limit) = encoding::MAX_BUFFER_BYTES.checked_add(1) {
            recording.target_bytes = NonZeroUsize::new(above_limit).unwrap();
            assert!(recording.validate().is_err());
        }
    }

    #[test]
    fn transport_checks_encoding_multiplication_and_u32_limit() {
        let recording = RecordingConfig::default();
        let mut transport =
            crate::transport::ChunkConfig::for_producers(NonZeroUsize::new(1).unwrap());
        let max_records = encoding::MAX_BUFFER_BYTES / encoding::MAX_EVENT_BYTES;
        transport.chunk_capacity = NonZeroUsize::new(max_records).unwrap();
        assert!(recording.validate_transport(&transport).is_ok());
        for records in [
            max_records + 1,
            usize::MAX / encoding::MAX_EVENT_BYTES + 1,
            usize::MAX,
        ] {
            transport.chunk_capacity = NonZeroUsize::new(records).unwrap();
            assert!(recording.validate_transport(&transport).is_err());
        }
    }
}
