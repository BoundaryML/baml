# Per-fixture cargo pre-warm for the sdk_test_rust crate - Windows.
#
# Windows counterpart of setup.sh — see that file for the rationale.
# Invoked automatically by `cargo nextest run` via the setup-script
# binding in `baml_language/.config/nextest.toml` (host = cfg(windows)).

$ErrorActionPreference = 'Stop'
Set-Location $PSScriptRoot

$WorkspaceRoot = (Resolve-Path '..\..\..').Path

# The generated SDKs load the engine as a shared library at run time
# (baml_bridge is dylib-only). Build the cdylib into the MAIN workspace
# target dir - before the fixture CARGO_TARGET_DIR assignment below -
# which is where the emitted tests look for it (next to their own
# binary, so ambient CARGO_TARGET_DIR/profile agree by construction).
Write-Host "==> cargo build -p bridge_cffi (engine cdylib)"
Push-Location $WorkspaceRoot
try {
    cargo build -p bridge_cffi
    if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }
} finally {
    Pop-Location
}

# Shared cargo build dir under target/, matching the CARGO_TARGET_DIR
# the emitted tests thread through (run_test_cmd / CACHE_SUBDIR in
# harness_setup/src/rust.rs).
$env:CARGO_TARGET_DIR = Join-Path $WorkspaceRoot 'target\sdk-rust-target'
New-Item -ItemType Directory -Force -Path $env:CARGO_TARGET_DIR | Out-Null

Get-ChildItem -Directory | ForEach-Object {
    $generated = Join-Path $_.FullName 'generated'
    if (Test-Path $generated) {
        # Never run cargo without the generated manifest: cargo discovers
        # manifests upward, so a missing Cargo.toml (codegen failure) would
        # silently turn this into a workspace-wide build. The failure itself
        # surfaces via the build_diagnostics test.
        $manifest = Join-Path $generated 'Cargo.toml'
        if (-not (Test-Path $manifest)) {
            Write-Host "==> skipping $($_.Name)/generated (no Cargo.toml - codegen failed?)"
            return
        }
        Write-Host "==> cargo test --no-run in $($_.Name)/generated"
        Push-Location $generated
        try {
            cargo test --no-run --manifest-path Cargo.toml
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
# var name in sync with SETUP_ENV_VAR in harness_setup/src/rust.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value 'SDK_TEST_RUST_SETUP=1'
}
