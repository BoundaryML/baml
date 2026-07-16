//! TypeScript SDK runtime matrix — build-script side.
//!
//! Mirrors [`crate::python_pydantic2`] but generates Node and Web SDKs and runs them through Node, Chromium, and workerd toolchains.
//!
//! `sdkgen_typescript_shared::sdkgen_typescript::to_source_code` runs directly (no `catch_unwind`):
//! the emitter has landed, so a panic is a real bug and aborts the build
//! loudly. Each fixture gets `generated/{node,web,workers}/baml_sdk/`, then the per-runtime `tsc` / `vitest` tests run under `cargo nextest`.
//!
//! **pnpm steps live in
//! `sdk_tests/crates/typescript/setup.sh`** (Unix) or
//! **`setup.ps1`** (Windows), NOT in build.rs. `cargo nextest run`
//! invokes the right one automatically via two platform-filtered
//! setup-script bindings (`cfg(unix)` / `cfg(windows)`) in
//! `baml_language/.config/nextest.toml`:
//!
//! ```sh
//! cd baml_language
//! cargo nextest run -p sdk_test_typescript_node
//! ```
//!
//! For plain `cargo test` (no nextest), run setup.sh (or setup.ps1
//! on Windows) manually between `cargo test --no-run` and
//! `cargo test`. Re-run when either bridge's source changes or when adding a new
//! fixture. build.rs's job is just codegen + scaffold emit — it
//! writes the per-fixture package and runtime config files that
//! the setup script consumes.
//!
//! Auto-discovery story: build.rs emits
//! `OUT_DIR/typescript_node_tests.rs` (a sequence of
//! `::sdk_test_harness_runner::*` invocations), the
//! `sdk_test_harness_runner::typescript_node::test_suite!()` macro
//! `include!`s it, and adding a new fixture is `mkdir
//! sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `sdk_tests/crates/typescript/<name>/`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use sdkgen_typescript_shared::{
    sdkgen_typescript::{self, NamingConvention},
    sdkgen_typescript_web,
};

use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, watch_dir,
};

/// Per-fixture package.json. `__PACKAGE_NAME__` is substituted per
/// fixture. The dev toolchain (vitest + typescript + types)
/// plus the BAML runtime dep on `@boundaryml/baml-bridge` (which
/// `file:`-points at the bridge_typescript source tree five levels up:
/// `crates/typescript/<F>/generated/` →
/// `crates/typescript/<F>/` → `crates/typescript/` →
/// `crates/` → `sdk_tests/` → `baml_language/`) is resolved by
/// `setup.sh`'s per-fixture `pnpm install`.
///
/// Lives at `src/templates/package.json` so editors give it real
/// JSON syntax highlighting + schema validation.
const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package.json");

/// Per-runtime tsconfig templates; written verbatim.
const TSCONFIG_JSON: &str = include_str!("templates/tsconfig.json");
const TSCONFIG_WEB_JSON: &str = include_str!("templates/tsconfig_web.json");
const TSCONFIG_WORKERS_JSON: &str = include_str!("templates/tsconfig_workers.json");
const VITEST_NODE_CONFIG: &str = include_str!("templates/vitest_node.config.ts");
const VITEST_WEB_CONFIG: &str = include_str!("templates/vitest_web.config.ts");
const VITEST_WORKERS_CONFIG: &str = include_str!("templates/vitest_workers.config.ts");

/// Shared pnpm store dir (`<workspace>/target/<CACHE_SUBDIR>`) —
/// `setup.sh` exports `CACHE_ENV_VAR=<that path>` during install, and
/// `sdk_test_harness_runner::run_test_cmd` sets the same env var when
/// invoking tsc/vitest. Harmless for tsc/vitest (they don't read pnpm
/// config) but keeps the harness_runner API uniform with python's uv
/// cache plumbing.
const CACHE_SUBDIR: &str = "pnpm-store";
const CACHE_ENV_VAR: &str = "npm_config_store_dir";

/// Env var the setup scripts write to `$NEXTEST_ENV` and the emitted
/// `setup_guard::ran` test checks for — the per-run breadcrumb that
/// the pnpm install + native build actually ran. Must stay in sync
/// with both `crates/typescript/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_TYPESCRIPT_NODE_SETUP";

/// Entry point for `crates/typescript/build.rs`. Discovers
/// fixtures, runs codegen for each (best-effort — see module docs),
/// writes templates, and emits the per-fixture test scaffold. Does
/// NOT touch pnpm — that's setup.sh's job (see module docs).
pub fn run_all() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let fixtures_root = fixtures_root_from_manifest(&manifest_dir);
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());

    let mut diagnostics = BuildDiagnostics::new(&out_dir);

    let fixtures = discover_fixtures(&fixtures_root);
    assert!(
        !fixtures.is_empty(),
        "no fixtures discovered under {}",
        fixtures_root.display()
    );

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
    has_node_tests: bool,
    has_web_tests: bool,
    has_workers_tests: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TestRuntime {
    Node,
    Web,
    Workers,
}

