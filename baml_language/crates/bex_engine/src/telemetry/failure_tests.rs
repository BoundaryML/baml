use std::{num::NonZeroUsize, sync::mpsc, time::Duration};

use sys_native::SysOpsExt;

use super::*;
use crate::{BexExternalValue, FunctionCallContextBuilder};

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

/// An engine whose telemetry recording files go through `write`. The fake
/// covers recording files only: telemetry's blob store still writes real files
/// under `directory`, so it must be a folder outside the source tree that
/// outlives the engine's shutdown, such as a temp dir declared before it.
fn engine_with_writer(
    directory: &Path,
    source: &str,
    write: impl FnMut(&SealedFile) -> Result<(), btel_file::LocalDeliveryError> + Send + 'static,
) -> Arc<BexEngine> {
    let mut engine = BexEngine::new(
        baml_test_support::compile_source(source),
        Arc::new(sys_native::SysOps::native()),
        vec![],
    )
    .unwrap();
    let telemetry = engine.telemetry.as_mut().unwrap();
    telemetry.runtime.finish().unwrap();
    let id = RecordingId::generate();
    let mut delivery = None;
    telemetry.runtime = btel_processor::TelemetryRuntime::with_publisher_factory(
        btel_settings::transport::ChunkConfig {
            chunk_capacity: nz(8),
            timing_chunks: nz(2),
            span_chunks: nz(2),
            max_producers: nz(2),
            preallocate: true,
        },
        |control| {
            let failure = control;
            let writer = btel_file::LocalDelivery::with_test_writer(
                directory.to_path_buf(),
                btel_file::LocalDeliveryConfig::default(),
                write,
                move |error| failure.disable(btel_processor::RuntimeError(error.to_string())),
            )?;
            let builder = RecordingBuilder::new(
                id,
                RecordingConfig {
                    target_bytes: nz(1),
                    ..RecordingConfig::default()
                },
            )
            .unwrap();
            let publisher = btel_file::LocalPublisher::new(builder, writer);
            delivery = publisher.delivery().cloned();
            Ok(publisher)
        },
    )
    .unwrap();
    telemetry.delivery = delivery.map(RecordingDelivery::Local);
    telemetry.recording_id = Some(id);
    telemetry.policies = Arc::new(bex_vm::telemetry::TelemetryPolicies::with_auto_level(
        btel_settings::mode::AutoTelemetryLevel::High,
    ));
    Arc::new(engine)
}
use btel_recorder::SealedFile;

fn context() -> crate::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disk_error_with_full_file_queue_lets_the_program_finish_without_failing_it() {
    for errno in [libc::ENOSPC, libc::EIO] {
        let directory = tempfile::tempdir().unwrap();
        let (entered, started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let error =
            btel_file::LocalDeliveryError(std::io::Error::from_raw_os_error(errno).to_string());
        let expected = error.to_string();
        let engine = engine_with_writer(
            directory.path(),
            r#"
            function leaf(n: int) -> int { n + 1 }
            function main() -> int {
                let n = 0;
                while (n < 1000) { n = leaf(n); }
                let child = spawn { leaf(n) };
                await child
            }
        "#,
            move |_| {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(15)).unwrap();
                Err(error.clone())
            },
        );
        let execution = Arc::clone(&engine);
        let task = tokio::spawn(async move {
            execution
                .call_function("main", vec![], context(), true)
                .await
        });
        // The disk worker is held independently of Tokio execution workers.
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        let transport = &engine.telemetry.as_ref().unwrap().runtime;
        // The writer stays held: the file queue fills, the processor blocks
        // on it, and the lanes run out. The program finishes anyway, on
        // chunks allocated beyond the lanes.
        let value = tokio::time::timeout(Duration::from_secs(30), task).await;
        // Read before the release: once the writer drains, the lanes shrink
        // back to their size.
        let stats = transport.stats();
        // Always release the writer before assertions, including on regression.
        release.send(()).unwrap();
        let value = value
            .expect("a held writer must not stall the program")
            .unwrap()
            .unwrap();
        assert_eq!(value, BexExternalValue::Int(1001));
        assert!(stats.sealed_chunks >= btel_settings::local_files::QUEUE_FILES.get() + 4);
        assert!(stats.allocated_timing_chunks + stats.allocated_span_chunks > 4);
        // The released writer fails on its own thread; the failure disables
        // recording without any help from the program.
        tokio::time::timeout(Duration::from_secs(10), async {
            while !transport.is_disabled() {
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await
        .expect("the writer's failure disables recording");
        assert!(
            crate::telemetry_state::EngineTelemetry::new_root(engine.telemetry.as_ref()).is_none()
        );
        assert_eq!(
            engine.telemetry_result().unwrap().unwrap_err().to_string(),
            expected
        );
        assert_eq!(transport.stats().active_producers, 0);
        // New invocations still work and do not restart recording.
        assert_eq!(
            engine
                .call_function("main", vec![], context(), true)
                .await
                .unwrap(),
            BexExternalValue::Int(1001)
        );
        tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
            .await
            .unwrap();
        let stats = transport.stats();
        assert_eq!(
            (
                stats.ready_chunks,
                stats.free_chunks,
                stats.active_producers
            ),
            (0, 0, 0)
        );
        assert_eq!(
            engine.telemetry_result().unwrap().unwrap_err().to_string(),
            expected
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_shutdown_does_not_reopen_a_closed_transport() {
    let directory = tempfile::tempdir().unwrap();
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut first = true;
    // A live manual owner also exercises terminal deferred publication without
    // reserving another producer from this deliberately tiny worker pool.
    let engine = engine_with_writer(
        directory.path(),
        r#"function main() -> int { let span = trace.start_span("pending"); 1 }"#,
        move |_| {
            if std::mem::take(&mut first) {
                entered.send(()).unwrap();
                released.recv_timeout(Duration::from_secs(10)).unwrap();
            }
            Ok(())
        },
    );
    engine
        .call_function("main", vec![], context(), true)
        .await
        .unwrap();
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    let closing = Arc::clone(&engine);
    let shutdown = tokio::spawn(async move {
        closing.shutdown().await;
    });
    // Processor completion proves admission closed and shutdown reached its
    // disk join; the writer remains blocked until explicitly released below.
    tokio::time::timeout(Duration::from_secs(5), async {
        while engine
            .telemetry
            .as_ref()
            .unwrap()
            .runtime
            .result()
            .is_none()
        {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap();
    shutdown.abort();
    assert!(shutdown.await.unwrap_err().is_cancelled());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    assert!(
        engine
            .call_function("main", vec![], context(), true)
            .await
            .is_err()
    );
}
