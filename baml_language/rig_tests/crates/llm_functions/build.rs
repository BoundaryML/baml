// Hand-written; drives codegen from real `.baml` source rather than an
// in-memory `SymbolPool`. Mirrors the pipeline `baml-cli generate`
// uses: discover files → ProjectDatabase → diagnostics gate →
// `build_symbol_pool` → `to_source_code`. Reference template for the
// other rig crates' build.rs / conftest.py / pyproject.toml shape.
use std::{
    env, fs,
    path::{Path, PathBuf},
};

use baml_codegen_python::UserBamlFile;
use baml_db::baml_compiler_diagnostics::Severity;
use baml_project::ProjectDatabase;

fn main() {
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
    let output = baml_codegen_python::to_source_code(&pool, &user_baml_files);
    for (path, content) in output {
        let file_path = baml_sdk_dir.join(&path);
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&file_path, &content).unwrap();
    }

    // 5. Symlink customizable/ files into generated/, identical to the
    //    template-based crates.
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
            std::os::unix::fs::symlink(&src, &dst).unwrap_or_else(|_| {
                panic!(
                    "Failed to symlink {} from {}",
                    file_name.to_string_lossy(),
                    src.display()
                )
            });
            #[cfg(windows)]
            std::os::windows::fs::symlink_file(&src, &dst).unwrap_or_else(|_| {
                panic!(
                    "Failed to symlink {} from {}",
                    file_name.to_string_lossy(),
                    src.display()
                )
            });
        }
    }

    // 6. conftest.py — stubs `baml.baml_core` so `baml_sdk/__init__.py`
    //    imports succeed without the runtime package installed.
    let conftest_py = r#"# Hand-written; stubs baml.baml_core for import-only tests.
"""Stub `baml.baml_core` so baml_sdk imports work without the runtime."""
import sys
import types


def _install_baml_stub() -> None:
    if "baml.baml_core" in sys.modules:
        return

    baml = types.ModuleType("baml")
    baml_core = types.ModuleType("baml.baml_core")

    class BamlRuntime:  # noqa: D401
        @staticmethod
        def initialize_runtime(*_args, **_kwargs):
            return None

    def define_function(fqn, _kind, param_names):
        def factory(*_args, **_kwargs):
            raise RuntimeError(
                f"baml runtime not installed (called {fqn})"
            )
        factory.__name__ = fqn.rsplit(".", 1)[-1]
        factory.param_names = list(param_names)
        return factory

    baml_core.BamlRuntime = BamlRuntime
    baml_core.define_function = define_function
    baml.baml_core = baml_core

    sys.modules["baml"] = baml
    sys.modules["baml.baml_core"] = baml_core


_install_baml_stub()
"#;
    fs::write(generated_dir.join("conftest.py"), conftest_py).unwrap();

    // 7. pyproject.toml + test.sh + test.ps1 — same shapes as the
    //    template-based crates.
    let pyproject_toml = r#"[project]
name = "baml-test-llm-functions"
version = "0.1.0"
requires-python = ">=3.8"
dependencies = []

[tool.pytest.ini_options]
testpaths = ["."]
python_files = ["test_*.py"]
python_classes = ["Test*"]
python_functions = ["test_*"]
addopts = "-v"

[tool.ruff]
line-length = 120
extend-exclude = ["*.pyi"]

[tool.ruff.lint]
ignore = ["F401", "F821", "E402"]
"#;
    fs::write(generated_dir.join("pyproject.toml"), pyproject_toml).unwrap();

    let test_sh = r#"#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"
export UV_CACHE_DIR="$(pwd)/.uv-cache"
if ! command -v uv &> /dev/null; then
    echo "Error: uv is not installed"
    exit 1
fi
echo "==> Running Python syntax check..."
python_files=$(find . -name "*.py" -o -name "*.pyi")
if [ -n "$python_files" ]; then
    echo "$python_files" | xargs uv run python -m py_compile
fi
echo "==> Running ruff lint..."
uv run --with ruff ruff check --config pyproject.toml baml_sdk
echo "==> Running pytest..."
uv run --with pytest --with pydantic --with typing-extensions pytest -v
echo "==> All checks passed!"
"#;
    fs::write(generated_dir.join("test.sh"), test_sh).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(generated_dir.join("test.sh"))
            .unwrap()
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(generated_dir.join("test.sh"), perms).unwrap();
    }

    let test_ps1 = r#"$ErrorActionPreference = "Stop"
Set-Location $PSScriptRoot
$env:UV_CACHE_DIR = Join-Path $PSScriptRoot ".uv-cache"
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    Write-Error "Error: uv is not installed"
    exit 1
}
Write-Host "==> Running Python syntax check..."
$pythonFiles = Get-ChildItem -Recurse -Include *.py,*.pyi | ForEach-Object { $_.FullName }
if ($pythonFiles) {
    foreach ($file in $pythonFiles) {
        uv run python -m py_compile $file
    }
}
Write-Host "==> Running ruff lint..."
uv run --with ruff ruff check --config pyproject.toml baml_sdk
Write-Host "==> Running pytest..."
uv run --with pytest --with pydantic --with typing-extensions pytest -v
Write-Host "==> All checks passed!"
"#;
    fs::write(generated_dir.join("test.ps1"), test_ps1).unwrap();

    // 8. rerun-if-changed for build.rs + every BAML and customizable
    //    file. baml_src/ rebuilds when contents change.
    println!("cargo:rerun-if-changed=build.rs");
    watch_dir(&baml_src);
    if customizable_dir.exists() {
        watch_dir(&customizable_dir);
    }
}

fn watch_dir(dir: &Path) {
    let walker = walk_files(dir);
    for path in walker {
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
