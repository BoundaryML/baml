//! Pack/generate verify exclusive environment write access and embed the original API key.
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
    matchers::{body_json, header, method, path, path_regex},
};

fn cli(project: &Path, api: &str, args: &[&str]) -> Output {
    Command::new(common::baml_cli())
        .args(["--agent-skill-check", "off"])
        .args(args)
        .current_dir(project)
        .env("BOUNDARY_API_URL", api)
        .env("BOUNDARY_API_KEY", "bdry_secret_builder_do_not_embed")
        .env("BAML_WRAPPER_BOUNDARY_API_KEY", "bdry_secret_embedded")
        .env(
            "BOUNDARY_PROJECT",
            "ambient/project_must_not_override_manifest",
        )
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
fn verification_response() -> Value {
    json!({
        "id":"key_embed", "name":"Embedded key", "apiKeyType":"public",
        "organization":{"id":"org_publisher","slug":"publisher"},
        "project":{"id":"project_app","name":"app"},
        "ingestEnvironments":[{"id":"env_opaque","name":"staging","projectId":"project_app","organizationId":"org_publisher"}],
        "permissions":{"ingest":[{"kind":"environment","environmentId":"env_opaque"}],"query":[],"personalEnvironments":"creator_only"}
    })
}
async fn embedding_api(server: &MockServer) {
    Mock::given(method("POST"))
        .and(path("/v1/auth/verify"))
        .and(body_json(json!({"fullResponse":true})))
        .and(header("authorization", "Bearer bdry_secret_embedded"))
        .respond_with(ResponseTemplate::new(200).set_body_json(verification_response()))
        .mount(server)
        .await;
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
[pack.build]
embed_telemetry_env = "staging"
embed_telemetry_api_key_from_env_var = "BAML_WRAPPER_BOUNDARY_API_KEY"
[bridge.build]
embed_telemetry_env = "staging"
embed_telemetry_api_key_from_env_var = "BAML_WRAPPER_BOUNDARY_API_KEY"
[generator.rust]
output_type = "rust"
output_dir = "generated"
naming_convention = "preserve-case"
{overrides}
"#
    )
}

