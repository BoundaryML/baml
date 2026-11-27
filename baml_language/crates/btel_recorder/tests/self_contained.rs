//! Every sealed file defines what its records reference, whatever order the
//! records reach the recorder in and however a seal falls between them: a
//! reader of one file alone rejects a file that does not.
use std::sync::Arc;

use btel_clock::{ClockMode, ClockRuntime};
use btel_processor::AggregateDelta;
use btel_recorder::{RecordingBuilder, RecordingConfig, SealedFile, proto};
use btel_records::{ErrorFrame, LogLevel, RaiseKind, RaiseStack, SpanRecord};
use btel_snapshot::Snapshot;
use btel_types::{
    AwaitDuration, CallPathId, CallPathNodeId, ClockDuration, ClockInstant, FunctionIdAllocator,
    InvocationOutcome, TelemetryId, allocate_telemetry_id,
};
use prost::Message;

type Rec = SpanRecord<Snapshot, Snapshot>;

fn builder() -> RecordingBuilder {
    RecordingBuilder::new(btel_types::new_recording_id(), RecordingConfig::default()).unwrap()
}

fn decode(file: &SealedFile) -> proto::RecordingFile {
    proto::RecordingFile::decode(file.bytes()).unwrap()
}

fn unresolved(file: &SealedFile) -> Vec<(&'static str, u64)> {
    btel_core::recording::unresolved_definitions(&decode(file))
}

fn announce(
    id: TelemetryId,
    parent: Option<TelemetryId>,
    spawn: CallPathId,
    clock: &Arc<btel_clock::ClockEpoch>,
) -> Rec {
    SpanRecord::ThreadSpanAnnouncement {
        id,
        parent_id: parent,
        spawn_call_path: spawn,
        started_at: ClockInstant::from_ticks(1),
        clock: Arc::clone(clock),
        name: None,
    }
}

fn path_defined(path: CallPathId, parent: CallPathId, callee: btel_types::FunctionId) -> Rec {
    SpanRecord::CallPathDefined {
        call_path: path,
        parent_call_path: parent,
        visible_caller: None,
        caller_pc: 0,
        callee,
        edge: btel_types::CallPathEdge::Synchronous,
    }
}

fn delta(path: CallPathId) -> AggregateDelta {
    AggregateDelta {
        node: CallPathNodeId::new(path, false),
        count: 1,
        total_duration: ClockDuration::from_ticks(9),
        total_self_await: AwaitDuration::ZERO,
        ..Default::default()
    }
}

/// A future's log arrives before its spawn path is defined: held, then written
/// in a later file. Does that later file define the log's function?
#[test]
fn held_log_names_its_function() {
    let alloc = FunctionIdAllocator::default();
    let (f_path, f_log) = (alloc.allocate().unwrap(), alloc.allocate().unwrap());
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
    let p = CallPathId::new_non_root(70).unwrap();
    let mut builder = builder();
    builder.span_reference(a, &announce(a, None, CallPathId::ROOT, &clock));
    builder.span_reference(b, &announce(b, Some(a), p, &clock));
    builder.span_reference(
        b,
        &SpanRecord::Log {
            parent_id: b,
            function: f_log,
            pc: 1,
            at: ClockInstant::from_ticks(2),
            level: LogLevel::Info,
            event_name: None,
            captured_data: None,
        },
    );
    let first = builder.flush_recording().unwrap().unwrap();
    builder.span_reference(a, &path_defined(p, CallPathId::ROOT, f_path));
    let second = builder.flush_recording().unwrap().unwrap();
    let logs = |f: &SealedFile| {
        decode(f)
            .spans
            .iter()
            .flat_map(|s| &s.sections)
            .flat_map(|s| &s.events)
            .filter(|e| matches!(e.event, Some(proto::span_event::Event::Log(_))))
            .count()
    };
    // The log waited for its thread's spawn path and rides the second file.
    assert_eq!((logs(&first), logs(&second)), (0, 1));
    assert_eq!(unresolved(&first), vec![]);
    assert_eq!(unresolved(&second), vec![], "held log's file");
}

