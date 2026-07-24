//! Python + pydantic2 sdk-test target — build-script side.
//!
//! [`run_all`] is called from `crates/python_pydantic2/build.rs`. It
//! discovers every fixture under `sdk_tests/fixtures/`, codegens
//! each into `crates/python_pydantic2/<fixture>/generated/baml_sdk/`,
//! symlinks `crates/python_pydantic2/<fixture>/customizable/*`
//! overlays into the generated tree, writes a per-fixture
//! `pyproject.toml`, and emits `OUT_DIR/python_pydantic2_tests.rs` —
//! the per-fixture `#[test]` scaffold the
//! `sdk_test_harness_runner::python_pydantic2::test_suite!()` macro
//! `include!`s.
//!
//! `uv sync` is NOT run here. It lives in
//! `crates/python_pydantic2/setup.sh`, invoked by `cargo nextest run`
//! via the setup-script binding in `.config/nextest.toml` (and run
//! manually after `cargo test --no-run` for plain `cargo test`). This
//! mirrors the TypeScript target's `pnpm install` placement,
//! keeps codegen deps the only thing build.rs pulls in, and — most
//! importantly — lets setup.sh pass `--reinstall-package baml_bridge`
//! so the maturin-built `.so` is rebuilt on incremental Rust edits
//! (which a plain `uv sync` skips, leaving a stale `.so`).
//!
//! The macro-via-include indirection means adding a new fixture is
//! just `mkdir sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `crates/python_pydantic2/<name>/`; no edits to either `build.rs`
//! or `src/lib.rs` needed.

use std::{
    env, fs, panic,
    path::{Path, PathBuf},
};

use sdkgen_python_pydantic2::NamingConvention;

use crate::{
    BuildDiagnostics, discover_fixtures, emit_cargo_line, fixtures_root_from_manifest,
    load_fixture, symlink_customizable, watch_dir, write_codegen_output,
};

/// uv-friendly pyproject template. Each fixture's pyproject gets a
/// unique `name` substituted in for `__PYPROJECT_NAME__`. `baml_bridge`
/// is wired to the local `sdks/python/` source via
/// `[tool.uv.sources]` — the relative path is 5 ancestors up from
/// `crates/python_pydantic2/<F>/generated/pyproject.toml`:
/// `generated` → `<F>` → `python_pydantic2` → `crates` → `sdk_tests`
/// → `baml_language`. `pytest-asyncio` + `asyncio_mode = "auto"` are
/// included universally — harmless for fixtures without async tests.
/// Lives at `src/templates/pyproject.toml` so editors give it real
/// TOML syntax highlighting + tooling validation.
const PYPROJECT_TEMPLATE: &str = include_str!("templates/pyproject.toml");

const CACHE_SUBDIR: &str = "uv-cache";
const CACHE_ENV_VAR: &str = "UV_CACHE_DIR";

/// Env var the setup scripts write to `$NEXTEST_ENV` and the emitted
/// `setup_guard::ran` test checks for — the per-run breadcrumb that
/// the `uv sync` step actually ran. Must stay in sync with both
/// `crates/python_pydantic2/setup.sh` and `setup.ps1`.
const SETUP_ENV_VAR: &str = "SDK_TEST_PYTHON_PYDANTIC2_SETUP";

/// Entry point for `crates/python_pydantic2/build.rs`. Drives codegen
/// across every fixture and emits the per-fixture test scaffold to
/// OUT_DIR.
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

    // `uv sync` is NOT run here — it lives in
    // `crates/python_pydantic2/setup.sh`, fired by `cargo nextest run`
    // (see module docs). build.rs only codegens + writes pyproject +
    // emits the scaffold, so it stays uv-free like `cargo check`.
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

    let pool = loaded.pool;
    let baml_bytecode = loaded.baml_bytecode;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        sdkgen_python_pydantic2::to_source_code_with_bytecode(
            &pool,
            &baml_bytecode,
            NamingConvention::PreserveCase,
        )
    }));
    match codegen_result {
        Ok(output) => write_codegen_output(&baml_sdk, output, fixture, diagnostics),
        Err(_) => {
            diagnostics.record(
                "codegen",
                fixture,
                "sdkgen_python_pydantic2::to_source_code panicked",
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

    let pyproject_name = format!("sdk-tests-python-pydantic2-{}", fixture.replace('_', "-"));
    let pyproject = PYPROJECT_TEMPLATE.replace("__PYPROJECT_NAME__", &pyproject_name);
    if let Err(e) = fs::write(generated.join("pyproject.toml"), pyproject) {
        diagnostics.record(
            "pyproject_write",
            fixture,
            format!("write pyproject.toml: {e}"),
        );
    }
}

/// Emit `OUT_DIR/python_pydantic2_tests.rs` — a sequence of
/// `::sdk_test_harness_runner::*` invocations. No test bodies authored here;
/// `build_diagnostics!` and `run_test_cmd` live in `sdk_test_harness_runner`.
fn write_fixtures_tests_rs(out_dir: &Path, fixtures: &[String]) {
    let mut buf = String::new();
    buf.push_str(
        "// Generated by sdk_test_harness_setup::python_pydantic2::run_all — do not edit.\n",
    );
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
    fn ruff() {{
        cmd("uv run ruff check --config pyproject.toml baml_sdk");
    }}

    #[test]
    fn pyright() {{
        cmd("uv run pyright");
    }}

    #[test]
    fn pytest() {{
        ::sdk_test_harness_runner::run_test_cmd_allowing_exit_codes(
            "{fixture}",
            "uv run pytest -v",
            "{cache_subdir}",
            "{cache_env_var}",
            &[5],
        );
    }}
}}
"#,
            fixture = fixture,
            cache_subdir = CACHE_SUBDIR,
            cache_env_var = CACHE_ENV_VAR,
        ));
    }
    let target = out_dir.join("python_pydantic2_tests.rs");
    fs::write(&target, buf).unwrap();
}
