//! Node.js + TypeScript sdk-test target — build-script side.
//!
//! Mirrors [`crate::python_pydantic2`] but for the
//! TypeScript/Node.js toolchain (pnpm + tsc + jest).
//!
//! Phase 2 (24a) wires `codegen_nodejs::to_source_code` end-to-end —
//! the call no longer panics, so the previous `std::panic::catch_unwind`
//! wrapper was dropped along with the `IGNORE_REASON` const that
//! `#[ignore]`d every emitted test. A future codegen panic is now
//! loud (the build script fails) rather than silently leaving
//! `baml_sdk/` empty.
//!
//! **pnpm steps live in
//! `sdk_tests/crates/nodejs_typescript/setup.sh`**, NOT in build.rs.
//! `cargo nextest run` invokes setup.sh automatically via the
//! setup-script binding in `baml_language/.config/nextest.toml`:
//!
//! ```sh
//! cd baml_language
//! cargo nextest run -p sdk_test_nodejs_typescript
//! ```
//!
//! For plain `cargo test` (no nextest), run setup.sh manually
//! between `cargo test --no-run` and `cargo test`. Re-run setup.sh
//! when bridge_nodejs's Rust source changes (the `.node` addon needs
//! rebuilding) or when adding a new fixture. build.rs's job is just
//! codegen + scaffold emit — it writes the per-fixture
//! `package.json` / `tsconfig.json` that setup.sh consumes.
//!
//! Auto-discovery story: build.rs emits
//! `OUT_DIR/nodejs_typescript_tests.rs` (a sequence of
//! `::sdk_test_harness_runner::*` invocations), the
//! `sdk_test_harness_runner::nodejs_typescript::test_suite!()` macro
//! `include!`s it, and adding a new fixture is `mkdir
//! sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `sdk_tests/crates/nodejs_typescript/<name>/`.

use std::{
    env, fs,
    path::{Path, PathBuf},
};

use codegen_nodejs::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, fixtures_root_from_manifest,
    load_fixture, watch_dir,
};

/// Per-fixture package.json. `__PACKAGE_NAME__` is substituted per
/// fixture. The dev toolchain (jest + ts-jest + typescript + types)
/// plus the BAML runtime dep on `@boundaryml/baml-node` (which
/// `file:`-points at the bridge_nodejs source tree five levels up:
/// `crates/nodejs_typescript/<F>/generated/` →
/// `crates/nodejs_typescript/<F>/` → `crates/nodejs_typescript/` →
/// `crates/` → `sdk_tests/` → `baml_language/`) is resolved by
/// `setup.sh`'s per-fixture `pnpm install`.
///
/// `haste.enableSymlinks = true` + `watchman = false` are required:
/// `customizable/*.test.ts` files are symlinked into the generated
/// dir, and jest's default haste module map skips symlinks (watchman
/// can't track them, so the two flags move together).
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
/// generated dir, so `@jest/globals` (and every other dev dep)
/// fails to resolve. With the flag, the resolver stays in the
/// symlink's view of the world and finds
/// `<F>/generated/node_modules/`.
const TSCONFIG_JSON: &str = include_str!("templates/tsconfig.json");

/// Shared pnpm store dir (`<workspace>/target/<CACHE_SUBDIR>`) —
/// `setup.sh` exports `CACHE_ENV_VAR=<that path>` during install, and
/// `sdk_test_harness_runner::run_test_cmd` sets the same env var when
/// invoking tsc/jest. Harmless for tsc/jest (they don't read pnpm
/// config) but keeps the harness_runner API uniform with python's uv
/// cache plumbing.
const CACHE_SUBDIR: &str = "pnpm-store";
const CACHE_ENV_VAR: &str = "npm_config_store_dir";

/// Entry point for `crates/nodejs_typescript/build.rs`. Discovers
/// fixtures, runs codegen for each, writes per-fixture templates, and
/// emits the test scaffold. Does NOT touch pnpm — that's setup.sh's
/// job (see module docs).
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

    for fixture in &fixtures {
        codegen_fixture(&fixtures_root, fixture, &manifest_dir, &mut diagnostics);
    }

    write_fixtures_tests_rs(&out_dir, &fixtures);
    diagnostics.finalize();

    println!("cargo:rerun-if-changed=build.rs");
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

    // codegen_nodejs::to_source_code is no longer a panicking stub
    // (24a Phase 2) — call it directly so any future codegen panic
    // is loud rather than soft-recorded. File-write failures stay
    // routed through `diagnostics` per the soft-fail design (env
    // issue, not a code bug).
    let output = codegen_nodejs::to_source_code(
        &loaded.pool,
        &loaded.user_baml_files,
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
        // Copy (rather than symlink, as python does) because node /
        // ts-jest follow symlinks during module resolution: the
        // realpath of a symlinked `*.test.ts` lives under
        // `customizable/` where there is no `node_modules`, so
        // `@jest/globals` (and every other dev dep) fails to resolve.
        // Setting `NODE_OPTIONS=--preserve-symlinks` to keep jest in
        // the symlink view also breaks `pnpm` itself (its CLI is
        // installed via symlink and uses a relative `require`).
        // Copying is cheaper than fighting that, and build.rs's
        // `rerun-if-changed` watch on `customizable/` re-stages on
        // edits.
        let copy_result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
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

/// Emit `OUT_DIR/nodejs_typescript_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored here;
/// `build_diagnostics!` and `run_test_cmd` live in `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdk_test_harness_setup::nodejs_typescript::run_all — do not edit.\n",
    );
    buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    for fixture in fixtures {
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
    fn tsc() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit");
    }}

    #[test]
    fn jest() {{
        cmd("node node_modules/jest/bin/jest.js");
    }}
}}
"#,
            fixture = fixture,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
        ));
    }
    let target = out_dir.join("nodejs_typescript_tests.rs");
    fs::write(&target, buf).unwrap();
}
