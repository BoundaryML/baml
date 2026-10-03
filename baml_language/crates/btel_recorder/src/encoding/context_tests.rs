use btel_types::allocate_telemetry_id;

use super::*;
use crate::proto::{self, thread_section::Context};

fn context(value: u64) -> Context {
    Context::ContextCasId(proto::CasId {
        low: value,
        high: value,
    })
}

fn event(writer: &mut EncodedSpans, thread: TelemetryId) {
    writer.reserve(writer.len() + MAX_EVENT_BYTES);
    let capacity = writer.capacity();
    writer.event(
        thread,
        Event::ThreadAnnouncement(proto::ThreadAnnouncement {}),
    );
    assert_eq!(writer.capacity(), capacity);
}

fn sections(writer: &mut EncodedSpans) -> Vec<proto::ThreadSection> {
    proto::RecordingFile::decode(writer.finish().as_slice())
        .unwrap()
        .spans
        .unwrap()
        .sections
}

#[test]
fn context_is_written_once_per_run_and_restoration_starts_a_new_run() {
    let mut writer = EncodedSpans::default();
    let thread = allocate_telemetry_id();
    for value in [1, 1, 2, 2, 1] {
        writer.select_context(thread, Some(context(value)));
        event(&mut writer, thread);
    }
    let runs = sections(&mut writer);
    assert_eq!(runs.len(), 3);
    for (run, (id, count)) in runs.iter().zip([(1, 2), (2, 2), (1, 1)]) {
        assert_eq!(run.thread_id, thread.get());
        assert_eq!(run.context, Some(context(id)));
        assert_eq!(run.events.len(), count);
    }
}

#[test]
fn context_does_not_leak_across_threads_or_unmarked_chunks() {
    let mut writer = EncodedSpans::default();
    let a = allocate_telemetry_id();
    let b = allocate_telemetry_id();
    writer.select_context(a, Some(context(1)));
    event(&mut writer, a);
    event(&mut writer, b);
    event(&mut writer, a);
    writer.select_context(a, Some(context(2)));
    event(&mut writer, a);
    writer.begin_chunk();
    event(&mut writer, a);
    let runs = sections(&mut writer);
    assert_eq!(
        runs.iter().map(|run| run.context).collect::<Vec<_>>(),
        vec![Some(context(1)), None, None, Some(context(2)), None]
    );
}

#[test]
fn every_file_repeats_the_run_context_without_a_dictionary() {
    let mut writer = EncodedSpans::default();
    let thread = allocate_telemetry_id();
    writer.select_context(thread, Some(context(7)));
    for _ in 0..2 {
        event(&mut writer, thread);
        let runs = sections(&mut writer);
        assert_eq!(runs.len(), 1);
        assert_eq!(runs[0].context, Some(context(7)));
    }
    writer.begin_chunk();
    event(&mut writer, thread);
    assert_eq!(sections(&mut writer)[0].context, None);
}

#[test]
fn finishing_resets_file_features_without_losing_context_selection() {
    let mut writer = EncodedSpans::default();
    let thread = allocate_telemetry_id();
    writer.select_context(thread, Some(context(7)));
    writer.event(
        thread,
        Event::Log(proto::LogEvent {
            parent_id: thread.get(),
            function_id: 1,
            level: proto::LogLevel::Info as i32,
            ..Default::default()
        }),
    );
    assert_eq!(
        writer.format_minor(),
        btel_settings::encoding::LOG_FORMAT_MINOR
    );
    assert_eq!(sections(&mut writer)[0].context, Some(context(7)));
    assert_eq!(writer.format_minor(), btel_settings::encoding::FORMAT_MINOR);

    event(&mut writer, thread);
    assert_eq!(
        writer.format_minor(),
        btel_settings::encoding::CONTEXT_FORMAT_MINOR
    );
    assert_eq!(sections(&mut writer)[0].context, Some(context(7)));

    writer.begin_chunk();
    event(&mut writer, thread);
    assert_eq!(writer.format_minor(), btel_settings::encoding::FORMAT_MINOR);
    assert_eq!(sections(&mut writer)[0].context, None);
}

#[test]
fn repeated_chunk_selection_can_coalesce_but_empty_and_unavailable_cannot() {
    let mut writer = EncodedSpans::default();
    let thread = allocate_telemetry_id();
    for _ in 0..2 {
        writer.begin_chunk();
        writer.select_context(thread, Some(context(1)));
        event(&mut writer, thread);
    }
    writer.select_context(thread, Some(Context::EmptyContext(true)));
    event(&mut writer, thread);
    writer.select_context(thread, None);
    event(&mut writer, thread);
    let runs = sections(&mut writer);
    assert_eq!(runs.len(), 3);
    assert_eq!(runs[0].events.len(), 2);
    assert_eq!(runs[1].context, Some(Context::EmptyContext(true)));
    assert_eq!(runs[2].context, None);
}

#[test]
fn context_overhead_is_per_run_not_per_event() {
    let thread = allocate_telemetry_id();
    let encode = |count, context| {
        let mut writer = EncodedSpans::default();
        writer.select_context(thread, context);
        for _ in 0..count {
            event(&mut writer, thread);
        }
        writer.finish().len()
    };
    for count in [1, 100, 1000] {
        let baseline = encode(count, None);
        assert_eq!(encode(count, Some(context(u64::MAX))) - baseline, 20);
        assert_eq!(
            encode(count, Some(Context::EmptyContext(true))) - baseline,
            2
        );
    }
}

#[test]
fn largest_completion_and_context_fit_admitted_capacity() {
    let mut writer = EncodedSpans::default();
    let thread = allocate_telemetry_id();
    writer.select_context(thread, Some(context(u64::MAX)));
    writer.reserve(MAX_EVENT_BYTES);
    let capacity = writer.capacity();
    writer.event(
        thread,
        Event::FunctionCompletion(proto::FunctionCompletion {
            id: u64::MAX,
            parent_id: u64::MAX,
            node: u64::MAX,
            entered_at_ticks: u64::MAX,
            exited_at_ticks: u64::MAX,
            self_await_ticks: u64::MAX,
            completion_flags: u32::MAX,
            panicked: true,
            value_cas_id: Some(proto::CasId {
                low: u64::MAX,
                high: u64::MAX,
            }),
        }),
    );
    assert_eq!(writer.capacity(), capacity);
    let thread_varint = prost::encoding::encoded_len_varint(thread.get());
    assert_eq!(writer.len() + (10 - thread_varint), MAX_EVENT_BYTES);
    assert_eq!(sections(&mut writer)[0].context, Some(context(u64::MAX)));
}

#[test]
fn changing_context_pays_for_a_new_section_not_just_the_hash() {
    let thread = allocate_telemetry_id();
    let mut shared = EncodedSpans::default();
    let mut changing = EncodedSpans::default();
    for index in 0..100 {
        shared.select_context(thread, Some(context(1)));
        changing.select_context(thread, Some(context(index + 1)));
        event(&mut shared, thread);
        event(&mut changing, thread);
    }
    let section_prefix = 1 + LENGTH_BYTES;
    let thread_field = 1 + prost::encoding::encoded_len_varint(thread.get());
    assert_eq!(
        changing.finish().len() - shared.finish().len(),
        99 * (section_prefix + thread_field + 20)
    );
}
