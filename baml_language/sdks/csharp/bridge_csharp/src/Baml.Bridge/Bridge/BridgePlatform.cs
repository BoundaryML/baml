using System.Runtime.InteropServices;

namespace Baml.Bridge;

internal static class BridgePlatform
{
    internal const string SupportedRuntimeIdentifiers =
        "linux-x64, linux-arm64, linux-musl-x64, linux-musl-arm64, "
        + "osx-x64, osx-arm64, win-x64, win-arm64";

    private static readonly HashSet<string> Supported = new(StringComparer.Ordinal)
    {
        "linux-x64",
        "linux-arm64",
        "linux-musl-x64",
        "linux-musl-arm64",
        "osx-x64",
        "osx-arm64",
        "win-x64",
        "win-arm64",
    };

    internal static bool IsSupportedRuntimeIdentifier(string runtimeIdentifier) =>
        Supported.Contains(runtimeIdentifier);

    internal static string? NormalizeRuntimeIdentifier(
        string reportedRuntimeIdentifier,
        bool isWindows,
        bool isMacOS,
        bool isLinux,
        Architecture architecture)
    {
        var architectureSuffix = architecture switch
        {
            Architecture.X64 => "x64",
            Architecture.Arm64 => "arm64",
            _ => null,
        };
        if (architectureSuffix is null)
        {
            return null;
        }

        if (isWindows)
        {
            return $"win-{architectureSuffix}";
        }

        if (isMacOS)
        {
            return $"osx-{architectureSuffix}";
        }

        if (!isLinux
            || reportedRuntimeIdentifier.Contains("bionic", StringComparison.OrdinalIgnoreCase))
        {
            return null;
        }

        var linux = reportedRuntimeIdentifier.Contains("musl", StringComparison.OrdinalIgnoreCase)
            ? "linux-musl"
            : "linux";
        return $"{linux}-{architectureSuffix}";
    }

    internal static void EnsureSupported()
    {
        var runtimeIdentifier = RuntimeInformation.RuntimeIdentifier;
        var portableRuntimeIdentifier = NormalizeRuntimeIdentifier(
            runtimeIdentifier,
            OperatingSystem.IsWindows(),
            OperatingSystem.IsMacOS(),
            OperatingSystem.IsLinux() && !OperatingSystem.IsAndroid(),
            RuntimeInformation.ProcessArchitecture);
        if (portableRuntimeIdentifier is null
            || !IsSupportedRuntimeIdentifier(portableRuntimeIdentifier))
        {
            throw new PlatformNotSupportedException(
                $"baml-bridge does not support runtime identifier '{runtimeIdentifier}' "
                + $"({RuntimeInformation.OSDescription}, {RuntimeInformation.ProcessArchitecture}). "
                + $"Supported RIDs: {SupportedRuntimeIdentifiers}.");
        }
    }
}
