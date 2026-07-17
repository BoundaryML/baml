using System.Reflection;

namespace Baml.Bridge;

internal static class BridgeVersion
{
    internal static string Current { get; } =
        typeof(BridgeVersion).Assembly
            .GetCustomAttributes<AssemblyMetadataAttribute>()
            .Single(attribute => attribute.Key == "BamlSdkVersion")
            .Value
        ?? throw new BamlBridgeException("The managed BAML bridge has no SDK version metadata.");
}
