$ErrorActionPreference = "Stop"

$testRoot = Split-Path -Parent $MyInvocation.MyCommand.Path
$workspaceRoot = (Resolve-Path (Join-Path $testRoot "../../..")).Path
$targetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $workspaceRoot "target" }
if (-not [IO.Path]::IsPathRooted($targetDir)) {
    $targetDir = Join-Path $workspaceRoot $targetDir
}
$targetDir = [IO.Path]::GetFullPath($targetDir)
$fixtureDir = Join-Path $targetDir "ruby-bridge-fixtures"
$includeDir = Join-Path $workspaceRoot "crates/bridge_cffi/include"
New-Item -ItemType Directory -Force -Path $fixtureDir | Out-Null

Push-Location $workspaceRoot
try {
    cargo build -p bridge_cffi
    if ($LASTEXITCODE -ne 0) { throw "cargo build -p bridge_cffi failed" }

    $env:BAML_ABI_PROBE_BYTECODE = Join-Path $fixtureDir "function-calls.bytecode"
    cargo test -p sdk_test_harness_setup csharp_abi_probe_tests::emit_bridge_probe_function_calls_bytecode -- --ignored --exact
    if ($LASTEXITCODE -ne 0) { throw "function_calls bytecode generation failed" }
} finally {
    Remove-Item Env:BAML_ABI_PROBE_BYTECODE -ErrorAction SilentlyContinue
    Pop-Location
}

$bridgeFixture = Join-Path $fixtureDir "baml_ruby_test_fixture.dll"
$missingGetter = Join-Path $fixtureDir "baml_ruby_missing_getter.dll"
$threadFixture = Join-Path $fixtureDir "baml_ruby_thread_callback.dll"

& cl.exe /nologo /std:c11 /W4 /WX /LD "/I$includeDir" `
    (Join-Path $testRoot "test/native/bridge_fixture.c") `
    "/Fe:$bridgeFixture" "/Fo:$(Join-Path $fixtureDir 'bridge_fixture.obj')"
if ($LASTEXITCODE -ne 0) { throw "bridge fixture compilation failed" }

& cl.exe /nologo /std:c11 /W4 /WX /LD `
    (Join-Path $testRoot "test/native/missing_getter.c") `
    "/Fe:$missingGetter" "/Fo:$(Join-Path $fixtureDir 'missing_getter.obj')"
if ($LASTEXITCODE -ne 0) { throw "missing-getter fixture compilation failed" }

& cl.exe /nologo /std:c++17 /EHsc /W4 /WX /LD `
    (Join-Path $testRoot "test/native/thread_callback.cpp") `
    "/Fe:$threadFixture" "/Fo:$(Join-Path $fixtureDir 'thread_callback.obj')"
if ($LASTEXITCODE -ne 0) { throw "thread-callback fixture compilation failed" }

$invalidLibrary = Join-Path $fixtureDir "not-a-library"
[IO.File]::WriteAllText($invalidLibrary, "not a dynamic library`n")

$env:BUNDLE_GEMFILE = Join-Path $testRoot "Gemfile"
$env:BUNDLE_PATH = Join-Path $targetDir "ruby-bundle"
& ruby -S bundle install --jobs 4 --retry 3
if ($LASTEXITCODE -ne 0) { throw "bundle install failed" }

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_RUBY_SORBET_SETUP=1"
    Add-Content -Path $env:NEXTEST_ENV -Value "BUNDLE_GEMFILE=$env:BUNDLE_GEMFILE"
    Add-Content -Path $env:NEXTEST_ENV -Value "BUNDLE_PATH=$env:BUNDLE_PATH"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_FIXTURE=$bridgeFixture"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_MISSING_GETTER=$missingGetter"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_INVALID_LIBRARY=$invalidLibrary"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_THREAD_FIXTURE=$threadFixture"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_REAL_RUNTIME=$(Join-Path $targetDir 'debug/bridge_cffi.dll')"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_RUBY_TEST_REAL_BYTECODE=$(Join-Path $fixtureDir 'function-calls.bytecode')"
}
