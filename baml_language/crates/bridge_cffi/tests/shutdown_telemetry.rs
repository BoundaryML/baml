//! SDK finalization uses the real bridge, recorder and delivery worker.
#![cfg(not(target_arch = "wasm32"))]

use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, CallFunctionArgs, baml_outbound_result, call_function_args::CallTarget,
};
use prost::Message;

#[tokio::test]
async fn shutdown_finalizes_telemetry_once_without_guessing_the_host_outcome() {
    const SCENARIO: &str = "BAML_TEST_SHUTDOWN_TELEMETRY";
    let Some(scenario) = baml_env::raw_var(SCENARIO) else {
        // Separate processes isolate the bridge singleton and telemetry configuration.
        for scenario in ["empty", "work", "replacement"] {
            let home = tempfile::tempdir().unwrap();
            let mut command = std::process::Command::new(std::env::current_exe().unwrap());
            let output = command
                .args([
                    "--exact",
                    "shutdown_finalizes_telemetry_once_without_guessing_the_host_outcome",
                    "--nocapture",
                ])
                .env(SCENARIO, scenario)
                .env("HOME", home.path())
                .env("USERPROFILE", home.path())
                .env("BAML_HOME", home.path().join("config"))
                .env("BAML_TELEMETRY", "high")
                .env("BOUNDARY_API_KEY", "local")
                .env_remove("BOUNDARY_API_URL")
                .env_remove("BOUNDARY_PROJECT")
                .env("DO_NOT_TRACK", "1")
                .current_dir(home.path())
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{scenario}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
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
    let first = bridge_cffi::initialize_runtime_from_blob(bytecode.as_bytes(), None).unwrap();
    if scenario == "replacement" {
        bridge_cffi::initialize_runtime_from_blob(bytecode.as_bytes(), None).unwrap();
        // Replacement shuts an engine down without finalizing the host lifetime.
        first.shutdown(None).await;
    }
    if scenario != "empty" {
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
        assert!(matches!(
            result.result,
            Some(baml_outbound_result::Result::Ok(_))
        ));
    }
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(bridge_cffi::get_runtime().is_err());

    let baml_home = std::path::PathBuf::from(baml_env::os_var("BAML_HOME").unwrap());
    let recordings = std::fs::read_dir(baml_home.join("btel/recordings"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(
        recordings.len(),
        if scenario == "replacement" { 2 } else { 1 }
    );
    let mut completions = 0;
    for directory in recordings {
        let read = btel_file::read_directory(&directory).unwrap();
        assert!(read.issues.is_empty(), "{:?}", read.issues);
        assert!(read.has_recording_end);
        assert_eq!(
            read.files.iter().filter(|file| file.end.is_some()).count(),
            1
        );
        let terminal = read.files.last().unwrap();
        if let Some(end) = terminal.end.as_ref().unwrap().process_end.as_ref() {
            completions += 1;
            assert_eq!(
                end.status,
                btel_recorder::proto::ProcessStatus::Unknown as i32
            );
            assert!(end.at_unix_ns > 0);
            assert_eq!(
                terminal.header.as_ref().unwrap().format_minor,
                btel_settings::encoding::UNKNOWN_PROCESS_OUTCOME_FORMAT_MINOR
            );
        }
    }
    assert_eq!(completions, 1);
}
