//! Shared build- and test-side logic for the `sdk_tests/crates/*`
//! crates. Each sdk-test crate's `tests/sdk_test.rs` collapses to:
//!
//! ```ignore
//! sdk_test_build::sdk_test_suite!();
//! ```
//!
//! `run` mirrors what `baml-cli generate` does end-to-end: discover
//! `.baml` files → ProjectDatabase → diagnostics gate →
//! `build_symbol_pool` → `codegen_python::to_source_code`. Then writes
//! a `pyproject.toml` and symlinks `customizable/*.py` into
//! `generated/`.
//!
//! `sdk_test_suite!` expands to four `#[test]` functions — `sync_only`,
//! `ruff`, `pyright`, `pytest`. The latter three each run `uv sync`
//! then `uv run <check>` inside `generated/` via
//! `Command::new(...).args(...)`. `sync_only` runs `uv sync` on its
//! own so CI can pre-warm the shared `target/uv-cache` (editable
//! build of `baml_core` + wheel extractions) by running it serially
//! across all sdk_test crates before the rest of the suite fans out
//! in parallel. Within a single crate, libtest's parallel threads
//! plus uv's own file locks make concurrent `uv sync` calls against
//! the same venv safe; cross-crate contention on the shared cache is
//! what the pre-warm exists to dodge.

use std::{
    env, fs,
    io::{self, ErrorKind},
    path::{Path, PathBuf},
    process::{Command, Output},
};

use baml_db::baml_compiler_diagnostics::Severity;
use baml_project::ProjectDatabase;
use codegen_python::UserBamlFile;

