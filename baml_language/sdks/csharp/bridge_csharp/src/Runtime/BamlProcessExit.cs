using System.Diagnostics.CodeAnalysis;

namespace Baml.Runtime;

internal static class BamlProcessExit
{
    [DoesNotReturn]
    internal static void Exit(long exitCode)
    {
        FlushEventsBestEffort();
        int processExitCode = exitCode switch
        {
            > int.MaxValue => int.MaxValue,
            < int.MinValue => int.MinValue,
            _ => (int)exitCode,
        };
        Environment.Exit(processExitCode);
    }

    private static void FlushEventsBestEffort()
    {
        // The canonical v1 function table currently exposes no telemetry flush
        // operation. Keep this hook strictly bounded until one is appended to the
        // native ABI; hard exit must never wait indefinitely for telemetry.
    }
}

internal static class BamlCancellationTokens
{
    internal static CancellationToken CreateEngineToken() =>
        new(canceled: true);
}
