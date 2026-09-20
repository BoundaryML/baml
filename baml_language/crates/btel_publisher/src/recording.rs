//! Bounded append/merge buffers and immutable file sealing, shared by destinations.
use std::{
    fmt,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Instant,
};

use btel_processor::{AggregateDelta, Publisher};
use btel_records::{CaptureDeferred, SpanRecord};
use btel_settings::{encoding, publisher as settings};
use btel_types::TelemetryId;
use prost::Message;
pub use settings::RecordingConfig;
use settings::{BASE_CHARGE, CHUNK_HEADROOM_PER_RECORD, FILE_OVERHEAD, ITEM_CHARGE};

use crate::{ConversionBuffer, PendingMessages, proto};
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RecordingId([u8; 16]);
impl RecordingId {
    pub fn generate() -> Self {
        Self(*uuid::Uuid::new_v4().as_bytes())
    }
    pub fn from_bytes(bytes: [u8; 16]) -> Option<Self> {
        (bytes != [0; 16]).then_some(Self(bytes))
    }
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RecordingError {
    InvalidConfig,
    BudgetExceeded,
    SequenceExhausted,
}
impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "telemetry recording: {self:?}")
    }
}
impl std::error::Error for RecordingError {}

struct RetainedBytes {
    used: AtomicUsize,
}
struct FileInner {
    id: RecordingId,
    sequence: NonZeroU64,
    bytes: Vec<u8>,
    charge: usize,
    retained: Arc<RetainedBytes>,
}
impl Drop for FileInner {
    fn drop(&mut self) {
        self.retained.used.fetch_sub(self.charge, Ordering::AcqRel);
    }
}
/// Clone shares the allocation. Both destinations and retries see identical
/// bytes and sequence; no reencoding, payload clone, or contribution reassignment.
#[derive(Clone)]
pub struct SealedFile(Arc<FileInner>);
impl SealedFile {
    pub fn recording_id(&self) -> RecordingId {
        self.0.id
    }
    pub fn sequence(&self) -> NonZeroU64 {
        self.0.sequence
    }
    pub fn bytes(&self) -> &[u8] {
        &self.0.bytes
    }
}

/// Recording publisher used with `Processor::with_publisher`. Callback execution
/// occurs after the chunk is recycled. Keep delivery outside this worker; the
/// callback may hand the immutable file to both sinks, which share its byte charge.
/// Budget errors terminate processing through its existing panic guard.
/// No output is replayed after an error. Final epoch lifecycle is separate: this
/// implementation seals data on shutdown but does not claim RecordingEnd/finality.
pub struct RecordingPublisher<F> {
    id: RecordingId,
    config: RecordingConfig,
    source_snapshot_id: Option<[u8; 32]>,
    buffer: ConversionBuffer,
    retained: Arc<RetainedBytes>,
    receive: F,
    next_sequence: Option<NonZeroU64>,
    deadline: Option<Instant>,
    conversion_charge: usize,
    pending_span_reservation: usize,
    span_credit: usize,
}
impl<F: FnMut(SealedFile)> RecordingPublisher<F> {
    pub fn new(
        id: RecordingId,
        config: RecordingConfig,
        receive: F,
    ) -> Result<Self, RecordingError> {
        config
            .validate()
            .map_err(|_| RecordingError::InvalidConfig)?;
        Ok(Self {
            id,
            config,
            source_snapshot_id: None,
            buffer: ConversionBuffer::default(),
            retained: Arc::new(RetainedBytes {
                used: AtomicUsize::new(0),
            }),
            receive,
            next_sequence: NonZeroU64::new(1),
            deadline: None,
            conversion_charge: 0,
            pending_span_reservation: 0,
            span_credit: 0,
        })
    }
    /// Optional source fingerprint, fixed for this recording before processing.
    #[must_use]
    pub fn with_source_snapshot(mut self, source_snapshot_id: Option<[u8; 32]>) -> Self {
        self.source_snapshot_id = source_snapshot_id;
        self
    }

