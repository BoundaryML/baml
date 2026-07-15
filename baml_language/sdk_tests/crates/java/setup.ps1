# Per-fixture Gradle setup for the sdk_test_java crate — Windows.
# Unix uses the parallel `setup.sh`; keep the two in sync (same steps,
# host shell of each platform).

$ErrorActionPreference = "Stop"

Set-Location $PSScriptRoot  # baml_language\sdk_tests\crates\java

$WorkspaceRoot = (Resolve-Path (Join-Path $PSScriptRoot "..\..\..")).Path

# Shared Gradle home under target/ — keep in sync with CACHE_SUBDIR /
# CACHE_ENV_VAR in harness_setup/src/java.rs.
$env:GRADLE_USER_HOME = Join-Path $WorkspaceRoot "target\gradle-home"
New-Item -ItemType Directory -Force -Path $env:GRADLE_USER_HOME | Out-Null

if (-not (Get-Command gradle -ErrorAction SilentlyContinue)) {
    Write-Warning "gradle not found on PATH; sdk_test_java tests will fail once un-ignored"
}

# Per-run breadcrumb for the `setup_guard::ran` test. Keep the var
# name in sync with SETUP_ENV_VAR in harness_setup/src/java.rs.
if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_JAVA_SETUP=1"
}