/// A future's raise arrives before its spawn path is defined: held, then
/// written in a later file. Does that later file define the raise's function?
#[test]
fn held_raise_names_its_function() {
    let alloc = FunctionIdAllocator::default();
    let (f_path, f_raise) = (alloc.allocate().unwrap(), alloc.allocate().unwrap());
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (a, b) = (allocate_telemetry_id(), allocate_telemetry_id());
    let p = CallPathId::new_non_root(71).unwrap();
    let mut builder = builder();
    builder.span_reference(a, &announce(a, None, CallPathId::ROOT, &clock));
    builder.span_reference(b, &announce(b, Some(a), p, &clock));
    builder.span_reference(
        b,
        &SpanRecord::ErrorRaised {
            raise_id: allocate_telemetry_id(),
            raised_at: ClockInstant::from_ticks(5),
            kind: RaiseKind::Throw,
            function: Some(f_raise),
            pc: Some(3),
            raise_call: None,
            raise_frame_may_promote: false,
            call_path: CallPathId::ROOT,
            frame_count: 1,
        },
    );
    let first = builder.flush_recording().unwrap().unwrap();
    builder.span_reference(a, &path_defined(p, CallPathId::ROOT, f_path));
    let second = builder.flush_recording().unwrap().unwrap();
    let raises = |f: &SealedFile| decode(f).errors.map_or(0, |e| e.raises.len());
    // The raise waited for its thread's spawn path and rides the second file.
    assert_eq!((raises(&first), raises(&second)), (0, 1));
    assert_eq!(unresolved(&second), vec![], "held raise's file");
}

/// A raise's stack and the raise itself, consecutive records of one defined
/// thread, straddle a seal (no reordering involved).
#[test]
fn raise_stack_and_raise_straddle_a_seal() {
    let alloc = FunctionIdAllocator::default();
    let f_frame = alloc.allocate().unwrap();
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let a = allocate_telemetry_id();
    let raise_id = allocate_telemetry_id();
    let mut builder = builder();
    builder.span_reference(a, &announce(a, None, CallPathId::ROOT, &clock));
    builder.span_reference(
        a,
        &SpanRecord::ErrorRaiseStack(Box::new(RaiseStack {
            raise_id,
            frames: vec![ErrorFrame {
                function: Some(f_frame),
                pc: Some(1),
                native: false,
            }],
            inherited: Vec::new(),
            inherited_count: 0,
        })),
    );
    let first = builder.flush_recording().unwrap().unwrap();
    builder.span_reference(
        a,
        &SpanRecord::ErrorRaised {
            raise_id,
            raised_at: ClockInstant::from_ticks(5),
            kind: RaiseKind::Throw,
            function: None,
            pc: None,
            raise_call: None,
            raise_frame_may_promote: false,
            call_path: CallPathId::ROOT,
            frame_count: 1,
        },
    );
    let second = builder.flush_recording().unwrap().unwrap();
    let frames = |f: &SealedFile| {
        decode(f).errors.map_or(0, |e| {
            e.raises.iter().map(|r| r.frames.len()).sum::<usize>()
        })
    };
    assert_eq!(unresolved(&first), vec![]);
    assert_eq!(frames(&second), 1);
    assert_eq!(unresolved(&second), vec![], "raise file");
}

fn spawner_forgotten(define_spawn_path_first: bool) -> Vec<(&'static str, u64)> {
    let alloc = FunctionIdAllocator::default();
    let callee = alloc.allocate().unwrap();
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (spawner, future) = (allocate_telemetry_id(), allocate_telemetry_id());
    let spawn = CallPathId::new_non_root(80).unwrap();
    let path = CallPathId::new_non_root(81).unwrap();
    let mut builder = builder();
    builder.span_reference(spawner, &announce(spawner, None, CallPathId::ROOT, &clock));
    if define_spawn_path_first {
        builder.span_reference(spawner, &path_defined(spawn, CallPathId::ROOT, callee));
        builder.span_reference(future, &announce(future, Some(spawner), spawn, &clock));
    } else {
        // The future's definition reaches the recorder before its spawn path's.
        builder.span_reference(future, &announce(future, Some(spawner), spawn, &clock));
        builder.span_reference(spawner, &path_defined(spawn, CallPathId::ROOT, callee));
    }
    builder.span_reference(future, &path_defined(path, spawn, callee));
    builder.aggregate(delta(path));
    let f1 = builder.flush_recording().unwrap().unwrap();
    assert_eq!(unresolved(&f1), vec![]);
    // The spawner finishes; the future keeps running.
    builder.span_reference(
        spawner,
        &SpanRecord::ThreadSpanCompletion {
            id: spawner,
            parent_id: None,
            spawn_call_path: CallPathId::ROOT,
            started_at: ClockInstant::from_ticks(1),
            completed_at: ClockInstant::from_ticks(2),
            outcome: InvocationOutcome::Ok,
            clock: Arc::clone(&clock),
            name: None,
        },
    );
    builder.aggregate(delta(path));
    let f2 = builder.flush_recording().unwrap().unwrap();
    assert_eq!(unresolved(&f2), vec![]);
    // Past the grace (two cache-flush intervals).
    std::thread::sleep(std::time::Duration::from_millis(2200));
    builder.aggregate(delta(path));
    let f3 = builder.flush_recording().unwrap().unwrap();
    assert_eq!(unresolved(&f3), vec![]);
    // The still-running future's call path is timed again.
    builder.aggregate(delta(path));
    let f4 = builder.flush_recording().unwrap().unwrap();
    unresolved(&f4)
}

