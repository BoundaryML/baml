namespace Baml;

internal static class RuntimeIdentity
{
    internal const string RuntimeName = "baml-bridge";
    internal const string ToolchainVersion = "0.18.0";
    internal const string BridgeRuntimeVersion = "0.18.0";
    internal const string PackageVersion = BridgeRuntimeVersion;
    internal const string RequiredBridgeVersion = ToolchainVersion;
    internal const int GeneratedContractVersion = 1;
}

public static class BamlBridge
{
    public static string GetToolchainVersion() => RuntimeIdentity.ToolchainVersion;
    public static string GetBridgeRuntimeVersion() => RuntimeIdentity.BridgeRuntimeVersion;
}
