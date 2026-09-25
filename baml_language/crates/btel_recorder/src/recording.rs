//! Size/time-triggered append/merge buffers and immutable file sealing for one sink.
use std::{fmt, num::NonZeroU64};

use btel_processor::{AggregateDelta, Publisher};
use btel_records::SpanRecord;
use btel_settings::{encoding, publisher as settings};
use btel_snapshot::Snapshot;
use btel_types::TelemetryId;
use prost::Message;
pub use settings::RecordingConfig;
use web_time::Instant;

use crate::{ConversionBuffer, proto};

/// `RecordingFile.errors`, written from pre-encoded bytes after the rest.
const ERRORS_FIELD: u32 = 8;
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
    /// Input arrived after `RecordingBuilder::end_recording`.
    InputAfterEnd,
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
    metadata: std::ops::Range<usize>,
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
    pub fn retained_bytes(&self) -> usize {
        std::mem::size_of::<Self>().saturating_add(self.bytes.capacity())
    }

    /// Encoded definitions and clock states introduced by this file, without events.
    /// Empty unless the builder enabled metadata replay.
    pub fn metadata_bytes(&self) -> &[u8] {
        &self.bytes[self.metadata.clone()]
    }

    /// Prepend older metadata from this recording before its newer definitions.
    /// Call before preparing uploads: the resulting bytes must remain immutable.
    pub fn prepend_metadata<'a>(&mut self, segments: impl Iterator<Item = &'a [u8]> + Clone) {
        let prefix_len: usize = segments.clone().map(<[u8]>::len).sum();
        if prefix_len == 0 {
            return;
        }
        let mut bytes = Vec::with_capacity(prefix_len + self.bytes.len());
        for segment in segments {
            bytes.extend_from_slice(segment);
        }
        bytes.extend_from_slice(&self.bytes);
        self.bytes = bytes;
        self.metadata.start += prefix_len;
        self.metadata.end += prefix_len;
    }
}

/// Sink-independent encoding, capture deduplication and recording sequencing.
/// Drain snapshots after input chunk recycling and before delivering sealed files.
/// Size/time sealing and `flush_recording` never claim `RecordingEnd`; only
/// `end_recording` does, once no further input can arrive and every observed
/// clock epoch is final.
/// Encoding errors are terminal; discard this builder rather than retrying.
pub struct RecordingBuilder {
    id: RecordingId,
    config: RecordingConfig,
    source_snapshot_id: Option<[u8; 32]>,
    buffer: ConversionBuffer,
    captures: btel_processor::CaptureProcessor,
    pending_captures: Vec<Snapshot>,
    next_sequence: Option<NonZeroU64>,
    deadline: Option<Instant>,
    pending_span_reservation: usize,
    span_credit: usize,
    metadata_replay: bool,
    ended: bool,
}
impl RecordingBuilder {
    pub fn new(id: RecordingId, config: RecordingConfig) -> Result<Self, RecordingError> {
        config
            .validate()
            .map_err(|_| RecordingError::InvalidConfig)?;
        Instant::now()
            .checked_add(config.flush_interval_duration)
            .ok_or(RecordingError::InvalidConfig)?;
        Ok(Self {
            id,
            config,
            source_snapshot_id: None,
            buffer: ConversionBuffer::default(),
            captures: btel_processor::CaptureProcessor::default(),
            pending_captures: Vec::new(),
            next_sequence: NonZeroU64::new(1),
            deadline: None,
            pending_span_reservation: 0,
            span_credit: 0,
            metadata_replay: false,
            ended: false,
        })
    }
    /// Expose essential metadata separately for a bounded best-effort cloud journal.
    #[must_use]
    pub fn with_metadata_replay(mut self) -> Self {
        self.metadata_replay = true;
        self
    }

