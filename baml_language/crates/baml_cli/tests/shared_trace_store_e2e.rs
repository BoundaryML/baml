//! The real bridge writes, the CLI writes, and fresh CLI processes read one store.
use std::{fmt::Write as _, path::Path, process::Command};

use bridge_cffi::baml_bridge::cffi::{
    BamlOutboundResult, CallFunctionArgs, InvocationOptions, baml_outbound_result,
    call_function_args::CallTarget,
};
use prost::Message as _;
use serde_json::{Value, json};

/// Run a local-only CLI subprocess without using the developer's cloud login.
fn cli(cwd: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .current_dir(cwd)
        .args(args)
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .env("BAML_CLI_ALLOW_DIRECT", "1")
        .env("BOUNDARY_API_KEY", "local")
        .env("DO_NOT_TRACK", "1")
        .output()
        .unwrap()
}

/// Count recorded processes through the public query command and JSON output.
fn query(cwd: &Path, from: Option<&Path>) -> Value {
    let mut args = vec![
        "query",
        "--format",
        "json",
        "SELECT COUNT(*) FROM processes",
    ];
    if let Some(from) = from {
        args.extend(["--from", from.to_str().unwrap()]);
    }
    let output = cli(cwd, &args);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

/// Bridge and CLI recordings must remain visible through every supported query source.
#[tokio::test]
async fn bridge_cli_and_query_share_the_resolved_store() {
    const STORE: &str = "BAML_TEST_SHARED_STORE";
    let Some(store) = std::env::var_os(STORE) else {
        // Isolate the bridge singleton, cwd, auth, and configuration per scenario.
        for scenario in ["project", "manifest", "absolute", "home"] {
            let tmp = tempfile::tempdir().unwrap();
            let project = tmp.path().join("app");
            let nested = project.join("baml_src/nested");
            std::fs::create_dir_all(&nested).unwrap();
            std::fs::write(
                project.join("baml_src/main.baml"),
                "function main() -> int { 7 }",
            )
            .unwrap();
            let state = match scenario {
                "manifest" => project.join("traces"),
                "absolute" => tmp.path().join("external-traces"),
                "home" => tmp.path().join("state"),
                _ => project.join(".baml"),
            };
            let mut manifest = "[package]\nname = 'shared-store'\n".to_owned();
            if scenario == "manifest" {
                manifest.push_str("[tracing]\nstore = 'traces'\n");
            } else if scenario == "absolute" {
                write!(
                    manifest,
                    "[tracing]\nstore = {:?}\n",
                    state.to_str().unwrap()
                )
                .unwrap();
            }
            // The default case also verifies discovery using only baml_src/.
            if scenario != "project" {
                std::fs::write(project.join("baml.toml"), manifest).unwrap();
            }
            let home = tmp.path().join("user");
            std::fs::create_dir(&home).unwrap();
            let mut child = Command::new(std::env::current_exe().unwrap());
            child
                .args([
                    "--exact",
                    "bridge_cli_and_query_share_the_resolved_store",
                    "--nocapture",
                ])
                .current_dir(nested)
                .env(STORE, &state)
                .env("BAML_TEST_SHARED_PROJECT", &project)
                .env(
                    "BAML_TEST_SHARED_ARTIFACT",
                    if scenario == "absolute" {
                        "generated"
                    } else {
                        "legacy"
                    },
                )
                .env("HOME", &home)
                .env("USERPROFILE", &home)
                .env("BOUNDARY_API_KEY", "local")
                .env("BAML_TELEMETRY", "high")
                .env("DO_NOT_TRACK", "1")
                .env_remove("BOUNDARY_PROJECT")
                .env_remove("BOUNDARY_API_URL")
                .env_remove("BAML_HOME");
            if scenario != "project" {
                child.env("BAML_HOME", tmp.path().join("state"));
            }
            let output = child.output().unwrap();
            assert!(
                output.status.success(),
                "{scenario}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            if scenario == "manifest" || scenario == "absolute" {
                assert!(
                    !tmp.path().join("state/btel").exists(),
                    "manifest wins over BAML_HOME"
                );
            }
        }
        return;
    };
    let store = std::path::PathBuf::from(store);
    let project = std::path::PathBuf::from(std::env::var_os("BAML_TEST_SHARED_PROJECT").unwrap());
    let cwd = std::env::current_dir().unwrap();
    bridge_cffi::register_bridge(bridge_cffi::BridgeInfo {
        language: bridge_cffi::BridgeLanguage::Python,
        bridge_runtime_name: "shared-store-test".into(),
        bridge_runtime_version: baml_version::CANONICAL_VERSION.into(),
        toolchain_version: baml_version::CANONICAL_VERSION.into(),
    })
    .unwrap();
    let program = baml_test_support::compile_source("function main() -> int { 7 }");
    let artifact = baml_artifact::encode(baml_artifact::ArtifactKind::Program, &program).unwrap();
    let blob = baml_artifact::encode_embedded(&artifact);
    // Exercise both legacy blobs and the generated SDK metadata path.
    let metadata =
        (std::env::var("BAML_TEST_SHARED_ARTIFACT").unwrap() == "generated").then(|| {
            let policy = btel_settings::artifact::ArtifactTelemetry::default();
            let codegen = toml::Table::from_iter([
                ("metadata_version".into(), 1.into()),
                ("telemetry".into(), toml::Value::try_from(&policy).unwrap()),
                (
                    "toolchain".into(),
                    toml::Value::Table(toml::Table::from_iter([(
                        "version".into(),
                        baml_version::CANONICAL_VERSION.into(),
                    )])),
                ),
            ]);
            toml::to_string(&toml::Table::from_iter([(
                "__baml_codegen".into(),
                toml::Value::Table(codegen),
            )]))
            .unwrap()
        });
    bridge_cffi::initialize_runtime_from_blob(blob.as_bytes(), metadata.as_deref()).unwrap();
    let args = CallFunctionArgs {
        call_id: bridge_cffi::new_function_call_id(),
        call_target: Some(CallTarget::FunctionName("main".into())),
        invocation: Some(InvocationOptions::default()),
        ..Default::default()
    }
    .encode_to_vec();
    let request = bridge_cffi::decode_invocation_request(&args).unwrap();
    let response = bridge_cffi::execute_invocation(request).await;
    let response = BamlOutboundResult::decode(response.as_slice()).unwrap();
    assert!(matches!(
        response.result,
        Some(baml_outbound_result::Result::Ok(_))
    ));
    bridge_cffi::shutdown_runtime(None).await.unwrap();
    assert!(store.join("btel/recordings").is_dir());
    let written = query(&cwd, None);
    assert_eq!(written["rows"], json!([[1]]));

    // CLI --from resolves its project independently of the launch directory.
    let run = cli(
        store.parent().unwrap(),
        &["run", "main", "--from", project.to_str().unwrap()],
    );
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    assert_eq!(query(&cwd, Some(&project))["rows"], json!([[2]]));
    if store.file_name().is_some_and(|name| name == "state") {
        // Without a project marker, query still follows the same BAML_HOME.
        assert_eq!(query(store.parent().unwrap(), None)["rows"], json!([[2]]));
    }

    // Trace directories override both enclosing projects and cloud credentials.
    // Point BAML_HOME elsewhere too: explicit --from must be authoritative.
    for from in [
        store.clone(),
        store.join("btel"),
        store.join("btel/recordings"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
            .current_dir(&cwd)
            .args([
                "query",
                "--from",
                from.to_str().unwrap(),
                "--format",
                "json",
                "SELECT COUNT(*) FROM processes",
            ])
            .env("BOUNDARY_API_KEY", "bdry_secret_should_not_contact_cloud")
            .env("BAML_HOME", store.join("other-home"))
            .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let result: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(result["rows"], json!([[2]]));
        assert_eq!(result["outcome"]["refresh"]["files_decoded"], 0);
    }
}

/// A malformed manifest must fail visibly without selecting a fallback store.
#[test]
fn bad_store_configuration_fails_without_creating_a_different_store() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("baml.toml"), "[tracing]\nstore = 42").unwrap();
    let output = cli(
        tmp.path(),
        &["query", "--local", "SELECT COUNT(*) FROM processes"],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(
        error.contains("baml.toml") && error.contains("tracing"),
        "{error}"
    );
    assert!(!tmp.path().join(".baml").exists());
}

/// An explicit empty store bypasses enclosing configuration and rejects cloud selectors.
#[test]
fn empty_trace_folder_bypasses_enclosing_project_configuration() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::write(tmp.path().join("baml.toml"), "[tracing]\nstore = 42").unwrap();
    let traces = tmp.path().join(".baml");
    std::fs::create_dir(&traces).unwrap();
    let result = query(tmp.path(), Some(&traces));
    assert_eq!(result["rows"], json!([[0]]));
    assert_eq!(result["outcome"]["source_missing"], true);
    assert!(!traces.join("btel").exists());
    let output = cli(
        tmp.path(),
        &[
            "query",
            "--from",
            traces.to_str().unwrap(),
            "--project",
            "org/project",
            "SELECT 1",
        ],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("local traces directory"));
}

