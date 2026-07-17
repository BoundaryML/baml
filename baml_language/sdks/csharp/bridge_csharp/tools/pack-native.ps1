[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string] $NativeLibrary,

    [Parameter(Mandatory = $true)]
    [string] $Rid,

    [Parameter(Mandatory = $true)]
    [string] $OutputDirectory,

    [string] $PlatformContract
)

$ErrorActionPreference = 'Stop'

if ([string]::IsNullOrWhiteSpace($PlatformContract)) {
    $PlatformContract = Join-Path $PSScriptRoot '../../../../../release/platforms.json'
}

$contractPath = (Resolve-Path -LiteralPath $PlatformContract).Path
$contract = Get-Content -LiteralPath $contractPath -Raw | ConvertFrom-Json
$matchingTargets = @($contract.targets | Where-Object {
    $null -ne $_.artifacts.csharp -and $_.artifacts.csharp.rid -ceq $Rid
})
if ($matchingTargets.Count -ne 1) {
    throw "Expected exactly one C# platform-contract entry for RID '$Rid', found $($matchingTargets.Count)"
}

$expectedName = $matchingTargets[0].artifacts.csharp.native_asset
$nativePath = (Resolve-Path -LiteralPath $NativeLibrary).Path
if ([IO.Path]::GetFileName($nativePath) -cne $expectedName) {
    throw "Native library for $Rid must be named $expectedName"
}

$scriptRoot = $PSScriptRoot
$bridgeProject = Join-Path $scriptRoot '../src/Baml.Bridge/Baml.Bridge.csproj'
$normalizerProject = Join-Path $scriptRoot 'Baml.NuGet.Normalize/Baml.NuGet.Normalize.csproj'
$outputPath = [IO.Path]::GetFullPath($OutputDirectory)
$workPath = Join-Path ([IO.Path]::GetTempPath()) ("baml-native-pack.{0}" -f [Guid]::NewGuid())
$partialOutput = $null

try {
    [IO.Directory]::CreateDirectory($outputPath) | Out-Null
    [IO.Directory]::CreateDirectory($workPath) | Out-Null
    $rawPath = Join-Path $workPath 'raw'

    & dotnet pack $bridgeProject `
        --configuration Release `
        --output $rawPath `
        '-p:NuGetAudit=false' `
        "-p:BamlNativeLibrary=$nativePath" `
        "-p:BamlNativeRid=$Rid"
    if ($LASTEXITCODE -ne 0) {
        throw "dotnet pack failed with exit code $LASTEXITCODE"
    }

    $packages = @(Get-ChildItem -LiteralPath $rawPath -Filter '*.nupkg' -File)
    if ($packages.Count -ne 1) {
        throw "Expected one NuGet package, found $($packages.Count)"
    }

    $finalOutput = Join-Path $outputPath $packages[0].Name
    $partialOutput = Join-Path $outputPath (".{0}.tmp.{1}" -f $packages[0].Name, $PID)
    & dotnet run `
        --project $normalizerProject `
        --configuration Release `
        -- $packages[0].FullName $partialOutput
    if ($LASTEXITCODE -ne 0) {
        throw "NuGet normalization failed with exit code $LASTEXITCODE"
    }

    Move-Item -LiteralPath $partialOutput -Destination $finalOutput -Force
    $partialOutput = $null
    Write-Output $finalOutput
}
finally {
    if ($null -ne $partialOutput) {
        Remove-Item -LiteralPath $partialOutput -Force -ErrorAction SilentlyContinue
    }
    Remove-Item -LiteralPath $workPath -Recurse -Force -ErrorAction SilentlyContinue
}
