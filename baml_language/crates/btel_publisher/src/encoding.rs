//! Span messages written once into the eventual `RecordingFile` allocation.
//!
//! Containers reserve five bytes for their protobuf length and backpatch by
//! index, so Vec relocation is safe. Non-minimal varints preserve the protobuf
//! schema; no whole-buffer shift or nested sizing pass is required. Current
//! event bodies contain only bounded scalars, so their lengths fit one byte.
use std::mem::MaybeUninit;

use btel_types::TelemetryId;
use prost::{Message, encoding::uint64};

use crate::proto::span_event::Event;

/// Includes a new `SpanBatch`, `ThreadSection`, selector and the largest current
/// scalar-only event. Value snapshots are not part of this wire vocabulary.
pub(crate) const MAX_EVENT_BYTES: usize = 128;
pub(crate) const MAX_BUFFER_BYTES: usize = u32::MAX as usize;
const LENGTH_BYTES: usize = 5;

#[derive(Default)]
pub(crate) struct EncodedSpans {
    bytes: Vec<u8>,
    thread: Option<TelemetryId>,
    section_length: usize,
}
impl EncodedSpans {
    pub(crate) fn len(&self) -> usize {
        self.bytes.len()
    }
    pub(crate) fn capacity(&self) -> usize {
        self.bytes.capacity()
    }
    pub(crate) fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
    pub(crate) fn reserve(&mut self, capacity: usize) {
        if capacity > self.bytes.capacity() {
            self.bytes.reserve_exact(capacity - self.bytes.len());
        }
    }

    #[inline]
    pub(crate) fn event(&mut self, thread: TelemetryId, event: Event) {
        if self.thread != Some(thread) {
            self.select(thread);
        }
        match event {
            Event::FunctionCompletion(m) => self.completion(4, &m),
            Event::LateFunctionCompletion(m) => self.completion(5, &m),
            other => {
                let length = begin(&mut self.bytes, 2, 1);
                match other {
                    Event::ThreadAnnouncement(m) => leaf(&mut self.bytes, 1, &m),
                    Event::ThreadCompletion(m) => leaf(&mut self.bytes, 2, &m),
                    Event::FunctionAnnouncement(m) => leaf(&mut self.bytes, 3, &m),
                    _ => unreachable!(),
                }
                end_short(&mut self.bytes, length);
            }
        }
    }

    #[inline]
    #[allow(unsafe_code)]
    fn completion(&mut self, tag: u8, message: &crate::proto::FunctionCompletion) {
        // Maximum protobuf body: 3*(tag+10-byte varint), 3*(tag+fixed64),
        // one tag+5-byte uint32 = 66 bytes, plus two tag/length pairs = 70.
        // Indexing stays checked; only publishing initialized bytes is unsafe.
        const MAX_COMPLETION_BYTES: usize = 70;
        let start = self.bytes.len();
        let region = &mut self.bytes.spare_capacity_mut()[..MAX_COMPLETION_BYTES];
        let mut writer = CompletionWriter { region, len: 0 };
        writer.byte(0x12); // ThreadSection.events = 2.
        writer.byte(0); // SpanEvent length, filled below.
        writer.byte((tag << 3) | 2);
        writer.byte(0); // FunctionCompletion length, filled below.
        writer.varint(0x08, message.id);
        writer.varint(0x10, message.parent_id);
        writer.varint(0x18, message.node);
        writer.fixed64(0x21, message.entered_at_ticks);
        writer.fixed64(0x29, message.exited_at_ticks);
        writer.fixed64(0x31, message.self_await_ticks);
        writer.varint(0x38, u64::from(message.completion_flags));
        let length = writer.len;
        writer.region[1].write(u8::try_from(length - 2).expect("bounded completion"));
        writer.region[3].write(u8::try_from(length - 4).expect("bounded completion"));
        // SAFETY: all bytes in [start,start+length) were initialized by checked
        // writes into spare capacity above. No allocation or references escape.
        unsafe {
            self.bytes.set_len(start + length);
        }
    }

    fn select(&mut self, thread: TelemetryId) {
        if self.thread.is_some() {
            end_container(&mut self.bytes, self.section_length);
        } else {
            // RecordingFile.spans = 5. Remains open until sealing.
            begin(&mut self.bytes, 5, LENGTH_BYTES);
        }
        self.section_length = begin(&mut self.bytes, 1, LENGTH_BYTES);
        uint64::encode(1, &thread.get(), &mut self.bytes);
        self.thread = Some(thread);
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        if self.thread.take().is_some() {
            end_container(&mut self.bytes, self.section_length);
            end_container(&mut self.bytes, 1);
        }
        std::mem::take(&mut self.bytes)
    }
}

struct CompletionWriter<'a> {
    region: &'a mut [MaybeUninit<u8>],
    len: usize,
}
impl CompletionWriter<'_> {
    #[inline]
    fn byte(&mut self, byte: u8) {
        self.region[self.len].write(byte);
        self.len += 1;
    }
    #[inline]
    fn varint(&mut self, tag: u8, mut value: u64) {
        if value == 0 {
            return;
        }
        self.byte(tag);
        while value >= 128 {
            self.byte(value.to_le_bytes()[0] | 0x80);
            value >>= 7;
        }
        self.byte(value.to_le_bytes()[0]);
    }
    #[inline]
    fn fixed64(&mut self, tag: u8, value: u64) {
        if value == 0 {
            return;
        }
        self.byte(tag);
        for (dst, byte) in self.region[self.len..self.len + 8]
            .iter_mut()
            .zip(value.to_le_bytes())
        {
            dst.write(byte);
        }
        self.len += 8;
    }
}

