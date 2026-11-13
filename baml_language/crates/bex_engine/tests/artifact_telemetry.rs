#![cfg(not(target_arch = "wasm32"))]

use std::{sync::Arc, time::Duration};

use bex_engine::{BexEngine, EngineConfig, FunctionCallContextBuilder, TelemetryRecording};
use btel_bcs::delivery::DeliveryConfig;
use btel_settings::artifact::{
    ArtifactTelemetry, InitialFailureAction, InitialFailurePolicy, RecordingLevel,
};
use sys_native::SysOpsExt;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path, path_regex},
};

/// Run enabled delivery fixtures in their own environment, even when the caller
/// has opted out. The parent never changes its environment or accesses user auth.
/// Returns true when the parent has completed the isolated test.
fn enabled_recording_child(test_name: &str) -> bool {
    const CHILD: &str = "BAML_TEST_ENABLED_ARTIFACT_RECORDING";
    if baml_env::raw_var(CHILD).as_deref() == Some(test_name) {
        return false;
    }
    let home = tempfile::tempdir().unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", test_name, "--nocapture"])
        .env(CHILD, test_name)
        .env("BAML_TELEMETRY", "medium")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .env("BAML_HOME", home.path())
        .env_remove("BOUNDARY_API_URL")
        .env_remove("BOUNDARY_API_KEY")
        .env_remove("BOUNDARY_PROJECT")
        .env_remove("DEV_BAML_TELEMETRY_DIAGNOSTICS_FILE")
        .output()
        .unwrap();
    assert!(output.status.success(), "{test_name}: {output:?}");
    true
}

fn engine(server: &MockServer, policy: ArtifactTelemetry) -> Arc<BexEngine> {
    let mut delivery = DeliveryConfig::new(server.uri().parse().unwrap());
    delivery.max_attempts = 2;
    delivery.retry_delay = Duration::from_millis(1);
    delivery.request_timeout = Duration::from_secs(1);
    engine_with(delivery, policy)
}

fn engine_with(delivery: DeliveryConfig, policy: ArtifactTelemetry) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(policy),
                recording: Some(TelemetryRecording::cloud(
                    btel_settings::publisher::RecordingConfig::default(),
                    btel_bcs::CloudPublisherConfig::default(),
                    delivery,
                )),
                clock_mode: btel_settings::clock::ClockMode::Monotonic,
                ..Default::default()
            },
        )
        .unwrap(),
    )
}

