using System.Runtime.ExceptionServices;

using Baml.Cffi;

namespace Baml.Runtime;

internal static class ProgramRegistrar
{
    private static readonly Lock Sync = new();
    internal static ProgramNativeState Register(ReadOnlySpan<byte> bytecode, string fingerprint, string? embeddedBamlToml, ulong? requestedKey)
    {
        lock (Sync)
        {
            NativeApi api = NativeApi.Instance;
            ulong key = api.RegisterProgram(bytecode, embeddedBamlToml, requestedKey);
            return new ProgramNativeState(api.ForRuntime(key), fingerprint);
        }
    }
}

internal sealed class ProgramNativeState(NativeApi api, string fingerprint)
{
    internal NativeApi Api { get; } = api;

    internal string Fingerprint { get; } = fingerprint;
}
