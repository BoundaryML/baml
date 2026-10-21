using System.Diagnostics.CodeAnalysis;

namespace Baml.Runtime;

internal static class BamlProcessExit
{
    [DoesNotReturn]
    internal static void Exit(long exitCode)
    {
        int processExitCode = exitCode switch
        {
            > int.MaxValue => int.MaxValue,
            < int.MinValue => int.MinValue,
            _ => (int)exitCode,
        };
        Environment.Exit(processExitCode);
    }
}

internal static class BamlCancellationTokens
{
    internal static CancellationToken CreateEngineToken() =>
        new(canceled: true);
}
