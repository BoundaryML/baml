use std::{num::NonZeroUsize, sync::mpsc, time::Duration};

use sys_native::SysOpsExt;

use super::*;
use crate::{BexExternalValue, FunctionCallContextBuilder};

fn nz(n: usize) -> NonZeroUsize {
    NonZeroUsize::new(n).unwrap()
}

fn engine_with_writer(
    source: &str,
    write: impl FnMut(&SealedFile) -> Result<(), btel_file::FileSinkError> + Send + 'static,
) -> Arc<BexEngine> {
    let mut engine = BexEngine::new(
        baml_db::testing::compile_source(source),
        Arc::new(sys_native::SysOps::native()),
        vec![],
    )
    .unwrap();
    let telemetry = engine.telemetry.as_mut().unwrap();
    telemetry.runtime.finish().unwrap();
    let id = RecordingId::generate();
    let mut sink = None;
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
            let writer = btel_file::FileSink::with_test_writer(
                PathBuf::new(),
                btel_file::FileSinkConfig::default(),
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
            sink = publisher.sink().cloned();
            Ok(publisher)
        },
    )
    .unwrap();
    telemetry.delivery = sink.map(RecordingDelivery::Local);
    telemetry.recording_id = Some(id);
    telemetry.policies = Arc::new(bex_vm::telemetry::TelemetryPolicies::with_mode(
        btel_settings::mode::TelemetryMode::High,
    ));
    Arc::new(engine)
}
use btel_publisher::SealedFile;

fn context() -> crate::FunctionCallContext {
    FunctionCallContextBuilder::new(sys_types::CallId::next()).build()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn disk_error_with_full_file_queue_and_chunk_pool_does_not_fail_application() {
    for errno in [libc::ENOSPC, libc::EIO] {
        let (entered, started) = mpsc::channel();
        let (release, released) = mpsc::channel();
        let error = btel_file::FileSinkError(std::io::Error::from_raw_os_error(errno).to_string());
        let expected = error.to_string();
        let engine = engine_with_writer(
            r#"
            function leaf(n: int) -> int { n + 1 }
            function main() -> int {
                let n = 0;
                while (n < 10000) { n = leaf(n); }
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
        let full = tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let stats = transport.stats();
                // One chunk is at the blocked writer, eight queued files, one
                // blocked delivery, and both remaining span chunks are ready.
                if stats.sealed_chunks >= btel_settings::local_files::QUEUE_FILES.get() + 4
                    && stats.ready_chunks == 2
                    && stats.active_producers > 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        })
        .await;
        let was_blocked = !task.is_finished();
        // Always release the writer before assertions, including on regression.
        release.send(()).unwrap();
        full.expect("both bounded queues must reach saturation");
        assert!(was_blocked);
        let value = tokio::time::timeout(Duration::from_secs(10), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        assert_eq!(value, BexExternalValue::Int(10001));
        assert!(transport.is_disabled());
        assert!(engine.telemetry.as_ref().unwrap().new_root().is_none());
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
            BexExternalValue::Int(10001)
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
    let (entered, started) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let mut first = true;
    let engine = engine_with_writer("function main() -> int { 1 }", move |_| {
        if std::mem::take(&mut first) {
            entered.send(()).unwrap();
            released.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        Ok(())
    });
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
