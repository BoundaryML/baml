//! Real VM → chunks → aggregation/encoding → bounded disk delivery → directory reader.
#![cfg(not(target_arch = "wasm32"))]
use std::{sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::{RecordingConfig, proto};
use sys_native::SysOpsExt;

fn context() -> bex_engine::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}
fn engine(program: bex_vm_types::Program, recording: TelemetryRecording) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            recording,
        )
        .unwrap(),
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_recording_is_readable_while_running_and_shutdown_finishes_disk_delivery() {
    let root = tempfile::tempdir().unwrap();
    let mut program = baml_test_support::compile_source(
        r#"
        function leaf() -> int { 7 }
        function main() -> int { let child = spawn { leaf() }; leaf() + (await child) }
    "#,
    );
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object {
            if f.name().rsplit('.').next() == Some("leaf") {
                f.body_meta = Some(bex_vm_types::FunctionMeta::Llm);
            }
        }
    }
    let recording = TelemetryRecording::local_files(
        root.path(),
        RecordingConfig {
            flush_interval_duration: Duration::from_millis(10),
            ..RecordingConfig::default()
        },
    );
    let id = recording.id();
    let engine = engine(program, recording);
    let directory = engine.telemetry_recording_directory().unwrap().to_owned();
    assert_eq!(
        directory,
        btel_file::recording_directory(&root.path().join(".baml/btel/recordings"), id)
    );
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(14)
    );
    let prefix = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            let read = btel_file::read_directory(&directory).unwrap();
            if read
                .files
                .iter()
                .any(|f| f.aggregates.as_ref().is_some_and(|a| !a.entries.is_empty()))
            {
                break read;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("idle flush must reach disk without another invocation");
    let first_bytes = std::fs::read(directory.join("00000000000000000001.btel")).unwrap();
    assert!(!prefix.has_recording_end);
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(engine.shutdown(), engine.shutdown());
    })
    .await
    .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    assert_eq!(
        std::fs::read(directory.join("00000000000000000001.btel")).unwrap(),
        first_bytes
    );
    let read = btel_file::read_directory(&directory).unwrap();
    assert!(read.issues.is_empty(), "{:?}", read.issues);
    assert!(read.files.len() > prefix.files.len());
    assert!(
        read.has_recording_end,
        "normal shutdown ends the recording once every run settled"
    );
    assert!(read.files.last().unwrap().end.is_some());
    let defined: std::collections::BTreeSet<u64> = read
        .files
        .iter()
        .flat_map(|file| &file.definitions.as_ref().unwrap().clock_epochs)
        .map(|epoch| epoch.epoch_id)
        .collect();
    let finals: std::collections::BTreeSet<u64> = read
        .files
        .iter()
        .flat_map(|file| &file.clock_states.as_ref().unwrap().states)
        .filter(|state| state.r#final)
        .map(|state| state.epoch_id)
        .collect();
    assert_eq!(defined, finals, "every recorded run's clock is final");
    let (mut announcements, mut spans, mut threads, mut aggregate_count, mut clocks) =
        (0, 0, 0, 0, 0);
    let mut span_counts = std::collections::HashMap::<u64, u64>::new();
    let mut aggregates = std::collections::HashMap::<u64, u64>::new();
    for file in &read.files {
        let defs = file.definitions.as_ref().unwrap();
        clocks += defs.clock_epochs.len();
        for epoch in &defs.clock_epochs {
            assert!(epoch.multiplier > 0);
            assert!(epoch.shift < 128);
        }
        for row in &file.aggregates.as_ref().unwrap().entries {
            aggregate_count += row.count;
            *aggregates.entry(row.node).or_default() += row.count;
        }
        for section in &file.spans.as_ref().unwrap().sections {
            for event in &section.events {
                match event.event.as_ref().unwrap() {
                    proto::span_event::Event::FunctionAnnouncement(a) => {
                        assert_cas_blob(root.path(), a.inputs_cas_id.unwrap());
                        announcements += 1;
                    }
                    proto::span_event::Event::FunctionCompletion(c) => {
                        assert_cas_blob(root.path(), c.value_cas_id.unwrap());
                        spans += 1;
                        *span_counts.entry(c.node).or_default() += 1;
                    }
                    proto::span_event::Event::ThreadCompletion(_) => threads += 1,
                    _ => {}
                }
            }
        }
    }
    assert_eq!((announcements, spans), (4, 4));
    assert!(threads >= 4 && aggregate_count > 4 && clocks > 0);
    for (node, count) in span_counts {
        assert_eq!(aggregates[&node], count);
    }
    engine.shutdown().await;
    assert_eq!(
        btel_file::read_directory(&directory).unwrap().files.len(),
        read.files.len()
    );
}

