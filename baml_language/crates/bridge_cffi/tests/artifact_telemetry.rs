#![cfg(not(target_arch = "wasm32"))]

use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, CallFunctionArgs, baml_outbound_result, baml_outbound_value,
    call_function_args::CallTarget,
};
use btel_settings::artifact::{
    ArtifactTelemetry, BytecodeDigest, DestinationOverrides, EmbeddedIngestion,
    IngestionDestination, PublicCredential, build_digest,
};
use prost::Message;

#[tokio::test]
async fn generated_bridge_honors_only_the_publishers_recording_variable() {
    const SCENARIO: &str = "BAML_TEST_ARTIFACT_SCENARIO";
    let Some(scenario) = baml_env::raw_var(SCENARIO) else {
        // Separate processes isolate both environment variables and the bridge singleton.
        for scenario in ["fixed", "custom_off"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "generated_bridge_honors_only_the_publishers_recording_variable",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BOUNDARY_API_KEY", "local")
                .env("BAML_TELEMETRY", "invalid")
                .env("ACME_TELEMETRY", "off")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{scenario}: {}",
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    let policy = ArtifactTelemetry {
        recording_level_envvar: (scenario != "fixed").then(|| "ACME_TELEMETRY".into()),
        ..Default::default()
    };
    run_generated_main(policy, false).await;
}

/// Telemetry settings never stop a generated SDK from loading, and it prints nothing
/// about them. An unrecognized level records nothing. A setting that can't select a
/// cloud destination (endpoint, key, saved login) leaves recording local, as it is
/// without cloud credentials, and never falls back to another credential.
#[tokio::test]
async fn generated_bridge_loads_silently_with_unusable_telemetry_settings() {
    const SCENARIO: &str = "BAML_TEST_UNUSABLE_TELEMETRY";
    if let Some(scenario) = baml_env::raw_var(SCENARIO) {
        let mut policy = ArtifactTelemetry {
            recording_level_envvar: Some("ACME_TELEMETRY".into()),
            ..Default::default()
        };
        // A publisher that embeds its credential and lets callers name a destination.
        let embedded = scenario.starts_with("embedded_");
        if embedded {
            policy.destination_overrides = DestinationOverrides::Named {
                project: "ACME_PROJECT".into(),
                api_key: "ACME_KEY".into(),
            };
        }
        run_generated_main(policy, embedded).await;
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
        (
            "public_key",
            &[
                ("BOUNDARY_API_URL", url.as_str()),
                ("BOUNDARY_API_KEY", "bdry_public_test"),
            ],
            1,
        ),
        // The saved login can't be read: see below.
        ("unreadable_login", &[("BOUNDARY_API_URL", url.as_str())], 1),
        // A project without a key needs the caller's saved login, and there is none.
        (
            "embedded_project_only",
            &[
                ("BOUNDARY_API_URL", url.as_str()),
                ("ACME_PROJECT", "org/app"),
            ],
            1,
        ),
        // The key is not Unicode (see below); the embedded credential is not used.
        (
            "embedded_key_not_unicode",
            &[("BOUNDARY_API_URL", url.as_str())],
            1,
        ),
    ] {
        let home = tempfile::tempdir().unwrap();
        if scenario == "unreadable_login" {
            // A file where the saved-login directory belongs.
            std::fs::create_dir(home.path().join("login")).unwrap();
            std::fs::write(home.path().join("login").join("cache"), "").unwrap();
        }
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
            .env_remove("ACME_PROJECT")
            .env_remove("ACME_KEY")
            .envs(env.iter().copied())
            .envs((scenario == "embedded_key_not_unicode").then(|| ("ACME_KEY", not_unicode())))
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

/// An environment value that is not valid Unicode.
fn not_unicode() -> std::ffi::OsString {
    #[cfg(unix)]
    {
        std::os::unix::ffi::OsStringExt::from_vec(vec![0xff])
    }
    #[cfg(windows)]
    {
        std::os::windows::ffi::OsStringExt::from_wide(&[0xd800])
    }
}

/// A public credential registered for the build with this fingerprint.
fn public_ingestion(bytecode_digest: BytecodeDigest) -> Box<EmbeddedIngestion> {
    Box::new(EmbeddedIngestion {
        token: PublicCredential::new(format!("bdry_public_{}", "a".repeat(64))),
        token_id: "token-bridge".into(),
        build_id: "build-bridge".into(),
        product_id: "product-bridge".into(),
        destination: IngestionDestination {
            org_id: "o".into(),
            project_id: "p".into(),
            environment_id: "e".into(),
        },
        bytecode_digest,
    })
}

/// Load `function main() -> int { 7 }` as a generated SDK does and run it.
async fn run_generated_main(mut policy: ArtifactTelemetry, embed_credential: bool) {
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: "baml-bridge-test".into(),
        bridge_runtime_version: baml_version::CANONICAL_VERSION.into(),
        toolchain_version: baml_version::CANONICAL_VERSION.into(),
    })
    .unwrap();
    let program = baml_test_support::compile_source("function main() -> int { 7 }");
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    if embed_credential {
        policy.embedded = Some(public_ingestion(build_digest(&artifact, &policy).unwrap()));
    }
    let bytecode = baml_artifact::encode_embedded(&artifact);
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
    let manifest = toml::to_string(&toml::Table::from_iter([(
        "__baml_codegen".into(),
        toml::Value::Table(metadata),
    )]))
    .unwrap();
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn bridge_embedded_ingestion_respects_binding_and_build_boundaries() {
    const SCENARIO: &str = "BAML_TEST_BRIDGE_EMBEDDED";
    let Some(scenario) = baml_env::raw_var(SCENARIO) else {
        use wiremock::{Mock, MockServer, ResponseTemplate, matchers::method};
        for scenario in ["fixed", "custom", "off", "mismatch"] {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .respond_with(ResponseTemplate::new(401))
                .mount(&server)
                .await;
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "bridge_embedded_ingestion_respects_binding_and_build_boundaries",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BOUNDARY_API_URL", server.uri())
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
            if matches!(scenario, "off" | "mismatch") {
                assert_eq!(requests.len(), 0);
                continue;
            }
            assert!(!requests.is_empty());
            for request in requests {
                let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
                if scenario == "fixed" {
                    assert_eq!(
                        request.headers["authorization"],
                        format!("Bearer bdry_public_{}", "a".repeat(64))
                    );
                    assert_eq!(body["buildId"], "build-bridge");
                    assert_eq!(
                        body["target"],
                        serde_json::json!({"orgId":"o","projectId":"p","environmentId":"e"})
                    );
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
    policy.embedded = Some(public_ingestion(build_digest(&bytecode, &policy).unwrap()));
    if scenario == "mismatch" {
        policy.embedded.as_mut().unwrap().bytecode_digest.value = "f".repeat(64);
    }
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
    if scenario == "mismatch" {
        let Err(error) = runtime else {
            panic!("mismatched build must fail before auth")
        };
        assert_eq!(
            error.to_string(),
            format!(
                r#"BAML startup failed: generated SDK bytecode could not be loaded.

`baml_sdk` generated using BAML toolchain {}, could not be loaded by baml-bridge-test {}: Embedded telemetry does not match this artifact. Rebuild with `baml pack --embed-telemetry` or `baml generate --embed-telemetry`."#,
                baml_version::CANONICAL_VERSION,
                baml_version::CANONICAL_VERSION
            )
        );
        return;
    }
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
