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
    fn test_primitive_slice_executes_sync_and_async() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("primitive_slice")
            .join("PrimitiveSlice.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# primitive consumer");
        assert!(
            output.status.success(),
            "C# primitive consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_primitive_slice=ok"),
            "C# primitive consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase5_slice_executes_nominals_collections_defaults_and_unions() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("phase5_slice").join("Phase5Slice.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 5 consumer");
        assert!(
            output.status.success(),
            "C# Phase 5 consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase5_slice=ok"),
            "C# Phase 5 consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase6_slice_executes_inferred_and_explicit_generics() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("phase6_slice").join("Phase6Slice.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 6 consumer");
        assert!(
            output.status.success(),
            "C# Phase 6 consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase6_slice=ok"),
            "C# Phase 6 consumer success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    #[cfg(unix)]
    // SDK_PARITY_LINT(skip): exercises C#-specific generated-surface compile coverage
    #[test]
    fn test_phase6_generated_surface_rejects_ambiguous_generic_calls() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let script = manifest
            .join("phase6_slice")
            .join("verify_compile_negative.sh");
        let output = Command::new(&script)
            .output()
            .expect("failed to launch the C# Phase 6 generated compile matrix");
        assert!(
            output.status.success(),
            "C# Phase 6 generated compile matrix failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("csharp_phase6_generated_compile_matrix=ok"),
            "C# Phase 6 generated compile marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase7_executes_typed_failures_cancellation_and_exit() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase7_failures")
            .join("Phase7Failures.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 7 failure consumer");
        assert!(
            output.status.success(),
            "C# Phase 7 failure consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase7_failures=ok"),
            "C# Phase 7 failure success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase9_executes_media_in_both_directions() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("phase9_media").join("Phase9Media.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 9 media consumer");
        assert!(
            output.status.success(),
            "C# Phase 9 media consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase9_media=ok"),
            "C# Phase 9 media success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase10_executes_generated_native_stream_and_request_failure() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest.join("phase10_stream").join("Phase10Stream.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 10 stream consumer");
        assert!(
            output.status.success(),
            "C# Phase 10 stream consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase10_stream_request=ok"),
            "C# Phase 10 stream success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    fn phase11_output() -> &'static Output {
        static OUTPUT: OnceLock<Output> = OnceLock::new();

        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase11_host_callable")
            .join("Phase11HostCallable.csproj");
        OUTPUT.get_or_init(|| {
            Command::new("dotnet")
                .args([
                    "run",
                    "--project",
                    project.to_str().expect("project path is not UTF-8"),
                    "--configuration",
                    "Release",
                ])
                .output()
                .expect("failed to launch the C# Phase 11 host-callable consumer")
        })
    }

    fn assert_phase11_marker(marker: &str) {
        let output = phase11_output();
        assert!(
            output.status.success(),
            "C# Phase 11 host-callable consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains(marker),
            "C# Phase 11 marker {marker:?} is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_is_a_native_callable_with_host_language_arguments() {
        assert_phase11_marker("baml_closure_is_a_native_callable_with_host_language_arguments=ok");
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_decodes_multiple_args_and_structured_return_values() {
        assert_phase11_marker("baml_closure_decodes_multiple_args_and_structured_return_values=ok");
    }

    // SDK_PARITY_LINT(skip): C# canonical coverage executes through its native integration harness
    #[test]
    fn test_baml_closure_is_reusable_and_retains_mutable_captures() {
        assert_phase11_marker("baml_closure_is_reusable_and_retains_mutable_captures=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    #[ignore = "flaky: B-1059 - CancelToken.any intermittently fails to preserve native state"]
    fn test_phase12_executes_native_typed_resource_apis_lifetimes_and_state() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase12_resources")
            .join("Phase12Resources.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 12 opaque-resource consumer");
        assert!(
            output.status.success(),
            "C# Phase 12 opaque-resource consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase12_resources=ok"),
            "C# Phase 12 opaque-resource success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase13_executes_native_primitive_and_nullable_edges() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase13_primitive_edges")
            .join("Phase13PrimitiveEdges.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 13 primitive-edge consumer");
        assert!(
            output.status.success(),
            "C# Phase 13 primitive-edge consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase13_primitive_edges=ok"),
            "C# Phase 13 primitive-edge success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase14_executes_native_stdlib_structural_roundtrips() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase14_stdlib_structurals")
            .join("Phase14StdlibStructurals.csproj");
        let output = Command::new("dotnet")
            .args([
                "run",
                "--project",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
            ])
            .output()
            .expect("failed to launch the C# Phase 14 stdlib-structural consumer");
        assert!(
            output.status.success(),
            "C# Phase 14 stdlib-structural consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout)
                .contains("csharp_phase14_stdlib_structurals=ok"),
            "C# Phase 14 stdlib-structural success marker is missing: {}",
            String::from_utf8_lossy(&output.stdout),
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_phase15_executes_native_dynamic_value_parity() {
        assert_eq!(
            env::var("SDK_TEST_CSHARP_SETUP").as_deref(),
            Ok("1"),
            "C# native test setup did not run"
        );
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let project = manifest
            .join("phase15_dynamic_values")
            .join("Phase15DynamicValues.csproj");
        let publish_dir = manifest
            .join("phase15_dynamic_values")
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
            .expect("failed to publish the trimmed C# Phase 15 dynamic-value consumer");
        assert!(
            publish.status.success(),
            "trimmed C# Phase 15 dynamic-value publish failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&publish.stdout),
            String::from_utf8_lossy(&publish.stderr),
        );
        let assembly = publish_dir.join("Baml.CSharp.Phase15DynamicValues.dll");
        let output = Command::new("dotnet")
            .arg(&assembly)
            .output()
            .expect("failed to launch the trimmed C# Phase 15 dynamic-value consumer");
        assert!(
            output.status.success(),
            "trimmed C# Phase 15 dynamic-value consumer failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        );
        assert!(
            String::from_utf8_lossy(&output.stdout).contains("csharp_phase15_dynamic_values=ok"),
            "C# Phase 15 dynamic-value success marker is missing: {}",
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
        let generated = manifest.join("primitive_slice/baml_sdk");
        let output = Command::new("dotnet")
            .args([
                "run",
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
