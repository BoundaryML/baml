//! C# sdk-test target build-script support.
//!
//! Fixtures opt in by adding `sdk_tests/crates/csharp/<fixture>/customizable/`.
//! The shared `type_shapes` fixture and canonical `function_calls` suite have
//! explicit Now/Later ledgers. The ledgers are checked against the canonical
//! Python suites so a new shared test identity cannot silently miss the C#
//! parity audit.

use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
};

use baml_codegen_types::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, emit_cargo_line,
    fixtures_root_from_manifest, load_fixture, watch_dir,
};

const CACHE_SUBDIR: &str = "nuget-packages";
const CACHE_ENV_VAR: &str = "NUGET_PACKAGES";
const SETUP_ENV_VAR: &str = "SDK_TEST_CSHARP_SETUP";

enum Gate {
    Now(&'static str),
    Later(&'static str),
}

enum CanonicalGate {
    Now { evidence: &'static str },
    Later(&'static str),
}

/// One row per canonical Python `type_shapes` test module. The identity is
/// relative to that fixture's `customizable/` directory.
const TYPE_SHAPES_TESTS: &[(&str, Gate)] = &[
    (
        "roundtrip_tests/test_aliases",
        Gate::Now("CSharpParity.RoundTripAliases.Run"),
    ),
    (
        "roundtrip_tests/test_class_refs",
        Gate::Later("shared C# class-reference port is pending"),
    ),
    (
        "roundtrip_tests/test_enums",
        Gate::Now("CSharpParity.RoundTripEnums.Run"),
    ),
    (
        "roundtrip_tests/test_forward_refs",
        Gate::Later("shared C# forward-reference port is pending"),
    ),
    (
        "roundtrip_tests/test_generics",
        Gate::Later("shared C# generic-model port is pending"),
    ),
    (
        "roundtrip_tests/test_handles",
        Gate::Later("resource handles remain in C#-specific fixtures"),
    ),
    (
        "roundtrip_tests/test_lists",
        Gate::Later("shared C# list port is pending"),
    ),
    (
        "roundtrip_tests/test_literals",
        Gate::Now("CSharpParity.RoundTripLiterals.Run"),
    ),
    (
        "roundtrip_tests/test_maps",
        Gate::Now("CSharpParity.RoundTripMaps.Run"),
    ),
    (
        "roundtrip_tests/test_media",
        Gate::Later("media handles remain covered by the focused primitive fixture"),
    ),
    (
        "roundtrip_tests/test_optional",
        Gate::Later("shared C# optional-model port is pending"),
    ),
    (
        "roundtrip_tests/test_primitives",
        Gate::Later("primitive runtime parity remains in the focused primitive fixture"),
    ),
    (
        "roundtrip_tests/test_recursion",
        Gate::Later("shared C# recursive-model port is pending"),
    ),
    (
        "roundtrip_tests/test_routing",
        Gate::Later("shared C# routing port is pending"),
    ),
    (
        "roundtrip_tests/test_streams",
        Gate::Later("stream and PromptAst behavior remains in the replay-backed fixture"),
    ),
    (
        "roundtrip_tests/test_symbol_collisions",
        Gate::Later("shared C# symbol-collision port is pending"),
    ),
    (
        "roundtrip_tests/test_unions",
        Gate::Later("shared C# structural-union port is pending"),
    ),
    (
        "roundtrip_tests/test_void",
        Gate::Later("void call boundaries are not supported by the C# bridge yet"),
    ),
    (
        "test_complex_models",
        Gate::Later("shared C# complex-model port is pending"),
    ),
    (
        "test_generic",
        Gate::Later("shared C# generic-model port is pending"),
    ),
    (
        "test_main",
        Gate::Later("the Python collection root has no standalone C# test module"),
    ),
];

/// One row per canonical Python `function_calls` module. A `Now` row points
/// to the existing C# integration program that exercises the same behavior.
const FUNCTION_CALLS_TESTS: &[(&str, CanonicalGate)] = &[
    (
        "optional_args_static",
        CanonicalGate::Later("compile-fail probes need a C# compiler-diagnostics harness"),
    ),
    (
        "test_cancellation",
        CanonicalGate::Now {
            evidence: "csharp_cancel_token/customizable/Program.cs",
        },
    ),
    (
        "test_errors",
        CanonicalGate::Later(
            "rich error decoding, trace splicing, panic, and hard-exit parity are pending",
        ),
    ),
    (
        "test_generic_calls",
        CanonicalGate::Later("generic free-function and class method bindings are pending"),
    ),
    (
        "test_generic_inference",
        CanonicalGate::Later("runtime generic type inference is not exposed by the C# SDK"),
    ),
    (
        "test_host_callables",
        CanonicalGate::Now {
            evidence: "function_calls/customizable/Program.cs",
        },
    ),
    (
        "test_main",
        CanonicalGate::Later("plain nullary and required-argument call smoke tests are not ported"),
    ),
    (
        "test_methods_on_classes",
        CanonicalGate::Later("generated static and instance method call parity is not ported"),
    ),
    (
        "test_optional_args",
        CanonicalGate::Later(
            "ordinary function and method optional-argument matrices are not ported",
        ),
    ),
    (
        "test_raises",
        CanonicalGate::Later("generated throws documentation has no C# parity contract"),
    ),
    (
        "test_stdlib_entrypoints",
        CanonicalGate::Later("stdlib callable entry-point filtering is not covered in C#"),
    ),
];

/// Canonical compiler-type migration cases whose support status would
/// otherwise be hidden across different fixtures. `evidence` is relative to
/// this C# crate and is checked by the build script.
const CANONICAL_TYPE_CASES: &[(&str, CanonicalGate)] = &[
    (
        "literal-union CLR collapse",
        CanonicalGate::Now {
            evidence: "type_shapes/customizable/roundtrip_tests/test_literals.cs",
        },
    ),
    (
        "string-literal-union map key",
        CanonicalGate::Now {
            evidence: "type_shapes/customizable/roundtrip_tests/test_maps.cs",
        },
    ),
    (
        "alias chains and namespace routing",
        CanonicalGate::Now {
            evidence: "type_shapes/customizable/roundtrip_tests/test_aliases.cs",
        },
    ),
    (
        "enum variant boundary",
        CanonicalGate::Now {
            evidence: "type_shapes/customizable/roundtrip_tests/test_enums.cs",
        },
    ),
    (
        "PromptAst boundary",
        CanonicalGate::Now {
            evidence: "llm_functions/customizable/Program.cs",
        },
    ),
    (
        "function throws contract",
        CanonicalGate::Now {
            evidence: "function_calls/customizable/Program.cs",
        },
    ),
    (
        "void boundary",
        CanonicalGate::Later("generated callables still reject void return types"),
    ),
    (
        "never boundary",
        CanonicalGate::Later("never has no supported managed value projection"),
    ),
    (
        "interface boundary",
        CanonicalGate::Later("interfaces have no public typed C# projection in v1"),
    ),
];

const PROJECT_TEMPLATE: &str = r#"<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <OutputType>Exe</OutputType>
    <TargetFramework>net10.0</TargetFramework>
    <LangVersion>14.0</LangVersion>
    <Nullable>enable</Nullable>
    <ImplicitUsings>enable</ImplicitUsings>
    <TreatWarningsAsErrors>true</TreatWarningsAsErrors>
    <NuGetAudit>false</NuGetAudit>
  </PropertyGroup>
  <ItemGroup>
    <ProjectReference Include="../../../../../sdks/csharp/bridge_csharp/src/Baml.Bridge/Baml.Bridge.csproj" />
  </ItemGroup>
</Project>
"#;

pub fn run_all() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let fixtures_root = fixtures_root_from_manifest(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let mut diagnostics = BuildDiagnostics::new(&out_dir);

    let fixtures: Vec<_> = discover_fixtures(&fixtures_root)
        .into_iter()
        .filter(|fixture| manifest_dir.join(fixture).join("customizable").is_dir())
        .collect();
    assert!(
        !fixtures.is_empty(),
        "no C# fixtures opted in under {}",
        manifest_dir.display()
    );
    assert_type_shapes_ledger(&manifest_dir);
    assert_function_calls_ledger(&manifest_dir);

    for fixture in &fixtures {
        codegen_fixture(&fixtures_root, fixture, &manifest_dir, &mut diagnostics);
    }
    write_fixtures_tests_rs(&out_dir, &fixtures);
    diagnostics.finalize();

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    diagnostics: &mut BuildDiagnostics,
) {
    let loaded = load_fixture(fixtures_root, fixture);
    let generated = manifest_dir.join(fixture).join("generated");
    let baml_sdk = generated.join("baml_sdk");

    if generated.exists() {
        fs::remove_dir_all(&generated)
            .unwrap_or_else(|error| panic!("remove stale {}: {error}", generated.display()));
    }
    fs::create_dir_all(&baml_sdk).unwrap();

    let output = match sdkgen_csharp::try_to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::Language,
    ) {
        Ok(output) => output,
        Err(error) => {
            diagnostics.record("codegen", fixture, error.to_string());
            return;
        }
    };
    for (relative, content) in output {
        let path = baml_sdk.join(relative);
        if let Some(parent) = path.parent() {
            if let Err(error) = fs::create_dir_all(parent) {
                diagnostics.record(
                    "codegen_write",
                    fixture,
                    format!("create {}: {error}", parent.display()),
                );
                continue;
            }
        }
        if let Err(error) = fs::write(&path, content) {
            diagnostics.record(
                "codegen_write",
                fixture,
                format!("write {}: {error}", path.display()),
            );
        }
    }

    copy_customizable(&manifest_dir.join(fixture).join("customizable"), &generated);
    if fixture == "type_shapes" {
        copy_type_shapes_tests(manifest_dir, &generated, diagnostics);
        if let Err(error) = fs::write(generated.join("Program.cs"), render_type_shapes_program()) {
            diagnostics.record("parity_program_write", fixture, error);
        }
    }
    if let Err(error) = fs::write(generated.join("SdkTest.csproj"), PROJECT_TEMPLATE) {
        diagnostics.record("project_write", fixture, error);
    }
}

fn copy_type_shapes_tests(
    manifest_dir: &Path,
    generated: &Path,
    diagnostics: &mut BuildDiagnostics,
) {
    let source_root = manifest_dir.join("type_shapes").join("customizable");
    for (identity, gate) in TYPE_SHAPES_TESTS {
        if !matches!(gate, Gate::Now(_)) {
            continue;
        }
        let source = source_root.join(format!("{identity}.cs"));
        let destination = generated.join(format!("{identity}.cs"));
        let parent = destination
            .parent()
            .expect("C# parity test destination must have a parent");
        if let Err(error) = fs::create_dir_all(parent) {
            diagnostics.record(
                "parity_test_copy",
                "type_shapes",
                format!("create {}: {error}", parent.display()),
            );
            continue;
        }
        if let Err(error) = fs::copy(&source, &destination) {
            diagnostics.record(
                "parity_test_copy",
                "type_shapes",
                format!("copy {}: {error}", source.display()),
            );
        }
    }
}

fn assert_type_shapes_ledger(manifest_dir: &Path) {
    let python_root = manifest_dir
        .parent()
        .expect("C# crate must be under sdk_tests/crates")
        .join("python_pydantic2")
        .join("type_shapes")
        .join("customizable");
    let discovered = discover_test_identities(&python_root, "py");
    let expected = TYPE_SHAPES_TESTS
        .iter()
        .map(|(identity, _)| (*identity).to_string())
        .collect::<BTreeSet<_>>();
    assert_eq!(
        discovered, expected,
        "C# type_shapes Now/Later ledger does not match the canonical Python suite"
    );

    let csharp_root = manifest_dir.join("type_shapes").join("customizable");
    for (identity, gate) in TYPE_SHAPES_TESTS {
        match gate {
            Gate::Now(_) => {
                let source = csharp_root.join(format!("{identity}.cs"));
                assert!(
                    source.is_file(),
                    "C# type_shapes parity row `{identity}` is Now but {} is missing",
                    source.display()
                );
            }
            Gate::Later(reason) => assert!(
                !reason.trim().is_empty(),
                "C# type_shapes parity row `{identity}` needs a Later reason"
            ),
        }
    }

    let mut canonical_cases = BTreeSet::new();
    for (identity, gate) in CANONICAL_TYPE_CASES {
        assert!(
            canonical_cases.insert(*identity),
            "duplicate C# canonical type case `{identity}`"
        );
        match gate {
            CanonicalGate::Now { evidence } => {
                let source = manifest_dir.join(evidence);
                assert!(
                    source.is_file(),
                    "C# canonical type case `{identity}` is Now but evidence {} is missing",
                    source.display()
                );
            }
            CanonicalGate::Later(reason) => assert!(
                !reason.trim().is_empty(),
                "C# canonical type case `{identity}` needs a Later reason"
            ),
        }
    }
}

fn assert_function_calls_ledger(manifest_dir: &Path) {
    let python_root = manifest_dir
        .parent()
        .expect("C# crate must be under sdk_tests/crates")
        .join("python_pydantic2")
        .join("function_calls")
        .join("customizable");
    let discovered = discover_test_identities(&python_root, "py");
    let mut expected = BTreeSet::new();
    for (identity, _) in FUNCTION_CALLS_TESTS {
        assert!(
            expected.insert((*identity).to_string()),
            "duplicate C# function_calls parity row `{identity}`"
        );
    }
    assert_eq!(
        discovered, expected,
        "C# function_calls Now/Later ledger does not match the canonical Python suite"
    );

    for (identity, gate) in FUNCTION_CALLS_TESTS {
        match gate {
            CanonicalGate::Now { evidence } => {
                let source = manifest_dir.join(evidence);
                assert!(
                    source.is_file(),
                    "C# function_calls parity row `{identity}` is Now but evidence {} is missing",
                    source.display()
                );
            }
            CanonicalGate::Later(reason) => assert!(
                !reason.trim().is_empty(),
                "C# function_calls parity row `{identity}` needs a Later reason"
            ),
        }
    }
}

fn discover_test_identities(root: &Path, extension: &str) -> BTreeSet<String> {
    fn visit(root: &Path, directory: &Path, extension: &str, output: &mut BTreeSet<String>) {
        for entry in fs::read_dir(directory)
            .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        {
            let path = entry.expect("read test-suite entry").path();
            if path.is_dir() {
                visit(root, &path, extension, output);
            } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
                let relative = path
                    .strip_prefix(root)
                    .expect("test path must remain under suite root")
                    .with_extension("");
                output.insert(relative.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    let mut output = BTreeSet::new();
    visit(root, root, extension, &mut output);
    output
}

fn render_type_shapes_program() -> String {
    let mut output =
        String::from("// Generated by sdk_test_harness_setup::csharp::run_all; do not edit.\n");
    for (_, gate) in TYPE_SHAPES_TESTS {
        match gate {
            Gate::Now(entrypoint) => {
                output.push_str(entrypoint);
                output.push_str("();\n");
            }
            Gate::Later(_) => {}
        }
    }
    output
        .push_str("global::System.Console.WriteLine(\"C# shared type-shapes parity passed.\");\n");
    output
}

fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut body =
        String::from("// Generated by sdk_test_harness_setup::csharp::run_all; do not edit.\n");
    body.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    body.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
    ));

    for fixture in fixtures {
        body.push_str(&format!(
            r#"
mod {fixture} {{
    #[test]
    fn dotnet() {{
        ::sdk_test_harness_runner::run_test_cmd(
            "{fixture}",
            "dotnet run --configuration Release",
            "{CACHE_SUBDIR}",
            "{CACHE_ENV_VAR}",
        );
    }}
}}
"#,
        ));
    }

    fs::write(out_dir.join("csharp_tests.rs"), body).unwrap();
}
