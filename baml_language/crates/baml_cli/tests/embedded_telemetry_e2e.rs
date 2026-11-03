//! Pack/generate embed the named API key without build-time cloud requests.
#[path = "../../btel_bcs/tests/support/mod.rs"]
mod cloud_protocol;
mod common;
use std::{
    path::Path,
    process::{Command, Output},
};

use serde_json::{Value, json};
use wiremock::{
    Mock, MockServer, Request, ResponseTemplate,
    matchers::{method, path_regex},
};

fn cli(project: &Path, api: &str, args: &[&str]) -> Output {
    Command::new(common::baml_cli())
        .args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(project)
        .env("BOUNDARY_API_URL", api)
        .env("BOUNDARY_API_KEY", "bdry_secret_builder_do_not_embed")
        .env("BAML_WRAPPER_BOUNDARY_API_KEY", "bdry_secret_embedded")
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
[pack]
[bridge]
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
{overrides}
"#
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packed_api_key_is_embedded_offline_and_fixed_with_endpoint_override() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let build_server = MockServer::start().await;
    succeeded(&cli(
        temp.path(),
        &build_server.uri(),
        &[
            "pack",
            "main",
            "--embed-telemetry",
            "BAML_WRAPPER_BOUNDARY_API_KEY",
            "-o",
            "out",
        ],
    ));
    assert!(build_server.received_requests().await.unwrap().is_empty());
    let binary = std::fs::read(temp.path().join("out")).unwrap();
    assert!(
        binary
            .windows(b"bdry_secret_embedded".len())
            .any(|bytes| bytes == b"bdry_secret_embedded")
    );
    assert!(
        !binary
            .windows(b"bdry_secret_builder_do_not_embed".len())
            .any(|bytes| bytes == b"bdry_secret_builder_do_not_embed")
    );
    let run_server = MockServer::start().await;
    ingest_api(&run_server).await;
    let run = || {
        Command::new(temp.path().join("out"))
            .env("BOUNDARY_API_URL", run_server.uri())
            .env_remove("BAML_WRAPPER_BOUNDARY_API_KEY")
            .env("BOUNDARY_API_KEY", "bdry_secret_customer_do_not_use")
            .env("BOUNDARY_PROJECT", "customer/other-project")
            .env("BAML_TELEMETRY", "off")
            .env("BAML_HOME", common::shared_baml_home())
            .output()
            .unwrap()
    };
    let output = run();
    succeeded(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert_eq!(String::from_utf8_lossy(&output.stderr), "");
    let requests = run_server.received_requests().await.unwrap();
    assert!(requests.iter().any(|request| request.method == "PUT"));
    for request in requests.iter().filter(|request| request.method == "POST") {
        assert_eq!(
            request.headers["authorization"],
            "Bearer bdry_secret_embedded"
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body.get("buildId"), None);
        assert_eq!(body["target"], json!({}));
    }
    assert!(build_server.received_requests().await.unwrap().is_empty());

    run_server.reset().await;
    Mock::given(method("POST"))
        .respond_with(ResponseTemplate::new(503))
        .expect(4)
        .mount(&run_server)
        .await;
    let output = run();
    succeeded(&output);
    assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
    assert!(String::from_utf8_lossy(&output.stderr).contains("warning:"));
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
    ingest_api(&server).await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &[
            "pack",
            "main",
            "--embed-telemetry",
            "BAML_WRAPPER_BOUNDARY_API_KEY",
            "-o",
            "out",
        ],
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
        .filter(|request| request.method == "POST")
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
async fn generate_embeds_named_api_key_and_plain_generate_omits_it() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &[
            "generate",
            "--embed-telemetry",
            "BAML_WRAPPER_BOUNDARY_API_KEY",
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
    assert_eq!(policy.api_url.as_deref(), Some(server.uri().as_str()));
    assert_eq!(
        policy.embedded.as_ref().unwrap().api_key.expose(),
        "bdry_secret_embedded"
    );
    assert!(server.received_requests().await.unwrap().is_empty());
    succeeded(&cli(temp.path(), &server.uri(), &["generate"]));
    let inlined =
        std::fs::read_to_string(temp.path().join("generated/baml_sdk/src/_inlinedbaml.rs"))
            .unwrap();
    assert!(!inlined.contains("bdry_secret_embedded"));
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_publisher_settings_fail_before_writing_an_artifact() {
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
    http_bodies
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
    http_bodies
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
                "BAML_WRAPPER_BOUNDARY_API_KEY",
                "-o",
                "out",
            ][..],
            &[
                "generate",
                "--embed-telemetry",
                "BAML_WRAPPER_BOUNDARY_API_KEY",
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

#[test]
fn embedding_requires_a_named_nonempty_variable_without_falling_back_to_other_credentials() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    for command in ["pack", "generate"] {
        for value in [None, Some(""), Some("  ")] {
            let mut cli = Command::new(common::baml_cli());
            cli.args(["--agent-skill-check", "off", command]);
            if command == "pack" {
                cli.args(["main", "-o", "out"]);
            }
            cli.args(["--embed-telemetry", "BAML_TEST_MISSING_EMBED_KEY"])
                .current_dir(temp.path())
                .env("BOUNDARY_API_KEY", "bdry_secret_do_not_fall_back")
                .env_remove("BAML_TEST_MISSING_EMBED_KEY")
                .env("BAML_HOME", common::shared_baml_home())
                .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
                .env("DO_NOT_TRACK", "1");
            if let Some(value) = value {
                cli.env("BAML_TEST_MISSING_EMBED_KEY", value);
            }
            let output = cli.output().unwrap();
            assert!(!output.status.success());
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains(if value.is_none() {
                    "environment variable BAML_TEST_MISSING_EMBED_KEY is not set."
                } else {
                    "environment variable BAML_TEST_MISSING_EMBED_KEY is empty."
                }),
                "{stderr}"
            );
            assert!(!stderr.contains("bdry_secret_do_not_fall_back"));
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
        let output = Command::new(common::baml_cli())
            .args(["--agent-skill-check", "off", command, "--embed-telemetry"])
            .env("BAML_HOME", common::shared_baml_home())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("a value is required"));
    }
}
