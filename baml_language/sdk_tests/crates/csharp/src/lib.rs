#[cfg(test)]
mod tests {
    use std::{
        env,
        path::PathBuf,
        process::{Command, Output},
        sync::OnceLock,
    };

    use sdk_test_harness_runner::csharp::{assert_marker, manifest_dir, run_command, run_project};

    // SDK_PARITY_LINT(skip): validates C#-specific generated union runtime source
    #[test]
    fn test_checked_in_union_runtime_source_matches_generator() {
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let language_root = manifest.join("../../..");
        let project = language_root
            .join("sdks/csharp/bridge_csharp/tools/Baml.UnionGenerator/Baml.UnionGenerator.csproj");
        let source = language_root.join("sdks/csharp/bridge_csharp/src/Values/BamlUnion.cs");
        run_project(
            &project,
            &[
                "--",
                "--check",
                source.to_str().expect("source path is not UTF-8"),
            ],
        );
    }

    // SDK_PARITY_LINT(skip): validates C#-specific generated-client repository hygiene
    #[test]
    fn test_generated_baml_clients_are_not_tracked() {
        let manifest =
            PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR is not set"));
        let output = run_command(
            Command::new("git").current_dir(&manifest).args([
                "ls-files",
                "--",
                ":(glob)baml_language/sdk_tests/crates/csharp/**/baml_sdk/**",
            ]),
            "inspect tracked C# generated clients",
        );
        // `git ls-files` reads the index, so this holds whether or not the
        // trees are on disk. Entries absent from the worktree are deliberately
        // NOT exempted: codegen runs from setup.sh now, so a fresh clone has no
        // `baml_sdk/` at all, and exempting them would hide the very mistake
        // this test exists to catch.
        let tracked = String::from_utf8_lossy(&output.stdout);
        assert!(
            tracked.trim().is_empty(),
            "generated C# client output must remain untracked:\n{tracked}",
        );
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_basic_calls_executes_sync_and_async() {
        let manifest = manifest_dir();
        let project = manifest.join("basic_calls").join("BasicCalls.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_basic_calls=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_type_roundtrips_executes_nominals_collections_defaults_and_unions() {
        let manifest = manifest_dir();
        let project = manifest
            .join("type_roundtrips")
            .join("TypeRoundtrips.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_type_roundtrips=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_generics_executes_inferred_and_explicit_generics() {
        let manifest = manifest_dir();
        let project = manifest.join("generics").join("Generics.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_generics=ok");
    }

    #[cfg(unix)]
    // SDK_PARITY_LINT(skip): exercises C#-specific generated-surface compile coverage
    #[test]
    fn test_generics_generated_surface_rejects_ambiguous_generic_calls() {
        let manifest = manifest_dir();
        let script = manifest.join("generics").join("verify_compile_negative.sh");
        let output = run_command(
            &mut Command::new(&script),
            "C# generics generated compile matrix",
        );
        assert_marker(&output, "csharp_generics_generated_compile_matrix=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_failures_and_cancellation_executes_typed_failures_cancellation_and_exit() {
        let manifest = manifest_dir();
        let project = manifest
            .join("failures_and_cancellation")
            .join("FailuresAndCancellation.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_failures_and_cancellation=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_media_executes_media_in_both_directions() {
        let manifest = manifest_dir();
        let project = manifest.join("media").join("Media.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_media=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_streaming_executes_generated_native_stream_and_request_failure() {
        let manifest = manifest_dir();
        let project = manifest.join("streaming").join("Streaming.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_streaming_request=ok");
    }

    fn host_callables_output() -> &'static Output {
        static OUTPUT: OnceLock<Output> = OnceLock::new();

        let manifest = manifest_dir();
        let project = manifest.join("host_callables").join("HostCallables.csproj");
        OUTPUT.get_or_init(|| run_project(&project, &[]))
    }

    fn assert_host_callables_marker(marker: &str) {
        let output = host_callables_output();
        assert_marker(output, marker);
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
        let manifest = manifest_dir();
        let project = manifest
            .join("stdlib_resources")
            .join("StdlibResources.csproj");
        run_project(&project, arguments)
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_stdlib_resources_executes_native_typed_resource_apis_lifetimes_and_state() {
        let output = stdlib_resources_output(&[]);
        assert_marker(&output, "csharp_stdlib_resources=ok");
    }

    // SDK_PARITY_LINT(skip): isolates the flaky native cancellation propagation check
    #[test]
    #[ignore = "flaky: B-1059 - CancelToken.any intermittently fails to preserve native state"]
    fn test_cancel_token_any_propagates_native_cancellation() {
        let output = stdlib_resources_output(&["--", "cancel-token-any"]);
        assert_marker(&output, "csharp_cancel_token_any=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_primitive_edges_executes_native_primitive_and_nullable_edges() {
        let manifest = manifest_dir();
        let project = manifest
            .join("primitive_edges")
            .join("PrimitiveEdges.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_primitive_edges=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_stdlib_structurals_executes_native_stdlib_structural_roundtrips() {
        let manifest = manifest_dir();
        let project = manifest
            .join("stdlib_structurals")
            .join("StdlibStructurals.csproj");
        let output = run_project(&project, &[]);
        assert_marker(&output, "csharp_stdlib_structurals=ok");
    }

    // SDK_PARITY_LINT(skip): exercises C#-specific native SDK integration coverage
    #[test]
    fn test_dynamic_values_executes_native_dynamic_value_parity() {
        let manifest = manifest_dir();
        let project = manifest.join("dynamic_values").join("DynamicValues.csproj");
        let publish_dir = manifest
            .join("dynamic_values")
            .join("obj")
            .join("trimmed-publish");
        run_command(
            Command::new("dotnet").args([
                "publish",
                project.to_str().expect("project path is not UTF-8"),
                "--configuration",
                "Release",
                "--property:PublishTrimmed=true",
                "--output",
                publish_dir
                    .to_str()
                    .expect("trimmed publish path is not UTF-8"),
            ]),
            "publish the trimmed C# dynamic-value consumer",
        );
        let assembly = publish_dir.join("Baml.CSharp.DynamicValues.dll");
        let output = run_command(
            Command::new("dotnet").arg(&assembly),
            "trimmed C# dynamic-value consumer",
        );
        assert_marker(&output, "csharp_dynamic_values=ok");
    }

    // SDK_PARITY_LINT(skip): validates the C#-specific documentation consumer
    #[test]
    fn test_canonical_documentation_consumer_compiles_and_executes() {
        let manifest = manifest_dir();
        let language_root = manifest.join("../../..");
        let project = language_root
            .join("sdks/csharp/bridge_csharp/tests/Baml.Bridge.DocumentationConsumer")
            .join("Baml.Bridge.DocumentationConsumer.csproj");
        let runtime = language_root.join("sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj");
        let generated = manifest.join("basic_calls/baml_sdk");
        let output = run_project(
            &project,
            &[
                &format!(
                    "-p:BamlBridgeProjectReference={}",
                    runtime.to_str().expect("runtime path is not UTF-8")
                ),
                &format!(
                    "-p:BamlGeneratedSourceRoot={}",
                    generated.to_str().expect("generated path is not UTF-8")
                ),
            ],
        );
        assert_marker(&output, "csharp_documentation_consumer=ok");
    }
}