/// Drive codegen for one sdk-test crate. `crate_name` is the rust
/// crate's `CARGO_PKG_NAME` (e.g. `"sdk_test_llm_functions"`); the
/// generated pyproject's `name` is derived by stripping the
/// `sdk_test_` prefix and replacing underscores with hyphens — so
/// `sdk_test_llm_functions` → `baml-test-llm-functions`.
pub fn run(crate_name: &str) {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let baml_src = manifest_dir.join("baml_src");
    let generated_dir = manifest_dir.join("generated");
    let baml_sdk_dir = generated_dir.join("baml_sdk");

    if generated_dir.exists() {
        fs::remove_dir_all(&generated_dir).unwrap();
    }
    fs::create_dir_all(&baml_sdk_dir).unwrap();

    // 1. Discover + load .baml files into the project DB.
    let canonical = fs::canonicalize(&baml_src)
        .unwrap_or_else(|_| panic!("baml_src not found at {}", baml_src.display()));
    let mut db = ProjectDatabase::new();
    db.set_project_root(&canonical);
    let baml_files = baml_workspace::discover_baml_files(&canonical);
    assert!(
        !baml_files.is_empty(),
        "no .baml files discovered under {}",
        canonical.display()
    );
    for file_path in &baml_files {
        let content = fs::read_to_string(file_path)
            .unwrap_or_else(|_| panic!("failed to read {}", file_path.display()));
        db.add_or_update_file(file_path, &content);
    }

    // 2. Diagnostics gate — bail loudly on any compile error so a
    //    broken `.baml` source doesn't masquerade as a codegen bug.
    let project = db.get_project().expect("no project context");
    let source_files = db.get_source_files();
    let diagnostics = baml_project::collect_diagnostics(&db, project, &source_files);
    let errors: Vec<_> = diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    if !errors.is_empty() {
        let messages: Vec<String> = errors.iter().map(|d| format!("{d:?}")).collect();
        panic!("baml_src has compile errors:\n{}", messages.join("\n"));
    }

    // 3. Build the codegen `SymbolPool` and the inlined-files list.
    let pool = baml_project::build_symbol_pool(&db);
    let user_baml_files: Vec<UserBamlFile> = source_files
        .iter()
        .map(|sf| {
            let path = sf.path(&db);
            let rel = path.strip_prefix(&canonical).unwrap_or(&path).to_path_buf();
            (rel, sf.text(&db).to_string())
        })
        .collect();

    // 4. Codegen.
    let output = codegen_python::to_source_code(
        &pool,
        &user_baml_files,
        codegen_python::NamingConvention::PreserveCase,
    );
    for (path, content) in output {
        let file_path = baml_sdk_dir.join(&path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, &content).unwrap();
    }

    // 5. Symlink customizable/ files into generated/. On Windows
    //    `symlink_file` requires Developer Mode or admin, so fall back
    //    to a plain copy on failure — the cost is that local edits to
    //    `customizable/` won't be picked up without re-running
    //    build.rs (which `cargo:rerun-if-changed=` below already
    //    handles).
    let customizable_dir = manifest_dir.join("customizable");
    if customizable_dir.exists() {
        for entry in fs::read_dir(&customizable_dir).unwrap() {
            let entry = entry.unwrap();
            let src = entry.path();
            let file_name = entry.file_name();
            let dst = generated_dir.join(&file_name);

            if !src.is_file() {
                continue;
            }
            if dst.exists() || dst.symlink_metadata().is_ok() {
                let _ = fs::remove_file(&dst);
            }
            #[cfg(unix)]
            let symlink_result = std::os::unix::fs::symlink(&src, &dst);
            #[cfg(windows)]
            let symlink_result = std::os::windows::fs::symlink_file(&src, &dst);
            if symlink_result.is_err() {
                fs::copy(&src, &dst).unwrap_or_else(|e| {
                    panic!(
                        "Failed to symlink or copy {} from {}: {e}",
                        file_name.to_string_lossy(),
                        src.display()
                    )
                });
            }
        }
    }

    // 6. pyproject.toml. `uv sync` is invoked at test time by the
    //    `sdk_test_suite!` macro (see `run_test_cmd` below), which
    //    installs `baml_core` from the local source via
    //    `[tool.uv.sources]`. uv drives the maturin build-backend
    //    declared in `sdks/python/pyproject.toml`, so the PyO3
    //    extension is compiled into the project venv as part of the
    //    sync. `[tool.uv] package = false` keeps uv from trying to
    //    install this directory as a wheel; the empty `dev` group
    //    satisfies maturin's `uv pip install --group dev` step.
    let short_name = crate_name.strip_prefix("sdk_test_").unwrap_or(crate_name);
    let pyproject_name = format!("baml-test-{}", short_name.replace('_', "-"));
    // `pytest-asyncio` + `asyncio_mode = "auto"` are included
    // universally so `async def test_*` works without per-crate config.
    // Harmless for crates that have no async tests — the dep is a tiny
    // wheel and the mode flag is inert without async tests.
    let pyproject_toml = r#"[project]
name = "__PYPROJECT_NAME__"
version = "0.1.0"
requires-python = ">=3.10"
dependencies = [
    "baml_core",
    "pydantic>=2",
    "typing-extensions",
    "pytest>=7",
    "pytest-asyncio>=0.23",
    "ruff",
    "pyright>=1.1",
]

[dependency-groups]
dev = []

[tool.uv]
package = false

[tool.uv.sources]
baml_core = { path = "../../../../sdks/python", editable = true }

[tool.pytest.ini_options]
testpaths = ["."]
python_files = ["test_*.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
addopts = "-v"
asyncio_mode = "auto"

[tool.ruff]
line-length = 120
extend-exclude = ["*.pyi"]

[tool.ruff.lint]
ignore = ["F401", "F821", "E402"]
"#
    .replace("__PYPROJECT_NAME__", &pyproject_name);
    fs::write(generated_dir.join("pyproject.toml"), pyproject_toml).unwrap();

    // 7. rerun-if-changed for build.rs + every BAML and customizable
    //    file.
    println!("cargo:rerun-if-changed=build.rs");
    watch_dir(&baml_src);
    if customizable_dir.exists() {
        watch_dir(&customizable_dir);
    }
}

fn watch_dir(dir: &Path) {
    for path in walk_files(dir) {
        println!("cargo:rerun-if-changed={}", path.display());
    }
}

fn walk_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                out.push(path);
            } else if path.is_dir() {
                out.extend(walk_files(&path));
            }
        }
    }
    out
}

