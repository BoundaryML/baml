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
    matchers::{method, path_regex},
};

fn engine(server: &MockServer, policy: ArtifactTelemetry) -> Arc<BexEngine> {
    let mut delivery = DeliveryConfig::new(server.uri().parse().unwrap());
    delivery.max_attempts = 2;
    delivery.retry_delay = Duration::from_millis(1);
    delivery.request_timeout = Duration::from_secs(1);
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
