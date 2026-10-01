#![cfg(not(target_arch = "wasm32"))]

use std::{process::Command, sync::Arc};

use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use sys_native::SysOpsExt;
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::method};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

// Each child owns its environment; no process-global mutation races other tests.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn native_engines_select_environment_and_preserve_explicit_destinations() {
    if let Ok(mode) = std::env::var("BAML_TEST_ENV_RECORDING") {
        let local = tempfile::tempdir().unwrap();
        let program = baml_db::testing::compile_source("function main() -> int { 7 }");
        let ops = Arc::new(sys_native::SysOps::native());
        let engine = Arc::new(if mode == "explicit" {
            BexEngine::new_with_telemetry_recording(
                program,
                ops,
                vec![],
                None,
                btel_settings::clock::DEFAULT_MODE,
                TelemetryRecording::local_files_in(
                    local.path(),
                    btel_recorder::RecordingConfig::default(),
                ),
            )
            .unwrap()
        } else {
            BexEngine::new(program, ops, vec![]).unwrap()
        });
        assert_eq!(engine.telemetry_recording_id().is_some(), mode != "off");
        assert_eq!(
            engine.telemetry_recording_directory().is_some(),
            mode == "explicit"
        );
        let context = FunctionCallContextBuilder::new(sys_types::CallId::next()).build();
        assert_eq!(
            engine
                .call_function("main", vec![], context, true)
                .await
                .unwrap(),
            BexExternalValue::Int(7)
        );
        engine.shutdown().await;
        if mode != "off" {
            assert_eq!(engine.telemetry_result(), Some(Ok(())));
        }
        return;
    }
    let server = MockServer::start().await;
    let base = server.uri();
    Mock::given(method("POST"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut delivered = 0;
    for mode in ["cloud", "explicit", "off"] {
        let endpoint = if mode == "cloud" {
            server.uri()
        } else {
            "invalid-url".to_owned()
        };
        let output = tokio::task::spawn_blocking(move || {
            Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "native_engines_select_environment_and_preserve_explicit_destinations",
                    "--nocapture",
                ])
                .env("BAML_TEST_ENV_RECORDING", mode)
                .env("BOUNDARY_URL", endpoint)
                .env("BOUNDARY_API_KEY", "test-key")
                .env(
                    "BAML_TELEMETRY",
                    if mode == "off" { "off" } else { "medium" },
                )
                .output()
                .unwrap()
        })
        .await
        .unwrap();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let requests = server.received_requests().await.unwrap();
        if mode == "cloud" {
            assert!(requests.iter().any(|request| request.method == "PUT"));
            for request in &requests {
                if request.method == "POST" {
                    assert_eq!(
                        request.headers.get("authorization").unwrap(),
                        "Bearer test-key"
                    );
                } else {
                    assert!(request.headers.get("authorization").is_none());
                }
            }
            delivered = requests.len();
        } else {
            assert_eq!(requests.len(), delivered);
        }
    }
}