impl TestRuntime {
    const ALL: [Self; 3] = [Self::Node, Self::Web, Self::Workers];

    const fn dir(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Web => "web",
            Self::Workers => "workers",
        }
    }
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    custom: &Path,
    diagnostics: &mut BuildDiagnostics,
) -> FixtureTests {
    // `load_fixture` panics on .baml compile errors / missing baml_src /
    // empty fixture — those are author bugs in our repo, not env
    // issues, so we keep the hard failure (see 00c doc, "Hard vs.
    // soft failures").
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = manifest_dir.join(fixture);
    let generated = fixture_root.join("generated");

    if generated.exists() {
        for entry in fs::read_dir(&generated).unwrap() {
            let entry = entry.unwrap();
            let path = entry.path();
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
    for runtime in TestRuntime::ALL {
        fs::create_dir_all(generated.join(runtime.dir()).join("baml_sdk")).unwrap();
    }

    // Run codegen directly — the emitter has landed, so a panic here is a
    // real bug and should abort the build loudly rather than being captured
    // and downgraded to an empty `baml_sdk/`.
    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let node_output = sdkgen_typescript::to_source_code_with_bytecode(
        &pool,
        &baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(
        generated.join("node/baml_sdk"),
        node_output,
        fixture,
        diagnostics,
    );
    let web_output = sdkgen_typescript_web::to_source_code_with_bytecode(
        &pool,
        &baml_bytecode,
        NamingConvention::PreserveCase,
    );
    write_codegen_output(
        generated.join("web/baml_sdk"),
        web_output.clone(),
        fixture,
        diagnostics,
    );
    write_codegen_output(
        generated.join("workers/baml_sdk"),
        web_output,
        fixture,
        diagnostics,
    );

    for runtime in TestRuntime::ALL {
        if custom.exists() {
            copy_customizable_for_runtime(custom, &generated.join(runtime.dir()), runtime);
        }
    }
    rewrite_test_bridge_imports(&generated.join("web"));
    rewrite_test_bridge_imports(&generated.join("workers"));

    let package_name = format!("sdk-tests-typescript-{}", fixture.replace('_', "-"));
    let files = [
        (
            "package.json",
            PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name),
        ),
        ("tsconfig.node.json", TSCONFIG_JSON.to_string()),
        ("tsconfig.web.json", TSCONFIG_WEB_JSON.to_string()),
        ("tsconfig.workers.json", TSCONFIG_WORKERS_JSON.to_string()),
        ("vitest.node.config.ts", VITEST_NODE_CONFIG.to_string()),
        ("vitest.web.config.ts", VITEST_WEB_CONFIG.to_string()),
        (
            "vitest.workers.config.ts",
            VITEST_WORKERS_CONFIG.to_string(),
        ),
        (
            "wrangler.jsonc",
            r#"{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "sdk-test-typescript-workers",
  "main": "worker.js",
  "compatibility_date": "2026-07-15",
  "compatibility_flags": ["nodejs_compat"]
}
"#
            .to_string(),
        ),
        (
            "worker.js",
            r#"export default {
  fetch() {
    return new Response("sdk-test-typescript-workers");
  },
};
"#
            .to_string(),
        ),
    ];
    for (relative, contents) in files {
        if let Err(error) = fs::write(generated.join(relative), contents) {
            diagnostics.record("package_json_write", fixture, error);
        }
    }

    FixtureTests {
        name: fixture.to_string(),
        has_node_tests: has_vitest_tests(&generated.join("node")),
        has_web_tests: has_vitest_tests(&generated.join("web")),
        has_workers_tests: has_vitest_tests(&generated.join("workers")),
    }
}

fn write_codegen_output(
    baml_sdk: PathBuf,
    output: impl IntoIterator<Item = (PathBuf, String)>,
    fixture: &str,
    diagnostics: &mut BuildDiagnostics,
) {
    for (rel, content) in output {
        let path = baml_sdk.join(rel);
        if let Some(parent) = path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                diagnostics.record(
                    "codegen_write",
                    fixture,
                    format!("create_dir_all {}: {e}", parent.display()),
                );
                continue;
            }
        }
        if let Err(e) = fs::write(&path, content) {
            diagnostics.record(
                "codegen_write",
                fixture,
                format!("write {}: {e}", path.display()),
            );
        }
    }
}

fn copy_customizable_for_runtime(source: &Path, destination: &Path, runtime: TestRuntime) {
    fs::create_dir_all(destination).unwrap();
    for entry in fs::read_dir(source).unwrap().flatten() {
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        if source_path.is_dir() {
            copy_customizable_for_runtime(&source_path, &destination_path, runtime);
        } else if source_applies_to_runtime(&source_path, runtime) {
            fs::copy(&source_path, &destination_path).unwrap_or_else(|error| {
                panic!(
                    "copy {} to {}: {error}",
                    source_path.display(),
                    destination_path.display()
                )
            });
        }
    }
}