    /// Optional source fingerprint, fixed for this recording before processing.
    #[must_use]
    pub fn with_source_snapshot(mut self, source_snapshot_id: Option<[u8; 32]>) -> Self {
        self.source_snapshot_id = source_snapshot_id;
        self
    }
    /// Owned metadata for functions known before execution. Each referenced
    /// function's definition is published once per recording; other functions
    /// keep `MetadataUnavailable` placeholders.
    #[must_use]
    pub fn with_function_metadata(
        mut self,
        functions: std::sync::Arc<btel_types::FunctionMetadataTable>,
    ) -> Self {
        self.buffer.functions.set_table(functions);
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
            .filter(|&n| u32::try_from(n).is_ok())
            .ok_or(RecordingError::EncodingTooLarge)?;
        if needed > self.buffer.spans.capacity() {
            let target = u32::try_from(settings::encoding_capacity(
                self.buffer.spans.capacity(),
                needed,
            ))
            .unwrap_or(u32::MAX) as usize;
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
        let bytes = records
            .checked_mul(encoding::MAX_EVENT_BYTES)
            .ok_or(RecordingError::EncodingTooLarge)?;
        self.reserve_encoding(bytes)?;
        self.span_credit = records;
        self.pending_span_reservation = 0;
        Ok(())
    }
    fn clear_batch(&mut self) {
        self.span_credit = 0;
        self.pending_span_reservation = 0;
    }
    fn seal(&mut self, end: bool) -> Result<Option<SealedFile>, RecordingError> {
        self.clear_batch();
        if self.ended {
            return if self.buffer.is_empty() {
                Ok(None)
            } else {
                Err(RecordingError::InputAfterEnd)
            };
        }
        // The terminal file exists even when nothing else is pending.
        if !end && self.buffer.is_empty() {
            return Ok(None);
        }
        let sequence = self
            .next_sequence
            .ok_or(RecordingError::SequenceExhausted)?;
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
            end: end.then_some(proto::RecordingEnd {}),
            // Appended below from its pre-encoded body.
            errors: None,
        };
        // Absent without exceptions, so error-free files keep their bytes.
        let errors_len = if ready.errors.is_empty() {
            0
        } else {
            prost::encoding::key_len(ERRORS_FIELD)
                + prost::encoding::encoded_len_varint(ready.errors.len() as u64)
                + ready.errors.len()
        };
        let length = file.encoded_len() + errors_len;
        self.reserve_encoding(length)?;
        let mut bytes = self.buffer.spans.finish();
        let metadata_start = bytes.len();
        if self.metadata_replay {
            let metadata = proto::RecordingFile {
                definitions: file
                    .definitions
                    .take()
                    .filter(|value| value.encoded_len() != 0),
                clock_states: file
                    .clock_states
                    .take()
                    .filter(|value| value.encoded_len() != 0),
                ..Default::default()
            };
            metadata.encode_raw(&mut bytes);
        }
        let metadata = metadata_start..bytes.len();
        file.encode_raw(&mut bytes);
        if errors_len != 0 {
            prost::encoding::encode_key(
                ERRORS_FIELD,
                prost::encoding::WireType::LengthDelimited,
                &mut bytes,
            );
            prost::encoding::encode_varint(ready.errors.len() as u64, &mut bytes);
            bytes.extend_from_slice(&ready.errors);
        }
        let sealed = SealedFile {
            id: self.id,
            sequence,
            bytes,
            metadata,
        };
        drop(file); // Release converted metadata before potentially blocking delivery.
        self.next_sequence = sequence.get().checked_add(1).and_then(NonZeroU64::new);
        Ok(Some(sealed))
    }
    /// Drain every batch after input chunk recycling, regardless of file sealing.
    /// Holding capture slots until sealing can exhaust the VM's snapshot pool.
    /// Dropping this iterator releases any remaining owners, including on unwind.
    pub fn take_snapshots(&mut self) -> impl Iterator<Item = Snapshot> + '_ {
        self.pending_captures.drain(..)
    }
    fn service(&mut self, now: Instant, force: bool) -> Result<Option<SealedFile>, RecordingError> {
        if force
            || self.buffer.encoded_size_hint() >= self.config.target_bytes.get()
            || self.deadline.is_some_and(|deadline| now >= deadline)
        {
            // Final clock states ride on files sealed anyway.
            if !self.buffer.is_empty() {
                self.buffer.observe_unsettled_clocks(false);
            }
            let file = self.seal(false)?;
            self.deadline = None;
            return Ok(file);
        }
        Ok(None)
    }
    /// Seal consumed input now. Missing references do not block emission;
    /// readers resolve them later. Not recording completion: subsequent input
    /// continues the sequence. `None` when nothing is pending.
    /// Callers using `Self::span` must drain `Self::take_snapshots` first, outside
    /// borrowed input chunks. Callers using `Self::span_reference` may seal
    /// during a chunk, but must defer sink delivery until the chunk is recycled.
    pub fn flush_recording(&mut self) -> Result<Option<SealedFile>, RecordingError> {
        self.service(Instant::now(), true)
    }

    /// Call once input is exhausted. Seals pending input with `RecordingEnd`
    /// only if every clock epoch this recording observed has settled; the file
    /// then exists even when nothing else is pending, so an empty recording or
    /// one whose last data was already sealed still ends. Otherwise it seals
    /// pending input and each unsettled epoch's latest, non-final status
    /// without an end, leaving the recording unsealed; a later call may end it.
    /// After the end, repeated calls return `None` and any new input is
    /// `InputAfterEnd`. Same snapshot and delivery ordering rules as
    /// `Self::flush_recording`.
    pub fn end_recording(&mut self) -> Result<Option<SealedFile>, RecordingError> {
        if self.ended {
            return self.seal(false);
        }
        let settled = self.buffer.observe_unsettled_clocks(true);
        let file = self.seal(settled)?;
        self.ended = settled;
        self.deadline = None;
        Ok(file)
    }
    pub fn aggregate(&mut self, delta: AggregateDelta) {
        self.buffer.aggregate(delta);
    }
    /// Encode references without retaining captures. The caller owns capture
    /// admission and must keep accepted owners until its sink consumes them.
    pub fn span_reference(&mut self, thread: TelemetryId, record: &SpanRecord<Snapshot, Snapshot>) {
        if self.span_credit == 0 {
            terminal(self.admit_spans());
        }
        self.span_credit -= 1;
        self.buffer.span(thread, record);
    }
    pub fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        self.span_reference(thread, record);
        if let Some(snapshot) = record.take_capture()
            && let Some(snapshot) = self.captures.retain(snapshot)
        {
            self.pending_captures.push(snapshot);
        }
    }
    /// Begin a non-sliding flush interval at the first event in the open file.
    pub fn start_deadline(&mut self, now: Instant) {
        self.deadline
            .get_or_insert(now + self.config.flush_interval_duration);
    }
    pub fn encoded_size_hint(&self) -> usize {
        self.buffer.encoded_size_hint()
    }
    /// Release all open-file allocations and capture owners after terminal failure.
    pub fn discard_pending(&mut self) {
        self.buffer = ConversionBuffer::default();
        self.pending_captures.clear();
        self.deadline = None;
        self.clear_batch();
    }
    pub fn before_batch(&mut self, records: usize) {
        self.clear_batch();
        self.pending_span_reservation = records;
    }
    pub fn before_span_chunk(&mut self, records: usize) {
        self.span_credit = 0;
        self.pending_span_reservation = self.pending_span_reservation.min(records);
    }
    pub fn after_batch(&mut self, records: usize) -> Result<Option<SealedFile>, RecordingError> {
        self.clear_batch();
        let now = Instant::now();
        if records != 0 {
            self.start_deadline(now);
        }
        self.service(now, false)
    }
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }
    /// Check the open file's size and fixed deadline without restarting its timer.
    pub fn flush_if_due(&mut self, now: Instant) -> Result<Option<SealedFile>, RecordingError> {
        self.clear_batch();
        self.service(now, false)
    }
}

