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
        for scenario in ["fixed", "custom_off", "custom_invalid"] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "generated_bridge_honors_only_the_publishers_recording_variable",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("BOUNDARY_API_KEY", "local")
                .env("BAML_TELEMETRY", "invalid")
                .env(
                    "ACME_TELEMETRY",
                    if scenario == "custom_invalid" {
                        "invalid"
                    } else {
                        "off"
                    },
                )
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
    let policy = btel_settings::artifact::ArtifactTelemetry {
        recording_level_envvar: (scenario != "fixed").then(|| "ACME_TELEMETRY".into()),
        ..Default::default()
    };
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
    let runtime = bridge_cffi::initialize_runtime_from_blob(bytecode.as_bytes(), Some(&manifest));
    if scenario == "custom_invalid" {
        let Err(error) = runtime else {
            panic!("invalid permitted variable must fail startup")
        };
        assert_eq!(
            error.to_string(),
            format!(
                r#"BAML startup failed: generated SDK bytecode could not be loaded.

`baml_sdk` generated using BAML toolchain {}, could not be loaded by baml-bridge-test {}: ACME_TELEMETRY must be off, low, medium, or high."#,
                baml_version::CANONICAL_VERSION,
                baml_version::CANONICAL_VERSION,
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
        panic!("BAML operation failed: {result:?}");
    };
    assert_eq!(value.value, Some(baml_outbound_value::Value::IntValue(7)));
    runtime.shutdown(None).await;
}