#[test]
fn spawner_forgotten_control() {
    assert_eq!(spawner_forgotten(true), vec![]);
}

#[test]
fn spawner_forgotten_after_out_of_order_define() {
    assert_eq!(spawner_forgotten(false), vec![]);
}

fn held_thread_completion(wait: bool) -> (usize, u64) {
    let alloc = FunctionIdAllocator::default();
    let callee = alloc.allocate().unwrap();
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (spawner, future) = (allocate_telemetry_id(), allocate_telemetry_id());
    let spawn = CallPathId::new_non_root(90).unwrap();
    let mut builder = builder();
    builder.span_reference(spawner, &announce(spawner, None, CallPathId::ROOT, &clock));
    builder.span_reference(future, &announce(future, Some(spawner), spawn, &clock));
    builder.span_reference(
        future,
        &SpanRecord::ThreadSpanCompletion {
            id: future,
            parent_id: Some(spawner),
            spawn_call_path: spawn,
            started_at: ClockInstant::from_ticks(1),
            completed_at: ClockInstant::from_ticks(2),
            outcome: InvocationOutcome::Ok,
            clock: Arc::clone(&clock),
            name: None,
        },
    );
    let mut files = vec![builder.flush_recording().unwrap().unwrap()];
    if wait {
        std::thread::sleep(std::time::Duration::from_millis(2200));
        files.push(builder.flush_recording().unwrap().unwrap());
    }
    // The spawn path's definition arrives, within the four seals spawner record waits.
    builder.span_reference(spawner, &path_defined(spawn, CallPathId::ROOT, callee));
    for _ in 0..5 {
        if let Some(file) = builder.flush_recording().unwrap() {
            files.push(file);
        }
    }
    let completions = files
        .iter()
        .map(decode)
        .flat_map(|callee| callee.spans.into_iter().flat_map(|s| s.sections))
        .flat_map(|s| s.events)
        .filter(|e| matches!(e.event, Some(proto::span_event::Event::ThreadCompletion(_))))
        .count();
    let dropped = files
        .iter()
        .map(decode)
        .map(|callee| callee.spans.map_or(0, |s| s.dropped_spans))
        .sum();
    for callee in &files {
        assert_eq!(unresolved(callee), vec![]);
    }
    (completions, dropped)
}

#[test]
fn held_thread_completion_control() {
    assert_eq!(held_thread_completion(false), (1, 0));
}

#[test]
fn held_thread_completion_outlives_grace() {
    assert_eq!(held_thread_completion(true), (1, 0));
}

fn log(thread: TelemetryId, function: btel_types::FunctionId) -> Rec {
    SpanRecord::Log {
        parent_id: thread,
        function,
        pc: 1,
        at: ClockInstant::from_ticks(2),
        level: LogLevel::Info,
        event_name: None,
        captured_data: None,
    }
}

fn completion(thread: TelemetryId, spawn: CallPathId, clock: &Arc<btel_clock::ClockEpoch>) -> Rec {
    SpanRecord::ThreadSpanCompletion {
        id: thread,
        parent_id: None,
        spawn_call_path: spawn,
        started_at: ClockInstant::from_ticks(1),
        completed_at: ClockInstant::from_ticks(2),
        outcome: InvocationOutcome::Ok,
        clock: Arc::clone(clock),
        name: None,
    }
}

