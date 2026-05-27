# Per-fixture uv setup for the sdk_test_python_pydantic2 crate - Windows.
#
# Windows counterpart of setup.sh. Invoked automatically by
# `cargo nextest run` via the setup-script binding in
# `baml_language/.config/nextest.toml` (host = cfg(windows)).
# For plain `cargo test` (no nextest), run this manually after
# `cargo test --no-run` populates each fixture's `generated/pyproject.toml`.
#
# See setup.sh for the full rationale. In short: `uv sync` per fixture
# creates each venv + editable-links baml_core, then a single
# `maturin develop` against a pinned build venv rebuilds the shared
# extension module incrementally (~7s steady state). The old
# `uv sync --reinstall-package baml_core` was strictly slower (~70s
# every run): uv's isolated build used an ephemeral interpreter whose
# path moved each run, busting pyo3's fingerprint and forcing a full
# bridge_python rebuild. The steps below are equivalent to setup.sh.

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path
$SdkPy = Join-Path $WorkspaceRoot 'sdks\python'

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

# 1. Ensure each fixture venv exists, deps are installed, and baml_core
#    is editable-linked. Plain `uv sync` (no --reinstall): on a fresh
#    checkout this builds the editable wheel once; afterward it's a
#    no-op. The shared extension it points at is (re)built by step 2.
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        Write-Host "==> uv sync in $($_.Name)/generated"
        Push-Location $generated
        try {
            & $UvBin sync
            if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
        } finally {
            Pop-Location
        }
    }
}

# 2. Rebuild the shared baml_core extension incrementally via maturin.
#    The build venv is pinned at a fixed path so its interpreter never
#    moves (see setup.sh for why that matters for the cargo cache).
#    abi3 wheels are interpreter-version-agnostic, so any >=3.10 works.
#
#    Windows deviation from setup.sh: install maturin via the ACTIVE-venv
#    mechanism ($env:VIRTUAL_ENV) instead of setup.sh's
#    `uv pip install --python <venv>/bin/python`. On the Windows runner
#    the `--python <path>` form fails with "The system cannot find the
#    file specified" even though `uv venv` reports success; the
#    active-venv form is how the per-fixture `uv sync` calls above
#    succeed on the same runner. setup.sh keeps the --python form (works
#    on Unix). The Test-Path line below is a diagnostic: if the venv
#    interpreter is missing, the failure is `uv venv`, not the install.
$BuildVenv = Join-Path $WorkspaceRoot 'target\maturin-build-venv'
$MaturinExe = Join-Path $BuildVenv 'Scripts\maturin.exe'
if (-not (Test-Path $MaturinExe)) {
    Write-Host "==> creating maturin build venv at $BuildVenv"
    & $UvBin venv $BuildVenv
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
    $VenvPy = Join-Path $BuildVenv 'Scripts\python.exe'
    Write-Host "==> venv interpreter $VenvPy exists=$(Test-Path -LiteralPath $VenvPy)"
    $env:VIRTUAL_ENV = $BuildVenv
    & $UvBin pip install 'maturin>=1.0,<2.0'
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
}
Write-Host '==> maturin develop (shared baml_core extension)'
Push-Location $SdkPy
try {
    $env:VIRTUAL_ENV = $BuildVenv
    & $MaturinExe develop
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
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
