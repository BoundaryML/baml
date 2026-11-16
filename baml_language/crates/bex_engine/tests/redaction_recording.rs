//! `@trace.redact` and `@@trace.redact` reach the recording: a compiled
//! program's logged value holds redaction markers where its declarations say,
//! and nothing a redaction withholds reaches any blob it wrote.
#![cfg(not(target_arch = "wasm32"))]
use std::{sync::Arc, time::Duration};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::RecordingConfig;
use btel_snapshot::{DecodeLimits, DecodedObject, DecodedValue, Redaction, decode_blob};
use sys_native::SysOpsExt;

const SOURCE: &str = include_str!("../../baml_tests/baml_src/ns_trace_redact/redact.baml");
const SECRETS: [&str; 3] = [
    "secret-in-a-redacted-field",
    "secret-in-a-redacted-class",
    "secret-in-a-copied-field",
];

/// Every blob under `directory`, recursively.
fn blobs(directory: &std::path::Path) -> Vec<Vec<u8>> {
    let mut found = Vec::new();
    for entry in std::fs::read_dir(directory).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            found.extend(blobs(&path));
        } else {
            found.push(std::fs::read(&path).unwrap());
        }
    }
    found
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn declared_redactions_reach_the_recording_and_withhold_their_values() {
    let directory = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            baml_test_support::compile_source(SOURCE),
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files_in(directory.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let result = tokio::time::timeout(
        Duration::from_secs(20),
        engine.call_function(
            "trace_redact_log",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    // Redaction changes the recording, never the program.
    assert_eq!(
        result,
        BexExternalValue::String(SECRETS[..2].concat().into())
    );
    // A class built at run time with `fields_from` keeps a copied field's
    // redaction.
    let copied = tokio::time::timeout(
        Duration::from_secs(20),
        engine.call_function(
            "trace_redact_copied_log",
            vec![],
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        ),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(copied, BexExternalValue::String("copied".into()));
    tokio::time::timeout(Duration::from_secs(20), engine.shutdown())
        .await
        .unwrap();
    assert_eq!(engine.telemetry_result(), Some(Ok(())));

    let blobs = blobs(&directory.path().join("cas"));
    assert!(!blobs.is_empty(), "the log's value was captured");
    for blob in &blobs {
        for secret in SECRETS {
            assert!(
                !blob
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes()),
                "{secret} reached a blob"
            );
        }
    }
    // The logged client: its name as written, its token and credentials
    // withheld by their declarations.
    let clients: Vec<_> = blobs
        .iter()
        .filter_map(|blob| decode_blob(blob, &DecodeLimits::default()).ok())
        .flat_map(|snapshot| snapshot.objects)
        .filter_map(|object| match object {
            DecodedObject::Instance { fields, .. } if fields.len() == 3 => Some(fields),
            _ => None,
        })
        .collect();
    // The client and its run-time copy.
    assert_eq!(clients.len(), 2, "the client and its copy: {clients:?}");
    for fields in &clients {
        let field = |name: &str| &fields.iter().find(|(key, _)| &**key == name).unwrap().1;
        assert_eq!(field("name"), &DecodedValue::String("visible-name".into()));
        assert_eq!(field("token"), &DecodedValue::Redacted(Redaction::Declared));
        assert_eq!(
            field("credentials"),
            &DecodedValue::Redacted(Redaction::Declared)
        );
    }
}
