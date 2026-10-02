//! Drive the real CLI against a loopback mock of the Boundary publisher.
mod common;

use std::{path::Path, process::Command};

use btel_bcs::{proto::CloudUploadEnvelope, wire::PrepareUploadsRequest};
use prost::Message as _;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path_regex},
};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

const KEY: &str = "bml_cli_test_key";
const RECORDINGS: &str = ".baml/btel/recordings";

fn run(project: &Path, boundary_url: &str, telemetry: &str) {
    let output = Command::new(common::baml_cli())
        .args(["run", "main"])
        .current_dir(project)
        .env("BOUNDARY_URL", boundary_url)
        .env("BOUNDARY_API_KEY", KEY)
        .env("BAML_TELEMETRY", telemetry)
        .env("BAML_HOME", project.join("home"))
        .env("BAML_CACHE_DIR", common::shared_cache_dir())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BAML_AGENT_SKILL_CHECK", "off")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "CLI failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    assert!(!project.join(RECORDINGS).exists());
}

#[test]
fn baml_run_uploads_from_boundary_env() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::create_dir(temp.path().join("home")).unwrap();
    std::fs::write(
        temp.path().join("home/config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();

    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let server = MockServer::start().await;
            let base = server.uri();
            let boundary_url = format!("{base}/publisher");
            Mock::given(method("POST"))
                .and(path_regex(
                    r"^/publisher/v1/recordings/[^/]+/uploads:prepare$",
                ))
                .respond_with(move |request: &Request| {
                    ResponseTemplate::new(200).set_body_json(cloud_protocol::response(
                        request,
                        &base,
                        &[],
                    ))
                })
                .mount(&server)
                .await;
            Mock::given(method("POST"))
                .and(path_regex(r"^/publisher/v1/recordings/[^/]+/heartbeat$"))
                .respond_with(ResponseTemplate::new(204))
                .mount(&server)
                .await;
            Mock::given(method("PUT"))
                .and(path_regex(r"^/put/"))
                .respond_with(ResponseTemplate::new(200))
                .mount(&server)
                .await;

            run(temp.path(), &boundary_url, "medium");
            let requests = server.received_requests().await.unwrap();
            let mut prepares = 0;
            let mut recordings = Vec::new();
            for request in &requests {
                if request.method == "POST" {
                    assert_eq!(request.headers["authorization"], format!("Bearer {KEY}"));
                    if request.url.path().ends_with("/uploads:prepare") {
                        prepares += 1;
                        let prepare: PrepareUploadsRequest =
                            serde_json::from_slice(&request.body).unwrap();
                        assert_eq!(
                            request.headers["idempotency-key"],
                            format!(
                                "{}:{}",
                                prepare.recording.recording_id,
                                prepare.recording.recording_file_sequence
                            ),
                        );
                    } else {
                        assert!(request.url.path().ends_with("/heartbeat"));
                    }
                } else {
                    assert_eq!(request.method, "PUT");
                    assert!(!request.headers.contains_key("authorization"));
                    assert_eq!(request.headers["x-required"], "signed-value");
                    let envelope = CloudUploadEnvelope::decode(request.body.as_slice()).unwrap();
                    if let Some(bytes) = envelope.recording_file {
                        recordings.push(
                            btel_recorder::proto::RecordingFile::decode(bytes.as_slice()).unwrap(),
                        );
                    }
                }
            }
            assert!(prepares > 0, "CLI must prepare cloud uploads");
            assert!(!recordings.is_empty(), "CLI must upload a recording");
            for recording in &recordings {
                let header = recording.header.as_ref().unwrap();
                assert_eq!(header.host.as_deref(), Some("baml"));
                assert!(
                    header.command.is_empty(),
                    "cloud must omit process arguments"
                );
                assert!(
                    header.source_cas_id.is_some(),
                    "CLI must include source metadata"
                );
            }
            assert!(recordings.iter().any(|recording| {
                recording
                    .aggregates
                    .as_ref()
                    .is_some_and(|batch| !batch.entries.is_empty())
            }));

            server.reset().await;
            run(temp.path(), &boundary_url, "off");
            assert!(server.received_requests().await.unwrap().is_empty());
            // Plain HTTP to a non-loopback host is rejected without failing BAML.
            run(temp.path(), "http://example.invalid/publisher", "medium");
            assert!(server.received_requests().await.unwrap().is_empty());
        });
}
