#![cfg(not(target_arch = "wasm32"))]

use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, CallFunctionArgs, baml_outbound_result, baml_outbound_value,
    call_function_args::CallTarget,
};
use prost::Message;

#[tokio::test]
async fn generated_bridge_honors_only_the_publishers_recording_variable() {
    const SCENARIO: &str = "BAML_TEST_ARTIFACT_SCENARIO";
    let Some(scenario) = baml_env::raw_var(SCENARIO) else {
        // Separate processes isolate both environment variables and the bridge singleton.
        for scenario in ["fixed", "custom_off", "custom_on"] {
            let home = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "generated_bridge_honors_only_the_publishers_recording_variable",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BAML_HOME", home.path())
                .env("BOUNDARY_API_KEY", "local")
                .env("BAML_TELEMETRY", "off")
                .env(
                    "ACME_TELEMETRY",
                    if scenario == "custom_off" {
                        "off"
                    } else {
                        "medium"
                    },
                )
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{scenario}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
            let recorded =
                std::fs::read_dir(home.path().join("btel/recordings")).map_or(0, Iterator::count);
            assert_eq!(
                recorded,
                usize::from(scenario != "custom_off"),
                "{scenario}"
            );
        }
        return;
    };
    run_generated_main(if scenario == "fixed" {
        "[bridge.env_var_names]\nBAML_TELEMETRY = false"
    } else {
        "[bridge.env_var_names]\nBAML_TELEMETRY = \"ACME_TELEMETRY\""
    })
    .await;
}

/// Telemetry settings never stop a generated SDK from loading, and it prints nothing
/// about them. An unrecognized level records nothing; an unusable endpoint or an
/// empty key leaves recording local, as it is without cloud credentials.
#[tokio::test]
async fn generated_bridge_loads_silently_with_unusable_telemetry_settings() {
    const SCENARIO: &str = "BAML_TEST_UNUSABLE_TELEMETRY";
    if baml_env::raw_var(SCENARIO).is_some() {
        run_generated_main("[bridge.env_var_names]\nBAML_TELEMETRY = \"ACME_TELEMETRY\"").await;
        return;
    }
    let server = wiremock::MockServer::start().await;
    let url = server.uri();
    let url_with_query = format!("{url}/?not=a-base-url");
    for (scenario, env, recordings) in [
        ("level_false", &[("ACME_TELEMETRY", "false")][..], 0),
        ("level_typo", &[("ACME_TELEMETRY", "hgih")], 0),
        ("level_empty", &[("ACME_TELEMETRY", "")], 1),
        (
            "malformed_url",
            &[
                ("BOUNDARY_API_URL", url_with_query.as_str()),
                ("BOUNDARY_API_KEY", "bdry_secret_test"),
            ],
            1,
        ),
        (
            "empty_key",
            &[("BOUNDARY_API_URL", url.as_str()), ("BOUNDARY_API_KEY", "")],
            1,
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "generated_bridge_loads_silently_with_unusable_telemetry_settings",
                "--nocapture",
            ])
            .env(SCENARIO, scenario)
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .env("BAML_HOME", home.path())
            .env_remove("ACME_TELEMETRY")
            .env_remove("BOUNDARY_API_URL")
            .env_remove("BOUNDARY_API_KEY")
            .env_remove("BOUNDARY_PROJECT")
            .envs(env.iter().copied())
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{scenario}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(String::from_utf8_lossy(&output.stderr), "", "{scenario}");
        let recorded =
            std::fs::read_dir(home.path().join("btel/recordings")).map_or(0, Iterator::count);
        assert_eq!(recorded, recordings, "{scenario}");
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

/// Load `function main() -> int { 7 }` as a generated SDK does and run it.
async fn run_generated_main(original_manifest: &str) {
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: "baml-bridge-test".into(),
        bridge_runtime_version: baml_version::CANONICAL_VERSION.into(),
        toolchain_version: baml_version::CANONICAL_VERSION.into(),
    })
    .unwrap();
    let program = baml_test_support::compile_source("function main() -> int { 7 }");
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    let bytecode = baml_artifact::encode_embedded(&artifact);
    let mut manifest: toml::Table = original_manifest.parse().unwrap();
    let policy = btel_settings::artifact::ArtifactTelemetry::from_manifest(
        &toml::Value::Table(manifest.clone()),
        btel_settings::artifact::ArtifactKind::Bridge,
    )
    .unwrap();
    let mut metadata = toml::Table::new();
    metadata.insert("metadata_version".into(), 1.into());
    metadata.insert(
        "toolchain".into(),
        toml::Value::Table(toml::Table::from_iter([(
            "version".into(),
            baml_version::CANONICAL_VERSION.into(),
        )])),
    );
    metadata.insert("telemetry".into(), toml::Value::try_from(&policy).unwrap());
    manifest.insert("__baml_codegen".into(), toml::Value::Table(metadata));
    let manifest = toml::to_string(&manifest).unwrap();
    let runtime =
        bridge_cffi::initialize_runtime_from_blob(bytecode.as_bytes(), Some(&manifest)).unwrap();
    let args = CallFunctionArgs {
        call_id: bridge_cffi::new_function_call_id(),
        call_target: Some(CallTarget::FunctionName("main".into())),
        invocation: Some(Default::default()),
        ..Default::default()
    }
    .encode_to_vec();
    let request = bridge_cffi::decode_invocation_request(&args).unwrap();
    let response = bridge_cffi::execute_invocation(request).await;
    let result = BamlOutboundResult::decode(response.as_slice()).unwrap();
    let Some(baml_outbound_result::Result::Ok(value)) = result.result else {
        panic!("BAML operation failed: {result:?}");
    };
    assert_eq!(value.value, Some(baml_outbound_value::Value::IntValue(7)));
    runtime.shutdown(None).await;
}

