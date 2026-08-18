//! Node TypeScript SDK test generation.
//!
//! The canonical TypeScript test corpus lives under `sdk_tests/crates/typescript`.
//! This module emits only the Node SDK, Node package configuration, and Node Rust
//! test scaffold. Browser and Workers generation lives in [`crate::typescript_web`].

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use sdkgen_typescript_shared::sdkgen_typescript::{self, NamingConvention};

use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, watch_dir, write_codegen_output,
};

const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package_node.json");
const TSCONFIG_JSON: &str = include_str!("templates/tsconfig.json");
const VITEST_NODE_CONFIG: &str = include_str!("templates/vitest_node.config.ts");
pub(crate) const TEST_RUNTIME: &str = include_str!("templates/test_runtime.ts");

pub(crate) const CACHE_SUBDIR: &str = "pnpm-store";
pub(crate) const CACHE_ENV_VAR: &str = "npm_config_store_dir";
const SETUP_ENV_VAR: &str = "SDK_TEST_TYPESCRIPT_SETUP";

pub fn run_all() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let fixtures_root = fixtures_root_from_manifest(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let fixtures = discover_fixtures(&fixtures_root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered under {}",
        fixtures_root.display()
    );

    let mut diagnostics = BuildDiagnostics::new(&out_dir);
    let mut fixture_tests = Vec::new();
    for fixture in &fixtures {
        let custom = manifest_dir.join(fixture).join("customizable");
        fixture_tests.push(codegen_fixture(
            &fixtures_root,
            fixture,
            &manifest_dir,
            &custom,
            &mut diagnostics,
        ));
    }

    write_fixtures_tests_rs(&out_dir, &fixture_tests);
    diagnostics.finalize();

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
}

struct FixtureTests {
    name: String,
    has_tests: bool,
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    custom: &Path,
    diagnostics: &mut BuildDiagnostics,
) -> FixtureTests {
    let loaded = load_fixture(fixtures_root, fixture);
    let generated = manifest_dir.join(fixture).join("generated");
    clean_generated(&generated);

    let node = generated.join("node");
    fs::create_dir_all(node.join("baml_sdk")).unwrap();
    let output = sdkgen_typescript::to_source_code_with_bytecode(
        &loaded.pool,
        &loaded.baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(&node.join("baml_sdk"), output, fixture, diagnostics);
    if custom.exists() {
        copy_customizable(custom, &node);
    }
    fs::write(node.join("test_runtime.ts"), TEST_RUNTIME).unwrap();

    let package_name = format!("sdk-tests-typescript-{}", fixture.replace('_', "-"));
    let files = [
        (
            "package.json",
            PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
        ),
        ("tsconfig.node.json", TSCONFIG_JSON.to_string()),
        ("vitest.node.config.ts", VITEST_NODE_CONFIG.to_string()),
    ];
    for (relative, contents) in files {
        if let Err(error) = fs::write(generated.join(relative), contents) {
            diagnostics.record("package_json_write", fixture, error);
        }
    }

    FixtureTests {
        name: fixture.to_string(),
        has_tests: has_vitest_tests(&node),
    }
}

pub(crate) fn clean_generated(generated: &Path) {
    if !generated.exists() {
        return;
    }
    for entry in fs::read_dir(generated).unwrap() {
        let path = entry.unwrap().path();
        if path.file_name().and_then(|name| name.to_str()) == Some("node_modules") {
            continue;
        }
        if path.is_dir() {
            fs::remove_dir_all(&path).unwrap();
        } else {
            fs::remove_file(&path).unwrap();
        }
    }
}

pub(crate) fn copy_customizable(source: &Path, destination: &Path) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap().flatten() {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_customizable(&source_path, &destination_path);
        } else {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            });
            // `fs::copy` carries the source's mode across. When the source
            // tree is read-only - a nix store path, under a per-crate build
            // system - every copy lands at 0444 and `rewrite_test_bridge_
            // imports` below dies EACCES on its own scratch file. Make the
            // copies writable; they belong to this build.
            let mut permissions = fs::metadata(&destination_path)
                .unwrap_or_else(|error| panic!("stat {}: {error}", destination_path.display()))
                .permissions();
            if permissions.readonly() {
                // Owner-write only: `set_readonly(false)` on unix would also
                // set the group/other write bits.
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    permissions.set_mode(permissions.mode() | 0o200);
                }
                #[cfg(not(unix))]
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
                fs::set_permissions(&destination_path, permissions).unwrap_or_else(|error| {
                    panic!("chmod {}: {error}", destination_path.display())
                });
            }
        }
    }
}

pub(crate) fn rewrite_test_bridge_imports(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            let skip = matches!(
                path.file_name().and_then(|name| name.to_str()),
                Some("baml_sdk") | Some("node_modules")
            );
            if !skip {
                rewrite_test_bridge_imports(&path);
            }
        } else if path.extension().and_then(|extension| extension.to_str()) == Some("ts") {
            let source = fs::read_to_string(&path).unwrap();
            let web_source =
                source.replace("@boundaryml/baml-bridge", "@boundaryml/baml-bridge-web");
            if source != web_source {
                fs::write(path, web_source).unwrap();
            }
        }
    }
}

pub(crate) fn has_vitest_tests(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if has_vitest_tests(&path) {
                return true;
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(".test.ts"))
        {
            return true;
        }
    }
    false
}

fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[FixtureTests]) {
    let mut buffer = String::new();
    buffer.push_str("// Generated by sdk_test_harness_setup::typescript::run_all — do not edit.\n");
    buffer.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buffer.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
    ));
    // Runs through the bridge's own `attw` package script rather than
    // `pnpm exec attw` so a local `pnpm attw` and this test stay the same
    // check — the script stages the pack without the native addon
    // (`typescript_src/attw-check.js` explains why).
    buffer.push_str(&format!(
        r#"
mod bridge_typescript {{
    #[test]
    fn attw() {{
        ::sdk_test_harness_runner::run_workspace_cmd(
            "sdks/typescript/bridge_typescript",
            "pnpm run attw",
            "{CACHE_SUBDIR}",
            "{CACHE_ENV_VAR}",
        );
    }}
}}
"#
    ));

    for fixture in fixtures {
        let name = &fixture.name;
        buffer.push_str(&format!(
            r#"
mod {name} {{
    fn cmd(command: &str) {{
        ::sdk_test_harness_runner::run_test_cmd(
            "{name}",
            command,
            "{CACHE_SUBDIR}",
            "{CACHE_ENV_VAR}",
        );
    }}

    #[test]
    fn esm_node() {{
        ::sdk_test_harness_runner::assert_typescript_node_generated_esm("{name}", "node");
    }}

    #[test]
    fn tsc_node() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.node.json");
    }}
"#
        ));
        if fixture.has_tests {
            buffer.push_str(
                r#"
    #[test]
    fn vitest_node() {
        cmd("pnpm exec vitest run --config vitest.node.config.ts");
    }
"#,
            );
        }
        buffer.push_str("\n}\n");
    }

    fs::write(out_dir.join("typescript_tests.rs"), buffer).unwrap();
}
