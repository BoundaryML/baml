use std::{cell::RefCell, num::NonZeroUsize, sync::Arc};

use bex_chunkedringbuffer::{ChunkPool, Config};
use btel_clock::{ClockMode, ClockRuntime};
use btel_processor::{Processor, Publisher};
use btel_records::TimingRecord;
use btel_types::{
    AwaitDuration, CallPathId, ClockDuration, ClockInstant, InvocationOutcome,
    allocate_telemetry_id,
};
use prost::Message;

use super::*;

struct ConvertedBatch {
    definitions: proto::Definitions,
    aggregates: proto::AggregateBatch,
    spans: proto::SpanBatch,
    clock_states: proto::ClockStateBatch,
}

// Inspect conversion independently of recording admission and delivery.
struct ProtobufPublisher<F> {
    buffer: ConversionBuffer,
    receive: F,
}
impl<F: FnMut(ConvertedBatch)> ProtobufPublisher<F> {
    fn new(receive: F) -> Self {
        Self {
            buffer: ConversionBuffer::default(),
            receive,
        }
    }
}
impl<F: FnMut(ConvertedBatch)> Publisher<CaptureDeferred, CaptureDeferred>
    for ProtobufPublisher<F>
{
    fn aggregate(&mut self, d: AggregateDelta) {
        self.buffer.aggregate(d);
    }
    fn span(&mut self, t: TelemetryId, r: &SpanRecord<CaptureDeferred, CaptureDeferred>) {
        self.buffer
            .spans
            .reserve(self.buffer.spans.len() + crate::encoding::MAX_EVENT_BYTES);
        self.buffer.span(t, r);
    }
    fn flush(&mut self) {
        if self.buffer.estimate == 0 {
            return;
        }
        let b = self.buffer.take();
        let bytes = self.buffer.spans.finish();
        let spans = proto::RecordingFile::decode(bytes.as_slice())
            .unwrap()
            .spans
            .unwrap_or_default();
        (self.receive)(ConvertedBatch {
            definitions: b.definitions,
            aggregates: b.aggregates,
            spans,
            clock_states: b.clock_states,
        });
    }
}

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