fn terminal<T>(result: Result<T, RecordingError>) -> T {
    result.unwrap_or_else(|error| std::panic::panic_any(error))
}

/// Callback adapter for `Processor::with_publisher`. Callbacks execute after
/// chunk recycling and can apply bounded delivery backpressure.
pub struct RecordingPublisher<F, C = fn(Snapshot)> {
    builder: RecordingBuilder,
    receive: F,
    receive_snapshot: C,
}
impl<F: FnMut(SealedFile)> RecordingPublisher<F> {
    /// Capture references are encoded, but their payloads are not persisted.
    pub fn new(
        id: RecordingId,
        config: RecordingConfig,
        receive: F,
    ) -> Result<Self, RecordingError> {
        Self::with_snapshot_receiver(id, config, receive, drop::<Snapshot>)
    }
}
impl<F: FnMut(SealedFile), C: FnMut(Snapshot)> RecordingPublisher<F, C> {
    pub fn with_snapshot_receiver(
        id: RecordingId,
        config: RecordingConfig,
        receive: F,
        receive_snapshot: C,
    ) -> Result<Self, RecordingError> {
        Ok(Self {
            builder: RecordingBuilder::new(id, config)?,
            receive,
            receive_snapshot,
        })
    }
    #[must_use]
    pub fn with_source_snapshot(mut self, source_snapshot_id: Option<[u8; 32]>) -> Self {
        self.builder = self.builder.with_source_snapshot(source_snapshot_id);
        self
    }

