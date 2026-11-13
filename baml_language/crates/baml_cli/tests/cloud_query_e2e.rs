//! Cloud queries follow the CLI's exit-code and output contract.
use std::process::Command;

use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::path};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_cloud_query_fails() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/query"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let outcome = json!({"queryOutcome": {
                "queryId": body["queryId"], "status": "cancelled"
            }});
            ResponseTemplate::new(200).set_body_raw(
                // Frames that arrive before the cancellation are not a result.
                format!(
                    "{}\n{}\n{outcome}\n",
                    json!({"columns": [{"name": "value", "type": "int64"}]}),
                    json!({"row": [1]})
                ),
                "application/x-ndjson",
            )
        })
        .expect(1)
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .args([
            "--agent-skill-check",
            "off",
            "query",
            "--output-format",
            "json",
            "SELECT 1",
        ])
        .current_dir(home.path())
        .env("BAML_HOME", home.path())
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DO_NOT_TRACK", "1")
        .env("BOUNDARY_API_URL", server.uri())
        .env("BOUNDARY_API_KEY", "bdry_secret_query_cancelled")
        .env_remove("BOUNDARY_PROJECT")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1), "{output:?}");
    assert!(output.stdout.is_empty(), "{output:?}");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cancelled"),
        "{output:?}"
    );
}