#[tokio::test]
async fn embedding_rejects_cli_variable_names_before_contacting_bcs() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    for command in ["pack", "generate"] {
        for extra in [
            vec!["--embed-telemetry", "BAML_WRAPPER_BOUNDARY_API_KEY"],
            vec!["--embed-telemetry=BAML_WRAPPER_BOUNDARY_API_KEY"],
        ] {
            let mut args = vec![command];
            if command == "pack" {
                args.extend(["main", "-o", "out"]);
            }
            args.extend(extra);
            let output = cli(temp.path(), &server.uri(), &args);
            assert!(!output.status.success(), "{output:?}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains("unexpected") || stderr.contains("unrecognized subcommand"),
                "{stderr}"
            );
            assert!(server.received_requests().await.unwrap().is_empty());
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_rejects_unverified_keys_and_legacy_or_malformed_responses() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    for command in ["pack", "generate"] {
        let args = if command == "pack" {
            vec!["pack", "main", "--embed-telemetry", "-o", "out"]
        } else {
            vec!["generate", "--embed-telemetry"]
        };
        for (status, body) in [
            (
                401,
                json!({"code":"UNAUTHORIZED","retryable":false,"message":"Refused bdry_secret_embedded"}),
            ),
            (
                503,
                json!({"code":"AUTHORIZATION_STATE_PENDING","retryable":true}),
            ),
            (204, Value::Null),
            (200, json!({})),
        ] {
            server.reset().await;
            let response = if status == 204 {
                ResponseTemplate::new(status)
            } else {
                ResponseTemplate::new(status).set_body_json(body)
            };
            Mock::given(method("POST"))
                .and(path("/v1/auth/verify"))
                .and(body_json(json!({"fullResponse":true})))
                .and(header("authorization", "Bearer bdry_secret_embedded"))
                .respond_with(response)
                .mount(&server)
                .await;
            let output = cli(temp.path(), &server.uri(), &args);
            assert!(!output.status.success(), "{command}/{status}");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(stderr.contains("API-key verification failed"), "{stderr}");
            assert!(!stderr.contains("bdry_secret_embedded"));
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.len(), 1);
            assert_eq!(
                serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
                json!({"fullResponse":true})
            );
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_rejects_ownership_permissions_and_inconsistent_environment_metadata() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    let mut cases = Vec::new();
    for (pointer, value) in [
        ("/organization/slug", json!(null)),
        ("/organization/slug", json!("other")),
        ("/organization/id", json!("")),
        ("/project/name", json!("other")),
        ("/project/name", json!("bdry_secret_embedded")),
        ("/project/id", json!("")),
        ("/ingestEnvironments", json!([])),
        ("/ingestEnvironments/0/name", json!("production")),
        ("/ingestEnvironments/0/id", json!("wrong")),
        ("/ingestEnvironments/0/projectId", json!("other")),
        ("/ingestEnvironments/0/organizationId", json!("other")),
        ("/permissions/ingest", json!([])),
        (
            "/permissions/ingest",
            json!([{"kind":"project","projectId":"project_app"}]),
        ),
        (
            "/permissions/ingest",
            json!([{"kind":"organization","orgId":"org_publisher"}]),
        ),
        (
            "/permissions/ingest",
            json!([{"kind":"environment","environmentId":"other"}]),
        ),
        (
            "/permissions/ingest",
            json!([{"kind":"environment","environmentId":"env_opaque"},{"kind":"environment","environmentId":"other"}]),
        ),
        (
            "/permissions/query",
            json!([{"kind":"organization","orgId":"org_publisher"}]),
        ),
    ] {
        let mut body = verification_response();
        *body.pointer_mut(pointer).unwrap() = value;
        cases.push(body);
    }
    for target in [
        json!({"kind":"organization","orgId":"org_publisher"}),
        json!({"kind":"project","projectId":"project_app"}),
        json!({"kind":"environment","environmentId":"env_opaque"}),
    ] {
        for read_only in [false, true] {
            let mut body = verification_response();
            body["permissions"]["query"] = json!([target]);
            if read_only {
                body["permissions"]["ingest"] = json!([]);
            }
            cases.push(body);
        }
    }
    let mut body = verification_response();
    body["permissions"]["mint"] = json!([]);
    cases.push(body);
    let mut body = verification_response();
    body.as_object_mut().unwrap().remove("ingestEnvironments");
    cases.push(body);
    for command in ["pack", "generate"] {
        for body in &cases {
            server.reset().await;
            Mock::given(path("/v1/auth/verify"))
                .and(body_json(json!({"fullResponse":true})))
                .respond_with(ResponseTemplate::new(200).set_body_json(body))
                .expect(1)
                .mount(&server)
                .await;
            let args = if command == "pack" {
                vec!["pack", "main", "--embed-telemetry", "-o", "out"]
            } else {
                vec!["generate", "--embed-telemetry"]
            };
            let output = cli(temp.path(), &server.uri(), &args);
            assert!(!output.status.success(), "{command}: {body}");
            assert!(!String::from_utf8_lossy(&output.stderr).contains("bdry_secret_embedded"));
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_requires_project_and_expected_environment_before_any_http() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    for command in ["pack", "generate"] {
        let baseline: toml::Value = manifest("").parse().unwrap();
        let mut cases = Vec::new();
        for project in [
            None,
            Some(""),
            Some("publisher"),
            Some("/app"),
            Some("publisher/"),
            Some("publisher/app/extra"),
            Some("publisher /app"),
        ] {
            let mut config = baseline.clone();
            if let Some(project) = project {
                config["boundary"]["project"] = project.into();
            } else {
                config["boundary"].as_table_mut().unwrap().remove("project");
            }
            cases.push(config);
        }
        for environment in [None, Some(""), Some("  ")] {
            let mut config = baseline.clone();
            let producer = if command == "pack" { "pack" } else { "bridge" };
            let table = &mut config[producer]["build"];
            let setting = "embed_telemetry_env";
            if let Some(value) = environment {
                table[setting] = value.into();
            } else {
                table.as_table_mut().unwrap().remove(setting);
            }
            cases.push(config);
        }
        for config in cases {
            std::fs::write(
                temp.path().join("baml.toml"),
                toml::to_string(&config).unwrap(),
            )
            .unwrap();
            let args = if command == "pack" {
                vec!["pack", "main", "--embed-telemetry", "-o", "out"]
            } else {
                vec!["generate", "--embed-telemetry"]
            };
            let output = cli(temp.path(), &server.uri(), &args);
            assert!(!output.status.success(), "{command}: {config}");
            assert!(!String::from_utf8_lossy(&output.stderr).contains("bdry_secret_embedded"));
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn packed_api_key_is_verified_and_fixed_with_endpoint_override() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let build_server = MockServer::start().await;
    embedding_api(&build_server).await;
    succeeded(&cli(
        temp.path(),
        &build_server.uri(),
        &["pack", "main", "--embed-telemetry", "-o", "out"],
    ));
    assert_eq!(build_server.received_requests().await.unwrap().len(), 1);
    let requests = build_server.received_requests().await.unwrap();
    assert_eq!(requests[0].url.path(), "/v1/auth/verify");
    assert_eq!(
        serde_json::from_slice::<Value>(&requests[0].body).unwrap(),
        json!({"fullResponse":true})
    );

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
    assert_eq!(build_server.received_requests().await.unwrap().len(), 1);

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
    embedding_api(&server).await;
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
        .filter(|request| {
            request.method == "POST" && !matches!(request.url.path(), "/v1/auth/verify")
        })
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
    embedding_api(&server).await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &["generate", "--embed-telemetry"],
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
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
    succeeded(&cli(temp.path(), &server.uri(), &["generate"]));
    let inlined =
        std::fs::read_to_string(temp.path().join("generated/baml_sdk/src/_inlinedbaml.rs"))
            .unwrap();
    assert!(!inlined.contains("bdry_secret_embedded"));
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn malformed_publisher_settings_fail_before_writing_an_artifact() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    let server = MockServer::start().await;
    for (table, key, value, expected) in [
        (
            "bridge",
            "telemetry_environment",
            "\"staging\"",
            "error: `bridge.telemetry_environment` has moved to `bridge.build.embed_telemetry_env`. Set `embed_telemetry_env` in the [bridge.build] table in baml.toml.",
        ),
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
    env_var_names
    build"#,
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
    env_var_names
    build"#,
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
            &["pack", "main", "--embed-telemetry", "-o", "out"][..],
            &["generate", "--embed-telemetry"][..],
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
        std::fs::write(
            temp.path().join("baml.toml"),
            manifest("").replace(
                "BAML_WRAPPER_BOUNDARY_API_KEY",
                "BAML_TEST_MISSING_EMBED_KEY",
            ),
        )
        .unwrap();
        for value in [None, Some(""), Some("  "), Some("local")] {
            let mut cli = Command::new(common::baml_cli());
            cli.args(["--agent-skill-check", "off", command]);
            if command == "pack" {
                cli.args(["main", "-o", "out"]);
            }
            cli.arg("--embed-telemetry")
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
                } else if value == Some("local") {
                    "local is not a Boundary API key."
                } else {
                    "environment variable BAML_TEST_MISSING_EMBED_KEY is empty."
                }),
                "{stderr}"
            );
            assert!(!stderr.contains("bdry_secret_do_not_fall_back"));
            assert!(!temp.path().join("out").exists());
            assert!(!temp.path().join("generated").exists());
        }
        let mut config: toml::Value = manifest("").parse().unwrap();
        let table = if command == "pack" { "pack" } else { "bridge" };
        config[table]["build"]
            .as_table_mut()
            .unwrap()
            .remove("embed_telemetry_api_key_from_env_var");
        std::fs::write(
            temp.path().join("baml.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();
        let mut cli = Command::new(common::baml_cli());
        cli.args(["--agent-skill-check", "off", command]);
        if command == "pack" {
            cli.args(["main", "-o", "out"]);
        }
        let output = cli
            .arg("--embed-telemetry")
            .current_dir(temp.path())
            .env("BAML_HOME", common::shared_baml_home())
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            stderr.contains(if command == "pack" {
                "pack.build.embed_telemetry_api_key_from_env_var"
            } else {
                "bridge.build.embed_telemetry_api_key_from_env_var"
            }),
            "{stderr}"
        );
        std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrapper_build_embeds_named_api_key_for_both_release_variants() {
    let built = common::ensure_built();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let server = MockServer::start().await;
    embedding_api(&server).await;
    let python = if cfg!(windows) { "python" } else { "python3" };
    let build = |binary: &Path, disabled: bool, embed: bool| {
        let mut command = Command::new(python);
        command
            .arg(repo.join("baml_language/wrapper/build"))
            .arg("--cli")
            .arg(&built.baml_cli)
            .arg("--pack-host")
            .arg(&built.baml_pack_host)
            .arg("--output")
            .arg(binary)
            .env("BOUNDARY_API_URL", server.uri())
            .env("BOUNDARY_PROJECT", "publisher/app")
            .env("BOUNDARY_API_KEY", "bdry_secret_builder_do_not_embed")
            .env("BAML_WRAPPER_BOUNDARY_API_KEY", "bdry_secret_embedded")
            .env("BAML_HOME", common::shared_baml_home())
            .env("BAML_TELEMETRY", "off")
            .env("DO_NOT_TRACK", "1")
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1");
        if disabled {
            command.arg("--no-self-update");
        }
        if embed {
            command.args([
                "--embed-telemetry",
                "--telemetry-project",
                "publisher/app",
                "--telemetry-environment",
                "staging",
            ]);
        }
        command.output().unwrap()
    };
    for (index, disabled) in [false, true].into_iter().enumerate() {
        let binary = temp.path().join(format!("wrapper-{index}"));
        succeeded(&build(&binary, disabled, true));
        let bytes = std::fs::read(&binary).unwrap();
        let embedded = b"bdry_secret_embedded";
        assert!(bytes.windows(embedded.len()).any(|part| part == embedded));
        let private = b"bdry_secret_builder_do_not_embed";
        assert!(!bytes.windows(private.len()).any(|part| part == private));
        let output = Command::new(&binary)
            .arg("--help")
            .env("BAML_TELEMETRY", "off")
            .env("BOUNDARY_API_URL", server.uri())
            .env("BOUNDARY_API_KEY", "bdry_secret_runtime_ignored")
            .env("DO_NOT_TRACK", "1")
            .env("BAML_HOME", temp.path().join("home"))
            .output()
            .unwrap();
        succeeded(&output);
        assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: baml"));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    succeeded(&build(&temp.path().join("offline-wrapper"), false, false));
    assert_eq!(server.received_requests().await.unwrap().len(), 2);

    // All three identity bindings are runtime controls: use the caller's key,
    // project, and recording level instead of the embedded defaults.
    ingest_api(&server).await;
    let output = Command::new(temp.path().join("wrapper-0"))
        .arg("--help")
        .env("BAML_TELEMETRY", "medium")
        .env("BOUNDARY_API_URL", server.uri())
        .env("BOUNDARY_API_KEY", "bdry_secret_runtime_override")
        .env("BOUNDARY_PROJECT", "caller/project")
        .env("DO_NOT_TRACK", "1")
        .env("BAML_HOME", temp.path().join("home"))
        .output()
        .unwrap();
    succeeded(&output);
    let requests = server.received_requests().await.unwrap();
    assert!(requests.iter().any(|request| request.method == "POST"));
    for request in requests.iter().filter(|request| {
        request.method == "POST" && !matches!(request.url.path(), "/v1/auth/verify")
    }) {
        assert_eq!(
            request.headers["authorization"],
            "Bearer bdry_secret_runtime_override"
        );
        let body: Value = serde_json::from_slice(&request.body).unwrap();
        assert_eq!(body["target"], json!({"project":"caller/project"}));
        assert_eq!(body.get("buildId"), None);
    }
}

#[test]
fn wrapper_command_boundaries_record_spans_without_values() {
    let built = common::ensure_built();
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let temp = tempfile::tempdir().unwrap();
    let home = temp.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let binary = temp.path().join(if cfg!(windows) {
        "wrapper.exe"
    } else {
        "wrapper"
    });
    let output = Command::new(if cfg!(windows) { "python" } else { "python3" })
        .arg(repo.join("baml_language/wrapper/build"))
        .arg("--cli")
        .arg(&built.baml_cli)
        .arg("--pack-host")
        .arg(&built.baml_pack_host)
        .arg("--output")
        .arg(&binary)
        .env("BAML_HOME", home.join(".baml"))
        .env("BAML_TELEMETRY", "off")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap();
    succeeded(&output);

    for (scenario, args, success) in [
        ("help", vec!["--help"], true),
        ("update", vec!["toolchain", "update"], false),
        ("self-update", vec!["self-update"], false),
        ("parse-error", vec!["--project"], false),
        ("helper", vec!["__flush-wrapper-telemetry"], true),
    ] {
        let output = Command::new(&binary)
            .args(args)
            .current_dir(temp.path())
            .env("BAML_HOME", home.join(".baml"))
            .env("BAML_TELEMETRY", "medium")
            .env(
                "BAML_CONTEXT",
                json!({"metadata": {"scenario": scenario}}).to_string(),
            )
            .env("BAML_TOOLCHAIN", "invalid-version")
            .env("BAML_MANIFEST_BASE_URL", "invalid://release-source")
            .env_remove("BOUNDARY_API_KEY")
            .env("DO_NOT_TRACK", "1")
            .output()
            .unwrap();
        assert_eq!(output.status.success(), success, "{scenario}: {output:?}");
    }

    let query = |sql: &str| {
        let output = Command::new(&built.baml_cli)
            .args(["query", "--local", "--format", "json", "--from"])
            .arg(&home)
            .arg(sql)
            .env("BAML_HOME", home.join(".baml"))
            .env("BAML_CLI_ALLOW_DIRECT", "1")
            .env("BAML_TELEMETRY", "off")
            .env("DO_NOT_TRACK", "1")
            .output()
            .unwrap();
        succeeded(&output);
        serde_json::from_slice::<Value>(&output.stdout).unwrap()["rows"].take()
    };
    assert_eq!(
        query(
            "SELECT context_metadata['scenario'], span_name, status FROM spans
               WHERE span_type = 'function' AND span_name IN (
                 'user.WrapperInvocation.parse', 'user.WrapperUpdater.update',
                 'user.<(user.UpdateCommand as user.ManagementCommand)>.preflight', 'user.dispatch')
               ORDER BY context_metadata['scenario'], span_name"
        ),
        json!([
            ["help", "user.WrapperInvocation.parse", "return"],
            ["help", "user.dispatch", "return"],
            ["helper", "user.dispatch", "return"],
            ["parse-error", "user.WrapperInvocation.parse", "user_error"],
            ["parse-error", "user.dispatch", "user_error"],
            ["self-update", "user.WrapperInvocation.parse", "return"],
            ["self-update", "user.WrapperUpdater.update", "user_error"],
            ["self-update", "user.dispatch", "user_error"],
            [
                "update",
                "user.<(user.UpdateCommand as user.ManagementCommand)>.preflight",
                "user_error"
            ],
            ["update", "user.WrapperInvocation.parse", "return"],
            ["update", "user.dispatch", "user_error"],
        ])
    );
    assert_eq!(
        query(
            "SELECT COUNT(*) FROM spans WHERE span_type = 'function' AND span_name IN (
                 'user.WrapperInvocation.parse', 'user.WrapperUpdater.update',
                 'user.<(user.UpdateCommand as user.ManagementCommand)>.preflight', 'user.dispatch')
               AND (baml_value_state(input_args) = 'present'
                 OR baml_value_state(output_value) = 'present'
                 OR baml_value_state(error_value) = 'present')"
        ),
        json!([[0]])
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedded_and_supplied_public_keys_have_the_same_runtime_request_contract() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(
        temp.path().join("baml.toml"),
        manifest(
            r#"
[pack]
on_initial_telemetry_failure = "ignore"
[pack.env_var_names]
BOUNDARY_PROJECT = "BOUNDARY_PROJECT"
BOUNDARY_API_KEY = "BOUNDARY_API_KEY"
"#,
        ),
    )
    .unwrap();
    let server = MockServer::start().await;
    embedding_api(&server).await;
    // The build helper selects the manifest variable, with this public credential unchanged.
    let key = "bdry_public_parity";
    Mock::given(path("/v1/auth/verify"))
        .and(header("authorization", format!("Bearer {key}")))
        .and(body_json(json!({"fullResponse":true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(verification_response()))
        .mount(&server)
        .await;
    let build = Command::new(common::baml_cli())
        .args([
            "--agent-skill-check",
            "off",
            "pack",
            "main",
            "--embed-telemetry",
            "-o",
            "out",
        ])
        .current_dir(temp.path())
        .env("BOUNDARY_API_URL", server.uri())
        .env("BAML_WRAPPER_BOUNDARY_API_KEY", key)
        .env("DO_NOT_TRACK", "1")
        .env("BAML_HOME", common::shared_baml_home())
        .output()
        .unwrap();
    succeeded(&build);
    for (status, code) in [
        (204, ""),
        (403, "ACCESS_REQUIRED"),
        (404, "NOT_FOUND"),
        (400, "ENVIRONMENT_REQUIRED"),
        (401, "UNAUTHORIZED"),
    ] {
        let mut contracts = Vec::new();
        for supplied in [false, true] {
            server.reset().await;
            if status == 204 {
                ingest_api(&server).await;
            } else {
                Mock::given(method("POST"))
                    .respond_with(
                        ResponseTemplate::new(status)
                            .set_body_json(json!({"code":code,"retryable":false})),
                    )
                    .mount(&server)
                    .await;
            }
            let mut command = Command::new(temp.path().join("out"));
            command
                .env("BOUNDARY_API_URL", server.uri())
                .env_remove("BOUNDARY_PROJECT")
                .env_remove("BOUNDARY_API_KEY")
                .env_remove("BAML_TELEMETRY")
                .env("BAML_HOME", common::shared_baml_home());
            if supplied {
                command.env("BOUNDARY_API_KEY", key);
            }
            let output = command.output().unwrap();
            succeeded(&output);
            assert_eq!(String::from_utf8_lossy(&output.stdout), "7\n");
            let mut contract = std::collections::BTreeMap::new();
            for request in server
                .received_requests()
                .await
                .unwrap()
                .into_iter()
                .filter(|r| r.method == "POST")
            {
                assert_eq!(request.headers["authorization"], format!("Bearer {key}"));
                let body: Value = serde_json::from_slice(&request.body).unwrap();
                assert_eq!(body["target"], json!({}));
                assert!(body.get("buildId").is_none());
                // Compare fields and destination constraints; recording IDs and timing vary.
                let mut fields: Vec<_> = body.as_object().unwrap().keys().cloned().collect();
                fields.sort();
                let operation = if request.url.path().ends_with("/heartbeat") {
                    "heartbeat"
                } else {
                    "prepare"
                };
                contract.insert(operation, fields);
            }
            assert!(contract.contains_key("heartbeat"));
            if status == 204 {
                assert!(contract.contains_key("prepare"));
            }
            contracts.push(contract);
        }
        assert_eq!(
            contracts[0], contracts[1],
            "credential source changed the {code} request contract"
        );
    }
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_unicode_embedding_credentials_prevent_pack_and_sdk_output() {
    use std::os::unix::ffi::OsStringExt;
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    for args in [
        vec!["pack", "main", "--embed-telemetry", "-o", "out"],
        vec!["generate", "--embed-telemetry"],
    ] {
        let output = Command::new(common::baml_cli())
            .args(["--agent-skill-check", "off"])
            .args(args)
            .current_dir(temp.path())
            .env("BOUNDARY_API_URL", server.uri())
            .env(
                "BAML_WRAPPER_BOUNDARY_API_KEY",
                std::ffi::OsString::from_vec(vec![0xff]),
            )
            .env("BAML_HOME", common::shared_baml_home())
            .env("DO_NOT_TRACK", "1")
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("not valid Unicode"));
        assert!(!temp.path().join("out").exists());
        assert!(!temp.path().join("generated").exists());
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn compilation_failure_prevents_embedded_artifact_and_sdk_output() {
    let temp = tempfile::tempdir().unwrap();
    common::write_project(
        temp.path(),
        "function main() -> int { missing_function() }\n",
    );
    std::fs::write(temp.path().join("baml.toml"), manifest("")).unwrap();
    let server = MockServer::start().await;
    embedding_api(&server).await;
    for args in [
        vec!["pack", "main", "--embed-telemetry", "-o", "out"],
        vec!["generate", "--embed-telemetry"],
    ] {
        let output = cli(temp.path(), &server.uri(), &args);
        assert!(!output.status.success());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("bdry_secret_embedded"));
        assert!(!temp.path().join("out").exists());
        assert!(!temp.path().join("generated").exists());
    }
    for request in server.received_requests().await.unwrap() {
        assert_eq!(request.url.path(), "/v1/auth/verify");
        assert_eq!(
            serde_json::from_slice::<Value>(&request.body).unwrap(),
            json!({"fullResponse":true})
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedding_matches_project_names_with_internal_spaces_exactly() {
    common::ensure_built();
    let temp = tempfile::tempdir().unwrap();
    common::write_project(temp.path(), "function main() -> int { 7 }\n");
    std::fs::write(
        temp.path().join("baml.toml"),
        manifest("").replace("publisher/app", "publisher/App Name"),
    )
    .unwrap();
    let server = MockServer::start().await;
    let mut response = verification_response();
    response["project"]["name"] = json!("App Name");
    Mock::given(path("/v1/auth/verify"))
        .and(body_json(json!({"fullResponse":true})))
        .respond_with(ResponseTemplate::new(200).set_body_json(response))
        .expect(2)
        .mount(&server)
        .await;
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &["pack", "main", "--embed-telemetry", "-o", "out"],
    ));
    succeeded(&cli(
        temp.path(),
        &server.uri(),
        &["generate", "--embed-telemetry"],
    ));
}