async fn execute(engine: &Arc<BexEngine>) -> bool {
    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await;
    // An initial rejection may race execution, so abort is inspected after shutdown.
    let succeeded = result.is_ok();
    engine.shutdown().await;
    succeeded
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_rejections_follow_the_artifact_policy_and_join_even_short_programs() {
    if enabled_recording_child(
        "initial_rejections_follow_the_artifact_policy_and_join_even_short_programs",
    ) {
        return;
    }
    for status in [401, 403, 400, 503] {
        for action in [
            InitialFailureAction::Abort,
            InitialFailureAction::Warn,
            InitialFailureAction::Ignore,
        ] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path_regex(r"/heartbeat$"))
                .respond_with(ResponseTemplate::new(status).set_delay(Duration::from_millis(50)))
                .expect(if status == 503 { 2 } else { 1 })
                .mount(&server)
                .await;
            let engine = engine(
                &server,
                ArtifactTelemetry {
                    initial_failure: InitialFailurePolicy {
                        action,
                        warning_message: Some("Telemetry unavailable; continuing.".into()),
                    },
                    ..Default::default()
                },
            );
            let execution_succeeded = execute(&engine).await;
            assert_eq!(
                engine.initial_cloud_authorization_error().is_some(),
                action == InitialFailureAction::Abort
            );
            if action == InitialFailureAction::Abort {
                assert!(engine.telemetry_result().unwrap().is_err());
            } else {
                assert!(execution_succeeded);
                assert!(engine.telemetry_result().unwrap().is_err());
                assert!(engine.initial_telemetry_failure_handled());
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_short_program_waits_for_an_unanswered_endpoint_only_the_shutdown_timeout() {
    if enabled_recording_child(
        "a_short_program_waits_for_an_unanswered_endpoint_only_the_shutdown_timeout",
    ) {
        return;
    }
    // Accepts connections and never answers them, like a firewall that drops
    // replies. The default retry window for the first connection is 40.6 s.
    let stalled = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let mut delivery = DeliveryConfig::new(
        format!("http://{}", stalled.local_addr().unwrap())
            .parse()
            .unwrap(),
    );
    delivery.shutdown_timeout = Duration::from_millis(300);
    let engine = engine_with(
        delivery,
        ArtifactTelemetry {
            initial_failure: InitialFailurePolicy {
                action: InitialFailureAction::Warn,
                warning_message: Some("Telemetry unavailable; continuing.".into()),
            },
            ..Default::default()
        },
    );
    let started = std::time::Instant::now();
    assert!(execute(&engine).await);
    assert!(started.elapsed() < Duration::from_secs(5));
    // The policy handled it as an initial failure: it warned, then gave up.
    assert!(engine.initial_telemetry_failure_handled());
    assert!(engine.telemetry_result().unwrap().is_err());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn recording_off_starts_no_telemetry_authorization() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(401))
        .expect(0)
        .mount(&server)
        .await;
    let engine = engine(
        &server,
        ArtifactTelemetry {
            recording_level: RecordingLevel::Off,
            ..Default::default()
        },
    );
    execute(&engine).await;
    assert_eq!(engine.telemetry_recording_id(), None);
    assert!(engine.initial_cloud_authorization_error().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn later_rejection_never_becomes_an_initial_failure_for_any_policy() {
    if enabled_recording_child("later_rejection_never_becomes_an_initial_failure_for_any_policy") {
        return;
    }
    for action in [
        InitialFailureAction::Abort,
        InitialFailureAction::Warn,
        InitialFailureAction::Ignore,
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/heartbeat$"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"/uploads:prepare$"))
            .respond_with(ResponseTemplate::new(401))
            .expect(1)
            .mount(&server)
            .await;
        let engine = engine(
            &server,
            ArtifactTelemetry {
                initial_failure: InitialFailurePolicy {
                    action,
                    warning_message: None,
                },
                ..Default::default()
            },
        );
        assert!(execute(&engine).await);
        assert!(engine.initial_cloud_authorization_error().is_none());
        assert!(engine.telemetry_result().unwrap().is_err());
    }
}

/// Disabled engines allocate no recording runtime, even with an explicitly
/// configured cloud sink, traced spans and otherwise functioning application IO.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn effective_off_has_no_engine_recording_resources() {
    const CHILD: &str = "BAML_TEST_ENGINE_RECORDING_OFF";
    if let Some(kind) = baml_env::raw_var(CHILD) {
        let url = baml_env::raw_var("BAML_TEST_APPLICATION_URL").unwrap();
        let program = baml_test_support::compile_source(&format!(
            r#"
function main() -> int {{
    let span = trace.start_span("explicit");
    let result = span.run(() -> int {{
        let response = baml.http.send(baml.http.Request {{ method: "GET", url: {url:?}, headers: {{}}, body: "" }});
        assert.is_true(response.text() == "ok");
        let child = spawn {{ 7 }};
        await child
    }});
    span.finish(result);
    result
}}
"#
        ));
        let policy = match kind.as_str() {
            "bare" => None,
            "frozen" => Some(ArtifactTelemetry::default()),
            "custom" => Some(ArtifactTelemetry {
                recording_level_envvar: Some("ACME_TELEMETRY".into()),
                ..Default::default()
            }),
            _ => unreachable!(),
        };
        let engine = Arc::new(
            BexEngine::new_with_config(
                program,
                Arc::new(sys_native::SysOps::native()),
                Vec::new(),
                EngineConfig {
                    artifact_telemetry: policy,
                    recording: Some(TelemetryRecording::cloud(
                        btel_settings::publisher::RecordingConfig::default(),
                        btel_bcs::CloudPublisherConfig::default(),
                        DeliveryConfig::new(
                            baml_env::raw_var("BOUNDARY_API_URL")
                                .unwrap()
                                .parse()
                                .unwrap(),
                        ),
                    )),
                    ..Default::default()
                },
            )
            .unwrap(),
        );
        for _ in 0..2 {
            assert_eq!(engine.telemetry_recording_id(), None);
            assert_eq!(engine.telemetry_recording_directory(), None);
            assert!(engine.reset_telemetry_clock_after_restore().is_none());
            assert_eq!(engine.telemetry_result(), None);
            assert_eq!(engine.telemetry_delivery_loss_count(), 0);
            #[cfg(feature = "telemetry_diagnostics")]
            assert!(engine.telemetry_diagnostics().is_none());
            let result = engine
                .call_function(
                    "main",
                    Vec::new(),
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
                .unwrap();
            assert_eq!(result, bex_external_types::BexExternalValue::Int(7));
        }
        engine.shutdown().await;
        engine.shutdown().await;
        assert_eq!(engine.telemetry_result(), None);
        return;
    }
    for kind in ["bare", "frozen", "custom"] {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/application"))
            .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
            .expect(2)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "effective_off_has_no_engine_recording_resources",
                "--nocapture",
            ])
            .env(CHILD, kind)
            .env("BAML_HOME", home.path())
            .env(
                "BAML_TEST_APPLICATION_URL",
                format!("{}/application", server.uri()),
            )
            .env("BOUNDARY_API_URL", server.uri())
            .env(
                "BAML_TELEMETRY",
                if kind == "custom" { "high" } else { "off" },
            )
            .env("ACME_TELEMETRY", " No ")
            .output()
            .unwrap();
        assert!(output.status.success(), "{kind}: {output:?}");
        assert!(!home.path().join("btel").exists());
        let requests = server.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2, "{kind}: {requests:?}");
        assert!(
            requests
                .iter()
                .all(|request| request.url.path() == "/application")
        );
    }
}
