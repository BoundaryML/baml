using System.Security.Cryptography;

namespace Baml;

public static class BamlBridge
{
    private const int MaxGeneratedBytecodeBytes = 8 * 1024 * 1024;
    private static readonly object ProgramLock = new();
    private static BamlProgram? _activeProgram;

    public static void FlushEvents()
    {
        Bridge.BridgePlatform.EnsureSupported();
        Bridge.NativeApi.FlushEvents();
    }

    public static BamlProgram RegisterProgram(
        ReadOnlySpan<byte> bytecode,
        ReadOnlySpan<byte> fingerprint,
        string generatedSdkVersion)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(generatedSdkVersion);
        if (!string.Equals(generatedSdkVersion, Bridge.BridgeVersion.Current, StringComparison.Ordinal))
        {
            throw new BamlSdkVersionMismatchException(
                generatedSdkVersion,
                Bridge.BridgeVersion.Current);
        }

        return RegisterProgram(bytecode, fingerprint);
    }

    [System.ComponentModel.EditorBrowsable(System.ComponentModel.EditorBrowsableState.Never)]
    public static BamlProgram RegisterEncodedProgram(
        IReadOnlyList<string> base64Segments,
        byte[] fingerprint,
        string generatedSdkVersion)
    {
        ArgumentNullException.ThrowIfNull(base64Segments);
        ArgumentNullException.ThrowIfNull(fingerprint);
        if (base64Segments.Count == 0)
        {
            throw new BamlBridgeException("The generated BAML bytecode carrier is missing.");
        }

        long encodedLength = 0;
        for (var index = 0; index < base64Segments.Count; index++)
        {
            var segment = base64Segments[index];
            if (string.IsNullOrEmpty(segment)
                || segment.Length % 4 != 0
                || (index < base64Segments.Count - 1 && segment.Contains('=')))
            {
                throw new BamlBridgeException("The generated BAML bytecode carrier is malformed.");
            }

            encodedLength += segment.Length;
            if (encodedLength > ((MaxGeneratedBytecodeBytes + 2L) / 3L) * 4L)
            {
                throw new BamlBridgeException(
                    $"The generated BAML bytecode carrier exceeds the {MaxGeneratedBytecodeBytes}-byte limit.");
            }
        }

        var last = base64Segments[^1];
        var padding = last.EndsWith("==", StringComparison.Ordinal)
            ? 2
            : last.EndsWith('=') ? 1 : 0;
        var decodedLength = checked((int)((encodedLength / 4L * 3L) - padding));
        if (decodedLength > MaxGeneratedBytecodeBytes)
        {
            throw new BamlBridgeException(
                $"The generated BAML bytecode carrier exceeds the {MaxGeneratedBytecodeBytes}-byte limit.");
        }
        if (decodedLength == 0)
        {
            throw new BamlBridgeException("The generated BAML bytecode carrier is empty.");
        }

        var bytecode = GC.AllocateUninitializedArray<byte>(decodedLength);
        var offset = 0;
        foreach (var segment in base64Segments)
        {
            if (!Convert.TryFromBase64Chars(segment, bytecode.AsSpan(offset), out var bytesWritten))
            {
                throw new BamlBridgeException("The generated BAML bytecode carrier is malformed.");
            }
            offset += bytesWritten;
        }
        if (offset != decodedLength)
        {
            throw new BamlBridgeException("The generated BAML bytecode carrier decoded to an invalid length.");
        }

        try
        {
            return RegisterProgram(bytecode, fingerprint, generatedSdkVersion);
        }
        catch (ArgumentException error) when (error.ParamName == "fingerprint")
        {
            throw new BamlBridgeException(
                "The generated BAML bytecode carrier does not match its fingerprint.",
                error);
        }
    }

    public static BamlProgram RegisterProgram(ReadOnlySpan<byte> bytecode, ReadOnlySpan<byte> fingerprint)
    {
        if (bytecode.IsEmpty)
        {
            throw new ArgumentException("BAML bytecode cannot be empty.", nameof(bytecode));
        }

        if (fingerprint.IsEmpty)
        {
            throw new ArgumentException("The BAML program fingerprint cannot be empty.", nameof(fingerprint));
        }

        var computedFingerprint = SHA256.HashData(bytecode);
        if (!CryptographicOperations.FixedTimeEquals(computedFingerprint, fingerprint))
        {
            throw new ArgumentException(
                "The BAML program fingerprint must be the SHA-256 digest of its bytecode.",
                nameof(fingerprint));
        }

        var fingerprintCopy = computedFingerprint;
        var fingerprintText = Convert.ToHexString(fingerprintCopy).ToLowerInvariant();

        lock (ProgramLock)
        {
            if (_activeProgram is not null)
            {
                if (CryptographicOperations.FixedTimeEquals(_activeProgram.Fingerprint, fingerprintCopy))
                {
                    return _activeProgram;
                }

                throw new BamlProgramConflictException(_activeProgram.FingerprintText, fingerprintText);
            }

            Bridge.BridgePlatform.EnsureSupported();
            Bridge.NativeApi.InitializeRuntime(bytecode);
            _activeProgram = new BamlProgram(fingerprintCopy, fingerprintText);
            return _activeProgram;
        }
    }
}