#[tokio::test]
async fn shutdown_reports_disk_errors_even_without_another_invocation() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(
        baml_test_support::compile_source("function main() -> int { 1 }"),
        TelemetryRecording::local_files_in(
            root.path(),
            RecordingConfig {
                flush_interval_duration: Duration::from_secs(3600),
                ..RecordingConfig::default()
            },
        ),
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    let directory = engine.telemetry_recording_directory().unwrap();
    std::fs::remove_dir(directory).unwrap();
    std::fs::write(directory, b"not a directory").unwrap();
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    let error = engine.telemetry_result().unwrap().unwrap_err();
    assert!(error.to_string().contains("telemetry file"));
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Err(error)));
}

/// Every reading is reported, and a file of rows is one JSON object per line.
#[cfg(feature = "telemetry_diagnostics")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn diagnostics_report_every_pool_and_append_one_line_per_engine() {
    let root = tempfile::tempdir().unwrap();
    let engine = engine(
        baml_test_support::compile_source("function main() -> int { 1 }"),
        TelemetryRecording::local_files_in(root.path(), RecordingConfig::default()),
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    let diagnostics = engine.telemetry_diagnostics().unwrap();
    assert_eq!(diagnostics["pid"], u64::from(std::process::id()));
    assert!(diagnostics["recording_id"].is_string());
    assert_eq!(diagnostics["delivery"]["kind"], "local");
    assert!(diagnostics["captures"]["budget_bytes"].as_u64().unwrap() > 0);
    assert!(diagnostics["chunks"]["sealed_records"].as_u64().unwrap() > 0);
    assert_eq!(diagnostics["chunks"]["overage_allocations"], 0);
    assert_eq!(diagnostics["overage_warned"], false);
    let file = root.path().join("diagnostics.jsonl");
    bex_engine::append_diagnostics_line(&file, &diagnostics).unwrap();
    engine.shutdown().await;
    bex_engine::append_diagnostics_line(&file, &engine.telemetry_diagnostics().unwrap()).unwrap();
    let rows: Vec<serde_json::Value> = std::fs::read_to_string(&file)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0], diagnostics);
    // After shutdown nothing is held, and the counts only grew: shutdown
    // seals the records that end the recording.
    assert_eq!(rows[1]["chunks"]["active_producers"], 0);
    assert_eq!(rows[1]["captures"]["in_use"], 0);
    assert!(
        rows[1]["chunks"]["sealed_records"].as_u64()
            >= rows[0]["chunks"]["sealed_records"].as_u64()
    );
}

#[test]
fn off_does_not_create_local_files() {
    const CHILD: &str = "BAML_FILE_SINK_OFF_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "off_does_not_create_local_files", "--nocapture"])
            .env(CHILD, "1")
            .env("BAML_TELEMETRY", "off")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("must-not-exist");
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        let engine = engine(
            baml_test_support::compile_source("function main() -> int { 1 }"),
            TelemetryRecording::local_files_in(&destination, RecordingConfig::default()),
        );
        assert!(engine.telemetry_recording_directory().is_none());
        assert_eq!(
            engine
                .call_function("main", vec![], context(), true)
                .await
                .unwrap(),
            BexExternalValue::Int(1)
        );
        engine.shutdown().await;
        assert!(engine.telemetry_result().is_none());
    });
    assert!(!destination.exists());
}

