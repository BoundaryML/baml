$ErrorActionPreference = "Stop"

$CrateDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$WorkspaceRoot = (Resolve-Path (Join-Path $CrateDir "../../..")).Path

Push-Location $WorkspaceRoot
try {
    cargo build -p bridge_cffi
    if ($LASTEXITCODE -ne 0) {
        throw "cargo build -p bridge_cffi failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

$TargetDir = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $WorkspaceRoot "target" }
if (-not [System.IO.Path]::IsPathRooted($TargetDir)) {
    $TargetDir = Join-Path $WorkspaceRoot $TargetDir
}
$TargetDir = [System.IO.Path]::GetFullPath($TargetDir)
$NativeLibrary = Join-Path $TargetDir "debug/bridge_cffi.dll"

# Build every fixture consumer (and the union-generator tool) in ONE
# MSBuild invocation: the projects compile in parallel across cores and the
# shared Baml.Bridge project builds exactly once. The tests then run with
# --no-build, which is what makes it safe for nextest to run them
# concurrently — at test time no MSBuild processes exist to race on the
# bridge's shared obj/ (the historical reason this suite was serialized).
Push-Location $CrateDir
try {
    dotnet build Fixtures.slnx --configuration Release -m --nologo
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet build Fixtures.slnx failed with exit code $LASTEXITCODE"
    }

    # The documentation consumer swaps its package reference for a project
    # reference via these properties, so it cannot ride along in the solution
    # build; its test passes the same properties with --no-build.
    $DocConsumer = Join-Path $WorkspaceRoot "sdks/csharp/bridge_csharp/tests/Baml.Bridge.DocumentationConsumer/Baml.Bridge.DocumentationConsumer.csproj"
    $BridgeProject = Join-Path $WorkspaceRoot "sdks/csharp/bridge_csharp/src/Baml.Bridge.csproj"
    $GeneratedRoot = Join-Path $CrateDir "basic_calls/baml_sdk"
    dotnet build $DocConsumer --configuration Release --nologo `
        "-p:BamlBridgeProjectReference=$BridgeProject" `
        "-p:BamlGeneratedSourceRoot=$GeneratedRoot"
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet build Baml.Bridge.DocumentationConsumer failed with exit code $LASTEXITCODE"
    }
} finally {
    Pop-Location
}

if ($env:NEXTEST_ENV) {
    Add-Content -Path $env:NEXTEST_ENV -Value "SDK_TEST_CSHARP_SETUP=1"
    Add-Content -Path $env:NEXTEST_ENV -Value "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY=$NativeLibrary"
}
