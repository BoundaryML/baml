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
# `maturin develop` against `sdks/python/.venv` rebuilds the shared
# extension module incrementally (~7s steady state). That venv is
# populated from `sdks/python/pyproject.toml`'s dev group so the maturin
# version constraint lives in TOML, not in an error-prone shell argument.
# The old `uv sync --reinstall-package baml_core` was strictly slower
# (~70s every run): uv's isolated build used an ephemeral interpreter
# whose path moved each run, busting pyo3's fingerprint and forcing a
# full bridge_python rebuild. The steps below are equivalent to setup.sh.

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

# EXPERIMENT (build-caching/04 approach B): build baml_core ONCE as a
# wheel, then install that prebuilt wheel into every fixture venv. This
# removes the redundant per-fixture editable build: plain `uv sync` used
# to build baml_core's editable wheel under build isolation with an
# *ephemeral* interpreter, busting pyo3's fingerprint and forcing a full
# bridge_python rebuild every run (~113s even with a warm target/) — and
# then `maturin develop` rebuilt it again. One `maturin build` (dev
# profile, reuses the warm cargo target/ ~20s) replaces both.

# 1. Build the shared baml_core wheel once (sdks/python/.venv supplies maturin).
$SdkPyVenv = Join-Path $SdkPy '.venv'
$MaturinExe = Join-Path $SdkPyVenv 'Scripts\maturin.exe'
$WheelDir = Join-Path $WorkspaceRoot 'target\wheels'
# Must exist before the first uv command: CI exports UV_FIND_LINKS=$WheelDir
# and uv errors on a missing find-links dir even when nothing needs it yet.
New-Item -ItemType Directory -Force -Path $WheelDir | Out-Null
Write-Host "==> uv sync (dev) in $SdkPy"
Push-Location $SdkPy
try {
    $env:UV_PROJECT_ENVIRONMENT = $SdkPyVenv
    & $UvBin sync --group dev --no-install-project
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}
Write-Host '==> maturin build (shared baml_core wheel)'
Push-Location $SdkPy
try {
    $env:VIRTUAL_ENV = $SdkPyVenv
    & $MaturinExe build --out $WheelDir
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# 2. Per fixture: install deps + the prebuilt baml_core wheel. No
#    per-fixture cdylib build. Strip the editable `[tool.uv.sources]
#    baml_core` block so baml_core resolves as a normal dependency from
#    UV_FIND_LINKS (set to $WheelDir; the local 0.1.3 wheel outranks any
#    public release). Plain `uv sync`/`uv run` then installs the prebuilt
#    wheel + dev tools without ever building baml_core.
if (-not $env:UV_FIND_LINKS) { $env:UV_FIND_LINKS = $WheelDir }
Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        # Delete the [tool.uv.sources] section (header → next blank line).
        $pp = Join-Path $generated 'pyproject.toml'
        $lines = Get-Content $pp
        $out = New-Object System.Collections.Generic.List[string]
        $skip = $false
        foreach ($line in $lines) {
            if ($line -match '^\[tool\.uv\.sources\]$') { $skip = $true; continue }
            if ($skip) { if ($line -match '^\s*$') { $skip = $false }; continue }
            $out.Add($line)
        }
        Set-Content -Path $pp -Value $out
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