    /// Owned function definitions copied before execution, shared with the recording builder.
    #[must_use]
    pub fn with_function_metadata(
        mut self,
        functions: std::sync::Arc<btel_types::FunctionMetadataTable>,
    ) -> Self {
        self.builder = self.builder.with_function_metadata(functions);
        self
    }
    fn snapshots(&mut self) {
        for snapshot in self.builder.take_snapshots() {
            (self.receive_snapshot)(snapshot);
        }
    }
    fn deliver(&mut self, file: Option<SealedFile>) {
        if let Some(file) = file {
            (self.receive)(file);
        }
    }
    /// Forced flush; the recording continues.
    pub fn flush_recording(&mut self) -> Result<(), RecordingError> {
        self.snapshots();
        let file = self.builder.flush_recording()?;
        self.deliver(file);
        Ok(())
    }
    /// Deliver the terminal file, or the unsealed remainder while clock epochs
    /// are unsettled. See `RecordingBuilder::end_recording`.
    pub fn end_recording(&mut self) -> Result<(), RecordingError> {
        self.snapshots();
        let file = self.builder.end_recording()?;
        self.deliver(file);
        Ok(())
    }
}
impl<F: FnMut(SealedFile), C: FnMut(Snapshot)> Publisher<Snapshot, Snapshot>
    for RecordingPublisher<F, C>
{
    const ERROR_EVIDENCE: bool = true;

    fn aggregate(&mut self, delta: AggregateDelta) {
        self.builder.aggregate(delta);
    }
    fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        self.builder.span(thread, record);
    }
    fn before_batch(&mut self, records: usize) {
        self.builder.before_batch(records);
    }
    fn before_span_chunk(&mut self, records: usize) {
        self.builder.before_span_chunk(records);
    }
    fn after_batch(&mut self, records: usize) {
        self.snapshots();
        let file = terminal(self.builder.after_batch(records));
        self.deliver(file);
    }
    fn max_chunks_per_batch(&self) -> usize {
        settings::MAX_BATCH_CHUNKS
    }
    fn manages_flush_deadline(&self) -> bool {
        true
    }
    fn deadline(&self) -> Option<Instant> {
        self.builder.deadline()
    }
    fn flush(&mut self) {
        terminal(self.flush_recording());
    }
    /// Input is exhausted: the recording ends once its clock epochs settled.
    fn finish(&mut self) {
        terminal(self.end_recording());
    }
}

#[cfg(test)]
mod tests;
#[cfg(test)]
use std::{num::NonZeroUsize, sync::Arc, time::Duration};