#[inline]
fn begin(bytes: &mut Vec<u8>, tag: u8, length_bytes: usize) -> usize {
    bytes.push((tag << 3) | 2);
    let length = bytes.len();
    bytes.resize(length + length_bytes, 0);
    length
}
#[inline]
fn leaf<M: Message>(bytes: &mut Vec<u8>, tag: u8, message: &M) {
    let length = begin(bytes, tag, 1);
    message.encode_raw(bytes);
    end_short(bytes, length);
}
#[inline]
fn end_short(bytes: &mut [u8], length: usize) {
    let size = bytes.len() - length - 1;
    assert!(
        size < 128,
        "span schema exceeded its scalar-only encoding bound"
    );
    bytes[length] = u8::try_from(size).expect("one-byte length checked above");
}
fn end_container(bytes: &mut [u8], length: usize) {
    let mut size = u32::try_from(bytes.len() - length - LENGTH_BYTES)
        .expect("recording allocation exceeded its admitted encoding bound");
    for i in 0..LENGTH_BYTES {
        bytes[length + i] =
            (size.to_le_bytes()[0] & 0x7f) | if i + 1 == LENGTH_BYTES { 0 } else { 0x80 };
        size >>= 7;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::proto;

    #[test]
    fn maximum_scalar_widths_fit_admission_and_decode_with_generated_schema() {
        let thread = btel_types::allocate_telemetry_id();
        let completion = proto::FunctionCompletion {
            id: u64::MAX,
            parent_id: u64::MAX,
            node: (u64::from(u32::MAX) << 1) | 1,
            entered_at_ticks: u64::MAX,
            exited_at_ticks: u64::MAX,
            self_await_ticks: u64::MAX,
            completion_flags: 7,
        };
        let events = vec![
            Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
            Event::ThreadCompletion(proto::ThreadCompletion {
                completed_at_ticks: u64::MAX,
                outcome: proto::InvocationOutcome::Cancelled as i32,
            }),
            Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                id: u64::MAX,
                parent_id: u64::MAX,
                call_path_id: u32::MAX,
                entered_at_ticks: u64::MAX,
                inputs: proto::CaptureState::Deferred as i32,
            }),
            Event::FunctionCompletion(proto::FunctionCompletion {
                completion_flags: 15,
                ..completion
            }),
            Event::LateFunctionCompletion(completion),
        ];
        let mut encoded = EncodedSpans::default();
        for event in &events {
            encoded.reserve(encoded.len() + MAX_EVENT_BYTES);
            let capacity = encoded.capacity();
            encoded.event(thread, *event);
            assert_eq!(
                encoded.capacity(),
                capacity,
                "write exceeded admitted capacity"
            );
        }
        let allocation = encoded.bytes.as_ptr();
        let bytes = encoded.finish();
        assert_eq!(
            bytes.as_ptr(),
            allocation,
            "sealing must transfer the allocation"
        );
        let file = proto::RecordingFile::decode(bytes.as_slice()).unwrap();
        assert_eq!(
            file.spans.unwrap().sections,
            vec![proto::ThreadSection {
                thread_id: thread.get(),
                events: events
                    .into_iter()
                    .map(|event| proto::SpanEvent { event: Some(event) })
                    .collect(),
            }]
        );
    }
}

#[cfg(test)]
mod equivalence_tests {
    use super::*;
    use crate::proto;

    #[test]
    fn specialized_completions_match_generated_encoding_at_integer_boundaries() {
        let thread = btel_types::allocate_telemetry_id();
        let values = [0, 1, u64::MAX]
            .into_iter()
            .chain((1..64).flat_map(|bit| [(1_u64 << bit) - 1, 1_u64 << bit]));
        for value in values {
            for field in 0..7 {
                let mut message = proto::FunctionCompletion::default();
                match field {
                    0 => message.id = value,
                    1 => message.parent_id = value,
                    2 => message.node = value,
                    3 => message.entered_at_ticks = value,
                    4 => message.exited_at_ticks = value,
                    5 => message.self_await_ticks = value,
                    _ => message.completion_flags = u32::try_from(value).unwrap_or(u32::MAX),
                }
                for event in [
                    Event::FunctionCompletion(message),
                    Event::LateFunctionCompletion(message),
                ] {
                    let expected = crate::proto::ThreadSection {
                        thread_id: thread.get(),
                        events: vec![crate::proto::SpanEvent { event: Some(event) }],
                    }
                    .encode_to_vec();
                    let mut writer = EncodedSpans::default();
                    writer.reserve(MAX_EVENT_BYTES);
                    writer.event(thread, event);
                    assert_eq!(
                        &writer.bytes[writer.section_length + LENGTH_BYTES..],
                        expected
                    );
                }
            }
        }
    }
}