#[tokio::test]
async fn storage_startup_failure_preserves_status_without_preventing_execution() {
    let root = tempfile::tempdir().unwrap();
    let destination = root.path().join("file-not-directory");
    std::fs::write(&destination, b"occupied").unwrap();
    let engine = engine(
        baml_test_support::compile_source(
            "function main() -> int { let child = spawn { 7 }; await child }",
        ),
        TelemetryRecording::local_files_in(destination, RecordingConfig::default()),
    );
    let failure = engine.telemetry_result().unwrap().unwrap_err();
    assert!(failure.to_string().contains("telemetry recording startup"));
    assert!(engine.telemetry_recording_directory().is_none());
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Err(failure)));
}

#[tokio::test]
async fn cas_storage_failure_disables_recording_but_execution_continues() {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join(".baml/btel")).unwrap();
    std::fs::write(root.path().join(".baml/btel/cas"), b"not a directory").unwrap();
    let mut program = baml_test_support::compile_source(
        "function leaf() -> int { 7 } function main() -> int { leaf() }",
    );
    for object in &mut program.objects.0 {
        if let bex_vm_types::Object::Function(f) = object {
            if f.name().rsplit('.').next() == Some("leaf") {
                f.body_meta = Some(bex_vm_types::FunctionMeta::Llm);
            }
        }
    }
    let engine = engine(
        program,
        TelemetryRecording::local_files(root.path(), RecordingConfig::default()),
    );
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    tokio::time::timeout(Duration::from_secs(5), async {
        while engine.telemetry_result().is_none_or(|r| r.is_ok()) {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(7)
    );
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    assert!(engine.telemetry_result().unwrap().is_err());
}

#[tokio::test]
async fn telemetry_disabled_inside_an_owned_span_does_not_stop_execution() {
    let root = tempfile::tempdir().unwrap();
    let program = baml_test_support::compile_source(
        r#"
        function main() -> int {
            let span = trace.start_span("operation");
            let total = span.run(() -> int {
                let total = 0;
                let i = 0;
                while (i < 100) {
                    baml.sys.sleep(baml.time.Duration.from_milliseconds(10n));
                    let child = spawn { i };
                    total += await child;
                    i += 1;
                }
                total
            });
            span.finish(total);
            total
        }
        "#,
    );
    let engine = engine(
        program,
        TelemetryRecording::local_files(
            root.path(),
            RecordingConfig {
                flush_interval_duration: Duration::from_millis(1),
                ..RecordingConfig::default()
            },
        ),
    );
    // Once the body is awaiting inside the owned span, the recording's
    // directory becomes a file: the next write fails and disables it.
    let recordings = root.path().join(".baml/btel/recordings");
    let breaker = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let dir = std::fs::read_dir(&recordings)
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        // The writer is still adding files: a removal can find the directory
        // refilled (DirectoryNotEmpty), and a replacement can find it
        // recreated. Keep at it until the replacement is in place. A rename
        // can't stand in, since Windows refuses one while files are open.
        let mut attempts = 0;
        loop {
            let broken = std::fs::remove_dir_all(&dir)
                .and_then(|()| std::fs::write(&dir, b"not a directory"))
                .is_ok()
                && dir.is_file();
            if broken {
                break;
            }
            attempts += 1;
            assert!(
                attempts < 1000,
                "the recording directory could not be broken"
            );
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });
    assert_eq!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .unwrap(),
        BexExternalValue::Int(4950)
    );
    breaker.await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    assert!(engine.telemetry_result().unwrap().is_err());
}

fn assert_cas_blob(project: &std::path::Path, id: proto::CasId) {
    let id = btel_snapshot::CasId::from(id);
    let path = btel_file::cas_path(&project.join(".baml/btel/cas"), id);
    let blob =
        std::fs::read(path).expect("every published CAS reference must have a completed blob");
    assert!(blob.len() >= 38);
    assert_eq!(&blob[..8], b"BTELCAS\0");
    assert_eq!(&blob[12..28], id.as_bytes());
}
