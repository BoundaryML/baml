//! Python + pydantic2 sdk-test target — build-script side.
//!
//! [`run_all`] is called from `crates/python_pydantic2/build.rs`. It
//! discovers every fixture under `sdk_tests/fixtures/`, codegens
//! each into `crates/python_pydantic2/<fixture>/generated/baml_sdk/`,
//! symlinks `crates/python_pydantic2/<fixture>/customizable/*`
//! overlays into the generated tree, writes a per-fixture
//! `pyproject.toml`, runs `uv sync` per fixture (which triggers the
//! maturin build of `baml_core`'s editable install), and emits
//! `OUT_DIR/python_pydantic2_tests.rs` — the per-fixture `#[test]`
//! scaffold the `sdk_test_harness_runner::python_pydantic2::test_suite!()`
//! macro `include!`s.
//!
//! `uv sync` (and the maturin build it kicks off) runs in build.rs
//! rather than from each `#[test]`; that mirrors the
//! nodejs_typescript target's `pnpm install` placement and avoids
//! re-syncing once per test.
//!
//! The macro-via-include indirection means adding a new fixture is
//! just `mkdir sdk_tests/fixtures/<name>/baml_src/` + dropping a
//! `customizable/` directory under
//! `crates/python_pydantic2/<name>/`; no edits to either `build.rs`
//! or `src/lib.rs` needed.

use std::{
    env, fs,
    io::ErrorKind,
    panic,
    path::{Path, PathBuf},
    process::Command,
};

use codegen_python::NamingConvention;

use crate::{
    BuildDiagnostics, discover_fixtures, fixtures_root_from_manifest, load_fixture,
    symlink_customizable, watch_dir, workspace_root_from_manifest,
};

/// uv-friendly pyproject template. Each fixture's pyproject gets a
/// unique `name` substituted in for `__PYPROJECT_NAME__`. `baml_core`
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

    // `uv sync` is done here in build.rs, serially per fixture,
    // mirroring the nodejs_typescript target's `pnpm install`
    // placement. uv's editable install of `baml_core` (declared in
    // `[tool.uv.sources]`) invokes the maturin build backend, which
    // compiles `bridge_python` once per fixture into the fixture's
    // venv. Tests then only run ruff / pyright / pytest against an
    // already-synced tree.
    let cache_dir = workspace_root_from_manifest(&manifest_dir)
        .join("target")
        .join(CACHE_SUBDIR);
    fs::create_dir_all(&cache_dir).unwrap();
    for fixture in &fixtures {
        let generated_dir = manifest_dir.join(fixture).join("generated");
        if let Err(msg) = uv_sync(&generated_dir, &cache_dir) {
            diagnostics.record("uv_sync", fixture, msg);
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

fn uv_sync(generated_fixture_dir: &Path, cache_dir: &Path) -> Result<(), String> {
    let spawn = Command::new("uv")
        .arg("sync")
        .current_dir(generated_fixture_dir)
        .env(CACHE_ENV_VAR, cache_dir)
        .output();
    let output = match spawn {
        Ok(o) => o,
        Err(e) if e.kind() == ErrorKind::NotFound => {
            return Err(format!(
                "spawn failed: `uv` not on PATH ({e})\nhint: install uv (https://astral.sh/uv) or via mise/asdf"
            ));
        }
        Err(e) => {
            return Err(format!(
                "failed to spawn `uv sync` in {}: {e}",
                generated_fixture_dir.display()
            ));
        }
    };
    if !output.status.success() {
        return Err(format!(
            "uv sync failed in {} (exit {}):\nstdout: {}\nstderr: {}",
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

    let pool = loaded.pool;
    let user_baml_files = loaded.user_baml_files;
    let codegen_result = panic::catch_unwind(panic::AssertUnwindSafe(|| {
        codegen_python::to_source_code(&pool, &user_baml_files, NamingConvention::PreserveCase)
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
                "codegen_python::to_source_code panicked",
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
        cmd("uv run pyright baml_sdk");
    }}

    #[test]
    fn pytest() {{
        cmd("uv run pytest -v");
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
