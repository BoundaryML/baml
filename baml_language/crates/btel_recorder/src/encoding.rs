//! Span messages written once into the eventual `RecordingFile` allocation.
//!
//! Containers reserve five bytes for their protobuf length and backpatch by
//! index, so Vec relocation is safe. Non-minimal varints preserve the protobuf
//! schema; no whole-buffer shift or nested sizing pass is required. Span
//! bodies have one-byte lengths; events with strings (logs, network spans)
//! use a full container prefix. Every key, length and completion is
//! `btel-core`'s (`btel_core::recording::frame`); what is here is the loop,
//! the buffer and when a section opens and closes.
use btel_core::recording::{field, frame, minor};
use btel_types::TelemetryId;
use prost::encoding::uint64;

use crate::proto::span_event::Event;

#[cfg(test)]
mod context_tests;

#[derive(Default)]
pub(crate) struct EncodedSpans {
    bytes: Vec<u8>,
    thread: Option<TelemetryId>,
    section_length: usize,
    context: Option<crate::proto::thread_section::Context>,
    selected_context: Option<(TelemetryId, Option<crate::proto::thread_section::Context>)>,
    required_minor: u32,
}
impl EncodedSpans {
    pub(crate) fn select_context(
        &mut self,
        thread: TelemetryId,
        context: Option<crate::proto::thread_section::Context>,
    ) {
        self.selected_context = Some((thread, context));
        // The selector may own a CAS snapshot. Materialize its section now so
        // capture pressure can seal and deliver it before the next observation.
        if self.thread != Some(thread) || self.context != context {
            self.select(thread, context);
        }
    }

    /// The context the current chunk selected for `thread`.
    pub(crate) fn context_of(
        &self,
        thread: TelemetryId,
    ) -> Option<crate::proto::thread_section::Context> {
        self.selected_context
            .filter(|(selected, _)| *selected == thread)
            .and_then(|(_, context)| context)
    }

    /// `event` in a section of `thread` under `context`, whatever the
    /// current chunk selected; the chunk's own selection stands after it.
    pub(crate) fn event_in(
        &mut self,
        thread: TelemetryId,
        context: Option<crate::proto::thread_section::Context>,
        event: Event,
    ) {
        let selected = self.selected_context.replace((thread, context));
        self.event(thread, &event);
        self.selected_context = selected;
    }

    pub(crate) fn begin_chunk(&mut self) {
        self.selected_context = None;
    }

    /// Every file is at least minor 16: its spans are whole in their end
    /// records, and a reader of any file of the recording relies on it.
    pub(crate) fn format_minor(&self) -> u32 {
        minor::WHOLE_SPAN.max(self.required_minor)
    }

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
    pub(crate) fn event(&mut self, thread: TelemetryId, event: &Event) {
        if self
            .selected_context
            .is_some_and(|(selected, _)| selected != thread)
        {
            self.selected_context = None;
        }
        let context = self.selected_context.and_then(|(_, context)| context);
        if self.thread != Some(thread) || self.context != context {
            self.select(thread, context);
        }
        match event {
            // A completion that carries more than scalars (its lineage, its
            // inputs, its usage) leaves the fixed-size path.
            Event::FunctionCompletion(m) if !fixed(m) => {
                self.variable(field::span_event::FUNCTION_COMPLETION, m);
            }
            Event::LateFunctionCompletion(m) if !fixed(m) => {
                self.variable(field::span_event::LATE_FUNCTION_COMPLETION, m);
            }
            Event::FunctionCompletion(m) => {
                self.completion(field::span_event::FUNCTION_COMPLETION, m);
            }
            Event::LateFunctionCompletion(m) => {
                self.completion(field::span_event::LATE_FUNCTION_COMPLETION, m);
            }
            Event::Log(message) => {
                self.require(if message.line.is_some() {
                    minor::LOG_LINE
                } else {
                    minor::LOG
                });
                self.variable(field::span_event::LOG, message);
            }
            Event::NetworkAnnouncement(message) => {
                self.require(minor::NETWORK);
                self.variable(field::span_event::NETWORK_ANNOUNCEMENT, message);
            }
            Event::NetworkEvent(message) => {
                self.require(minor::NETWORK);
                self.variable(field::span_event::NETWORK_EVENT, message);
            }
            Event::ThreadAnnouncement(m) => {
                frame::event(&mut self.bytes, field::span_event::THREAD_ANNOUNCEMENT, m);
            }
            Event::ThreadCompletion(m) => {
                self.variable(field::span_event::THREAD_COMPLETION, m);
            }
            Event::ThreadRunning(m) => {
                self.require(minor::THREAD_RUNNING);
                frame::event(&mut self.bytes, field::span_event::THREAD_RUNNING, m);
            }
            Event::FunctionAnnouncement(m) => {
                if m.manual {
                    self.require(minor::MANUAL_SPAN);
                }
                if m.type_args_cas_id.is_some() {
                    self.require(minor::TYPE_ARGS);
                }
                frame::event(&mut self.bytes, field::span_event::FUNCTION_ANNOUNCEMENT, m);
            }
            Event::NetworkCompletion(m) => {
                self.variable(field::span_event::NETWORK_COMPLETION, m);
            }
        }
    }

