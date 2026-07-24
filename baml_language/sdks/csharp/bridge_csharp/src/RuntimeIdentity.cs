using System.Reflection;

namespace Baml;

internal static class RuntimeIdentity
{
    internal static string PackageVersion { get; } =
        typeof(RuntimeIdentity).Assembly
            .GetCustomAttribute<AssemblyInformationalVersionAttribute>()
            ?.InformationalVersion
        ?? throw new InvalidOperationException(
            "Baml.Bridge is missing its informational package version.");

    internal static string RequiredBridgeVersion => PackageVersion;

    internal const int GeneratedContractVersion = 1;
}