/// Disabled source execution must not inspect local-store settings or create traces.
#[test]
fn telemetry_off_does_not_read_local_store_configuration() {
    let tmp = tempfile::tempdir().unwrap();
    std::fs::create_dir(tmp.path().join("baml_src")).unwrap();
    std::fs::write(
        tmp.path().join("baml_src/main.baml"),
        "function main() -> int { 7 }",
    )
    .unwrap();
    std::fs::write(
        tmp.path().join("baml.toml"),
        "[package]\nname = 'app'\n[tracing]\nstore = 42",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_baml-cli"))
        .current_dir(tmp.path())
        .args(["run", "main"])
        .env("BAML_TELEMETRY", "off")
        .env("BOUNDARY_API_KEY", "local")
        .env("BAML_HOME", tmp.path().join("home"))
        .env("DO_NOT_TRACK", "1")
        .env("DEV_BAML_CLI_DISABLE_AGENT_DETECTION", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!tmp.path().join(".baml").exists());
    assert!(!tmp.path().join("home/btel").exists());
}

/// Missing source paths must identify both the flag and the supplied directory.
#[test]
fn missing_from_path_is_named_in_the_error() {
    let tmp = tempfile::tempdir().unwrap();
    let missing = tmp.path().join("missing-traces");
    let output = cli(
        tmp.path(),
        // Reach query's source resolver rather than the preceding skill check.
        &[
            "--agent-skill-check",
            "off",
            "query",
            "--from",
            missing.to_str().unwrap(),
            "SELECT 1",
        ],
    );
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("--from"), "{error}");
    assert!(error.contains(missing.to_str().unwrap()), "{error}");
    assert!(!missing.exists());
}
