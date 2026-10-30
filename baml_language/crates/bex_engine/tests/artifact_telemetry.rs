#![cfg(not(target_arch = "wasm32"))]

use std::{num::NonZeroUsize, sync::Arc, time::Duration};

use bex_engine::{BexEngine, EngineConfig, FunctionCallContextBuilder, TelemetryRecording};
use btel_bcs::delivery::DeliveryConfig;
use btel_settings::artifact::{
    ArtifactTelemetry, InitialFailureAction, InitialFailurePolicy, RecordingLevel,
};
use sys_native::SysOpsExt;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path_regex},
};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

fn engine(server: &MockServer, policy: ArtifactTelemetry) -> Arc<BexEngine> {
    let mut delivery = DeliveryConfig::new(server.uri().parse().unwrap());
    delivery.max_attempts = 2;
    delivery.retry_delay = Duration::from_millis(1);
    delivery.request_timeout = Duration::from_secs(1);
    engine_with(delivery, policy)
}

fn engine_with(delivery: DeliveryConfig, policy: ArtifactTelemetry) -> Arc<BexEngine> {
    engine_with_recording(
        TelemetryRecording::cloud(
            btel_settings::publisher::RecordingConfig::default(),
            btel_bcs::CloudPublisherConfig::default(),
            delivery,
        ),
        policy,
    )
}

fn engine_with_recording(
    recording: TelemetryRecording,
    policy: ArtifactTelemetry,
) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(policy),
                recording: Some(recording),
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
async fn a_short_program_waits_for_an_unanswered_endpoint_only_the_shutdown_timeout() {
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
async fn background_telemetry_does_not_wait_for_a_stalled_collector() {
    let stalled = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    stalled.set_nonblocking(true).unwrap();
    let delivery = DeliveryConfig::new(
        format!("http://{}", stalled.local_addr().unwrap())
            .parse()
            .unwrap(),
    );
    let engine = engine_with(
        delivery,
        ArtifactTelemetry {
            background_delivery: true,
            initial_failure: InitialFailurePolicy {
                action: InitialFailureAction::Ignore,
                warning_message: None,
            },
            ..Default::default()
        },
    );
    // Prove recording actually starts a connection; keep it unanswered.
    let _connection = tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match stalled.accept() {
                Ok((connection, _)) => break connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                Err(error) => panic!("accept: {error}"),
            }
        }
    })
    .await
    .expect("BCS recording must remain enabled");
    assert!(
        tokio::time::timeout(Duration::from_secs(1), execute(&engine))
            .await
            .expect("collector must not delay execution or shutdown")
    );
    assert!(engine.initial_cloud_authorization_error().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_later_synchronous_policy_preserves_the_configured_delivery_timeout() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let base = server.uri();
    Mock::given(method("POST"))
        .and(path_regex(r"/uploads:prepare$"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200)
                .set_delay(Duration::from_millis(50))
                .set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let mut delivery = DeliveryConfig::new(server.uri().parse().unwrap());
    delivery.shutdown_timeout = Duration::from_secs(5);
    let recording = TelemetryRecording::cloud(
        btel_settings::publisher::RecordingConfig {
            // Keep the final recording pending until shutdown must deliver it.
            flush_interval_duration: Duration::from_secs(3600),
            ..Default::default()
        },
        btel_bcs::CloudPublisherConfig::default(),
        delivery,
    )
    .with_artifact_telemetry(ArtifactTelemetry {
        background_delivery: true,
        ..Default::default()
    });
    // EngineConfig replaces the earlier background policy with its final policy.
    let engine = engine_with_recording(recording, ArtifactTelemetry::default());
    assert!(execute(&engine).await);
    assert!(engine.telemetry_result().unwrap().is_ok());
    assert!(
        server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT")
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn background_cloud_policy_still_finishes_local_files_before_shutdown_returns() {
    let root = tempfile::tempdir().unwrap();
    let engine = Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(ArtifactTelemetry {
                    background_delivery: true,
                    ..Default::default()
                }),
                recording: Some(TelemetryRecording::local_files_in(
                    root.path(),
                    btel_settings::publisher::RecordingConfig {
                        flush_interval_duration: Duration::from_secs(3600),
                        ..Default::default()
                    },
                )),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let directory = engine.telemetry_recording_directory().unwrap().to_owned();
    assert!(execute(&engine).await);
    assert_eq!(engine.telemetry_result(), Some(Ok(())));
    let recording = btel_file::read_directory(&directory).unwrap();
    assert!(recording.issues.is_empty(), "{:?}", recording.issues);
    assert!(
        recording.has_recording_end,
        "shutdown left the local recording unfinished"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn background_telemetry_still_delivers_to_a_healthy_collector_while_running() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(&server)
        .await;
    let base = server.uri();
    Mock::given(method("POST"))
        .and(path_regex(r"/uploads:prepare$"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(&server)
        .await;
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&server)
        .await;
    let engine = Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(ArtifactTelemetry {
                    background_delivery: true,
                    initial_failure: InitialFailurePolicy {
                        action: InitialFailureAction::Ignore,
                        warning_message: None,
                    },
                    ..Default::default()
                }),
                recording: Some(TelemetryRecording::cloud(
                    btel_settings::publisher::RecordingConfig {
                        target_bytes: NonZeroUsize::new(1).unwrap(),
                        ..Default::default()
                    },
                    btel_bcs::CloudPublisherConfig {
                        candidate_target: 1,
                        ..Default::default()
                    },
                    DeliveryConfig::new(server.uri().parse().unwrap()),
                )),
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let result = engine
        .call_function(
            "main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .unwrap();
    assert_eq!(result, bex_engine::BexExternalValue::Int(7));
    let delivered = tokio::time::timeout(Duration::from_secs(5), async {
        while !server
            .received_requests()
            .await
            .unwrap()
            .iter()
            .any(|request| request.method == "PUT")
        {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;
    engine.shutdown().await;
    delivered.expect("background mode must upload BCS recordings while the application runs");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn background_telemetry_startup_failure_does_not_prevent_execution() {
    let temp = tempfile::tempdir().unwrap();
    let invalid_recording = btel_settings::publisher::RecordingConfig {
        flush_interval_duration: Duration::ZERO,
        ..Default::default()
    };
    let engine = Arc::new(
        BexEngine::new_with_config(
            baml_test_support::compile_source("function main() -> int { 7 }"),
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            EngineConfig {
                artifact_telemetry: Some(ArtifactTelemetry {
                    background_delivery: true,
                    initial_failure: InitialFailurePolicy {
                        action: InitialFailureAction::Ignore,
                        warning_message: None,
                    },
                    ..Default::default()
                }),
                recording: Some(TelemetryRecording::local_files_in(
                    temp.path(),
                    invalid_recording,
                )),
                ..Default::default()
            },
        )
        .expect("telemetry setup must not fail engine construction"),
    );
    assert!(execute(&engine).await);
    assert!(engine.telemetry_recording_id().is_none());
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
