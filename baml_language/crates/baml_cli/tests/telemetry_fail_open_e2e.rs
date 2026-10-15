//! Telemetry observes a model call; it must not change its result or rerun it.
mod common;

use std::{process::Command, time::Duration};

use sha2::Digest as _;
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path, path_regex},
};

#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;

#[derive(Clone, Copy, Debug)]
enum Delivery {
    DelayedRejection,
    ImmediateRejection,
    Unreachable,
    UploadRejected,
    Uploaded,
    Off,
}

#[derive(Clone, Copy, Debug)]
enum Execution {
    Success,
    FunctionFailure,
    ModelFailure,
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn telemetry_preserves_model_command_results_and_exit_codes() {
    for delivery in [
        Delivery::DelayedRejection,
        Delivery::ImmediateRejection,
        Delivery::Unreachable,
        Delivery::UploadRejected,
        Delivery::Uploaded,
        Delivery::Off,
    ] {
        for execution in [
            Execution::Success,
            Execution::FunctionFailure,
            Execution::ModelFailure,
        ] {
            run_case(delivery, execution).await;
        }
    }
}

async fn run_case(delivery: Delivery, execution: Execution) {
    let model = MockServer::start().await;
    let boundary = MockServer::start().await;
    let model_response = if matches!(execution, Execution::ModelFailure) {
        ResponseTemplate::new(400).set_body_json(serde_json::json!({
            "error": {"message": "mock-model-failed", "type": "invalid_request_error"}
        }))
    } else {
        ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "id": "mock", "object": "chat.completion", "created": 0, "model": "m",
            "choices": [{"index": 0, "finish_reason": "stop", "message": {
                "role": "assistant", "content": "ok"
            }}],
            "usage": {"prompt_tokens": 1, "completion_tokens": 1, "total_tokens": 2}
        }))
    };
    Mock::given(method("POST"))
        .and(path("/v1/chat/completions"))
        .respond_with(model_response.set_delay(Duration::from_millis(150)))
        .expect(1)
        .mount(&model)
        .await;

    let initially_rejected = matches!(
        delivery,
        Delivery::DelayedRejection | Delivery::ImmediateRejection
    );
    let rejection_delay = if matches!(delivery, Delivery::DelayedRejection) {
        Duration::from_millis(500)
    } else {
        Duration::ZERO
    };
    let heartbeat = if initially_rejected {
        ResponseTemplate::new(400)
            .set_body_json(serde_json::json!({
                "code": "ENVIRONMENT_REQUIRED", "retryable": false,
                "message": "Select an environment: the target cannot be inferred unambiguously.",
                "internal": "private-server-detail"
            }))
            .set_delay(rejection_delay)
    } else {
        ResponseTemplate::new(204)
    };
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(heartbeat)
        .mount(&boundary)
        .await;
    let base = boundary.uri();
    Mock::given(method("POST"))
        .and(path_regex(r"/uploads:prepare$"))
        .respond_with(move |request: &Request| {
            if matches!(delivery, Delivery::UploadRejected) {
                ResponseTemplate::new(403)
            } else {
                ResponseTemplate::new(200).set_body_json(cloud_protocol::response(
                    request,
                    &base,
                    &[],
                ))
            }
        })
        .mount(&boundary)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/put/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(&boundary)
        .await;

    let temp = tempfile::tempdir().unwrap();
    let project = temp.path();
    let main = if matches!(execution, Execution::FunctionFailure) {
        "function main() -> string { let result = Hello(); throw \"mock-function-failed\"; }"
    } else {
        "function main() -> string { Hello() }"
    };
    common::write_project(
        project,
        &format!(
            r#"
client Mock = openai.GenericClient.new(model = "m", api_key = "dummy-model-key", base_url = "{}/v1");
function Hello() -> string {{
    client: Mock
    prompt: `${{role("user")}}Reply ok. ${{ctx.output_format()}}`
}}
{main}
"#,
            model.uri()
        ),
    );
    let home = project.join("baml-home");
    std::fs::create_dir_all(&home).unwrap();
    common::share_build_cache(&home);
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();
    // A dummy saved login exercises default telemetry with neither a key nor a project.
    let login = home.join("login/cache").join(format!(
        "{}.json",
        hex::encode(sha2::Sha256::digest(boundary.uri().as_bytes()))
    ));
    std::fs::create_dir_all(login.parent().unwrap()).unwrap();
    std::fs::write(
        login,
        serde_json::to_vec(&serde_json::json!({
            "refreshToken": "bdry_session_dummy_test",
            "caller": {"userId": "dummy", "email": "dummy@example.test"}
        }))
        .unwrap(),
    )
    .unwrap();
    // Use a freshly closed loopback port for a real connection-refused error.
    let closed_port = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let unreachable = format!("http://{}", closed_port.local_addr().unwrap());
    drop(closed_port);
    let endpoint = if matches!(delivery, Delivery::Unreachable) {
        unreachable
    } else {
        boundary.uri()
    };
    let mut command = Command::new(common::baml_cli());
    command
        .args(["--agent-skill-check", "off", "run", "main"])
        .current_dir(project)
        .env_clear()
        .env("HOME", project.join("os-home"))
        .env("XDG_CONFIG_HOME", project.join("config"))
        .env("BAML_HOME", &home)
        .env("BOUNDARY_API_URL", endpoint)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("DO_NOT_TRACK", "1")
        .env("NO_COLOR", "1");
    if !initially_rejected {
        command
            .env("BOUNDARY_API_KEY", "bml_dummy_test")
            .env("BAML_TELEMETRY", "medium");
    }
    if matches!(delivery, Delivery::Off) {
        command.env("BAML_TELEMETRY", "off");
    }
    let output = command.output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let context = format!("{delivery:?}/{execution:?}: {output:?}");
    let success = matches!(execution, Execution::Success);
    assert_eq!(
        output.status.code(),
        Some(i32::from(!success)),
        "{context}"
    );
    assert_eq!(stdout, if success { "\"ok\"\n" } else { "" }, "{context}");
    assert_eq!(
        model.received_requests().await.unwrap().len(),
        1,
        "completed model calls must never be retried: {context}"
    );
    match execution {
        Execution::FunctionFailure => assert!(stderr.contains("mock-function-failed"), "{context}"),
        Execution::ModelFailure => assert!(stderr.contains("mock-model-failed"), "{context}"),
        Execution::Success => assert!(!stderr.contains("error:"), "{context}"),
    }
    let warnings = stderr
        .lines()
        .filter(|line| {
            line.starts_with("warning:")
                && (line.contains("Boundary") || line.contains("telemetry"))
        })
        .count();
    assert_eq!(
        warnings,
        usize::from(!matches!(delivery, Delivery::Uploaded | Delivery::Off)),
        "{context}"
    );
    assert!(!stderr.contains("Execution cancelled"), "{context}");
    for secret in [
        "dummy-model-key",
        "bdry_session_dummy_test",
        "bml_dummy_test",
        "private-server-detail",
    ] {
        assert!(!stderr.contains(secret), "{context}");
    }
    let requests = boundary.received_requests().await.unwrap();
    if initially_rejected {
        assert_eq!(requests.len(), 1, "{context}");
        let body: serde_json::Value = serde_json::from_slice(&requests[0].body).unwrap();
        assert_eq!(body["target"], serde_json::json!({}), "{context}");
        assert!(stderr.contains("ENVIRONMENT_REQUIRED"), "{context}");
        assert!(stderr.contains("Program execution continues"), "{context}");
    } else if matches!(delivery, Delivery::Off | Delivery::Unreachable) {
        assert!(requests.is_empty(), "{context}");
    } else if matches!(delivery, Delivery::Uploaded) {
        assert!(
            requests.iter().any(|request| request.method == "PUT"),
            "{context}"
        );
    } else {
        assert!(
            requests
                .iter()
                .any(|request| request.url.path().ends_with("/uploads:prepare")),
            "{context}"
        );
    }
    assert!(!project.join(".baml/btel/recordings").exists(), "{context}");
}
