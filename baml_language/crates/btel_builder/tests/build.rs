use btel_builder::{BuildErrorKind, OpenSpan, TelemetryEvent, TelemetryEventBuilder};
use btel_core::{
    ids::{BexCallId, BexThreadId, FunctionId},
    marker::{
        CallSiteSourceSpan, DecodeError, FunctionEndStatus, MAX_RECORD_LEN, Marker, ThreadEndStatus,
    },
    stage::MarkerRange,
};

fn enter(thread: u64, call: u64) -> Marker<'static> {
    Marker::FunctionEnter {
        flags: 7,
        thread_id: BexThreadId(thread),
        call_id: BexCallId(call),
        parent_call_id: BexCallId(call.saturating_sub(1)),
        function_id: FunctionId(42),
        call_site: Some(CallSiteSourceSpan {
            file_id: 2,
            start_offset: 3,
            end_offset: 9,
            line: 1,
        }),
        ts_ticks: 10,
    }
}
fn exit(thread: u64, call: u64) -> Marker<'static> {
    Marker::FunctionExit {
        thread_id: BexThreadId(thread),
        call_id: BexCallId(call),
        status: FunctionEndStatus::Errored,
        ts_ticks: 30,
    }
}
fn bytes(markers: &[Marker<'_>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for marker in markers {
        let mut buffer = [0; MAX_RECORD_LEN];
        let len = marker.encode(&mut buffer);
        bytes.extend_from_slice(&buffer[..len]);
    }
    bytes
}
fn push(
    builder: &mut TelemetryEventBuilder,
    source: u64,
    markers: &[Marker<'_>],
) -> Vec<TelemetryEvent> {
    let bytes = bytes(markers);
    let mut out = Vec::new();
    builder
        .try_build(
            MarkerRange {
                source_id: source,
                bytes: &bytes,
            },
            |event| out.push(event),
        )
        .unwrap();
    // Outputs must outlive the transport's bytes.
    out
}

#[test]
fn matches_across_sources_in_both_arrival_orders_with_owned_fields() {
    for reverse in [false, true] {
        let mut builder = TelemetryEventBuilder::default();
        let (first, second) = if reverse {
            (exit(9, 2), enter(9, 2))
        } else {
            (enter(9, 2), exit(9, 2))
        };
        assert!(push(&mut builder, 10, &[first]).is_empty());
        assert_eq!(builder.open_spans().len(), 1);
        let out = push(&mut builder, 20, &[second]);
        let [TelemetryEvent::ClosedSpan(span)] = out.as_slice() else {
            panic!("expected span");
        };
        assert_eq!(span.key.thread_id, BexThreadId(9));
        assert_eq!(span.key.call_id, BexCallId(2));
        assert_eq!(span.start.parent_call_id, BexCallId(1));
        assert_eq!(span.start.function_id, FunctionId(42));
        assert_eq!(span.start.flags, 7);
        assert_eq!(span.start.call_site.unwrap().end_offset, 9);
        assert_eq!(span.start.source_id, if reverse { 20 } else { 10 });
        assert_eq!(span.end.source_id, if reverse { 10 } else { 20 });
        assert_eq!(span.end.status, FunctionEndStatus::Errored);
        assert_eq!(span.duration_ticks(), Some(20));
        assert!(builder.open_spans().is_empty());
    }
}

#[test]
fn logical_identity_separates_reused_call_ids_without_requiring_a_stack() {
    let mut builder = TelemetryEventBuilder::default();
    let out = push(
        &mut builder,
        1,
        &[
            enter(1, 1),
            enter(1, 2),
            enter(2, 1),
            exit(1, 1),
            exit(2, 1),
            exit(1, 2),
        ],
    );
    let keys: Vec<_> = out
        .iter()
        .map(|event| {
            let TelemetryEvent::ClosedSpan(span) = event else {
                panic!("expected span");
            };
            (span.key.thread_id.0, span.key.call_id.0)
        })
        .collect();
    assert_eq!(keys, [(1, 1), (2, 1), (1, 2)]);
    assert!(builder.open_spans().is_empty());
}

#[test]
fn awaited_exit_preserves_status_and_clock_inversion() {
    let mut builder = TelemetryEventBuilder::default();
    let out = push(
        &mut builder,
        1,
        &[
            Marker::FunctionExitAwaited {
                status: FunctionEndStatus::Cancelled,
                thread_id: BexThreadId(3),
                call_id: BexCallId(7),
                ts_ticks: 5,
                await_ns: 300,
                await_count: 2,
            },
            enter(3, 7),
        ],
    );
    let [TelemetryEvent::ClosedSpan(span)] = out.as_slice() else {
        panic!("expected span");
    };
    assert_eq!(span.end.await_ns, 300);
    assert_eq!(span.end.await_count, 2);
    assert_eq!(span.end.status, FunctionEndStatus::Cancelled);
    assert_eq!(span.duration_ticks(), None);
}

#[test]
fn thread_end_does_not_close_or_delete_unmatched_calls() {
    let mut builder = TelemetryEventBuilder::default();
    let out = push(
        &mut builder,
        1,
        &[
            enter(1, 1),
            exit(1, 2),
            Marker::BexThreadEnd {
                status: ThreadEndStatus::Completed,
                thread_id: BexThreadId(1),
                ts_ticks: 40,
            },
        ],
    );
    assert!(matches!(
        out.as_slice(),
        [TelemetryEvent::ThreadEnded { .. }]
    ));
    assert_eq!(builder.open_spans().len(), 2);
    assert_eq!(push(&mut builder, 2, &[exit(1, 1)]).len(), 1);
    let store = builder.into_open_spans();
    assert_eq!(store.len(), 1);
    let (key, half) = store.iter().next().unwrap();
    assert_eq!(key.call_id, BexCallId(2));
    assert!(matches!(half, OpenSpan::End(_)));
}

#[test]
fn metadata_is_owned_and_late_ids_are_immediate_events() {
    let mut builder = TelemetryEventBuilder::default();
    let out = push(
        &mut builder,
        4,
        &[
            Marker::BexThreadStart {
                flags: 1,
                thread_id: BexThreadId(1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 2,
                name: b"root",
            },
            Marker::BexThreadStartSpawned {
                flags: 2,
                thread_id: BexThreadId(2),
                parent_thread_id: BexThreadId(1),
                parent_call_id: BexCallId(3),
                ts_ticks: 5,
                spawn_site: None,
                name: b"child",
            },
            enter(2, 1),
            exit(2, 1),
            Marker::SetBoundaryLocalId {
                thread_id: BexThreadId(2),
                call_id: BexCallId(1),
                id: [9; 16],
                ts_ticks: 40,
            },
        ],
    );
    assert!(
        matches!(&out[0], TelemetryEvent::ThreadStarted { name, spawn_site: None, .. } if name == b"root")
    );
    assert!(
        matches!(&out[1], TelemetryEvent::ThreadStarted { name, spawn_site: Some(None), .. } if name == b"child")
    );
    assert!(matches!(&out[2], TelemetryEvent::ClosedSpan(_)));
    assert!(matches!(&out[3], TelemetryEvent::BoundaryLocalId { id, .. } if *id == [9; 16]));
    assert!(builder.open_spans().is_empty());
}

#[test]
fn corruption_and_duplicate_halves_report_offsets_without_overwriting() {
    for marker in [enter(1, 1), exit(1, 1)] {
        let mut builder = TelemetryEventBuilder::default();
        let buffer = bytes(&[marker, marker]);
        let error = builder
            .try_build(
                MarkerRange {
                    source_id: 12,
                    bytes: &buffer,
                },
                |_| panic!("unexpected output"),
            )
            .unwrap_err();
        assert_eq!(error.source_id, 12);
        assert_eq!(error.byte_offset, marker.encoded_len());
        assert!(matches!(error.kind, BuildErrorKind::DuplicateHalf(_)));
        assert_eq!(builder.open_spans().len(), 1);
    }
    let mut builder = TelemetryEventBuilder::default();
    let mut buffer = bytes(&[enter(1, 1)]);
    buffer.push(255);
    let error = builder
        .try_build(
            MarkerRange {
                source_id: 8,
                bytes: &buffer,
            },
            |_| {},
        )
        .unwrap_err();
    assert_eq!(error.byte_offset, enter(1, 1).encoded_len());
    assert_eq!(
        error.kind,
        BuildErrorKind::Decode(DecodeError::UnknownTag(255))
    );
    let error = builder
        .try_build(
            MarkerRange {
                source_id: 8,
                bytes: &[1],
            },
            |_| {},
        )
        .unwrap_err();
    assert_eq!(error.kind, BuildErrorKind::Decode(DecodeError::Truncated));
    assert_eq!(builder.open_spans().len(), 1);
}

#[test]
fn shuffled_halves_across_many_ranges_leave_only_truly_missing_facts() {
    let mut records = Vec::new();
    for thread in 1..=8 {
        for call in 1..=100 {
            records.push(enter(thread, call));
            records.push(exit(thread, call));
        }
    }
    // Deterministic shuffle, no additional RNG dependency.
    let mut state = 42u64;
    for i in (1..records.len()).rev() {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1);
        let j = usize::try_from(state % (i as u64 + 1)).unwrap();
        records.swap(i, j);
    }
    let mut builder = TelemetryEventBuilder::default();
    let mut keys = std::collections::HashSet::new();
    for (i, chunk) in records.chunks(7).enumerate() {
        for event in push(&mut builder, i as u64 % 3, chunk) {
            let TelemetryEvent::ClosedSpan(span) = event else {
                panic!("expected span");
            };
            assert!(keys.insert(span.key));
        }
    }
    assert_eq!(keys.len(), 800);
    assert!(builder.into_open_spans().is_empty());
}

#[test]
fn generic_stage_shutdown_emits_unmatched_facts_without_fabricating_spans() {
    use btel_core::stage::EventBuilder;
    let mut builder = TelemetryEventBuilder::default();
    push(&mut builder, 1, &[enter(1, 1), exit(2, 1)]);
    let mut events = Vec::new();
    builder.finish(|event| events.push(event));
    assert_eq!(events.len(), 2);
    assert!(
        events
            .iter()
            .all(|event| matches!(event, TelemetryEvent::UnmatchedSpan { .. }))
    );
    assert!(builder.open_spans().is_empty());
    builder.finish(|_| panic!("must not emit twice"));
}
