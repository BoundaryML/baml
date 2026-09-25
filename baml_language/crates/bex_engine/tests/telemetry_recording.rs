//! Exercise real VM → chunks → processor → encoded files, through the sole local-file sink.
#![cfg(not(target_arch = "wasm32"))]
use std::{collections::HashMap, sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::{CompletionFlags, RecordingConfig, proto};
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

/// Every entry of a recording directory with its bytes, sorted by name:
/// completed files, and any unfinished `.part` file a writer left behind.
fn recording_bytes(directory: &std::path::Path) -> Vec<(std::ffi::OsString, Vec<u8>)> {
    let mut entries: Vec<_> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), std::fs::read(entry.path()).unwrap())
        })
        .collect();
    entries.sort();
    entries
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
    let root = tempfile::tempdir().unwrap();
    let recording = TelemetryRecording::local_files_in(
        root.path(),
        RecordingConfig {
            flush_interval_duration: Duration::from_secs(3600),
            ..RecordingConfig::default()
        },
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
    assert_eq!(engine.telemetry_recording_id(), Some(id));
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
    let directory = engine.telemetry_recording_directory().unwrap();
    let read = btel_file::read_directory(directory).unwrap();
    // No gap, unfinished file, invalid file or file after the end.
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    let written = recording_bytes(directory);
    // A later shutdown writes, replays and rewrites nothing, end included.
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    assert_eq!(
        recording_bytes(directory),
        written,
        "shutdown must not replay or rewrite output"
    );
    let files = read.files;
    let count = files.len();
    assert!(count > 0);
    // Both threads finished before shutdown, so the run settled: the
    // concurrent shutdowns end the recording once, in its last file, whose
    // sequence declares how many files there are.
    let ends: Vec<u64> = files
        .iter()
        .filter(|file| file.end.is_some())
        .map(|file| file.sequence)
        .collect();
    assert_eq!(
        ends,
        [count as u64],
        "exactly one end marker, on the final file"
    );
    assert!(read.has_recording_end);
    // The end is only written once every recorded clock is final.
    let defined: std::collections::BTreeSet<u64> = files
        .iter()
        .flat_map(|file| &file.definitions.as_ref().unwrap().clock_epochs)
        .map(|epoch| epoch.epoch_id)
        .collect();
    let finals: std::collections::BTreeSet<u64> = files
        .iter()
        .flat_map(|file| &file.clock_states.as_ref().unwrap().states)
        .filter(|state| state.r#final)
        .map(|state| state.epoch_id)
        .collect();
    assert!(!defined.is_empty());
    assert_eq!(defined, finals, "every recorded run's clock is final");
    let mut aggregates = HashMap::<u64, u64>::new();
    let mut completions = HashMap::<u64, u64>::new();
    let mut announcements = 0;
    let mut threads = HashMap::new();
    for (index, file) in files.into_iter().enumerate() {
        assert_eq!(file.sequence, index as u64 + 1);
        let header = file.header.unwrap();
        assert_eq!(header.recording_id, id.as_bytes());
        assert_eq!(header.source_snapshot_id, source.map(|s| s.to_vec()));
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
                        assert!(entry.inputs_cas_id.is_some());
                        announcements += 1;
                    }
                    proto::span_event::Event::FunctionCompletion(done) => {
                        let flags =
                            CompletionFlags::from_wire(done.completion_flags, false).unwrap();
                        assert!(done.value_cas_id.is_some());
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

#[test]
fn telemetry_environment_modes() {
    const CHILD: &str = "BAML_TEST_TELEMETRY_MODE_CHILD";
    if std::env::var_os(CHILD).is_none() {
        for mode in [
            None,
            Some("off"),
            Some("low"),
            Some("medium"),
            Some("high"),
            Some("auto"),
            Some("invalid"),
        ] {
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            command
                .args(["--exact", "telemetry_environment_modes", "--nocapture"])
                .env(CHILD, "1");
            if let Some(mode) = mode {
                command.env("BAML_TELEMETRY", mode);
            } else {
                command.env_remove("BAML_TELEMETRY");
            }
            let result = command.output().unwrap();
            assert!(
                result.status.success(),
                "mode {mode:?}:\n{}\n{}",
                String::from_utf8_lossy(&result.stdout),
                String::from_utf8_lossy(&result.stderr)
            );
        }
        return;
    }
    let mode = std::env::var("BAML_TELEMETRY").unwrap_or_else(|_| "medium".into());
    tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build().unwrap().block_on(async {
        let program = baml_db::testing::compile_source(r#"
            function leaf() -> int { "abc".length() }
            function main() -> int { let child = spawn { leaf() }; leaf() + (await child) }
            function fail() -> int { baml.sys.exit(7); 1 }
        "#);
        let root = tempfile::tempdir().unwrap();
        let result = BexEngine::new_with_telemetry_recording(program,
            Arc::new(sys_native::SysOps::native()), vec![], None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(root.path(), RecordingConfig::default()));
        if mode == "invalid" || mode == "auto" {
            assert!(matches!(result, Err(bex_engine::EngineError::Other(message)) if message.contains("BAML_TELEMETRY")));
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
            return;
        }
        let engine = Arc::new(result.unwrap());
        assert_eq!(engine.telemetry_recording_id().is_none(), mode == "off");
        assert_eq!(engine.call_function("main", vec![], context(), true).await.unwrap(), BexExternalValue::Int(6));
        assert!(matches!(engine.call_function("fail", vec![], context(), true).await,
            Err(bex_engine::EngineError::Exit { code: 7 })));
        engine.shutdown().await;
        if mode == "off" {
            assert_eq!(engine.telemetry_result(), None);
            assert!(engine.reset_telemetry_clock_after_restore().is_none());
            assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
            return;
        }
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
        let (mut aggregates, mut spans, mut announcements, mut threads) = (0, 0, 0, 0);
        for file in btel_file::read_directory(engine.telemetry_recording_directory().unwrap()).unwrap().files {
            if let Some(batch) = file.aggregates {
                aggregates += batch.entries.iter().map(|row| row.count).sum::<u64>();
            }
            for section in file.spans.unwrap().sections {
                for event in section.events {
                    match event.event.unwrap() {
                        proto::span_event::Event::FunctionCompletion(done) => {
                            assert!(done.value_cas_id.is_none());
                            spans += 1;
                        }
                        proto::span_event::Event::FunctionAnnouncement(entry) => {
                            assert!(entry.inputs_cas_id.is_none());
                            announcements += 1;
                        }
                        proto::span_event::Event::ThreadCompletion(_) => threads += 1,
                        _ => {}
                    }
                }
            }
        }
        assert!(threads >= 3, "root, child, and failing root");
        if mode == "low" { assert_eq!((aggregates, spans, announcements), (0, 0, 0)); }
        else if mode == "high" {
            assert!(spans > 0);
            assert_eq!(aggregates, spans);
            assert_eq!(announcements, spans);
        } else {
            assert!(aggregates > 0);
            assert_eq!((spans, announcements), (0, 0));
        }
    });
}
