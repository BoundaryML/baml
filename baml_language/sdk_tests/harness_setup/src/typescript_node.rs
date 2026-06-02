//! Node.js + TypeScript sdk-test target — build-script side.
//!
//! Mirrors [`crate::python_pydantic2`] but for the
//! TypeScript/Node.js toolchain (pnpm + tsc + vitest).
//!
//! `codegen_nodejs::to_source_code` runs directly (no `catch_unwind`):
//! the emitter has landed, so a panic is a real bug and aborts the build
//! loudly. Each fixture's `baml_sdk/` is generated, then the per-fixture
//! `tsc` / `vitest` tests run under `cargo nextest`.
//!
//! **pnpm steps live in
//! `sdk_tests/crates/typescript_node/setup.sh`** (Unix) or
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
//! `cargo test`. Re-run when bridge_nodejs's Rust source changes
//! (the `.node` addon needs rebuilding) or when adding a new
//! fixture. build.rs's job is just codegen + scaffold emit — it
//! writes the per-fixture `package.json` / `tsconfig.json` that
//! the setup script consumes.
//!
//! Auto-discovery story: build.rs emits
//! `OUT_DIR/typescript_node_tests.rs` (a sequence of
//! `::sdk_test_harness_runner::*` invocations), the
//! `sdk_test_harness_runner::typescript_node::test_suite!()` macro
//! `include!`s it, and adding a new fixture is `mkdir
//! sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `sdk_tests/crates/typescript_node/<name>/`.

use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use codegen_nodejs::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, fixtures_root_from_manifest,
    load_fixture, watch_dir,
};

/// Per-fixture package.json. `__PACKAGE_NAME__` is substituted per
/// fixture. The dev toolchain (vitest + typescript + types)
/// plus the BAML runtime dep on `@boundaryml/baml-core-node` (which
/// `file:`-points at the bridge_nodejs source tree five levels up:
/// `crates/typescript_node/<F>/generated/` →
/// `crates/typescript_node/<F>/` → `crates/typescript_node/` →
/// `crates/` → `sdk_tests/` → `baml_language/`) is resolved by
/// `setup.sh`'s per-fixture `pnpm install`.
///
/// Lives at `src/templates/package.json` so editors give it real
/// JSON syntax highlighting + schema validation.
const PACKAGE_JSON_TEMPLATE: &str = include_str!("templates/package.json");

/// Per-fixture tsconfig.json — no placeholder; written verbatim.
///
/// `preserveSymlinks: true` is load-bearing. Customizable
/// `*.test.ts` files are symlinked into the generated dir; without
/// `preserveSymlinks`, TypeScript resolves the test file to its
/// realpath under `customizable/` and then walks up from there
/// looking for `node_modules` — which doesn't exist outside the
/// generated dir, so test-framework packages (and every other dev dep)
/// fails to resolve. With the flag, the resolver stays in the
/// symlink's view of the world and finds
/// `<F>/generated/node_modules/`.
const TSCONFIG_JSON: &str = include_str!("templates/tsconfig.json");

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
/// with both `crates/typescript_node/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_TYPESCRIPT_NODE_SETUP";

/// Entry point for `crates/typescript_node/build.rs`. Discovers
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
        codegen_fixture(&fixtures_root, fixture, &manifest_dir, &mut diagnostics);
        fixture_tests.push(FixtureTests {
            name: fixture.clone(),
            has_vitest_tests: has_vitest_tests(&manifest_dir.join(fixture).join("customizable")),
        });
    }

    write_fixtures_tests_rs(&out_dir, &fixture_tests);
    diagnostics.finalize();

    println!("cargo:rerun-if-changed=build.rs");
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
}

struct FixtureTests {
    name: String,
    has_vitest_tests: bool,
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    diagnostics: &mut BuildDiagnostics,
) {
    // `load_fixture` panics on .baml compile errors / missing baml_src /
    // empty fixture — those are author bugs in our repo, not env
    // issues, so we keep the hard failure (see 00c doc, "Hard vs.
    // soft failures").
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = manifest_dir.join(fixture);
    let generated = fixture_root.join("generated");
    let baml_sdk = generated.join("baml_sdk");

    if generated.exists() {
        fs::remove_dir_all(&generated).unwrap();
    }
    fs::create_dir_all(&baml_sdk).unwrap();

    // Run codegen directly — the emitter has landed, so a panic here is a
    // real bug and should abort the build loudly rather than being captured
    // and downgraded to an empty `baml_sdk/`.
    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let output = codegen_nodejs::to_source_code_with_bytecode(
        &pool,
        &baml_bytecode,
        NamingConvention::PreserveCase,
    );
    for (rel, content) in output {
        let path = baml_sdk.join(&rel);
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

    let custom = fixture_root.join("customizable");
    if custom.exists() {
        // Copying keeps test module resolution inside `generated/`, where the
        // fixture-local node_modules lives. It also avoids pnpm/symlink
        // interactions in setup scripts.
        let copy_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            copy_customizable(&custom, &generated);
        }));
        if copy_result.is_err() {
            diagnostics.record(
                "copy_customizable",
                fixture,
                format!(
                    "copy_customizable({}, {}) panicked",
                    custom.display(),
                    generated.display()
                ),
            );
        }
    }

    let package_name = format!("sdk-tests-nodejs-typescript-{}", fixture.replace('_', "-"));
    let package_json = PACKAGE_JSON_TEMPLATE.replace("__PACKAGE_NAME__", &package_name);
    if let Err(e) = fs::write(generated.join("package.json"), package_json) {
        diagnostics.record(
            "package_json_write",
            fixture,
            format!("write package.json: {e}"),
        );
    }
    if let Err(e) = fs::write(generated.join("tsconfig.json"), TSCONFIG_JSON) {
        diagnostics.record(
            "package_json_write",
            fixture,
            format!("write tsconfig.json: {e}"),
        );
    }
}

/// Emit `OUT_DIR/typescript_node_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored here;
/// `build_diagnostics!` and `run_test_cmd` live in `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[FixtureTests]) {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdk_test_harness_setup::typescript_node::run_all — do not edit.\n",
    );
    buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buf.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
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
    fn esm_output() {{
        ::sdk_test_harness_runner::assert_typescript_node_generated_esm("{fixture}");
    }}

    #[test]
    fn tsc() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit");
    }}
"#,
            fixture = name,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
        ));

        if fixture.has_vitest_tests {
            buf.push_str(
                r#"
    #[test]
    fn vitest() {
        cmd("pnpm exec vitest run");
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
