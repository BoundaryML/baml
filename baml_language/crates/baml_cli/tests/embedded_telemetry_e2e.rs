//! Real pack/generate commands against the proposed public credential API.
#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;
mod common;
use std::{
    path::Path,
    process::{Command, Output},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
};

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{header, method, path, path_regex},
};

fn cli(project: &Path, api: &str, args: &[&str]) -> Output {
    Command::new(common::baml_cli())
        .args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(project)
        .env("BOUNDARY_API_URL", api)
        .env("BOUNDARY_API_KEY", "bdry_secret_builder_private")
        .env_remove("BOUNDARY_PROJECT")
        .env("BAML_HOME", common::shared_baml_home())
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap()
}
fn succeeded(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
async fn mint_api(server: &MockServer) {
    let count = Arc::new(AtomicUsize::new(0));
    Mock::given(method("POST")).and(path("/v1/build-tokens"))
        .and(header("Authorization", "Bearer bdry_secret_builder_private"))
        .respond_with(move |request: &Request| {
            let body: Value = serde_json::from_slice(&request.body).unwrap();
            assert_eq!(body["target"], json!({"kind":"handle","project":"publisher/app","environment":"staging"}));
            assert_eq!(body["bytecodeDigest"]["version"], 1);
            assert_eq!(body["bytecodeDigest"]["algorithm"], "sha256");
            let number = count.fetch_add(1, Ordering::SeqCst);
            ResponseTemplate::new(201).set_body_json(json!({"buildId":"build-1", "tokenId":format!("token-{number}"),
                "token": format!("bdry_public_{number:064x}"), "productId":"product-1",
                "destination":{"orgId":"o","projectId":"p","environmentId":"e"}, "bytecodeDigest":body["bytecodeDigest"]}))
        }).mount(server).await;
}
async fn ingest_api(server: &MockServer) {
    let base = server.uri();
    Mock::given(method("POST"))
        .and(path_regex(r"/uploads:prepare$"))
        .respond_with(move |request: &Request| {
            ResponseTemplate::new(200).set_body_json(cloud_protocol::response(request, &base, &[]))
        })
        .mount(server)
        .await;
    Mock::given(method("POST"))
        .and(path_regex(r"/heartbeat$"))
        .respond_with(ResponseTemplate::new(204))
        .mount(server)
        .await;
    Mock::given(method("PUT"))
        .and(path_regex(r"^/put/"))
        .respond_with(ResponseTemplate::new(200))
        .mount(server)
        .await;
}
fn manifest(overrides: &str) -> String {
    format!(
        r#"[package]
name = "embedded-test"
[boundary]
project = "publisher/app"
[pack]
telemetry_environment = "staging"
[bridge]
telemetry_environment = "staging"
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
{overrides}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packed_public_credentials_are_fresh_and_fixed_with_endpoint_override() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let build_server = MockServer::start().await;
    mint_api(&build_server).await;
    for _ in 0..2 {
        succeeded(&cli(
            temp.path(),
            &build_server.uri(),
            &["pack", "main", "--embed-telemetry", "-o", "out"],
        ));
    }
    let mints = build_server.received_requests().await.unwrap();
    assert_eq!(mints.len(), 2);
    assert_ne!(
        mints[0].headers["idempotency-key"],
        mints[1].headers["idempotency-key"]
    );
    let first: Value = serde_json::from_slice(&mints[0].body).unwrap();
    let second: Value = serde_json::from_slice(&mints[1].body).unwrap();
    assert_eq!(first["bytecodeDigest"], second["bytecodeDigest"]);
    let run_server = MockServer::start().await;
    ingest_api(&run_server).await;
    let output = Command::new(temp.path().join("out"))
        .env("BOUNDARY_API_URL", run_server.uri())
        .env("BOUNDARY_API_KEY", "bdry_secret_customer_do_not_use")
        .env("BOUNDARY_PROJECT", "customer/other-project")
        .env("BAML_TELEMETRY", "off")
        .env("BAML_HOME", common::shared_baml_home())
        .output()
        .unwrap();
    succeeded(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let requests = run_server.received_requests().await.unwrap();
    assert!(requests.iter().any(|request| request.method == "PUT"));
    for request in requests.iter().filter(|request| request.method == "POST") {
        assert_eq!(
            request.headers["authorization"],
            format!("Bearer bdry_public_{:064x}", 1)
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["buildId"], "build-1");
        assert_eq!(
            body["target"],
            json!({"orgId":"o","projectId":"p","environmentId":"e"})
        );
    }
    assert_eq!(build_server.received_requests().await.unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn explicit_destination_bindings_use_caller_credentials_and_allow_local_or_off() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(
        temp.path().join("baml.toml"),
        manifest(
            r#"[pack.env_var_names]
BOUNDARY_PROJECT = "ACME_PROJECT"
BOUNDARY_API_KEY = "ACME_KEY"
BAML_TELEMETRY = "ACME_TELEMETRY"
"#,
        ),
    )
    .unwrap();
    let server = MockServer::start().await;
    mint_api(&server).await;
    ingest_api(&server).await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &["pack", "main", "--embed-telemetry", "-o", "out"],
    ));
    let output = Command::new(temp.path().join("out"))
        .env("BOUNDARY_API_URL", server.uri())
        .env("BOUNDARY_API_KEY", "bdry_secret_generic_ignored")
        .env("BOUNDARY_PROJECT", "generic/ignored")
        .env("ACME_KEY", "bdry_secret_customer")
        .env("ACME_PROJECT", "customer/app")
        .env("BAML_HOME", common::shared_baml_home())
        .output()
        .unwrap();
    succeeded(&output);
    for request in server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|request| request.method == "POST" && request.url.path() != "/v1/build-tokens")
    {
        assert_eq!(
            request.headers["authorization"],
            "Bearer bdry_secret_customer"
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["target"], json!({"project":"customer/app"}));
        assert_eq!(body.get("buildId"), None);
    }
    let count = server.received_requests().await.unwrap().len();
    for (key, level) in [("local", "medium"), ("bdry_secret_invalid", "off")] {
        let output = Command::new(temp.path().join("out"))
            .env("BOUNDARY_API_URL", server.uri())
            .env("ACME_KEY", key)
            .env("ACME_TELEMETRY", level)
            .env("BAML_HOME", common::shared_baml_home())
            .output()
            .unwrap();
        succeeded(&output);
        assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    }
    assert_eq!(server.received_requests().await.unwrap().len(), count);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn generate_embeds_registration_matching_bytecode_and_plain_generate_never_mints() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    mint_api(&server).await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &[
            "generate",
            "--embed-telemetry",
            "--telemetry-environment=staging",
        ],
    ));
    let inlined =
        std::fs::read_to_string(temp.path().join("generated/baml_sdk/src/_inlinedbaml.rs"))
            .unwrap();
    // Rust's emitter places the metadata in a quoted string inside Option::Some.
    let literal = inlined
        .split("::core::option::Option::Some(")
        .nth(1)
        .unwrap()
        .trim();
    let mut stream = serde_json::Deserializer::from_str(literal).into_iter::<String>();
    let metadata = stream.next().unwrap().unwrap();
    let manifest: toml::Value = metadata.parse().unwrap();
    let policy: btel_settings::artifact::ArtifactTelemetry =
        manifest["__baml_codegen"]["telemetry"]
            .clone()
            .try_into()
            .unwrap();
    let program =
        std::fs::read(temp.path().join("generated/baml_sdk/src/_inlinedbaml.bin")).unwrap();
    policy.verify_build(&program).unwrap();
    assert_eq!(policy.embedded.as_ref().unwrap().build_id, "build-1");
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    succeeded(&cli(temp.path(), &server.uri(), &["generate"]));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_publisher_settings_fail_before_minting_or_writing_an_artifact() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    for (table, key, value, expected) in [
        (
            "pack",
            "on_inital_telemetry_failure",
            "\"ignore\"",
            r#"error: Unknown setting `pack.on_inital_telemetry_failure` in baml.toml.

  Did you mean `pack.on_initial_telemetry_failure`?

  Valid [pack] settings:
    on_initial_telemetry_failure
    initial_telemetry_warning_message
    telemetry_environment
    env_var_names"#,
        ),
        (
            "bridge",
            "extra",
            "true",
            r#"error: Unknown setting `bridge.extra` in baml.toml.

  Valid [bridge] settings:
    on_initial_telemetry_failure
    initial_telemetry_warning_message
    telemetry_environment
    env_var_names"#,
        ),
        (
            "bridge.env_var_names",
            "BOUNDARY_API_KE",
            "\"CUSTOM_KEY\"",
            r#"error: Unknown setting `bridge.env_var_names.BOUNDARY_API_KE` in baml.toml.

  Did you mean `bridge.env_var_names.BOUNDARY_API_KEY`?

  Valid [bridge.env_var_names] settings:
    BAML_TELEMETRY
    BOUNDARY_PROJECT
    BOUNDARY_API_KEY"#,
        ),
        (
            "boundary",
            "api_ur",
            "\"https://api.cloud.boundaryml.com\"",
            r#"error: Unknown setting `boundary.api_ur` in baml.toml.

  Did you mean `boundary.api_url`?

  Valid [boundary] settings:
    project
    api_url"#,
        ),
        (
            "pack",
            "telemetry_environment",
            "false",
            r#"error: pack.telemetry_environment must be a non-empty string."#,
        ),
    ] {
        let mut config: toml::Value = manifest("").parse().unwrap();
        let parsed: toml::Value = format!("[{table}]\n{key} = {value}").parse().unwrap();
        if table.ends_with(".env_var_names") {
            config["bridge"].as_table_mut().unwrap().insert(
                "env_var_names".into(),
                parsed["bridge"]["env_var_names"].clone(),
            );
        } else {
            config[table]
                .as_table_mut()
                .unwrap()
                .insert(key.into(), parsed[table][key].clone());
        }
        std::fs::write(
            temp.path().join("baml.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();
        for args in [
            &[
                "pack",
                "main",
                "--embed-telemetry",
                "--telemetry-environment=staging",
                "-o",
                "out",
            ][..],
            &[
                "generate",
                "--embed-telemetry",
                "--telemetry-environment=staging",
            ][..],
        ] {
            let output = cli(temp.path(), &server.uri(), args);
            assert!(!output.status.success(), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            let error = stderr
                .find("error: ")
                .unwrap_or_else(|| panic!("missing diagnostic: {stderr}"));
            assert_eq!(stderr[error..].trim(), expected);
            assert_eq!(server.received_requests().await.unwrap().len(), 0);
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
    }
}
