//! Cloud queries preserve the CLI's public exit-code contract.
use std::process::Command;

use serde_json::{Value, json};
use wiremock::{Mock, MockServer, Request, ResponseTemplate, matchers::path};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_cloud_query_returns_the_cancellation_exit_code() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/query"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            let outcome = json!({"queryOutcome": {
                "queryId": body["queryId"], "status": "cancelled"
            }});
            ResponseTemplate::new(200).set_body_raw(
                format!("{}\n{outcome}\n", json!({"columns": []})),
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
            "--format",
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
    assert_eq!(output.status.code(), Some(4), "{output:?}");
    let result: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|error| panic!("{error}: {output:?}"));
    assert_eq!(result["outcome"]["status"], "cancelled");
    assert_eq!(result["rows"], json!([]));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn published_cloud_examples_select_the_documented_target() {
    let server = MockServer::start().await;
    Mock::given(path("/v1/query"))
        .respond_with(|request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["target"]["project"], "acme/app");
            assert!(body["sql"].as_str().unwrap().starts_with("SELECT "));
            let columns = json!({"columns": []});
            let outcome =
                json!({"queryOutcome": {"queryId": body["queryId"], "status": "complete"}});
            ResponseTemplate::new(200)
                .set_body_raw(format!("{columns}\n{outcome}\n"), "application/x-ndjson")
        })
        .mount(&server)
        .await;
    let home = tempfile::tempdir().unwrap();
    let mut examples = 0;
    for (topic, doc) in baml_builtins2::language_topics() {
        if topic != "query" && !topic.starts_with("query.") {
            continue;
        }
        for line in doc.syntax.as_deref().unwrap_or_default().lines() {
            let Some(command) = line.trim().strip_prefix("baml query ") else {
                continue;
            };
            let Some((flags, quoted)) = command.split_once('"') else {
                continue;
            };
            if !flags.split_whitespace().any(|flag| flag == "--project") {
                continue;
            }
            let sql = quoted.strip_suffix('"').expect("SQL is the last argument");
            let output = cloud_cli(home.path(), &server.uri())
                .arg("query")
                .args(flags.split_whitespace())
                .arg(sql)
                .output()
                .unwrap();
            assert!(output.status.success(), "{topic}: {line}\n{output:?}");
            examples += 1;
        }
    }
    assert!(examples > 0);
    let requests = server.received_requests().await.unwrap();
    assert_eq!(requests.len(), examples);
    assert!(requests.iter().any(|request| {
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        body["target"]["environment"] == "development"
    }));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn documented_cloud_jsonl_outcomes_and_exit_codes() {
    for (status, code) in [
        ("complete", 0),
        ("incomplete", 0),
        ("truncated", 3),
        ("failed", 5),
    ] {
        let server = MockServer::start().await;
        Mock::given(path("/v1/query"))
            .respond_with(move |request: &Request| {
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                let columns = json!({"columns": [{"name": "n", "type": "Int64"}]});
                let row = json!({"row": [1]});
                let outcome = json!({"queryOutcome": {
                    "queryId": body["queryId"], "status": status,
                    "error": if status == "failed" { json!({"code": "INVALID_SQL", "message": "Invalid SQL"}) } else { Value::Null }
                }});
                ResponseTemplate::new(200).set_body_raw(format!("{columns}\n{row}\n{outcome}\n"), "application/x-ndjson")
            })
            .expect(1)
            .mount(&server)
            .await;
        let home = tempfile::tempdir().unwrap();
        let output = cloud_cli(home.path(), &server.uri())
            .args(["query", "--format", "jsonl", "SELECT 1 AS n"])
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(code), "{status}: {output:?}");
        let text = String::from_utf8(output.stdout).unwrap();
        let frames: Vec<Value> = text
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert!(frames[0]["columns"].is_array());
        assert_eq!(frames[1]["row"], json!([1]));
        assert_eq!(frames[2]["queryOutcome"]["status"], status);
        if status == "incomplete" {
            assert!(String::from_utf8_lossy(&output.stderr).contains("incomplete"));
        }
    }
}

fn cloud_cli(home: &std::path::Path, endpoint: &str) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_baml-cli"));
    cmd.args(["--agent-skill-check", "off"])
        .current_dir(home)
        .env("BAML_HOME", home)
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("DO_NOT_TRACK", "1")
        .env("BOUNDARY_API_URL", endpoint)
        .env("BOUNDARY_API_KEY", "bdry_secret_query_docs")
        .env_remove("BOUNDARY_PROJECT");
    cmd
}
