using System.Runtime.ExceptionServices;

using Baml.Cffi;

namespace Baml.Runtime;

internal static class ProgramRegistrar
{
    private static readonly Lock Sync = new();
    private static string? registeredFingerprint;
    private static ProgramNativeState? registeredState;
    private static ExceptionDispatchInfo? initializationFailure;

    internal static ProgramNativeState Register(
        ReadOnlySpan<byte> bytecode,
        string fingerprint,
        string? embeddedBamlToml)
    {
        lock (Sync)
        {
            if (registeredFingerprint is not null
                && !StringComparer.Ordinal.Equals(registeredFingerprint, fingerprint))
            {
                throw new BamlProgramConflictException(
                    "This process already registered a different generated BAML program. Restart the process to replace it.");
            }

            initializationFailure?.Throw();
            if (registeredState is not null)
            {
                return registeredState;
            }

            registeredFingerprint = fingerprint;
            try
            {
                NativeApi api = NativeApi.Instance;
                api.InitializeRuntime(bytecode, embeddedBamlToml);
                registeredState = new ProgramNativeState(api, fingerprint);
                return registeredState;
            }
            catch (Exception error)
            {
                initializationFailure = ExceptionDispatchInfo.Capture(error);
                throw;
            }
        }
    }
}

internal sealed class ProgramNativeState(NativeApi api, string fingerprint)
{
    internal NativeApi Api { get; } = api;

    internal string Fingerprint { get; } = fingerprint;
}
