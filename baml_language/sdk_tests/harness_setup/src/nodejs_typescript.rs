//! Node.js + TypeScript sdk-test target — build-script side.
//!
//! Mirrors [`crate::python_pydantic2`] but for the
//! TypeScript/Node.js toolchain (pnpm + tsc + jest).
//!
//! Until `codegen_nodejs::to_source_code` lands, the codegen call is
//! wrapped in [`std::panic::catch_unwind`] so the build script
//! survives the stub's `unimplemented!()` — `baml_sdk/` is just left
//! empty for that fixture, and the `tsc` / `jest` tests fail at
//! runtime (tsc can't resolve imports, jest can't load modules)
//! with a clear error rather than the entire crate failing to
//! build. Every emitted `#[test]` (including
//! `build_diagnostics::no_build_failures`) is `#[ignore]`d on this
//! crate while the stub stays in place — see [`IGNORE_REASON`].
//!
//! `pnpm install` runs in `build.rs` (serially, once per fixture)
//! rather than from each `#[test]`; that's the only way to keep
//! parallel test processes from racing on each fixture's
//! `node_modules/.pnpm/` virtual store.
//!
//! Same auto-discovery story as `python_pydantic2`: build.rs emits
//! `OUT_DIR/nodejs_typescript_tests.rs` (a sequence of
//! `::sdk_test_harness_runner::*` invocations), the
//! `sdk_test_harness_runner::nodejs_typescript::test_suite!()` macro
//! `include!`s it, and adding a new fixture is `mkdir
//! sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `sdk_tests/crates/nodejs_typescript/<name>/`.

use std::{
    env, fs,
    io::ErrorKind,
    panic,
    path::{Path, PathBuf},
    process::Command,
};

use codegen_nodejs::NamingConvention;

use crate::{
    BuildDiagnostics, copy_customizable, discover_fixtures, fixtures_root_from_manifest,
    load_fixture, watch_dir, workspace_root_from_manifest,
};

/// Reason string stamped onto every nodejs_typescript test's
/// `#[ignore]` attr — per-fixture `tsc`/`jest` plus the shared
/// `build_diagnostics`. While `codegen_nodejs::to_source_code` is a
/// stub every fixture records a `codegen` failure and produces an
/// empty `baml_sdk/`, so the toolchain tests can't pass. Remove the
/// `#[ignore]`s — and the `Some(...)` arg to `build_diagnostics!`
/// below — once the emitter lands.
pub const IGNORE_REASON: &str = "codegen_nodejs is a stub; re-enable once the emitter lands";

/// Per-fixture package.json. `__PACKAGE_NAME__` is substituted per
/// fixture. `pnpm install` resolves the dev toolchain (jest +
/// ts-jest + typescript + types); the BAML runtime dep on
/// `@boundaryml/baml-node` (which would point at
/// `sdks/nodejs/bridge_nodejs` via a `file:` source) is deliberately
/// omitted while codegen_nodejs is a stub — adding it would force a
/// NAPI build per fixture for no test value.
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

// `pnpm install --ignore-workspace` is required because the repo has
// a top-level `pnpm-workspace.yaml` (at `/Users/sam/baml3/`); without
// the flag pnpm walks up to that workspace and skips installing into
// the fixture dir entirely. `ignore-workspace` is a CLI flag in
// pnpm — it isn't an .npmrc setting, so it has to live on the
// command rather than the per-fixture config.

const CACHE_SUBDIR: &str = "pnpm-store";
const CACHE_ENV_VAR: &str = "npm_config_store_dir";

/// Entry point for `crates/nodejs_typescript/build.rs`. Discovers
/// fixtures, runs codegen for each (best-effort — see module docs),
/// writes templates, and emits the per-fixture test scaffold.
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

    // `pnpm install` is done here in build.rs, serially per fixture, so
    // the parallel `#[test]` processes don't race on each fixture's
    // `.pnpm` virtual store (pnpm's per-project install isn't safe under
    // concurrent invocations into the same `node_modules/`). The tests
    // themselves only run `tsc` / `jest`, which are read-only against
    // an already-populated tree.
    let store_dir = workspace_root_from_manifest(&manifest_dir)
        .join("target")
        .join(CACHE_SUBDIR);
    fs::create_dir_all(&store_dir).unwrap();
    for fixture in &fixtures {
        let generated_dir = manifest_dir.join(fixture).join("generated");
        if let Err(msg) = pnpm_install(&generated_dir, &store_dir) {
            diagnostics.record("pnpm_install", fixture, msg);
        }
    }

    write_fixtures_tests_rs(&out_dir, &fixtures);
    diagnostics.finalize();

    println!("cargo:rerun-if-changed=build.rs");
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        watch_dir(&manifest_dir.join(fixture).join("customizable"));
    }
}

fn pnpm_install(generated_fixture_dir: &Path, store_dir: &Path) -> Result<(), String> {
    let spawn = Command::new("pnpm")
        .args(["install", "--ignore-workspace"])
        .current_dir(generated_fixture_dir)
        .env(CACHE_ENV_VAR, store_dir)
        .output();
    let output = match spawn {
        Ok(o) => o,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "spawn failed: `pnpm` not on PATH ({e})\nhint: install pnpm (https://pnpm.io) or via corepack"
            ));
        }
        Err(e) => {
            return Err(format!(
                "failed to spawn `pnpm install` in {}: {e}",
                generated_fixture_dir.display()
            ));
        }
    };
    if !output.status.success() {
        return Err(format!(
            "pnpm install failed in {} (exit {}):\nstdout: {}\nstderr: {}",
            generated_fixture_dir.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    Ok(())
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

    // codegen_nodejs is a stub — `to_source_code` currently
    // panics with `unimplemented!()`. Catch the unwind, record it,
    // and leave `baml_sdk/` empty; tsc / jest tests then fail at
    // runtime with a clearer "tsc can't find module" / "jest can't
    // require" signal than a hard build abort.
    let pool = loaded.pool;
    let user_baml_files = loaded.user_baml_files;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        codegen_nodejs::to_source_code(&pool, &user_baml_files, NamingConvention::PreserveCase)
    }));
    match codegen_result {
        Ok(output) => {
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
        }
        Err(_) => {
            diagnostics.record(
                "codegen",
                fixture,
                "codegen_nodejs::to_source_code panicked (likely the not-yet-implemented stub) — baml_sdk/ left empty",
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

/// Emit `OUT_DIR/nodejs_typescript_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored here;
/// `build_diagnostics!` and `run_test_cmd` live in `sdk_test_harness_runner`.
/// Every emitted test is `#[ignore]`d while `codegen_nodejs` is a
/// stub (see [`IGNORE_REASON`]).
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdk_test_harness_setup::nodejs_typescript::run_all — do not edit.\n",
    );
    buf.push_str(&format!(
        "::sdk_test_harness_runner::build_diagnostics!(ignore = {IGNORE_REASON:?});\n"
    ));
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
    #[ignore = {ignore_reason:?}]
    fn tsc() {{
        cmd("node node_modules/typescript/bin/tsc --noEmit");
    }}

    #[test]
    #[ignore = {ignore_reason:?}]
    fn jest() {{
        cmd("node node_modules/jest/bin/jest.js");
    }}
}}
"#,
            fixture = fixture,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
            ignore_reason = IGNORE_REASON,
        ));
    }
    let target = out_dir.join("nodejs_typescript_tests.rs");
    fs::write(&target, buf).unwrap();
}