    pub(crate) fn require(&mut self, minor: u32) {
        self.required_minor = self.required_minor.max(minor);
    }

    /// An event with strings, under a full-width length prefix.
    fn variable<M: prost::Message>(&mut self, member: u32, message: &M) {
        frame::event_with_text(&mut self.bytes, member, message);
    }

    #[inline]
    #[allow(unsafe_code)]
    fn completion(&mut self, member: u32, message: &crate::proto::FunctionCompletion) {
        let flags = message.completion_flags;
        if btel_core::recording::CompletionFlags::closed_bit(flags)
            && !btel_core::recording::CompletionFlags::manual_bit(flags)
        {
            self.require(minor::OWNED_SPAN);
        }
        if btel_core::recording::CompletionFlags::manual_bit(flags) {
            self.require(minor::MANUAL_SPAN);
        }
        // A variable-length event may have used the capacity reserved for
        // this one; the region must be there whatever came before.
        self.bytes.reserve(frame::MAX_COMPLETION_BYTES);
        // Indexing stays checked; only publishing initialized bytes is unsafe.
        let start = self.bytes.len();
        let region = &mut self.bytes.spare_capacity_mut()[..frame::MAX_COMPLETION_BYTES];
        let length = frame::function_completion(region, member, message);
        // SAFETY: `function_completion` initialized the `length` bytes from
        // the start of the spare capacity. No allocation or references escape.
        unsafe {
            self.bytes.set_len(start + length);
        }
    }

    fn select(
        &mut self,
        thread: TelemetryId,
        context: Option<crate::proto::thread_section::Context>,
    ) {
        if self.thread.is_some() {
            frame::close_padded(&mut self.bytes, self.section_length);
        } else {
            // Remains open until sealing.
            frame::open(
                &mut self.bytes,
                field::recording_file::SPANS,
                frame::LENGTH_BYTES,
            );
        }
        self.section_length = frame::open(
            &mut self.bytes,
            field::span_batch::SECTIONS,
            frame::LENGTH_BYTES,
        );
        uint64::encode(
            field::thread_section::THREAD_ID,
            &thread.get(),
            &mut self.bytes,
        );
        if let Some(context) = context {
            use crate::proto::thread_section::Context;
            match context {
                Context::ContextCasId(id) => {
                    prost::encoding::message::encode(
                        field::thread_section::CONTEXT_CAS_ID,
                        &id,
                        &mut self.bytes,
                    );
                }
                Context::EmptyContext(empty) => {
                    prost::encoding::bool::encode(
                        field::thread_section::EMPTY_CONTEXT,
                        &empty,
                        &mut self.bytes,
                    );
                }
            }
            self.required_minor = self.required_minor.max(minor::CONTEXT);
        }
        self.thread = Some(thread);
        self.context = context;
    }

    pub(crate) fn finish(&mut self) -> Vec<u8> {
        if self.thread.take().is_some() {
            frame::close_padded(&mut self.bytes, self.section_length);
            // The spans' length, after the one-byte key at the start.
            frame::close_padded(&mut self.bytes, 1);
        }
        self.required_minor = 0;
        std::mem::take(&mut self.bytes)
    }
}

#[cfg(test)]
mod tests {
    use prost::Message as _;

    use super::*;
    use crate::proto;

    #[test]
    fn container_lengths_decode_across_varint_boundaries() {
        for size in [0, 1, 127, 128, 16_383, 16_384, 65_535] {
            let mut bytes = Vec::new();
            let length = frame::open(&mut bytes, 1, frame::LENGTH_BYTES);
            bytes.resize(bytes.len() + size, 0);
            frame::close_padded(&mut bytes, length);
            let mut prefix = &bytes[length..length + frame::LENGTH_BYTES];
            assert_eq!(
                prost::encoding::decode_varint(&mut prefix).unwrap(),
                u64::try_from(size).unwrap()
            );
            assert!(prefix.is_empty());
        }
    }

