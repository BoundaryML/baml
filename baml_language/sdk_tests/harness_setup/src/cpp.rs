//! C++ sdk-test target — build-script side.
//!
//! [`run_all`] is called from `crates/cpp/build.rs`. It discovers every
//! fixture under `sdk_tests/fixtures/`, codegens each into
//! `crates/cpp/<fixture>/generated/baml_sdk/` (via `sdkgen_cpp`; panics land
//! in build diagnostics), symlinks
//! `crates/cpp/<fixture>/customizable/*` overlays into the generated tree
//! (test sources live in `customizable/tests/*.cc`), writes the per-fixture
//! `test.sh` compile-and-run driver, and emits `OUT_DIR/cpp_tests.rs` — the
//! scaffold `sdk_test_harness_runner::cpp::test_suite!()` `include!`s.
//!
//! The bridge_cffi cdylib build is NOT run here. It lives in
//! `crates/cpp/setup.sh`, invoked by `cargo nextest run` via the
//! setup-script binding in `.config/nextest.toml`, mirroring the other
//! targets' placement so `cargo check` stays toolchain-free.
//!
use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, symlink_customizable, watch_dir, write_codegen_output,
};

/// Per-fixture compile-and-run driver, written to `<fixture>/generated/test.sh`.
const TEST_SH_TEMPLATE: &str = include_str!("templates/cpp_test.sh");

const CACHE_SUBDIR: &str = "cpp-cache";
const CACHE_ENV_VAR: &str = "SDK_TEST_CPP_CACHE_DIR";

/// Env var the setup scripts write to `$NEXTEST_ENV`; must stay in sync with
/// `crates/cpp/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_CPP_SETUP";

/// Entry point for `crates/cpp/build.rs`.
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

    emit_cargo_line(format_args!("cargo:rerun-if-changed=build.rs"));
    watch_dir(&fixtures_root);
    for fixture in &fixtures {
        let custom = manifest_dir.join(fixture).join("customizable");
        watch_dir(&custom);
        // watch_dir registers existing files only; watching the directory
        // path itself makes cargo detect newly added test files too.
        if custom.exists() {
            emit_cargo_line(format_args!("cargo:rerun-if-changed={}", custom.display()));
        }
    }
}

fn codegen_fixture(
    fixtures_root: &Path,
    fixture: &str,
    manifest_dir: &Path,
    diagnostics: &mut BuildDiagnostics,
) {
    let loaded = load_fixture(fixtures_root, fixture);
    let fixture_root = manifest_dir.join(fixture);
    let generated = fixture_root.join("generated");
    let baml_sdk = generated.join("baml_sdk");

    if generated.exists() {
        fs::remove_dir_all(&generated).unwrap();
    }
    fs::create_dir_all(&baml_sdk).unwrap();

    let pool = loaded.pool;
    let user_baml_paths: Vec<sdkgen_cpp::UserBamlFile> = loaded
        .user_baml_files
        .into_iter()
        .map(|(rel, _)| rel)
        .collect();
    let baml_bytecode = loaded.baml_bytecode;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        sdkgen_cpp::to_source_code_with_bytecode(&pool, &user_baml_paths, &baml_bytecode)
    }));
    match codegen_result {
        Ok(output) => write_codegen_output(&baml_sdk, output, fixture, diagnostics),
        Err(_) => {
            diagnostics.record(
                "codegen",
                fixture,
                "sdkgen_cpp::to_source_code_with_bytecode panicked",
            );
        }
    }

    let custom = fixture_root.join("customizable");
    if custom.exists() {
        let symlink_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
            symlink_customizable(&custom, &generated);
        }));
        if symlink_result.is_err() {
            diagnostics.record(
                "symlink_customizable",
                fixture,
                format!(
                    "symlink_customizable({}, {}) panicked",
                    custom.display(),
                    generated.display()
                ),
            );
        }
    }

    if let Err(e) = fs::write(generated.join("test.sh"), TEST_SH_TEMPLATE) {
        diagnostics.record("test_sh_write", fixture, format!("write test.sh: {e}"));
    }
}

/// Emit `OUT_DIR/cpp_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations, two checks per fixture
/// (`compile`, `run`; `run` recompiles so the tests stay independent under
/// nextest's process-per-test model).
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut buf = String::new();
    buf.push_str("// Generated by sdk_test_harness_setup::cpp::run_all — do not edit.\n");
    buf.push_str("::sdk_test_harness_runner::build_diagnostics!();\n");
    buf.push_str(&format!(
        "::sdk_test_harness_runner::setup_guard!({SETUP_ENV_VAR:?});\n"
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
    fn compile() {{
        cmd("bash test.sh compile");
    }}

    #[test]
    fn run() {{
        cmd("bash test.sh run");
    }}
}}
"#,
            fixture = fixture,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
        ));
    }
    let target = out_dir.join("cpp_tests.rs");
    fs::write(&target, buf).unwrap();
}