    fn used(&self) -> usize {
        BASE_CHARGE
            .saturating_add(self.conversion_charge)
            .saturating_add(self.span_credit.saturating_mul(CHUNK_HEADROOM_PER_RECORD))
            .saturating_add(self.buffer.aggregates.allocation_charge())
            .saturating_add(self.buffer.spans.capacity())
            .saturating_add(self.retained.used.load(Ordering::Acquire))
    }
    fn check(&self, extra: usize) -> Result<(), RecordingError> {
        if self.used().saturating_add(extra) > self.config.max_resident_bytes.get() {
            Err(RecordingError::BudgetExceeded)
        } else {
            Ok(())
        }
    }
    // Charge the old and new allocations during growth, before any record is
    // consumed. Exact reservation avoids Vec's implicit, uncharged doubling.
    fn reserve_encoding(
        &mut self,
        additional: usize,
        headroom: usize,
    ) -> Result<(), RecordingError> {
        let needed = self.buffer.spans.len().saturating_add(additional);
        if needed > encoding::MAX_BUFFER_BYTES {
            return Err(RecordingError::BudgetExceeded);
        }
        if needed > self.buffer.spans.capacity() {
            let target = settings::encoding_capacity(self.buffer.spans.capacity(), needed)
                .min(encoding::MAX_BUFFER_BYTES);
            self.check(
                target
                    .saturating_add(headroom)
                    .saturating_add(FILE_OVERHEAD),
            )?;
            self.buffer.spans.reserve(target);
            self.check(headroom.saturating_add(FILE_OVERHEAD))?;
        }
        Ok(())
    }
    fn batch_charge(&self, records: usize) -> usize {
        let record_charge = settings::batch_record_charge(records);
        let needed = self
            .buffer
            .spans
            .len()
            .saturating_add(records.saturating_mul(encoding::MAX_EVENT_BYTES));
        let growth = if needed > self.buffer.spans.capacity() {
            settings::encoding_capacity(self.buffer.spans.capacity(), needed)
        } else {
            0
        };
        record_charge
            .saturating_add(growth)
            .saturating_add(FILE_OVERHEAD)
    }
    #[cold]
    #[inline(never)]
    fn admit_spans(&mut self) -> Result<(), RecordingError> {
        // Delay allocation until the first span so a timing-only batch does not
        // allocate a worst-case span buffer. Reserve once for the whole batch.
        let records = self.pending_span_reservation.max(1);
        let charge = records.saturating_mul(CHUNK_HEADROOM_PER_RECORD);
        self.check(charge)?;
        self.reserve_encoding(records.saturating_mul(encoding::MAX_EVENT_BYTES), charge)?;
        self.span_credit = records;
        self.pending_span_reservation = 0;
        Ok(())
    }
    fn clear_batch(&mut self) {
        self.span_credit = 0;
        self.pending_span_reservation = 0;
    }
    fn seal(&mut self) -> Result<(), RecordingError> {
        self.clear_batch();
        if self.buffer.estimate == 0 {
            return Ok(());
        }
        let sequence = self
            .next_sequence
            .ok_or(RecordingError::SequenceExhausted)?;
        let estimate = self.buffer.estimate;
        let ready = self.buffer.take();
        let mut file = proto::RecordingFile {
            header: Some(proto::RecordingHeader {
                format_major: encoding::FORMAT_MAJOR,
                format_minor: encoding::FORMAT_MINOR,
                recording_id: self.id.0.to_vec(),
                source_snapshot_id: self.source_snapshot_id.map(|id| id.to_vec()),
            }),
            sequence: sequence.get(),
            definitions: Some(ready.definitions),
            aggregates: Some(ready.aggregates),
            spans: self.buffer.spans.is_empty().then(proto::SpanBatch::default),
            clock_states: Some(ready.clock_states),
            end: None,
        };
        let length = file.encoded_len();
        // Span bytes already occupy their final allocation. Admission covers
        // cold-message encoding and any allocation growth before closing the
        // open messages, so a failed seal can be retried without losing input.
        if let Err(error) = self
            .check(FILE_OVERHEAD)
            .and_then(|()| self.reserve_encoding(length, 0))
        {
            self.buffer.pending = PendingMessages {
                definitions: file.definitions.take().unwrap(),
                aggregates: file.aggregates.take().unwrap(),
                clock_states: file.clock_states.take().unwrap(),
            };
            self.buffer.estimate = estimate;
            return Err(error);
        }
        let mut bytes = self.buffer.spans.finish();
        file.encode_raw(&mut bytes);
        let charge = bytes.capacity().saturating_add(FILE_OVERHEAD);
        self.retained.used.fetch_add(charge, Ordering::AcqRel);
        let sealed = SealedFile(Arc::new(FileInner {
            id: self.id,
            sequence,
            bytes,
            charge,
            retained: Arc::clone(&self.retained),
        }));
        drop(file);
        self.conversion_charge = 0;
        self.next_sequence = sequence.get().checked_add(1).and_then(NonZeroU64::new);
        (self.receive)(sealed);
        Ok(())
    }
    fn service(&mut self, now: Instant, force: bool) -> Result<(), RecordingError> {
        if force
            || self.buffer.estimate >= self.config.target_bytes.get()
            || self.deadline.is_some_and(|deadline| now >= deadline)
        {
            self.seal()?;
            self.deadline = None;
        }
        Ok(())
    }
    /// Seal consumed input. Missing references do not block emission; readers
    /// resolve them later. Final epoch status/RecordingEnd integration is separate.
    pub fn finish_recording(&mut self) -> Result<(), RecordingError> {
        self.service(Instant::now(), true)
    }
    fn terminal(result: Result<(), RecordingError>) {
        if let Err(error) = result {
            std::panic::panic_any(error);
        }
    }
}
impl<F: FnMut(SealedFile)> Publisher<CaptureDeferred, CaptureDeferred> for RecordingPublisher<F> {
    fn aggregate(&mut self, delta: AggregateDelta) {
        Self::terminal(self.check(ITEM_CHARGE));
        let before = self.buffer.estimate;
        self.buffer.aggregate(delta);
        if self.buffer.estimate != before {
            self.conversion_charge = self.conversion_charge.saturating_add(ITEM_CHARGE);
        }
    }
    fn span(&mut self, thread: TelemetryId, record: &SpanRecord<CaptureDeferred, CaptureDeferred>) {
        if self.span_credit == 0 {
            Self::terminal(self.admit_spans());
        }
        self.span_credit -= 1;
        self.buffer.span(thread, record);
        let items = match record {
            SpanRecord::ThreadSpanAnnouncement { .. } | SpanRecord::ThreadSpanCompletion { .. } => {
                settings::THREAD_ITEMS
            }
            SpanRecord::CallPathDefined { visible_caller, .. } => {
                settings::CALL_PATH_ITEMS
                    + usize::from(visible_caller.is_some()) * settings::VISIBLE_CALLER_ITEMS
            }
            _ => settings::OTHER_ITEMS,
        };
        self.conversion_charge = self.conversion_charge.saturating_add(items * ITEM_CHARGE);
    }
    fn before_batch(&mut self, records: usize) {
        self.clear_batch();
        if self.check(self.batch_charge(records)).is_err() {
            Self::terminal(self.service(Instant::now(), true));
            if self.check(self.batch_charge(records)).is_err() {
                self.buffer.aggregates.release_empty_capacity();
            }
        }
        Self::terminal(self.check(self.batch_charge(records)));
        self.pending_span_reservation = records;
    }
    fn before_span_chunk(&mut self, records: usize) {
        self.span_credit = 0;
        self.pending_span_reservation = self.pending_span_reservation.min(records);
    }
    fn after_batch(&mut self, records: usize) {
        self.clear_batch();
        let now = Instant::now();
        if records != 0 {
            self.deadline
                .get_or_insert(now + self.config.flush_interval_duration);
        }
        Self::terminal(self.service(now, false));
    }
    fn max_chunks_per_batch(&self) -> usize {
        settings::MAX_BATCH_CHUNKS
    }
    fn manages_flush_deadline(&self) -> bool {
        true
    }
    fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    fn flush(&mut self) {
        Self::terminal(self.service(Instant::now(), true));
    }
    fn finish(&mut self) {
        Self::terminal(self.finish_recording());
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
use std::{num::NonZeroUsize, time::Duration};