fn source_applies_to_runtime(path: &Path, runtime: TestRuntime) -> bool {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("");
    let qualified = if name.ends_with(".node.test.ts") || name.ends_with(".node.ts") {
        Some(TestRuntime::Node)
    } else if name.ends_with(".web.test.ts") || name.ends_with(".web.ts") {
        Some(TestRuntime::Web)
    } else if name.ends_with(".workers.test.ts") || name.ends_with(".workers.ts") {
        Some(TestRuntime::Workers)
    } else {
        None
    };
    qualified.is_none_or(|target| target == runtime)
}

fn rewrite_test_bridge_imports(dir: &Path) {
    for entry in fs::read_dir(dir).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().and_then(|name| name.to_str()) != Some("baml_sdk") {
                rewrite_test_bridge_imports(&path);
            }
        } else if path.extension().and_then(|ext| ext.to_str()) == Some("ts") {
            let source = fs::read_to_string(&path).unwrap();
            let web_source =
                source.replace("@boundaryml/baml-bridge", "@boundaryml/baml-bridge-web");
            if source != web_source {
                fs::write(path, web_source).unwrap();
            }
        }
    }
}

/// Emit `OUT_DIR/typescript_node_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored here;
/// `build_diagnostics!` and `run_test_cmd` live in `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[FixtureTests]) {
    let mut buf = String::new();
    buf.push_str("// Generated by sdk_test_harness_setup::typescript::run_all — do not edit.\n");
    buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buf.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
    ));
    buf.push_str(&format!(
        r#"
mod bridge_typescript {{
    #[test]
    fn attw() {{
        ::sdk_test_harness_runner::run_workspace_cmd(
            "sdks/typescript/bridge_typescript",
            "pnpm exec attw --pack --profile esm-only",
            "{cache_subdir}",
            "{cache_env_var}",
        );
    }}
}}
"#,
        cache_subdir = CACHE_SUBDIR,
        cache_env_var = CACHE_ENV_VAR,
    ));
    for fixture in fixtures {
        let name = &fixture.name;
        buf.push_str(&format!(
            r#"
mod {fixture} {{
    fn cmd(c: &str) {{
        ::sdk_test_harness_runner::run_test_cmd(
            "{fixture}",
            c,
            "{cache_subdir}",
            "{cache_env_var}",
        );
    }}

    #[test]
    fn esm_node() {{
        ::sdk_test_harness_runner::assert_typescript_node_generated_esm("{fixture}", "node");
    }}

    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn esm_web() {{
        ::sdk_test_harness_runner::assert_typescript_web_generated_esm("{fixture}", "web");
    }}

    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn esm_workers() {{
        ::sdk_test_harness_runner::assert_typescript_web_generated_esm("{fixture}", "workers");
    }}

    #[test]
    fn tsc_node() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.node.json");
    }}

    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn tsc_web() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.web.json");
    }}

    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn tsc_workers() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit --project tsconfig.workers.json");
    }}
"#,
            fixture = name,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
        ));

        if fixture.has_node_tests {
            buf.push_str(
                r#"
    #[test]
    fn vitest_node() {
        cmd("pnpm exec vitest run --config vitest.node.config.ts");
    }
"#,
            );
        }
        if fixture.has_web_tests {
            buf.push_str(
                r#"
    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn vitest_web() {
        cmd("pnpm exec vitest run --config vitest.web.config.ts");
    }
"#,
            );
        }
        if fixture.has_workers_tests {
            buf.push_str(
                r#"
    #[test]
    #[ignore = "typescript/web runtime lands in the Web implementation change"]
    fn vitest_workers() {
        cmd("pnpm exec vitest run --config vitest.workers.config.ts");
    }
"#,
            );
        }

        buf.push_str(
            r#"
}
"#,
        );
    }
    let target = out_dir.join("typescript_node_tests.rs");
    fs::write(&target, buf).unwrap();
}

fn has_vitest_tests(dir: &Path) -> bool {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_qualified_sources_only_target_the_named_runtime() {
        for runtime in TestRuntime::ALL {
            assert!(source_applies_to_runtime(
                Path::new("shared.test.ts"),
                runtime
            ));
            assert_eq!(
                source_applies_to_runtime(Path::new("only.node.test.ts"), runtime),
                runtime == TestRuntime::Node
            );
            assert_eq!(
                source_applies_to_runtime(Path::new("only.web.test.ts"), runtime),
                runtime == TestRuntime::Web
            );
            assert_eq!(
                source_applies_to_runtime(Path::new("only.workers.test.ts"), runtime),
                runtime == TestRuntime::Workers
            );
            assert_eq!(
                source_applies_to_runtime(Path::new("helper.node.ts"), runtime),
                runtime == TestRuntime::Node
            );
        }
    }
}