    /// A failed future that also used a model takes the usage path, and still
    /// declares the minor that introduced its error.
    #[test]
    fn a_thread_error_requires_its_minor_with_or_without_usage() {
        for usage in [Vec::new(), vec![proto::ModelUsage::default()]] {
            let mut encoded = EncodedSpans::default();
            encoded.reserve(frame::MAX_EVENT_BYTES);
            encoded.event(
                btel_types::allocate_telemetry_id(),
                &Event::ThreadCompletion(proto::ThreadCompletion {
                    outcome: proto::InvocationOutcome::Errored as i32,
                    usage,
                    error_cas_id: Some(proto::CasId { low: 1, high: 2 }),
                    ..proto::ThreadCompletion::default()
                }),
            );
            assert!(
                encoded.format_minor() >= minor::THREAD_ERROR,
                "{}",
                encoded.format_minor()
            );
        }
    }

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
            completion_flags: 3,
            value_cas_id: Some(proto::CasId {
                low: u64::MAX,
                high: u64::MAX,
            }),
            panicked: true,
            ..proto::FunctionCompletion::default()
        };
        let events = vec![
            Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
            Event::ThreadCompletion(proto::ThreadCompletion {
                completed_at_ticks: u64::MAX,
                outcome: proto::InvocationOutcome::Errored as i32,
                panicked: true,
                usage: Vec::new(),
                error_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
                ..proto::ThreadCompletion::default()
            }),
            Event::FunctionAnnouncement(proto::FunctionAnnouncement {
                manual: false,
                id: u64::MAX,
                parent_id: u64::MAX,
                call_path_id: u32::MAX,
                entered_at_ticks: u64::MAX,
                inputs_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
                type_args_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
            }),
            Event::FunctionCompletion(proto::FunctionCompletion {
                completion_flags: 11,
                ..completion.clone()
            }),
            Event::LateFunctionCompletion(completion),
            // Network events' strings reserve their own bytes; the rest fits.
            Event::NetworkAnnouncement(proto::NetworkAnnouncement {
                id: u64::MAX,
                parent_id: u64::MAX,
                call_path_id: u32::MAX,
                started_at_ticks: u64::MAX,
                method: String::new(),
                url: String::new(),
                request_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
            }),
            Event::NetworkEvent(proto::NetworkEvent {
                span_id: u64::MAX,
                name: String::new(),
                at_ticks: u64::MAX,
                payload_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
            }),
            Event::NetworkCompletion(proto::NetworkCompletion {
                span_id: u64::MAX,
                completed_at_ticks: u64::MAX,
                outcome: proto::InvocationOutcome::Errored as i32,
                panicked: true,
                error_cas_id: Some(proto::CasId {
                    low: u64::MAX,
                    high: u64::MAX,
                }),
                ..proto::NetworkCompletion::default()
            }),
        ];
        let mut encoded = EncodedSpans::default();
        for event in &events {
            encoded.reserve(encoded.len() + frame::MAX_EVENT_BYTES);
            let capacity = encoded.capacity();
            encoded.event(thread, event);
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
                context: None,
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
    use prost::Message as _;

    use super::*;
    use crate::proto;

    #[test]
    fn specialized_completions_match_generated_encoding_at_integer_boundaries() {
        let thread = btel_types::allocate_telemetry_id();
        let values = [0, 1, u64::MAX]
            .into_iter()
            .chain((1..64).flat_map(|bit| [(1_u64 << bit) - 1, 1_u64 << bit]));
        for value in values {
            for field in 0..11 {
                let mut message = proto::FunctionCompletion::default();
                match field {
                    0 => message.id = value,
                    1 => message.parent_id = value,
                    2 => message.node = value,
                    3 => message.entered_at_ticks = value,
                    4 => message.exited_at_ticks = value,
                    5 => message.self_await_ticks = value,
                    6 => message.completion_flags = u32::try_from(value).unwrap_or(u32::MAX),
                    7 => {
                        message.value_cas_id = Some(proto::CasId {
                            low: value,
                            high: 0,
                        });
                    }
                    8 => {
                        message.value_cas_id = Some(proto::CasId {
                            low: 0,
                            high: value,
                        });
                    }
                    9 => message.panicked = value != 0,
                    _ => {
                        message.value_cas_id = Some(proto::CasId {
                            low: value,
                            high: value,
                        });
                    }
                }
                for event in [
                    Event::FunctionCompletion(message.clone()),
                    Event::LateFunctionCompletion(message),
                ] {
                    let expected = crate::proto::ThreadSection {
                        thread_id: thread.get(),
                        context: None,
                        events: vec![crate::proto::SpanEvent {
                            event: Some(event.clone()),
                        }],
                    }
                    .encode_to_vec();
                    let mut writer = EncodedSpans::default();
                    writer.reserve(frame::MAX_EVENT_BYTES);
                    writer.event(thread, &event);
                    assert_eq!(
                        &writer.bytes[writer.section_length + frame::LENGTH_BYTES..],
                        expected
                    );
                }
            }
        }
    }
}

/// A completion of scalars alone, which the fixed-size path writes.
fn fixed(message: &crate::proto::FunctionCompletion) -> bool {
    message.usage.is_empty()
        && message.total_usage.is_empty()
        && message.lineage.is_none()
        && message.inputs_cas_id.is_none()
        && message.type_args_cas_id.is_none()
}
