using System.Security.Cryptography;

internal sealed class ProgramProbe
{
    internal ProgramProbe(string fingerprint)
    {
        Fingerprint = fingerprint;
    }

    internal string Fingerprint { get; }
}

internal sealed class ProgramProbeConflictException : Exception
{
    internal ProgramProbeConflictException(
        string expectedFingerprint,
        string receivedFingerprint)
        : base(
            $"A different BAML program is already registered: expected {expectedFingerprint}, received {receivedFingerprint}.")
    {
        ExpectedFingerprint = expectedFingerprint;
        ReceivedFingerprint = receivedFingerprint;
    }

    internal string ExpectedFingerprint { get; }

    internal string ReceivedFingerprint { get; }
}

internal sealed class ProgramProbeIntegrityException : Exception
{
    internal ProgramProbeIntegrityException(
        string expectedFingerprint,
        string actualFingerprint)
        : base(
            $"Generated BAML bytecode failed integrity validation: expected {expectedFingerprint}, received {actualFingerprint}.")
    {
        ExpectedFingerprint = expectedFingerprint;
        ActualFingerprint = actualFingerprint;
    }

    internal string ActualFingerprint { get; }

    internal string ExpectedFingerprint { get; }
}

internal sealed class ProgramProbeInitializationException : Exception
{
    internal ProgramProbeInitializationException(string diagnostic)
        : base($"Native BAML program initialization failed: {diagnostic}")
    {
        Diagnostic = diagnostic;
    }

    internal string Diagnostic { get; }
}

internal static class ProgramProbeRuntime
{
    internal const string ManagedVersion =
        global::Baml.Generated.BamlGeneratedProgram.RequiredBridgeVersion;

    private static readonly object Gate = new();
    private static Func<byte[], string>? initializer;
    private static ProgramProbe? program;
    private static string? registeredFingerprint;
    private static int initializationCount;

    internal static int InitializationCount =>
        Volatile.Read(ref initializationCount);

    internal static void ConfigureInitializer(
        Func<byte[], string> value)
    {
        ArgumentNullException.ThrowIfNull(value);
        lock (Gate)
        {
            if (initializer is not null
                || program is not null
                || registeredFingerprint is not null)
            {
                throw new InvalidOperationException(
                    "program runtime is already configured or initialized");
            }

            initializer = value;
        }
    }

    internal static ProgramProbe RegisterProgram(
        byte[] bytecode,
        string fingerprint,
        string generatedVersion)
    {
        ArgumentNullException.ThrowIfNull(bytecode);
        ArgumentException.ThrowIfNullOrWhiteSpace(fingerprint);
        ArgumentException.ThrowIfNullOrWhiteSpace(generatedVersion);
        if (!StringComparer.Ordinal.Equals(
                generatedVersion,
                ManagedVersion))
        {
            throw new InvalidOperationException(
                $"version mismatch: generated={generatedVersion}, managed={ManagedVersion}");
        }

        string actualFingerprint = Convert.ToHexString(
                SHA256.HashData(bytecode))
            .ToLowerInvariant();
        if (!StringComparer.Ordinal.Equals(
                actualFingerprint,
                fingerprint))
        {
            throw new ProgramProbeIntegrityException(
                fingerprint,
                actualFingerprint);
        }

        lock (Gate)
        {
            if (registeredFingerprint is not null
                && !StringComparer.Ordinal.Equals(
                    registeredFingerprint,
                    fingerprint))
            {
                throw new ProgramProbeConflictException(
                    registeredFingerprint,
                    fingerprint);
            }

            if (program is not null)
            {
                return program;
            }

            Func<byte[], string> initialize = initializer
                ?? throw new InvalidOperationException(
                    "native initializer was not configured");
            string diagnostic = initialize(bytecode);
            Interlocked.Increment(ref initializationCount);
            if (diagnostic.Length != 0)
            {
                throw new ProgramProbeInitializationException(
                    diagnostic);
            }

            registeredFingerprint = fingerprint;
            program = new ProgramProbe(fingerprint);
            return program;
        }
    }
}
