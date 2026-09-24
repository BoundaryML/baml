//! Size/time-triggered append/merge buffers and immutable file sealing for one sink.
use std::{fmt, num::NonZeroU64, time::Instant};

use btel_processor::{AggregateDelta, Publisher};
use btel_records::SpanRecord;
use btel_settings::{encoding, publisher as settings};
use btel_snapshot::Snapshot;
use btel_types::TelemetryId;
use prost::Message;
pub use settings::RecordingConfig;

use crate::{ConversionBuffer, proto};
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
    EncodingTooLarge,
    SequenceExhausted,
}
impl fmt::Display for RecordingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "telemetry recording: {self:?}")
    }
}
impl std::error::Error for RecordingError {}

/// A completed file has one owner and is moved to the recording's sole sink.
pub struct SealedFile {
    id: RecordingId,
    sequence: NonZeroU64,
    bytes: Vec<u8>,
}
impl SealedFile {
    pub fn recording_id(&self) -> RecordingId {
        self.id
    }
    pub fn sequence(&self) -> NonZeroU64 {
        self.sequence
    }
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

/// Recording publisher used with `Processor::with_publisher`. Callback execution
/// occurs after the chunk is recycled, so bounded sink delivery can wait for capacity.
/// Files move to one sink. Size and elapsed time are the
/// only automatic sealing triggers; explicit flush/shutdown also seal pending data.
/// No output is replayed after an error. Final epoch lifecycle is separate: this
/// implementation seals data on shutdown but does not claim RecordingEnd/finality.
pub struct RecordingPublisher<F, C = fn(Snapshot)> {
    id: RecordingId,
    config: RecordingConfig,
    source_snapshot_id: Option<[u8; 32]>,
    buffer: ConversionBuffer,
    receive: F,
    receive_snapshot: C,
    captures: btel_processor::CaptureProcessor,
    pending_captures: Vec<Snapshot>,
    next_sequence: Option<NonZeroU64>,
    deadline: Option<Instant>,
    pending_span_reservation: usize,
    span_credit: usize,
}
impl<F: FnMut(SealedFile)> RecordingPublisher<F> {
    /// Snapshots are captured and hashed, but released without persistence.
    /// Files contain real CAS references whose payloads may be unavailable.
    pub fn new(
        id: RecordingId,
        config: RecordingConfig,
        receive: F,
    ) -> Result<Self, RecordingError> {
        Self::with_snapshot_receiver(id, config, receive, drop::<Snapshot>)
    }
}
impl<F: FnMut(SealedFile), C: FnMut(Snapshot)> RecordingPublisher<F, C> {
    /// Both callbacks run after input chunk recycling. Snapshot ownership moves
    /// without copying; receivers must not wait for the VM to release resources.
    pub fn with_snapshot_receiver(
        id: RecordingId,
        config: RecordingConfig,
        receive: F,
        receive_snapshot: C,
    ) -> Result<Self, RecordingError> {
        config
            .validate()
            .map_err(|_| RecordingError::InvalidConfig)?;
        Ok(Self {
            id,
            config,
            source_snapshot_id: None,
            buffer: ConversionBuffer::default(),
            receive,
            receive_snapshot,
            captures: btel_processor::CaptureProcessor::default(),
            pending_captures: Vec::new(),
            next_sequence: NonZeroU64::new(1),
            deadline: None,
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

    // Reserve span storage once per chunk. The limit belongs to the encoder's
    // u32 length representation, not a resident-memory budget or sealing policy.
    fn reserve_encoding(&mut self, additional: usize) -> Result<(), RecordingError> {
        let needed = self
            .buffer
            .spans
            .len()
            .checked_add(additional)
            .filter(|&n| n <= encoding::MAX_BUFFER_BYTES)
            .ok_or(RecordingError::EncodingTooLarge)?;
        if needed > self.buffer.spans.capacity() {
            let target = settings::encoding_capacity(self.buffer.spans.capacity(), needed)
                .min(encoding::MAX_BUFFER_BYTES);
            self.buffer.spans.reserve(target);
        }
        Ok(())
    }
    #[cold]
    #[inline(never)]
    fn admit_spans(&mut self) -> Result<(), RecordingError> {
        // Delay allocation until the first span so a timing-only batch does not
        // allocate a worst-case span buffer. Reserve once for the whole batch.
        let records = self.pending_span_reservation.max(1);
        self.reserve_encoding(records.saturating_mul(encoding::MAX_EVENT_BYTES))?;
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
        if self.buffer.is_empty() {
            return Ok(());
        }
        let sequence = self
            .next_sequence
            .ok_or(RecordingError::SequenceExhausted)?;
        let ready = self.buffer.take();
        let file = proto::RecordingFile {
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
        self.reserve_encoding(length)?;
        let mut bytes = self.buffer.spans.finish();
        file.encode_raw(&mut bytes);
        let sealed = SealedFile {
            id: self.id,
            sequence,
            bytes,
        };
        drop(file); // Release converted metadata before potentially blocking delivery.
        self.next_sequence = sequence.get().checked_add(1).and_then(NonZeroU64::new);
        (self.receive)(sealed);
        Ok(())
    }
    fn service(&mut self, now: Instant, force: bool) -> Result<(), RecordingError> {
        // Never hold capture slots until the file seal threshold: the VM could
        // exhaust its snapshot pool before producing enough bytes to seal.
        // Drain drops the remaining owners if a receiver unwinds.
        for snapshot in self.pending_captures.drain(..) {
            (self.receive_snapshot)(snapshot);
        }
        if force
            || self.buffer.encoded_size_hint() >= self.config.target_bytes.get()
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
impl<F: FnMut(SealedFile), C: FnMut(Snapshot)> Publisher<Snapshot, Snapshot>
    for RecordingPublisher<F, C>
{
    fn aggregate(&mut self, delta: AggregateDelta) {
        self.buffer.aggregate(delta);
    }
    fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        if self.span_credit == 0 {
            Self::terminal(self.admit_spans());
        }
        self.span_credit -= 1;
        self.buffer.span(thread, record);
        if let Some(snapshot) = record.take_capture()
            && let Some(snapshot) = self.captures.retain(snapshot)
        {
            self.pending_captures.push(snapshot);
        }
    }
    fn before_batch(&mut self, records: usize) {
        self.clear_batch();
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
use std::{num::NonZeroUsize, sync::Arc, time::Duration};