/// Thread X's call path arrives `gap` seals before X's definition, and a
/// child spawned at that path keeps X's placeholder. Once X is defined and the
/// child is forgotten, X still runs: its next log and timing are written.
fn late_thread_outlives_its_placeholder(gap: usize) -> (usize, u64, u64) {
    let alloc = FunctionIdAllocator::default();
    let (callee, logger) = (alloc.allocate().unwrap(), alloc.allocate().unwrap());
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (root, late, child) = (
        allocate_telemetry_id(),
        allocate_telemetry_id(),
        allocate_telemetry_id(),
    );
    let path = CallPathId::new_non_root(95).unwrap();
    let mut builder = builder();
    builder.span_reference(root, &announce(root, None, CallPathId::ROOT, &clock));
    // X's path, then a child spawned at it, before X itself is defined.
    builder.span_reference(late, &path_defined(path, CallPathId::ROOT, callee));
    builder.span_reference(child, &announce(child, None, path, &clock));
    let mut files = Vec::new();
    for _ in 0..gap {
        builder.span_reference(root, &log(root, logger));
        files.push(builder.flush_recording().unwrap().unwrap());
    }
    builder.span_reference(late, &announce(late, None, CallPathId::ROOT, &clock));
    // The child completes and, past its grace, is forgotten.
    builder.span_reference(child, &completion(child, path, &clock));
    files.push(builder.flush_recording().unwrap().unwrap());
    std::thread::sleep(std::time::Duration::from_millis(2200));
    for _ in 0..6 {
        builder.span_reference(root, &log(root, logger));
        files.push(builder.flush_recording().unwrap().unwrap());
    }
    // X runs on.
    builder.span_reference(late, &log(late, logger));
    builder.aggregate(delta(path));
    files.push(builder.flush_recording().unwrap().unwrap());
    for file in &files {
        assert_eq!(unresolved(file), vec![]);
    }
    let last = decode(files.last().unwrap());
    let logs = last
        .spans
        .iter()
        .flat_map(|spans| &spans.sections)
        .filter(|section| section.thread_id == late.get())
        .flat_map(|section| &section.events)
        .filter(|event| matches!(event.event, Some(proto::span_event::Event::Log(_))))
        .count();
    let dropped_spans: u64 = files
        .iter()
        .map(|file| decode(file).spans.map_or(0, |spans| spans.dropped_spans))
        .sum();
    let dropped_invocations: u64 = files
        .iter()
        .map(|file| decode(file).aggregates.map_or(0, |a| a.dropped_invocations))
        .sum();
    (logs, dropped_spans, dropped_invocations)
}

#[test]
fn a_thread_defined_one_seal_after_its_path_outlives_its_child() {
    assert_eq!(late_thread_outlives_its_placeholder(1), (1, 0, 0));
}

#[test]
fn a_thread_defined_six_seals_after_its_path_outlives_its_child() {
    assert_eq!(late_thread_outlives_its_placeholder(6), (1, 0, 0));
}

/// A thread defined well after its path, with nothing keeping the
/// placeholder: the path comes back with the thread.
#[test]
fn a_thread_defined_after_its_placeholder_went_gets_its_paths_back() {
    let alloc = FunctionIdAllocator::default();
    let (callee, logger) = (alloc.allocate().unwrap(), alloc.allocate().unwrap());
    let clock = ClockRuntime::new(ClockMode::Monotonic).start_run();
    let (root, late) = (allocate_telemetry_id(), allocate_telemetry_id());
    let path = CallPathId::new_non_root(96).unwrap();
    let mut builder = builder();
    builder.span_reference(root, &announce(root, None, CallPathId::ROOT, &clock));
    builder.span_reference(late, &path_defined(path, CallPathId::ROOT, callee));
    let mut files = Vec::new();
    for _ in 0..6 {
        builder.span_reference(root, &log(root, logger));
        files.push(builder.flush_recording().unwrap().unwrap());
    }
    builder.span_reference(late, &announce(late, None, CallPathId::ROOT, &clock));
    builder.aggregate(delta(path));
    files.push(builder.flush_recording().unwrap().unwrap());
    let last = decode(files.last().unwrap());
    let aggregates = last.aggregates.unwrap_or_default();
    assert_eq!(
        (aggregates.entries.len(), aggregates.dropped_invocations),
        (1, 0)
    );
    for file in &files {
        assert_eq!(unresolved(file), vec![]);
    }
}