/// The ordinary generated SDK path must honor the host switch even when cloud
/// credentials are present. Enabling recording must still select those credentials.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ordinary_bridge_honors_telemetry_off_with_cloud_credentials() {
    const SCENARIO: &str = "BAML_TEST_BRIDGE_RECORDING_SWITCH";
    if let Some(scenario) = baml_env::raw_var(SCENARIO) {
        if scenario == "saved_login" {
            let endpoint = bcs_api::Endpoint::from_env().unwrap();
            bcs_api::Store::new(&endpoint)
                .unwrap()
                .write(&bcs_api::StoredSession {
                    refresh_token: bcs_api::Secret::new("bdry_session_bridge_test".into()),
                    caller: bcs_api::auth::Caller {
                        user_id: "test-user".into(),
                        email: "test@example.invalid".into(),
                    },
                })
                .unwrap();
        }
        run_generated_main("").await;
        return;
    }
    use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
    for scenario in ["api_key", "saved_login"] {
        for level in ["off", "0", "false", "no", "medium"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(401))
                .mount(&server)
                .await;
            let home = tempfile::tempdir().unwrap();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "ordinary_bridge_honors_telemetry_off_with_cloud_credentials",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BAML_HOME", home.path())
                .env("BAML_TELEMETRY", level)
                .env("BOUNDARY_API_URL", server.uri())
                .env("BOUNDARY_PROJECT", "acme/app")
                .env_remove("BOUNDARY_API_KEY")
                .envs(
                    (scenario == "api_key")
                        .then_some(("BOUNDARY_API_KEY", "bdry_secret_bridge_test")),
                )
                .output()
                .unwrap();
            assert!(output.status.success(), "{scenario}/{level}: {output:?}");
            let requests = server.received_requests().await.unwrap();
            assert_eq!(requests.is_empty(), level != "medium", "{scenario}/{level}");
            for request in requests {
                assert_eq!(
                    request.headers["authorization"],
                    if scenario == "api_key" {
                        "Bearer bdry_secret_bridge_test"
                    } else {
                        "Bearer bdry_session_bridge_test"
                    }
                );
            }
            assert!(!home.path().join("btel").exists(), "{scenario}/{level}");
            if level != "medium" {
                assert!(output.stderr.is_empty(), "{scenario}/{level}: {output:?}");
            }
        }
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_embedded_ingestion_respects_bindings() {
    use btel_settings::artifact::{
        ArtifactTelemetry, DestinationOverrides, EmbeddedCredential, EmbeddedIngestion,
    };
    const SCENARIO: &str = "BAML_TEST_BRIDGE_EMBEDDED";
    let Some(scenario) = baml_env::raw_var(SCENARIO) else {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        for scenario in ["fixed", "custom", "off"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(401))
                .mount(&server)
                .await;
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "bridge_embedded_ingestion_respects_bindings",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BOUNDARY_API_URL", server.uri())
                .env("BAML_TELEMETRY", "off")
                .env("BOUNDARY_API_KEY", "bdry_secret_ambient_ignored")
                .env("BOUNDARY_PROJECT", "ambient/ignored")
                .env("ACME_KEY", "bdry_secret_customer")
                .env("ACME_PROJECT", "customer/app")
                .env(
                    "ACME_TELEMETRY",
                    if scenario == "off" { "off" } else { "medium" },
                )
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{scenario}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            let requests = server.received_requests().await.unwrap();
            if scenario == "off" {
                assert_eq!(requests.len(), 0);
                continue;
            }
            assert!(!requests.is_empty());
            for request in requests {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                if scenario == "fixed" {
                    assert_eq!(
                        request.headers["authorization"],
                        "Bearer bdry_secret_embedded"
                    );
                    assert_eq!(body.get("buildId"), None);
                    assert_eq!(body["target"], serde_json::json!({}));
                } else {
                    assert_eq!(
                        request.headers["authorization"],
                        "Bearer bdry_secret_customer"
                    );
                    assert_eq!(body.get("buildId"), None);
                    assert_eq!(
                        body["target"],
                        serde_json::json!({"project":"customer/app"})
                    );
                }
            }
        }
        return;
    };
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: "baml-bridge-test".into(),
        bridge_runtime_version: baml_version::CANONICAL_VERSION.into(),
        toolchain_version: baml_version::CANONICAL_VERSION.into(),
    })
    .unwrap();
    let program = baml_test_support::compile_source("function main() -> int { 7 }");
    let bytecode = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    let mut policy = ArtifactTelemetry {
        recording_level_envvar: Some("ACME_TELEMETRY".into()),
        destination_overrides: if scenario == "custom" {
            DestinationOverrides::Named {
                project: "ACME_PROJECT".into(),
                api_key: "ACME_KEY".into(),
            }
        } else {
            DestinationOverrides::Default
        },
        ..Default::default()
    };
    policy.embedded = Some(Box::new(EmbeddedIngestion {
        api_key: EmbeddedCredential::new("bdry_secret_embedded".into()),
    }));
    let metadata = toml::Table::from_iter([
        ("metadata_version".into(), 1.into()),
        (
            "toolchain".into(),
            toml::Value::Table(toml::Table::from_iter([(
                "version".into(),
                baml_version::CANONICAL_VERSION.into(),
            )])),
        ),
        ("telemetry".into(), toml::Value::try_from(&policy).unwrap()),
    ]);
    let manifest = toml::to_string(&toml::Table::from_iter([(
        "__baml_codegen".into(),
        toml::Value::Table(metadata),
    )]))
    .unwrap();
    let blob = baml_artifact::encode_embedded(&bytecode);
    let runtime = bridge_cffi::initialize_runtime_from_blob(blob.as_bytes(), Some(&manifest));
    let runtime = runtime.unwrap();
    let args = CallFunctionArgs {
        call_id: bridge_cffi::new_function_call_id(),
        call_target: Some(CallTarget::FunctionName("main".into())),
        invocation: Some(Default::default()),
        ..Default::default()
    }
    .encode_to_vec();
    let request = bridge_cffi::decode_invocation_request(&args).unwrap();
    let response = bridge_cffi::execute_invocation(request).await;
    let result = BamlOutboundResult::decode(response.as_slice()).unwrap();
    let Some(baml_outbound_result::Result::Ok(value)) = result.result else {
        panic!("operation failed: {result:?}");
    };
    assert_eq!(value.value, Some(baml_outbound_value::Value::IntValue(7)));
    runtime.shutdown(None).await;
}
