using System.ComponentModel;
using System.Security.Cryptography;

using Baml.Runtime;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public static partial class BamlGeneratedContract
{
    public const int Version = 1;

    public static BamlGeneratedRegistryBuilder CreateRegistryBuilder(int requestedVersion)
    {
        RequireContractVersion(requestedVersion);
        return new BamlGeneratedRegistryBuilder(new RegistryOwner());
    }

    public static BamlGeneratedProgram RegisterProgram(
        int contractVersion,
        ReadOnlyMemory<byte> bytecode,
        string fingerprint,
        string generatedVersion,
        string requiredBridgeVersion,
        BamlGeneratedRegistry registry)
    {
        return RegisterProgram(
            contractVersion,
            bytecode,
            fingerprint,
            embeddedBamlToml: null,
            registry);
    }

    public static BamlGeneratedProgram RegisterProgram(
        int contractVersion,
        ReadOnlyMemory<byte> bytecode,
        string fingerprint,
        string? embeddedBamlToml,
        BamlGeneratedRegistry registry)
    {
        RequireContractVersion(contractVersion);

        ArgumentNullException.ThrowIfNull(registry);
        if (!IsLowercaseSha256(fingerprint))
        {
            throw new BamlProgramIntegrityException(
                "The generated BAML fingerprint must be a lowercase SHA-256 digest.");
        }

        string actual = Convert.ToHexString(SHA256.HashData(bytecode.Span)).ToLowerInvariant();
        if (!StringComparer.Ordinal.Equals(fingerprint, actual))
        {
            throw new BamlProgramIntegrityException(
                "Generated BAML bytecode does not match its fingerprint.");
        }

        ProgramNativeState nativeState =
            ProgramRegistrar.Register(bytecode.Span, fingerprint, embeddedBamlToml);
        var program = new BamlGeneratedProgram(registry, nativeState);
        registry.AttachProgram(program);
        return program;
    }

    private static void RequireContractVersion(int requestedVersion)
    {
        if (requestedVersion != Version)
        {
            throw new BamlVersionMismatchException(
                $"Generated-code contract {requestedVersion} is incompatible with runtime contract {Version}.");
        }
    }

    private static bool IsLowercaseSha256(string? value) =>
        value is { Length: 64 }
        && value.All(character =>
            char.IsAsciiDigit(character) || character is >= 'a' and <= 'f');
}