#[test]
fn completion_variants_roundtrip_without_losing_wire_semantics() {
    let id = allocate_telemetry_id();
    let parent_id = allocate_telemetry_id();
    let call_path = CallPathId::new_non_root(u32::MAX).unwrap();
    // Preserve backwards raw timestamps; only aggregation clamps their delta.
    let entered_at = ClockInstant::from_ticks(u64::MAX);
    let exited_at = ClockInstant::from_ticks(3);
    let await_time = AwaitDuration::ZERO.saturating_add(ClockDuration::from_ticks(17));
    let mut batches = Vec::new();
    let mut publisher = ProtobufPublisher::new(|batch| batches.push(batch));
    let mut expected = Vec::new();
    for captured in [false, true] {
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, false, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionOkNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, false, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, true, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionOkReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, true, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, false, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionErroredNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, false, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, true, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionErroredReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, true, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, false, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionCancelledNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, false, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, true, false, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::FunctionSpanCompletionCancelledReentryNeedsAnnouncement {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, true, true, false, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionOk {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, false, false, true, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionOkReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Ok, true, false, true, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionErrored {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, false, false, true, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionErroredReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Errored, true, false, true, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionCancelled {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, false, false, true, captured));
        publisher.span(
            parent_id,
            &SpanRecord::LateFunctionSpanCompletionCancelledReentry {
                id,
                parent_id,
                call_path,
                entered_at,
                exited_at,
                await_time,
                captured_value: captured.then(|| Box::new(CaptureDeferred)),
            },
        );
        expected.push((InvocationOutcome::Cancelled, true, false, true, captured));
    }
    publisher.flush();
    publisher.flush(); // No duplicate contribution from an empty flush.
    drop(publisher);
    assert_eq!(batches.len(), 1);
    let original = &batches[0].spans;
    let decoded = proto::SpanBatch::decode(original.encode_to_vec().as_slice()).unwrap();
    assert_eq!(decoded, *original);
    assert_eq!(decoded.sections.len(), 1);
    for (event, (outcome, reentry, dependency, late, captured)) in
        decoded.sections[0].events.iter().zip(expected)
    {
        let (completion, is_late) = match event.event.as_ref().unwrap() {
            proto::span_event::Event::FunctionCompletion(value) => (value, false),
            proto::span_event::Event::LateFunctionCompletion(value) => (value, true),
            _ => panic!("expected completion"),
        };
        let flags = CompletionFlags::from_wire(completion.completion_flags, is_late).unwrap();
        assert_eq!(flags.outcome(), outcome);
        assert_eq!(flags.requires_announcement(), dependency);
        assert_eq!(flags.capture_deferred(), captured);
        assert_eq!(is_late, late);
        assert_eq!(
            completion.node,
            (u64::from(u32::MAX) << 1) | u64::from(reentry)
        );
        assert_eq!(
            (completion.id, completion.parent_id),
            (id.get(), parent_id.get())
        );
        assert_eq!(
            (
                completion.entered_at_ticks,
                completion.exited_at_ticks,
                completion.self_await_ticks
            ),
            (u64::MAX, 3, 17)
        );
        // Field 7 + a one-byte varint: explicit format assertion independent of
        // Rust enum layout and prost decoding's interpretation of the value.
        let bytes = completion.encode_to_vec();
        assert_eq!(
            &bytes[bytes.len() - 2..],
            &[0x38, u8::try_from(completion.completion_flags).unwrap()]
        );
    }
    for bits in [0, 4, 8, 12, 16, u32::MAX] {
        assert!(CompletionFlags::from_wire(bits, false).is_none());
    }
    for bits in [9, 10, 11, 13, 14, 15] {
        assert!(CompletionFlags::from_wire(bits, true).is_none());
    }
}

#[test]
fn converted_output_outlives_recycled_chunks_and_preserves_selector_context() {
    let batches = RefCell::new(Vec::new());
    let pool = ChunkPool::new(Config {
        chunk_capacity: nz(8),
        timing_chunks: nz(1),
        span_chunks: nz(1),
        max_producers: nz(1),
        preallocate: true,
    })
    .unwrap();
    let mut processor = Processor::with_publisher(
        pool.bind_consumer().unwrap(),
        nz(8),
        ProtobufPublisher::new(|batch| batches.borrow_mut().push(batch)),
    );
    let mut producer = pool.register_producer().unwrap();
    let thread = allocate_telemetry_id();
    let child = allocate_telemetry_id();
    let path = CallPathId::new_non_root(1).unwrap();
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let function = btel_types::FunctionIdAllocator::default()
        .allocate()
        .unwrap();
    producer.write_timing(TimingRecord::ThreadSelected { thread_id: thread });
    producer.write_timing(TimingRecord::FunctionTimingCompletion {
        call_path: path,
        entered_at: ClockInstant::from_ticks(3),
        exited_at: ClockInstant::from_ticks(2),
        await_time: AwaitDuration::ZERO,
        outcome: InvocationOutcome::Errored,
        reentry: false,
    });
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    // A completion alone supplies thread and epoch definitions; no announcement.
    producer.write_span(SpanRecord::ThreadSpanCompletion {
        id: thread,
        parent_id: None,
        spawn_call_path: CallPathId::ROOT,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(8),
        outcome: InvocationOutcome::Errored,
        clock: clock.clone(),
    });
    producer.write_span(SpanRecord::CallPathDefined {
        call_path: path,
        parent_call_path: CallPathId::ROOT,
        visible_caller: None,
        caller_pc: u32::MAX,
        callee: function,
        edge: btel_types::CallPathEdge::Synchronous,
    });
    producer.write_span(
        SpanRecord::FunctionSpanCompletionOkReentryNeedsAnnouncement {
            id: allocate_telemetry_id(),
            parent_id: thread,
            call_path: path,
            entered_at: ClockInstant::from_ticks(2),
            exited_at: ClockInstant::from_ticks(7),
            await_time: AwaitDuration::ZERO,
            captured_value: Some(Box::new(CaptureDeferred)),
        },
    );
    producer.seal();
    processor.process_available();
    assert_eq!(pool.stats().free_chunks, 2);
    assert_eq!(
        Arc::strong_count(&clock),
        1,
        "converter did not retain epoch ownership"
    );
    assert!(
        batches.borrow().is_empty(),
        "converted values still private until flush"
    );
    // Reuse the very same allocation, initially without another selector.
    let announcement = |parent_id| SpanRecord::FunctionSpanAnnouncement {
        id: allocate_telemetry_id(),
        parent_id,
        call_path: path,
        entered_at: ClockInstant::from_ticks(4),
        captured_inputs: Some(Box::new(CaptureDeferred)),
    };
    producer.write_span(announcement(thread));
    producer.write_span(SpanRecord::ThreadSelected { thread_id: child });
    producer.write_span(announcement(child));
    producer.write_span(SpanRecord::ThreadSelected { thread_id: thread });
    producer.write_span(announcement(thread));
    drop(producer);
    pool.close_admission();
    assert!(processor.process_available().complete);
    assert_eq!(pool.stats().free_chunks, 2);
    drop(processor);
    let batches = batches.into_inner();
    assert_eq!(batches.len(), 1);
    let batch = &batches[0];
    assert_eq!(
        batch
            .spans
            .sections
            .iter()
            .map(|s| s.thread_id)
            .collect::<Vec<_>>(),
        [thread.get(), child.get(), thread.get()]
    );
    assert_eq!(
        batch
            .aggregates
            .entries
            .iter()
            .map(|d| d.count)
            .sum::<u64>(),
        2,
        "span and timing aggregated exactly once"
    );
    assert_eq!(
        batch
            .aggregates
            .entries
            .iter()
            .find(|d| d.node == 2)
            .unwrap()
            .total_duration_ticks,
        0
    );
    assert_eq!(
        batch
            .aggregates
            .entries
            .iter()
            .find(|d| d.node == 3)
            .unwrap()
            .total_duration_ticks,
        5
    );
    assert_eq!(batch.definitions.threads[0].thread_id, thread.get());
    assert_eq!(
        batch.definitions.clock_epochs[0].epoch_id,
        clock.metadata().epoch.get()
    );
    assert_eq!(batch.definitions.functions[0].function_id, function.get());
    assert!(matches!(
        batch.definitions.functions[0].resolution,
        Some(proto::function_definition::Resolution::Unavailable(_))
    ));
    assert_eq!(batch.definitions.call_paths[0].caller_pc, u32::MAX);
    assert!(!batch.clock_states.states[0].r#final);
    let encoded = batch.spans.encode_to_vec();
    assert_eq!(
        proto::SpanBatch::decode(encoded.as_slice()).unwrap(),
        batch.spans
    );
}

#[test]
fn clock_snapshots_are_owned_immutable_and_never_claim_finality() {
    let runtime = ClockRuntime::new(ClockMode::Monotonic);
    let epoch = runtime.start_run();
    epoch.attach_thread();
    let before = clock::definition(epoch.metadata());
    let status_before = clock::state(&epoch);
    let fresh = runtime.reset_after_restore();
    assert_eq!(clock::definition(epoch.metadata()), before);
    assert_ne!(fresh.metadata().epoch.get(), before.epoch_id);
    let expected_status = if status_before.status == proto::TimingStatus::Valid as i32 {
        proto::TimingStatus::Restored as i32
    } else {
        // A heavily descheduled origin probe may already be uncertain. Reset
        // preserves that invalid status rather than rewriting its first cause.
        status_before.status
    };
    assert_eq!(clock::state(&epoch).status, expected_status);
    assert!(!status_before.r#final);
    let bytes = before.encode_to_vec();
    assert_eq!(
        proto::ClockEpochDefinition::decode(bytes.as_slice()).unwrap(),
        before
    );
    let utc = before.utc.unwrap().unix_nanos.unwrap();
    let recovered = (i128::from(utc.high) << 64) | i128::from(utc.low);
    assert_eq!(recovered, epoch.metadata().utc.unix_nanos.get());
    epoch.finish_thread();
}

#[test]
fn second_level_merging_preserves_all_contributions_and_resets_windows() {
    use std::collections::BTreeMap;

    use btel_types::CallPathNodeId;
    let mut batches = Vec::new();
    let mut publisher = ProtobufPublisher::new(|batch| batches.push(batch));
    let mut expected = BTreeMap::<u64, [u128; 3]>::new();
    for i in 0..600_u32 {
        let path = CallPathId::new_non_root((1 + i % 65) | ((i % 2) << 31)).unwrap();
        let node = CallPathNodeId::new(path, i % 3 == 0);
        let delta = AggregateDelta {
            node,
            count: 1,
            total_duration: ClockDuration::from_ticks(u64::from(i)),
            total_io_duration: AwaitDuration::ZERO
                .saturating_add(ClockDuration::from_ticks(u64::from(i % 19))),
        };
        publisher.aggregate(delta);
        let totals = expected.entry(node.get()).or_default();
        totals[0] += 1;
        totals[1] += u128::from(i);
        totals[2] += u128::from(i % 19);
    }
    publisher.flush();
    publisher.flush();
    // Same node in a new window must contribute only newly received data.
    let node = CallPathNodeId::new(CallPathId::new_non_root(1).unwrap(), true);
    publisher.aggregate(AggregateDelta {
        node,
        count: 7,
        total_duration: ClockDuration::from_ticks(13),
        total_io_duration: AwaitDuration::ZERO,
    });
    publisher.flush();
    drop(publisher);
    assert_eq!(batches.len(), 2);
    let first = &batches[0].aggregates.entries;
    assert_eq!(
        first.len(),
        expected.len(),
        "one entry per full node, not per incoming delta"
    );
    let actual: BTreeMap<_, _> = first
        .iter()
        .map(|d| {
            (
                d.node,
                [
                    u128::from(d.count),
                    u128::from(d.total_duration_ticks),
                    u128::from(d.total_self_await_ticks),
                ],
            )
        })
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(
        batches[1].aggregates.entries,
        [proto::AggregateDelta {
            node: node.get(),
            count: 7,
            total_duration_ticks: 13,
            total_self_await_ticks: 0
        }]
    );
}

#[test]
fn overflow_spills_whole_delta_without_partial_updates_or_mid_callback_flush() {
    use btel_types::CallPathNodeId;
    let batches = RefCell::new(Vec::new());
    let mut publisher = ProtobufPublisher::new(|batch| batches.borrow_mut().push(batch));
    let mut expected = std::collections::BTreeMap::<u64, [u128; 3]>::new();
    for (raw, values) in [
        (1, [u64::MAX, 4, 6]),
        (2, [2, u64::MAX, 6]),
        (3, [2, 4, u64::MAX]),
    ] {
        let node = CallPathNodeId::new(CallPathId::new_non_root(raw).unwrap(), raw % 2 == 0);
        for values in [values, [1, 1, 1], [2, 3, 4]] {
            publisher.aggregate(AggregateDelta {
                node,
                count: values[0],
                total_duration: ClockDuration::from_ticks(values[1]),
                total_io_duration: AwaitDuration::ZERO
                    .saturating_add(ClockDuration::from_ticks(values[2])),
            });
            for (total, value) in expected
                .entry(node.get())
                .or_default()
                .iter_mut()
                .zip(values)
            {
                *total += u128::from(value);
            }
        }
    }
    assert!(
        batches.borrow().is_empty(),
        "overflow must not invoke a receiver mid-chunk"
    );
    publisher.flush();
    drop(publisher);
    let batches = batches.into_inner();
    assert_eq!(batches.len(), 1);
    assert_eq!(
        batches[0].aggregates.entries.len(),
        6,
        "one spill and one remainder per node"
    );
    let mut actual = std::collections::BTreeMap::<u64, [u128; 3]>::new();
    for d in &batches[0].aggregates.entries {
        for (total, value) in actual.entry(d.node).or_default().iter_mut().zip([
            d.count,
            d.total_duration_ticks,
            d.total_self_await_ticks,
        ]) {
            *total += u128::from(value);
        }
    }
    assert_eq!(actual, expected);
}