/// Test-side helper used by the [`sdk_test_suite!`] macro. Splits `cmd`
/// on whitespace and runs it as a single `Command::new(prog).args(rest)`
/// from inside `<CARGO_MANIFEST_DIR>/generated/`, panicking on non-zero
/// exit. Cargo sets `CARGO_MANIFEST_DIR` for the test binary at runtime,
/// so the helper resolves the right sdk-test crate without the macro
/// having to thread it through.
///
/// The uv cache is anchored at `<workspace>/target/uv-cache` so
/// rust-analyzer and `cargo test` share it; uv's file locks make the
/// concurrent `uv sync` calls (one per test) safe. If `uv` is managed
/// by mise but its shim is not on PATH, the helper falls back to
/// `mise which uv` before giving up.
pub fn run_test_cmd(cmd: &str) {
    let manifest = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set; run via `cargo test`"),
    );
    let dir = manifest.join("generated");
    assert!(
        dir.exists(),
        "generated/ not found at {} — did build.rs run?",
        dir.display()
    );

    // sdk-test crates always live at `<workspace>/sdk_tests/crates/<name>/`,
    // so the workspace root is the 3rd ancestor of the manifest dir.
    let workspace_root = manifest
        .ancestors()
        .nth(3)
        .expect("sdk-test crate not at <workspace>/sdk_tests/crates/<name>/");
    let uv_cache = workspace_root.join("target").join("uv-cache");

    // Naive whitespace split — quoted args would silently break apart.
    assert!(
        !cmd.contains('"') && !cmd.contains('\''),
        "run_test_cmd does not handle quoted args: `{cmd}`"
    );
    let mut words = cmd.split_whitespace();
    let prog = words.next().unwrap_or_else(|| panic!("empty command"));
    let args: Vec<&str> = words.collect();

    let output = run_test_process(prog, &args, &dir, &uv_cache)
        .unwrap_or_else(|e| panic!("failed to spawn `{cmd}`: {e}"));
    assert!(
        output.status.success(),
        "`{cmd}` failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn run_test_process(prog: &str, args: &[&str], dir: &Path, uv_cache: &Path) -> io::Result<Output> {
    let output = Command::new(prog)
        .args(args)
        .current_dir(dir)
        .env("UV_CACHE_DIR", uv_cache)
        .output();

    match output {
        Err(err) if err.kind() == ErrorKind::NotFound && prog == "uv" => {
            let uv = resolve_mise_uv()?;
            Command::new(uv)
                .args(args)
                .current_dir(dir)
                .env("UV_CACHE_DIR", uv_cache)
                .output()
        }
        other => other,
    }
}

fn resolve_mise_uv() -> io::Result<PathBuf> {
    let output = Command::new("mise").args(["which", "uv"]).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            format!(
                "`uv` is not on PATH and `mise which uv` failed:\n{}.",
                String::from_utf8_lossy(&output.stderr)
            ),
        ));
    }

    let path = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    if path.is_empty() {
        return Err(io::Error::new(
            ErrorKind::NotFound,
            "`uv` is not on PATH and `mise which uv` returned an empty path",
        ));
    }

    Ok(PathBuf::from(path))
}

/// Expands to four `#[test]` functions — `sync_only`, `ruff`,
/// `pyright`, `pytest`. The latter three each run `uv sync` then
/// `uv run <check>` inside the sdk-test crate's `generated/`.
/// `sync_only` runs `uv sync` on its own as a pre-warm hook so CI can
/// populate the shared editable-build and wheel caches serially
/// across all sdk_test crates (one process at a time) before fanning
/// the rest out in parallel — see `nextest run -E
/// 'test(=sync_only)' --test-threads=1` in the sdk-tests CI job. The
/// `uv sync` calls in `ruff`/`pyright`/`pytest` are kept so each test
/// remains runnable on its own (e.g. `cargo test -p sdk_test_X
/// ruff`), and become near-no-ops once the cache is warm.
///
/// Invoke from `tests/sdk_test.rs` as:
///
/// ```ignore
/// sdk_test_build::sdk_test_suite!();
/// ```
#[macro_export]
macro_rules! sdk_test_suite {
    () => {
        #[test]
        fn sync_only() {
            $crate::run_test_cmd("uv sync");
        }

        #[test]
        fn ruff() {
            $crate::run_test_cmd("uv sync");
            $crate::run_test_cmd("uv run ruff check --config pyproject.toml baml_sdk");
        }

        #[test]
        fn pyright() {
            $crate::run_test_cmd("uv sync");
            $crate::run_test_cmd("uv run pyright baml_sdk");
        }

        #[test]
        fn pytest() {
            $crate::run_test_cmd("uv sync");
            $crate::run_test_cmd("uv run pytest -v");
        }
    };
}
