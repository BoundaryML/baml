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

// Compare the complete error block; unrelated command progress precedes/follows it.
fn diagnostic(output: &std::process::Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let lines: Vec<_> = stderr.lines().collect();
    let start = lines
        .iter()
        .position(|line| line.starts_with("error: "))
        .unwrap_or_else(|| panic!("missing error diagnostic: {stderr}"));
    let end = lines[start..]
        .iter()
        .position(|line| {
            matches!(
                *line,
                "  Execution cancelled." | "  Query failed." | "  Authentication command failed."
            )
        })
        .unwrap_or_else(|| panic!("missing error outcome: {stderr}"));
    assert_eq!(
        lines.iter().filter(|line| **line == lines[start]).count(),
        1,
        "duplicate diagnostic: {stderr}"
    );
    lines[start..=start + end].join("\n")
}

fn assert_api_key_rejection(
    output: &std::process::Output,
    endpoint: &str,
    query: bool,
    recording_off_allowed: bool,
) {
    let actual = diagnostic(output);
    let local_and_outcome = if query {
        r#"    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
    } else if recording_off_allowed {
        r#"    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
    } else {
        r#"    • Record locally: rerun with BOUNDARY_API_KEY=local.

  Execution cancelled."#
    };
    // Saved login storage may be available or unavailable on the host. Every accepted variant
    // is compared in full; unit tests independently pin each specific saved-login state.
    let expected = [
        "Unset BOUNDARY_API_KEY to use your saved Boundary login.",
        "Unset BOUNDARY_API_KEY, then run `baml auth login`.",
        "Your saved login could not be read. Unset BOUNDARY_API_KEY, then run `baml auth login` to restore user authentication.",
    ].map(|login_recovery| format!(r#"error: Boundary rejected BOUNDARY_API_KEY (401).
  Endpoint: {endpoint}

  To continue, choose one:
    • Replace BOUNDARY_API_KEY with a valid API key.
    • {login_recovery}
{local_and_outcome}"#));
    assert!(
        expected.contains(&actual),
        "unexpected complete diagnostic:\n{actual}\nexpected one of:\n{}",
        expected.join("\n---\n")
    );
}

fn assert_ingest_forbidden(output: &std::process::Output, endpoint: &str) {
    assert_eq!(
        diagnostic(output),
        format!(
            r#"error: Boundary denied cloud telemetry ingestion (HTTP 403).
  Endpoint: {endpoint}

  To continue, choose one:
    • Use a credential with ingestion permission for the selected project and environment.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
        )
    );
}

fn run(project: &Path, boundary_url: &str, telemetry: &str, expected_success: bool) {
    common::share_build_cache(&project.join("home"));
    let output = Command::new(common::baml_cli())
        .args(["run", "main"])
        .current_dir(project)
        .env("BOUNDARY_API_URL", boundary_url)
        .env("BOUNDARY_API_KEY", KEY)
        .env("BOUNDARY_PROJECT", "acme/app")
        .env("BAML_TELEMETRY", telemetry)
        .env("BAML_HOME", project.join("home"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap();
    assert_eq!(
        output.status.success(),
        expected_success,
        "unexpected CLI status:\n{}\n{}",
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

            run(temp.path(), &boundary_url, "medium", true);
            let requests = server.received_requests().await.unwrap();
            let mut prepares = 0;
            let mut recordings = Vec::new();
            for request in &requests {
                if request.method == "POST" {
                    assert_eq!(request.headers["authorization"], format!("Bearer {KEY}"));
                    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                    assert_eq!(body["target"], serde_json::json!({"project": "acme/app"}));
                    if request.url.path().ends_with("/uploads:prepare") {
                        prepares += 1;
                        let prepare: PrepareUploadsRequest = cloud_protocol::prepare(request);
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
                    header.source_cas_id.is_none(),
                    "cloud recordings must omit source metadata"
                );
            }
            assert!(recordings.iter().any(|recording| {
                recording
                    .aggregates
                    .as_ref()
                    .is_some_and(|batch| !batch.entries.is_empty())
            }));

            server.reset().await;
            run(temp.path(), &boundary_url, "off", true);
            assert!(server.received_requests().await.unwrap().is_empty());
            // Invalid cloud configuration cancels execution before any request.
            let output = execute_case(temp.path(), "http://example.invalid/publisher", KEY, &["run", "main"]);
            assert!(!output.status.success(), "{output:?}");
            assert_eq!(diagnostic(&output), r#"error: Boundary configuration is invalid: Boundary API URL must use HTTPS, or HTTP on loopback.

  To continue, choose one:
    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#);
            assert!(server.received_requests().await.unwrap().is_empty());
        });
}

fn execute_case(
    project: &Path,
    endpoint: &str,
    credential: &str,
    args: &[&str],
) -> std::process::Output {
    common::share_build_cache(&project.join("home"));
    Command::new(common::baml_cli())
        .args(args)
        .current_dir(project)
        .env("BOUNDARY_API_URL", endpoint)
        .env("BOUNDARY_API_KEY", credential)
        .env("BOUNDARY_PROJECT", "acme/app")
        .env("BAML_TELEMETRY", "medium")
        .env("BAML_HOME", project.join("home"))
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap()
}

fn fixture(source: &str) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), source);
    std::fs::create_dir(temp.path().join("home")).unwrap();
    std::fs::write(
        temp.path().join("home/config.toml"),
        "[update]\nauto_check = false\n",
    )
    .unwrap();
    temp
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_rejection_fails_even_if_execution_finishes_before_authorization() {
    let temp = fixture("function main() -> int { 7 }\n");
    for status in [401, 403] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/heartbeat$"))
            .respond_with(
                ResponseTemplate::new(status).set_delay(std::time::Duration::from_millis(300)),
            )
            .expect(1)
            .mount(&server)
            .await;
        let output = execute_case(temp.path(), &server.uri(), KEY, &["run", "main"]);
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        if status == 401 {
            assert_api_key_rejection(&output, &server.uri(), false, true);
        } else {
            assert_ingest_forbidden(&output, &server.uri());
        }
        assert!(!temp.path().join(RECORDINGS).exists());
        assert!(!stderr.contains(KEY), "credential leaked: {stderr}");
        assert!(
            !stderr.contains("telemetry recording failed:"),
            "duplicate diagnostic: {stderr}"
        );
        assert!(
            server
                .received_requests()
                .await
                .unwrap()
                .iter()
                .all(|request| request.url.path().ends_with("/heartbeat"))
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_sentinel_records_and_queries_locally_without_boundary_requests() {
    let temp = fixture("function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    let output = execute_case(temp.path(), &server.uri(), "local", &["run", "main"]);
    assert!(output.status.success(), "{output:?}");
    assert!(temp.path().join(RECORDINGS).exists());
    let output = execute_case(
        temp.path(),
        &server.uri(),
        "local",
        &[
            "query",
            "SELECT COUNT(*) AS recordings FROM processes",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(result["rows"][0][0].as_i64().unwrap() > 0, "{result}");
    assert!(server.received_requests().await.unwrap().is_empty());

    // Local selection precedes endpoint validation and saved-login discovery.
    let output = execute_case(temp.path(), "not-a-boundary-url", "local", &["run", "main"]);
    assert!(output.status.success(), "{output:?}");
    let output = execute_case(
        temp.path(),
        "not-a-boundary-url",
        "local",
        &[
            "query",
            "SELECT COUNT(*) FROM processes",
            "--format",
            "json",
        ],
    );
    assert!(output.status.success(), "{output:?}");
    let output = execute_case(
        temp.path(),
        &server.uri(),
        "local",
        &["query", "SELECT 1", "--project", "acme/app"],
    );
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stderr)
            .lines()
            .find(|line| line.starts_with("error: "))
            .unwrap(),
        r#"error: BOUNDARY_API_KEY=local selects local recordings; unset BOUNDARY_API_KEY to use --project or --environment for a cloud query"#
    );

    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_query_uses_shared_diagnostics_with_query_specific_recovery() {
    let temp = fixture("function main() -> int { 7 }\n");
    for status in [401, 403, 400, 404, 503] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/query$"))
            .respond_with(ResponseTemplate::new(status).set_body_string("private-server-body"))
            .expect(1)
            .mount(&server)
            .await;
        let output = execute_case(temp.path(), &server.uri(), KEY, &["query", "SELECT 1"]);
        assert!(!output.status.success(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let endpoint = server.uri();
        if status == 401 {
            assert_api_key_rejection(&output, &endpoint, true, true);
        } else {
            let expected = match status {
                403 => format!(
                    r#"error: Boundary denied the cloud query (HTTP 403).
  Endpoint: {endpoint}

  To continue, choose one:
    • Use a credential with query permission for the selected project and environment.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
                ),
                400 => format!(
                    r#"error: Boundary rejected the cloud query (HTTP 400).
  Endpoint: {endpoint}

  To continue, choose one:
    • Check the command inputs and requested project/environment.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
                ),
                404 => format!(
                    r#"error: Boundary rejected the cloud query (HTTP 404).
  Endpoint: {endpoint}

  To continue, choose one:
    • Check the endpoint and requested project/environment, then retry.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
                ),
                503 => format!(
                    r#"error: Boundary rejected the cloud query (HTTP 503).
  Endpoint: {endpoint}

  To continue, choose one:
    • Retry the command after Boundary becomes available.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
                ),
                _ => unreachable!(),
            };
            assert_eq!(diagnostic(&output), expected);
        }
        assert!(
            !stderr.contains("caused by:"),
            "duplicate diagnostic: {stderr}"
        );
        assert!(!stderr.contains(KEY), "credential leaked: {stderr}");
        assert!(
            !stderr.contains("private-server-body"),
            "response body leaked: {stderr}"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_query_distinguishes_bad_responses_from_connection_failures() {
    let temp = fixture("function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"^/v1/query$"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_raw("private-malformed-response\n", "application/x-ndjson"),
        )
        .expect(1)
        .mount(&server)
        .await;
    let output = execute_case(temp.path(), &server.uri(), KEY, &["query", "SELECT 1"]);
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    let endpoint = server.uri();
    assert_eq!(
        diagnostic(&output),
        format!(
            r#"error: Boundary returned an invalid response: Invalid cloud query frame.
  Endpoint: {endpoint}

  To continue, choose one:
    • Check that the endpoint is a Boundary API compatible with this BAML client.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        )
    );
    assert!(!stderr.contains("private-malformed-response"), "{stderr}");

    let output = execute_case(
        temp.path(),
        "http://127.0.0.1:1",
        KEY,
        &["query", "SELECT 1"],
    );
    assert!(!output.status.success());
    assert_eq!(
        diagnostic(&output),
        r#"error: Could not communicate with Boundary for the cloud query.
  Endpoint: http://127.0.0.1:1

  To continue, choose one:
    • Check the endpoint and network connection, then retry the command.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cloud_query_displays_structured_error_codes_and_optional_messages() {
    let temp = fixture("function main() -> int { 7 }\n");
    for message in [
        Some("Select an environment: this credential can query multiple environments."),
        None,
    ] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"^/v1/query$"))
            .respond_with(ResponseTemplate::new(400).set_body_json(serde_json::json!({
                "code": "ENVIRONMENT_REQUIRED", "retryable": false, "message": message,
                "internal": KEY,
            })))
            .expect(1)
            .mount(&server)
            .await;
        let output = execute_case(temp.path(), &server.uri(), KEY, &["query", "SELECT 1"]);
        assert!(!output.status.success());
        let endpoint = server.uri();
        let expected = if message.is_some() {
            format!(
                r#"error: Boundary rejected the cloud query (HTTP 400).
  Endpoint: {endpoint}
  Code: ENVIRONMENT_REQUIRED
  Select an environment: this credential can query multiple environments.

  To continue, choose one:
    • Select an environment with --environment <name> and, when needed, --project <org/project>.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
            )
        } else {
            format!(
                r#"error: Boundary rejected the cloud query (HTTP 400).
  Endpoint: {endpoint}
  Code: ENVIRONMENT_REQUIRED

  To continue, choose one:
    • Select an environment with --environment <name> and, when needed, --project <org/project>.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
            )
        };
        assert_eq!(diagnostic(&output), expected);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_telemetry_rejection_preserves_the_public_api_message() {
    let temp = fixture("function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
            "code": "ACCESS_REQUIRED", "retryable": false,
            "message": "This credential cannot ingest into the selected environment.",
            "internal": KEY,
        })))
        .expect(1)
        .mount(&server)
        .await;
    let output = execute_case(temp.path(), &server.uri(), KEY, &["run", "main"]);
    assert!(!output.status.success());
    let endpoint = server.uri();
    assert_eq!(
        diagnostic(&output),
        format!(
            r#"error: Boundary denied cloud telemetry ingestion (HTTP 403).
  Endpoint: {endpoint}
  Code: ACCESS_REQUIRED
  This credential cannot ingest into the selected environment.

  To continue, choose one:
    • Use a credential with ingestion permission for the selected project and environment.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
        )
    );
}

#[test]
fn cloud_configuration_errors_share_diagnostics_without_exposing_invalid_url_credentials() {
    let temp = fixture("function main() -> int { 7 }\n");
    for args in [&["query", "SELECT 1"][..], &["auth", "login"][..]] {
        let output = execute_case(
            temp.path(),
            "https://user:private-password@example.test",
            KEY,
            args,
        );
        assert!(!output.status.success(), "{output:?}");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let expected = if args[0] == "query" {
            r#"error: Boundary configuration is invalid: Boundary API URL must be a base URL without credentials, query or fragment.

  To continue, choose one:
    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml.
    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.

  Query failed."#
        } else {
            r#"error: Boundary configuration is invalid: Boundary API URL must be a base URL without credentials, query or fragment.

  To continue, choose one:
    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml.

  Authentication command failed."#
        };
        assert_eq!(diagnostic(&output), expected);
        assert!(!stderr.contains("private-password"), "{stderr}");
        assert!(!stderr.contains("caused by:"), "{stderr}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_session_rejection_cancels_execution_without_waiting_to_start() {
    let temp = fixture(
        r#"
function main() -> int {
    baml.io.println("execution-started");
    let child = spawn with baml.spawn.Root.new() {
        baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
        1
    };
    baml.sys.sleep(baml.time.Duration.from_milliseconds(60000n));
    await child;
    7
}
"#,
    );
    let server = MockServer::start().await;
    let requested_at = std::sync::Arc::new(std::sync::Mutex::new(None));
    let responding = requested_at.clone();
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(move |_: &Request| {
            *responding.lock().unwrap() = Some(std::time::Instant::now());
            ResponseTemplate::new(401).set_delay(std::time::Duration::from_millis(500))
        })
        .expect(1)
        .mount(&server)
        .await;
    let output = execute_case(
        temp.path(),
        &server.uri(),
        "bdry_session_invalid",
        &["run", "main"],
    );
    let requested = requested_at
        .lock()
        .unwrap()
        .unwrap_or_else(|| panic!("heartbeat was never requested: {output:?}"));
    assert!(
        requested.elapsed() < std::time::Duration::from_secs(5),
        "initial refusal must interrupt the 60-second call: {output:?}"
    );
    assert!(
        !String::from_utf8_lossy(&output.stderr).contains("abandoned"),
        "rooted work must cancel too: {output:?}"
    );
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("execution-started"),
        "execution must start before authorization finishes: {output:?}"
    );
    assert_api_key_rejection(&output, &server.uri(), false, true);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn authorization_rejection_after_initial_success_does_not_fail_execution() {
    let temp = fixture("function main() -> int { 7 }\n");
    for status in [401, 403] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/heartbeat$"))
            .respond_with(ResponseTemplate::new(204))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path_regex(r"/uploads:prepare$"))
            .respond_with(ResponseTemplate::new(status))
            .expect(1)
            .mount(&server)
            .await;
        let output = execute_case(temp.path(), &server.uri(), KEY, &["run", "main"]);
        assert!(output.status.success(), "{output:?}");
        assert_eq!(
            String::from_utf8_lossy(&output.stderr)
                .lines()
                .filter(|line| line.starts_with("warning: telemetry recording failed:"))
                .collect::<Vec<_>>(),
            [r#"warning: telemetry recording failed: cloud recording delivery: Unauthorized"#]
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn initial_rejection_fails_expressions_and_test_runs() {
    let temp = fixture(
        "function main() -> int { 7 }\ntestset \"smoke\" { test \"ok\" { assert.is_true(true) } }\n",
    );
    for args in [&["run", "-e", "1 + 2"][..], &["test"][..]] {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path_regex(r"/heartbeat$"))
            .respond_with(
                ResponseTemplate::new(403).set_delay(std::time::Duration::from_millis(300)),
            )
            .expect(1)
            .mount(&server)
            .await;
        let output = execute_case(temp.path(), &server.uri(), KEY, args);
        assert!(!output.status.success(), "{args:?}: {output:?}");
        assert_ingest_forbidden(&output, &server.uri());
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn standalone_binary_reports_initial_refusal_after_a_fast_main() {
    common::ensure_built();
    let temp = fixture("function main() -> int { 7 }\n");
    let binary = temp.path().join("packed");
    let packed = execute_case(
        temp.path(),
        "http://127.0.0.1:1",
        KEY,
        &["pack", "main", "-o", binary.to_str().unwrap()],
    );
    assert!(packed.status.success(), "{packed:?}");
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(401).set_delay(std::time::Duration::from_millis(300)))
        .expect(1)
        .mount(&server)
        .await;
    let output = Command::new(binary)
        .current_dir(temp.path())
        .env("BOUNDARY_API_URL", server.uri())
        .env("BOUNDARY_API_KEY", KEY)
        .env("BOUNDARY_PROJECT", "acme/app")
        .env("BAML_TELEMETRY", "medium")
        .output()
        .unwrap();
    assert!(!output.status.success(), "{output:?}");
    assert_api_key_rejection(&output, &server.uri(), false, false);
}

#[test]
fn invalid_boundary_url_cancels_with_or_without_credentials_unless_local_is_selected() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let invalid = "not a URL";
    for manifest_url in [false, true] {
        for key in [None, Some(KEY), Some("local")] {
            let manifest = if manifest_url {
                "[package]\nname = \"invalid-url-test\"\n[boundary]\napi_url = \"not a URL\"\n"
            } else {
                "[package]\nname = \"invalid-url-test\"\n"
            };
            std::fs::write(temp.path().join("baml.toml"), manifest).unwrap();
            let mut command = Command::new(common::baml_cli());
            command
                .args(["--agent-skill-check", "off", "run", "main"])
                .current_dir(temp.path())
                .env("BAML_TELEMETRY", "medium")
                .env("BAML_HOME", common::shared_baml_home())
                .env("BAML_CLI_ALLOW_DIRECT", "1")
                .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
                .env("DO_NOT_TRACK", "1")
                .env_remove("BOUNDARY_PROJECT")
                .env_remove("BOUNDARY_API_KEY")
                .env_remove("BOUNDARY_API_URL");
            if !manifest_url {
                command.env("BOUNDARY_API_URL", invalid);
            }
            if let Some(key) = key {
                command.env("BOUNDARY_API_KEY", key);
            }
            let output = command.output().unwrap();
            if key == Some("local") {
                assert!(output.status.success(), "{output:?}");
                assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
            } else {
                assert!(!output.status.success(), "{output:?}");
                assert_eq!(String::from_utf8_lossy(&output.stdout), "");
                assert_eq!(
                    diagnostic(&output),
                    r#"error: Boundary configuration is invalid: Boundary API URL must be a valid absolute URL.

  To continue, choose one:
    • Fix BOUNDARY_API_URL or boundary.api_url in baml.toml.
    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off.

  Execution cancelled."#
                );
            }
        }
    }
}

#[test]
fn corrupt_saved_login_errors_for_query_and_execution_but_explicit_local_recording_works() {
    use sha2::Digest as _;

    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let home = temp.path().join("home");
    let endpoint = "https://example.com";
    let file = home.join("login").join("cache").join(format!(
        "{}.json",
        hex::encode(sha2::Sha256::digest(endpoint.as_bytes()))
    ));
    std::fs::create_dir_all(file.parent().unwrap()).unwrap();
    std::fs::write(&file, "{bdry_session_never_print_this").unwrap();
    std::fs::write(home.join("config.toml"), "[update]\nauto_check = false\n").unwrap();

    for (args, outcome, recovery) in [
        (
            ["run", "main"],
            "Execution cancelled.",
            r#"    • Record locally: rerun with BOUNDARY_API_KEY=local.
    • Disable recording: rerun with BAML_TELEMETRY=off."#,
        ),
        (
            ["query", "SELECT 7"],
            "Query failed.",
            "    • Query local recordings: rerun with BOUNDARY_API_KEY=local or --local.",
        ),
    ] {
        let output = Command::new(common::baml_cli())
            .args(["--agent-skill-check", "off"])
            .args(args)
            .current_dir(temp.path())
            .env("BAML_HOME", &home)
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("BOUNDARY_API_URL", endpoint)
            .env("BAML_TELEMETRY", "medium")
            .env("DO_NOT_TRACK", "1")
            .env_remove("BOUNDARY_PROJECT")
            .env_remove("BOUNDARY_API_KEY")
            .output()
            .unwrap();
        assert!(!output.status.success(), "{output:?}");
        assert_eq!(
            diagnostic(&output),
            format!(
                r#"error: The saved Boundary login in the local credential file at {} is invalid.
  Endpoint: {endpoint}

  To continue, choose one:
    • Remove the invalid saved login: run `baml auth logout`, then `baml auth login`.
    • Set BOUNDARY_API_KEY to a valid API key.
{recovery}

  {outcome}"#,
                file.display()
            )
        );
    }

    for (variable, value) in [("BOUNDARY_API_KEY", "local"), ("BAML_TELEMETRY", "off")] {
        let output = Command::new(common::baml_cli())
            .args(["--agent-skill-check", "off", "run", "main"])
            .current_dir(temp.path())
            .env("BAML_HOME", &home)
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("BOUNDARY_API_URL", endpoint)
            .env("BAML_TELEMETRY", "medium")
            .env("DO_NOT_TRACK", "1")
            .env_remove("BOUNDARY_PROJECT")
            .env_remove("BOUNDARY_API_KEY")
            .env(variable, value)
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    }
}
