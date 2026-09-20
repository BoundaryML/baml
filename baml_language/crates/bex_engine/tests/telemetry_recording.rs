//! Exercise real VM → chunks → processor → encoded files, without a sink.
#![cfg(not(target_arch = "wasm32"))]
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_publisher::{CompletionFlags, RecordingConfig, SealedFile, proto};
use prost::Message;
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn execution_reaches_encoded_files_and_shutdown_drains_once() {
    let mut program = baml_db::testing::compile_source(
        r#"
        function leaf() -> int { 7 }
        function main() -> int {
            let child = spawn { leaf() };
            leaf() + (await child)
        }
    "#,
    );
    // Exercise the existing AI full-capture defaults without network I/O or
    // exposing a policy setter solely for this test. Executable code is unchanged.
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(function) = object {
            if function.name.rsplit('.').next() == Some("leaf") {
                function.body_meta = Some(bex_vm_types::FunctionMeta::Llm {
                    client: "test".into(),
                });
            }
        }
    }
    let source = program.source_content_hash;
    let files = Arc::new(Mutex::new(Vec::<SealedFile>::new()));
    let received = Arc::clone(&files);
    let recording = TelemetryRecording::new(
        RecordingConfig {
            flush_interval: Duration::from_secs(3600),
            ..RecordingConfig::default()
        },
        move |file| received.lock().unwrap().push(file),
    );
    let id = recording.id();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    assert_eq!(engine.telemetry_recording_id(), id);
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(14)
    );
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(engine.shutdown(), engine.shutdown());
    })
    .await
    .expect("shutdown must drain without a heap-permit deadlock");
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let count = files.lock().unwrap().len();
    engine.shutdown().await;
    assert_eq!(
        files.lock().unwrap().len(),
        count,
        "shutdown must not replay output"
    );
    assert!(count > 0);
    let mut aggregates = HashMap::<u64, u64>::new();
    let mut completions = HashMap::<u64, u64>::new();
    let mut announcements = 0;
    let mut threads = HashMap::new();
    for (index, sealed) in files.lock().unwrap().iter().enumerate() {
        let file = proto::RecordingFile::decode(sealed.bytes()).unwrap();
        assert_eq!(file.sequence, index as u64 + 1);
        let header = file.header.unwrap();
        assert_eq!(header.recording_id, id.as_bytes());
        assert_eq!(header.source_snapshot_id, source.map(|s| s.to_vec()));
        // Draining is not a claim of settled epochs or durable sink delivery.
        assert!(file.end.is_none());
        for row in file.aggregates.unwrap().entries {
            *aggregates.entry(row.node).or_default() += row.count;
        }
        for section in file.spans.unwrap().sections {
            for event in section.events {
                match event.event.unwrap() {
                    proto::span_event::Event::ThreadCompletion(done) => {
                        assert_eq!(done.outcome, proto::InvocationOutcome::Ok as i32);
                        assert!(threads.insert(section.thread_id, done).is_none());
                    }
                    proto::span_event::Event::FunctionAnnouncement(entry) => {
                        assert_eq!(entry.inputs, proto::CaptureState::Deferred as i32);
                        announcements += 1;
                    }
                    proto::span_event::Event::FunctionCompletion(done) => {
                        let flags =
                            CompletionFlags::from_wire(done.completion_flags, false).unwrap();
                        assert!(flags.capture_deferred());
                        assert!(flags.requires_announcement());
                        *completions.entry(done.node).or_default() += 1;
                    }
                    _ => {}
                }
            }
        }
    }
    assert!(threads.len() >= 2, "root and child must both finish");
    assert_eq!(announcements, 2);
    assert_eq!(completions.values().sum::<u64>(), 2);
    for (node, count) in completions {
        assert_eq!(aggregates[&node], count, "span must aggregate exactly once");
    }
    assert!(
        aggregates.values().sum::<u64>() > 2,
        "timing-only functions also aggregate"
    );
}

#[tokio::test]
async fn idle_deadline_delivers_without_another_vm_call() {
    let program = baml_db::testing::compile_source("function main() -> int { 1 }");
    let (send, mut receive) = tokio::sync::mpsc::unbounded_channel();
    let recording = TelemetryRecording::new(
        RecordingConfig {
            flush_interval: Duration::from_millis(10),
            ..RecordingConfig::default()
        },
        move |file| send.send(file).unwrap(),
    );
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let sealed = receive.recv().await.unwrap();
            let file = proto::RecordingFile::decode(sealed.bytes()).unwrap();
            if file
                .aggregates
                .is_some_and(|batch| !batch.entries.is_empty())
            {
                break;
            }
        }
    })
    .await
    .expect("idle processor must flush on its deadline");
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
}

#[tokio::test]
async fn delivery_failure_is_retained_after_shutdown() {
    let program = baml_db::testing::compile_source("function main() -> int { 1 }");
    let recording = TelemetryRecording::new(
        RecordingConfig {
            flush_interval: Duration::from_secs(3600),
            ..RecordingConfig::default()
        },
        |_| panic!("test delivery failed"),
    );
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    engine.shutdown().await;
    let failure = engine.telemetry_result().unwrap().unwrap_err();
    assert!(failure.to_string().contains("test delivery failed"));
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Err(failure)));
}

#[tokio::test]
async fn cancelling_shutdown_does_not_reopen_a_closed_transport() {
    let program = baml_db::testing::compile_source("function main() -> int { 1 }");
    let (entered, entered_rx) = tokio::sync::oneshot::channel();
    let mut entered = Some(entered);
    let (release, released) = std::sync::mpsc::channel();
    let recording = TelemetryRecording::new(
        RecordingConfig {
            flush_interval: Duration::from_secs(3600),
            ..RecordingConfig::default()
        },
        move |_| {
            if let Some(entered) = entered.take() {
                entered.send(()).unwrap();
                // Deliberately stop delivery to cancel shutdown at the join boundary.
                released.recv_timeout(Duration::from_secs(10)).unwrap();
            }
        },
    );
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    let shutting_down = Arc::clone(&engine);
    let task = tokio::spawn(async move {
        shutting_down.shutdown().await;
    });
    tokio::time::timeout(Duration::from_secs(5), entered_rx)
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    // A subsequent call must reject normally, rather than reach a closed pool.
    assert!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .is_err()
    );
}
