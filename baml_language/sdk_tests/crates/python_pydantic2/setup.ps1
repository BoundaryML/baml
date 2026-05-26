# Per-fixture uv setup for the sdk_test_python_pydantic2 crate - Windows.
#
# Windows counterpart of setup.sh. Invoked automatically by
# `cargo nextest run` via the setup-script binding in
# `baml_language/.config/nextest.toml` (host = cfg(windows)).
# For plain `cargo test` (no nextest), run this manually after
# `cargo test --no-run` populates each fixture's `generated/pyproject.toml`.
#
# See setup.sh for the rationale on `uv sync --reinstall-package
# baml_core` (forcing the maturin rebuild of baml_core's extension
# module that a plain `uv sync` skips on incremental Rust edits); the
# steps are identical.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path

# Shared uv cache under target/, matching the UV_CACHE_DIR the emitted
# tests thread through (run_test_cmd / CACHE_ENV_VAR in harness_setup).
$env:UV_CACHE_DIR = Join-Path $WorkspaceRoot 'target\uv-cache'
New-Item -ItemType Directory -Force -Path $env:UV_CACHE_DIR | Out-Null

# `uv` may not be on PATH directly; fall back to `mise which uv`, the
# same fallback run_test_cmd uses for the test-time `uv run` calls.
$UvBin = 'uv'
if (-not (Get-Command uv -ErrorAction SilentlyContinue)) {
    $UvBin = (mise which uv).Trim()
}

# baml_core is editable-installed into every fixture's venv but resolves
# to the same shared extension module, so the first reinstall rebuilds
# it and the rest are quick no-ops. We loop all fixtures anyway to
# guarantee each venv exists and is synced.
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        Write-Host "==> uv sync --reinstall-package baml_core in $($_.Name)/generated"
        Push-Location $generated
        try {
            & $UvBin sync --reinstall-package baml_core
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

# Per-run breadcrumb for the in-test guard. nextest reads $NEXTEST_ENV
# after this script and injects these vars into the matched tests'
# processes - so `setup_guard::ran` (see harness_runner) can prove this
# script ran *this* run. Plain `cargo test` has no $NEXTEST_ENV, so the
# var stays unset and the guard fails with a helpful message. Keep the
# var name in sync with SETUP_ENV_VAR in
# harness_setup/src/python_pydantic2.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_PYTHON_PYDANTIC2_SETUP=1'
}
