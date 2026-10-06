//! Runtime GC pauses survive the real recording/index/query pipeline.
mod support;

use std::{sync::Arc, time::Duration};

use bex_engine::{
    BexEngine, BexExternalValue, EngineConfig, FunctionCallContextBuilder, ProcessStatus,
    TelemetryRecording,
};
use bex_heap::CollectionLevel;
use btel_recorder::RecordingConfig;
use serde_json::json;
use support::{engine, index, sql};
use sys_native::SysOpsExt;

#[tokio::test]
async fn all_collections_share_one_root_profiler_node() {
    let project = tempfile::tempdir().unwrap();
    let mut expected_explicit = vec![];
    // Both levels, repeated across independently created engines/recordings,
    // must still fold into the same node in this process's profiler.
    for _ in 0..2 {
        let engine = engine(project.path(), RecordingConfig::default());
        for (level, name) in [
            (CollectionLevel::Minor, "minor"),
            (CollectionLevel::Major, "major"),
        ] {
            let stats = engine.collect_garbage(level).await;
            expected_explicit.push(vec![
                json!(name),
                json!(stats.live_count),
                json!(stats.collected_count),
                json!(stats.promoted_to_gen1),
                json!(stats.promoted_to_gen2),
                json!(stats.live_payload_bytes),
            ]);
        }
        engine.record_process_exit(ProcessStatus::Success);
        tokio::time::timeout(Duration::from_secs(10), engine.shutdown())
            .await
            .expect("GC telemetry must not block shutdown");
        assert_eq!(engine.telemetry_result(), Some(Ok(())));
    }

    let mut index = index(project.path());
    let explicit = sql(
        &mut index,
        "SELECT context_metadata['collection_level'], context_metadata['live_slots'],
           context_metadata['reclaimed_slots'], context_metadata['promoted_to_gen1'],
           context_metadata['promoted_to_gen2'], context_metadata['live_payload_bytes']
         FROM spans WHERE span_name = 'baml.gc' AND context_metadata['trigger'] = 'explicit'
         ORDER BY start_time",
    );
    assert_eq!(explicit.rows, expected_explicit);

    let roots = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(DISTINCT span_id), COUNT(DISTINCT profiler_node_id),
           SUM(parent_span_id IS NOT NULL), SUM(duration <= 0),
           SUM(span_type != 'future'), SUM(status != 'return')
         FROM spans WHERE span_name = 'baml.gc'",
    );
    let collections = roots.rows[0][0].as_i64().unwrap();
    assert!(
        collections >= 6,
        "explicit and shutdown collections are recorded"
    );
    assert_eq!(
        roots.rows,
        vec![vec![
            json!(collections),
            json!(collections),
            json!(1),
            json!(0),
            json!(0),
            json!(0),
            json!(0)
        ]]
    );
    let shutdown = sql(
        &mut index,
        "SELECT context_metadata['collection_level'], COUNT(*) FROM spans
         WHERE span_name = 'baml.gc' AND context_metadata['trigger'] = 'shutdown'
         GROUP BY context_metadata['collection_level']",
    );
    assert_eq!(shutdown.rows, vec![vec![json!("major"), json!(2)]]);

    let profile = sql(
        &mut index,
        "SELECT node_type, parent_profiler_node_id, invocation_count, return_count,
           total_time > 0, total_time = self_time
         FROM profiler WHERE function_name = 'baml.gc'",
    );
    assert_eq!(
        profile.rows,
        vec![vec![
            json!("future"),
            json!(null),
            json!(collections),
            json!(collections),
            json!(1),
            json!(1)
        ]]
    );
}

#[tokio::test]
async fn allocation_and_idle_collections_are_visible() {
    let project = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(
                r#"
            function main() -> int {
                let texts: string[] = [];
                let i = 0;
                while (i < 2048) {
                    texts.push("x".repeat(32768));
                    i = i + 1;
                }
                texts.length()
            }
        "#,
            ),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(project.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let result = engine
        .call_function(
            "main",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, BexExternalValue::Int(2048));
    let automatic = engine.heap().gc_budget().full_collections;
    assert!(automatic > 0, "the workload must force allocation GC");
    tokio::time::timeout(Duration::from_secs(5), async {
        while engine.heap().gc_budget().full_collections == automatic {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("completed work must trigger idle GC");
    engine.record_process_exit(ProcessStatus::Success);
    engine.shutdown().await;
    assert_eq!(engine.telemetry_result(), Some(Ok(())));

    let mut index = index(project.path());
    let triggers = sql(
        &mut index,
        "SELECT DISTINCT context_metadata['trigger'], context_metadata['collection_level'],
           parent_span_id FROM spans WHERE span_name = 'baml.gc'
         ORDER BY context_metadata['trigger']",
    );
    assert_eq!(
        triggers.rows,
        vec![
            vec![json!("automatic"), json!("major"), json!(null)],
            vec![json!("idle"), json!("major"), json!(null)],
            vec![json!("shutdown"), json!("major"), json!(null)],
        ]
    );
}

#[tokio::test]
async fn collections_work_with_telemetry_disabled() {
    let engine = Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 1 }"),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            EngineConfig {
                artifact_telemetry: Some(btel_settings::artifact::ArtifactTelemetry {
                    recording_level: btel_settings::artifact::RecordingLevel::Off,
                    ..Default::default()
                }),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    assert!(engine.telemetry_recording_directory().is_none());
    for level in [CollectionLevel::Minor, CollectionLevel::Major] {
        assert_eq!(engine.collect_garbage(level).await.level, level);
    }
    tokio::time::timeout(Duration::from_secs(10), engine.shutdown())
        .await
        .expect("disabled GC telemetry must not block shutdown");
    assert!(engine.telemetry_result().is_none());
}
