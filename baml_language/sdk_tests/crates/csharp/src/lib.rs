#[cfg(test)]
mod tests {
    use std::{
        collections::HashSet,
        env,
        path::PathBuf,
        process::{Command, Output},
        sync::OnceLock,
    };

    // SDK_PARITY_LINT(skip): validates C#-specific generated union runtime source
    #[test]
    fn test_checked_in_union_runtime_source_matches_generator() {
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let language_root = manifest.join("../../..");
        let project = language_root
            .join("sdks/csharp/bridge_csharp/tools/Baml.UnionGenerator/Baml.UnionGenerator.csproj");
        let source = language_root.join("sdks/csharp/bridge_csharp/src/Values/BamlUnion.cs");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
                "--",
                "--check",
                source.to_str().expect("source path is not UTF-8"),
            ])
            .output()
            .expect("failed to launch the C# union source generator");
        assert!(
            output.status.success(),
            "checked-in C# union source is stale:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    // SDK_PARITY_LINT(skip): validates C#-specific generated-client repository hygiene
    #[test]
    fn test_generated_baml_clients_are_not_tracked() {
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let output = Command::new("git")
            .current_dir(&manifest)
            .args([
                "ls-files",
                "--",
                ":(glob)baml_language/sdk_tests/crates/csharp/**/baml_sdk/**",
            ])
            .output()
            .expect("failed to inspect tracked C# generated clients");
        assert!(
            output.status.success(),
            "git ls-files failed: {}",
            String::from_utf8_lossy(&output.stderr),
        );
        let deleted = Command::new("git")
            .current_dir(&manifest)
            .args([
                "ls-files",
                "--deleted",
                "--",
                ":(glob)baml_language/sdk_tests/crates/csharp/**/baml_sdk/**",
            ])
            .output()
            .expect("failed to inspect deleted C# generated clients");
        assert!(
            deleted.status.success(),
            "git ls-files --deleted failed: {}",
            String::from_utf8_lossy(&deleted.stderr),
        );
        let deleted = String::from_utf8_lossy(&deleted.stdout)
            .lines()
            .map(str::to_string)
            .collect::<HashSet<_>>();
        let tracked_output = String::from_utf8_lossy(&output.stdout);
        let tracked = tracked_output
            .lines()
            .filter(|path| !deleted.contains(*path))
            .collect::<Vec<_>>();
        assert!(
            tracked.is_empty(),
            "generated C# client output must remain untracked:\n{}",
            tracked.join("\n"),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_basic_calls_executes_sync_and_async() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("basic_calls").join("BasicCalls.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# basic-call consumer");
        assert!(
            output.status.success(),
            "C# basic-call consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_basic_calls=ok"),
            "C# basic-call consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_type_roundtrips_executes_nominals_collections_defaults_and_unions() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("type_roundtrips")
            .join("TypeRoundtrips.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# type-roundtrip consumer");
        assert!(
            output.status.success(),
            "C# type-roundtrip consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_type_roundtrips=ok"),
            "C# type-roundtrip consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_generics_executes_inferred_and_explicit_generics() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("generics").join("Generics.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# generics consumer");
        assert!(
            output.status.success(),
            "C# generics consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_generics=ok"),
            "C# generics consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    #[cfg(unix)]
    // SDK_PARITY_LINT(skip): exercises C#-specific generated-surface compile coverage
    #[test]
    fn test_generics_generated_surface_rejects_ambiguous_generic_calls() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let script = manifest.join("generics").join("verify_compile_negative.sh");
        let output = Command::new(&script)
            .output()
            .expect("failed to launch the C# generics generated compile matrix");
        assert!(
            output.status.success(),
            "C# generics generated compile matrix failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("csharp_generics_generated_compile_matrix=ok"),
            "C# generics generated compile marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_failures_and_cancellation_executes_typed_failures_cancellation_and_exit() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("failures_and_cancellation")
            .join("FailuresAndCancellation.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# failure and cancellation consumer");
        assert!(
            output.status.success(),
            "C# failure and cancellation consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_failures_and_cancellation=ok"),
            "C# failure and cancellation success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_media_executes_media_in_both_directions() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("media").join("Media.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# media consumer");
        assert!(
            output.status.success(),
            "C# media consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_media=ok"),
            "C# media success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_streaming_executes_generated_native_stream_and_request_failure() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("streaming").join("Streaming.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# streaming consumer");
        assert!(
            output.status.success(),
            "C# streaming consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_streaming_request=ok"),
            "C# streaming success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    fn host_callables_output() -> &'static Output {
        static OUTPUT: OnceLock<Output> = OnceLock::new();

        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("host_callables").join("HostCallables.csproj");
        OUTPUT.get_or_init(|| {
            Command::new("dotnet")
                .args([
                    "run",
                    "--no-build",
                    "--project",
                    project.to_str().expect("project path is not UTF-8"),
                    "--configuration",
                    "Release",
                ])
                .output()
                .expect("failed to launch the C# host-callable consumer")
        })
    }

    fn assert_host_callables_marker(marker: &str) {
        let output = host_callables_output();
        assert!(
            output.status.success(),
            "C# host-callable consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(marker),
            "C# host-callable marker {marker:?} is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_is_a_native_callable_with_host_language_arguments() {
        assert_host_callables_marker(
            "baml_closure_is_a_native_callable_with_host_language_arguments=ok",
        );
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_decodes_multiple_args_and_structured_return_values() {
        assert_host_callables_marker(
            "baml_closure_decodes_multiple_args_and_structured_return_values=ok",
        );
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_is_reusable_and_retains_mutable_captures() {
        assert_host_callables_marker("baml_closure_is_reusable_and_retains_mutable_captures=ok");
    }

    fn stdlib_resources_output(arguments: &[&str]) -> Output {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("stdlib_resources")
            .join("StdlibResources.csproj");
        Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .args(arguments)
            .output()
            .expect("failed to launch the C# stdlib-resource consumer")
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_stdlib_resources_executes_native_typed_resource_apis_lifetimes_and_state() {
        let output = stdlib_resources_output(&[]);
        assert!(
            output.status.success(),
            "C# stdlib-resource consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_stdlib_resources=ok"),
            "C# stdlib-resource success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): isolates the flaky native cancellation propagation check
    #[test]
    #[ignore = "flaky: B-1059 - CancelToken.any intermittently fails to preserve native state"]
    fn test_cancel_token_any_propagates_native_cancellation() {
        let output = stdlib_resources_output(&["--", "cancel-token-any"]);
        assert!(
            output.status.success(),
            "C# CancelToken.any consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_cancel_token_any=ok"),
            "C# CancelToken.any success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_primitive_edges_executes_native_primitive_and_nullable_edges() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("primitive_edges")
            .join("PrimitiveEdges.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# primitive-edge consumer");
        assert!(
            output.status.success(),
            "C# primitive-edge consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_primitive_edges=ok"),
            "C# primitive-edge success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_stdlib_structurals_executes_native_stdlib_structural_roundtrips() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("stdlib_structurals")
            .join("StdlibStructurals.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# stdlib-structural consumer");
        assert!(
            output.status.success(),
            "C# stdlib-structural consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_stdlib_structurals=ok"),
            "C# stdlib-structural success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_dynamic_values_executes_native_dynamic_value_parity() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("dynamic_values").join("DynamicValues.csproj");
        let publish_dir = manifest
            .join("dynamic_values")
            .join("obj")
            .join("trimmed-publish");
        let publish = Command::new("dotnet")
            .args([
                "publish",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
                "--property:PublishTrimmed=true",
                "--output",
                publish_dir
                    .to_str()
                    .expect("trimmed publish path is not UTF-8"),
            ])
            .output()
            .expect("failed to publish the trimmed C# dynamic-value consumer");
        assert!(
            publish.status.success(),
            "trimmed C# dynamic-value publish failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&publish.stdout),
            String::from_utf8_lossy(&publish.stderr),
        );
        let assembly = publish_dir.join("Baml.CSharp.DynamicValues.dll");
        let output = Command::new("dotnet")
            .arg(&assembly)
            .output()
            .expect("failed to launch the trimmed C# dynamic-value consumer");
        assert!(
            output.status.success(),
            "trimmed C# dynamic-value consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_dynamic_values=ok"),
            "C# dynamic-value success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): validates the C#-specific documentation consumer
    #[test]
    fn test_canonical_documentation_consumer_compiles_and_executes() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let language_root = manifest.join("../../..");
        let project = language_root
            .join("sdks/csharp/bridge_csharp/tests/Baml.Bridge.DocumentationConsumer")
            .join("Baml.Bridge.DocumentationConsumer.csproj");
        let runtime = language_root.join("sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj");
        let generated = manifest.join("basic_calls/baml_sdk");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--no-build",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
                &format!(
                    "-p:BamlBridgeProjectReference={}",
                    runtime.to_str().expect("runtime path is not UTF-8")
                ),
                &format!(
                    "-p:BamlGeneratedSourceRoot={}",
                    generated.to_str().expect("generated path is not UTF-8")
                ),
            ])
            .output()
            .expect("failed to launch the canonical C# documentation consumer");
        assert!(
            output.status.success(),
            "canonical C# documentation consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_documentation_consumer=ok"),
            "canonical C# documentation success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }
}
