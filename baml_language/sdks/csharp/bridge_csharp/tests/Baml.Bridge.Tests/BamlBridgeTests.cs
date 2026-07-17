using Baml;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Security.Cryptography;

namespace Baml.Bridge.Tests;

public sealed class BamlBridgeTests
{
    [Fact]
    public void AssemblyCarriesTheNativeHandshakeVersion()
    {
        var assembly = typeof(BamlBridge).Assembly;
        var sdkVersion = assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .Single(attribute => attribute.Key == "BamlSdkVersion")
            .Value;
        var informationalVersion = assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()!
            .InformationalVersion;

        Assert.False(string.IsNullOrWhiteSpace(sdkVersion));
        Assert.StartsWith(sdkVersion, informationalVersion, StringComparison.Ordinal);
    }

    [Fact]
    public void RegisterProgramRejectsMismatchedGeneratedSdkVersionBeforeNativeInitialization()
    {
        var error = Assert.Throws<BamlSdkVersionMismatchException>(
            () => BamlBridge.RegisterProgram([1], [2], "999.0.0"));
        var runtimeVersion = typeof(BamlBridge).Assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .Single(attribute => attribute.Key == "BamlSdkVersion")
            .Value;

        Assert.Equal("999.0.0", error.GeneratedVersion);
        Assert.Equal(runtimeVersion, error.RuntimeVersion);
        Assert.Contains("Regenerate", error.Message, StringComparison.Ordinal);
    }

    [Theory]
    [InlineData("linux-x64")]
    [InlineData("linux-arm64")]
    [InlineData("linux-musl-x64")]
    [InlineData("linux-musl-arm64")]
    [InlineData("osx-x64")]
    [InlineData("osx-arm64")]
    [InlineData("win-x64")]
    [InlineData("win-arm64")]
    public void PlatformContractAcceptsEveryPackagedRid(string runtimeIdentifier)
    {
        Assert.True(BridgePlatform.IsSupportedRuntimeIdentifier(runtimeIdentifier));
    }

    [Theory]
    [InlineData("freebsd-x64")]
    [InlineData("linux-riscv64")]
    [InlineData("linux-bionic-arm64")]
    public void PlatformContractRejectsUnpackagedRids(string runtimeIdentifier)
    {
        Assert.False(BridgePlatform.IsSupportedRuntimeIdentifier(runtimeIdentifier));
    }

    [Theory]
    [InlineData("ubuntu.26.04-x64", true, "linux-x64")]
    [InlineData("rhel.10-arm64", false, "linux-arm64")]
    [InlineData("linux-musl-x64", true, "linux-musl-x64")]
    public void PlatformContractNormalizesLinuxDistributionRids(
        string reportedRuntimeIdentifier,
        bool x64,
        string expected)
    {
        var actual = BridgePlatform.NormalizeRuntimeIdentifier(
            reportedRuntimeIdentifier,
            isWindows: false,
            isMacOS: false,
            isLinux: true,
            architecture: x64 ? Architecture.X64 : Architecture.Arm64);

        Assert.Equal(expected, actual);
    }

    [Theory]
    [InlineData("linux-bionic-arm64", false, false, true, Architecture.Arm64)]
    [InlineData("freebsd-x64", false, false, false, Architecture.X64)]
    [InlineData("linux-x86", false, false, true, Architecture.X86)]
    public void PlatformContractDoesNotNormalizeUnsupportedHosts(
        string reportedRuntimeIdentifier,
        bool isWindows,
        bool isMacOS,
        bool isLinux,
        Architecture architecture)
    {
        Assert.Null(BridgePlatform.NormalizeRuntimeIdentifier(
            reportedRuntimeIdentifier,
            isWindows,
            isMacOS,
            isLinux,
            architecture));
    }

    [Fact]
    public void RegisterProgramRejectsFingerprintThatDoesNotMatchBytecode()
    {
        var error = Assert.Throws<ArgumentException>(
            () => BamlBridge.RegisterProgram([1, 2, 3], [4, 5, 6]));

        Assert.Equal("fingerprint", error.ParamName);
    }

    [Fact]
    public void EncodedProgramRejectsMissingOrMalformedCarrier()
    {
        string[][] carriers =
        [
            [],
            [null!],
            [""],
            ["not base64"],
            ["YQ==", "YQ=="],
        ];

        foreach (var carrier in carriers)
        {
            var error = Assert.Throws<BamlBridgeException>(() => BamlBridge.RegisterEncodedProgram(
                carrier,
                SHA256.HashData("a"u8),
                BridgeVersion.Current));

            Assert.Contains("carrier", error.Message, StringComparison.OrdinalIgnoreCase);
        }
    }

    [Fact]
    public void EncodedProgramWrapsFingerprintCorruptionBeforeNativeInitialization()
    {
        var error = Assert.Throws<BamlBridgeException>(() => BamlBridge.RegisterEncodedProgram(
            ["YQ=="],
            SHA256.HashData("b"u8),
            BridgeVersion.Current));

        Assert.Contains("fingerprint", error.Message, StringComparison.OrdinalIgnoreCase);
        Assert.IsType<ArgumentException>(error.InnerException);
    }

    [Fact]
    public void EncodedProgramRejectsCarrierAboveTheGeneratorLimit()
    {
        const int encodedLimit = ((8 * 1024 * 1024 + 2) / 3) * 4;
        var segments = Enumerable.Repeat(new string('A', 12_000), encodedLimit / 12_000 + 1).ToArray();

        var error = Assert.Throws<BamlBridgeException>(() => BamlBridge.RegisterEncodedProgram(
            segments,
            SHA256.HashData("unused"u8),
            BridgeVersion.Current));

        Assert.Contains("limit", error.Message, StringComparison.OrdinalIgnoreCase);
    }
}
