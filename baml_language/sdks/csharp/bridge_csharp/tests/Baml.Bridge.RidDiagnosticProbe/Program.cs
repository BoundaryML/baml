using System.Runtime.InteropServices;

internal static class Program
{
    private const string SupportedRids =
        "osx-arm64, osx-x64, linux-arm64, linux-musl-arm64, "
        + "linux-x64, linux-musl-x64, win-x64, win-arm64";

    public static int Main(string[] args)
    {
        if (args.Length > 1)
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.RidDiagnosticProbe [expected-rid]");
            return 2;
        }

        var supported = new Dictionary<PlatformFacts, string>
        {
            [new(HostOs.MacOs, Architecture.Arm64, IsMusl: false)] =
                "osx-arm64",
            [new(HostOs.MacOs, Architecture.X64, IsMusl: false)] =
                "osx-x64",
            [new(HostOs.Linux, Architecture.Arm64, IsMusl: false)] =
                "linux-arm64",
            [new(HostOs.Linux, Architecture.Arm64, IsMusl: true)] =
                "linux-musl-arm64",
            [new(HostOs.Linux, Architecture.X64, IsMusl: false)] =
                "linux-x64",
            [new(HostOs.Linux, Architecture.X64, IsMusl: true)] =
                "linux-musl-x64",
            [new(HostOs.Windows, Architecture.X64, IsMusl: false)] =
                "win-x64",
            [new(HostOs.Windows, Architecture.Arm64, IsMusl: false)] =
                "win-arm64",
        };
        foreach ((PlatformFacts facts, string rid) in supported)
        {
            Require(
                StringComparer.Ordinal.Equals(
                    RuntimeRidPolicy.Resolve(facts),
                    rid),
                $"supported platform resolved incorrectly: {facts}");
        }

        PlatformFacts[] unsupported =
        [
            new(HostOs.Other, Architecture.X64, IsMusl: false),
            new(HostOs.Windows, Architecture.X86, IsMusl: false),
            new(HostOs.MacOs, Architecture.Arm, IsMusl: false),
            new(HostOs.Linux, Architecture.S390x, IsMusl: false),
            new(HostOs.Linux, Architecture.RiscV64, IsMusl: true),
            new(HostOs.Windows, Architecture.X64, IsMusl: true),
        ];
        foreach (PlatformFacts facts in unsupported)
        {
            PlatformNotSupportedException error =
                Expect<PlatformNotSupportedException>(
                    () => _ = RuntimeRidPolicy.Resolve(facts));
            Require(
                error.Message.Contains(
                    SupportedRids,
                    StringComparison.Ordinal)
                && error.Message.Contains(
                    facts.OperatingSystem.ToString(),
                    StringComparison.Ordinal)
                && error.Message.Contains(
                    facts.Architecture.ToString(),
                    StringComparison.Ordinal),
                "unsupported-platform diagnostic omitted detected facts or supported RIDs");
        }

        string actual = RuntimeRidPolicy.Resolve(
            RuntimeRidPolicy.DetectCurrent());
        Require(
            supported.Values.Contains(actual, StringComparer.Ordinal),
            $"current host resolved to an unsupported RID: {actual}");
        if (args.Length == 1)
        {
            Require(
                StringComparer.Ordinal.Equals(actual, args[0]),
                $"current host RID mismatch: detected={actual}, expected={args[0]}");
        }

        Console.WriteLine("rid_policy=8_exact_no_substitution");
        Console.WriteLine("unsupported_runtime=PlatformNotSupportedException");
        Console.WriteLine($"detected_rid={actual}");
        return 0;
    }

    private static TException Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException exception)
        {
            return exception;
        }

        throw new InvalidOperationException(
            $"expected {typeof(TException).Name}");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private enum HostOs
    {
        Windows,
        MacOs,
        Linux,
        Other,
    }

    private readonly record struct PlatformFacts(
        HostOs OperatingSystem,
        Architecture Architecture,
        bool IsMusl);

    private static class RuntimeRidPolicy
    {
        internal static PlatformFacts DetectCurrent()
        {
            HostOs operatingSystem =
                RuntimeInformation.IsOSPlatform(OSPlatform.Windows)
                    ? HostOs.Windows
                    : RuntimeInformation.IsOSPlatform(OSPlatform.OSX)
                        ? HostOs.MacOs
                        : RuntimeInformation.IsOSPlatform(OSPlatform.Linux)
                            ? HostOs.Linux
                            : HostOs.Other;
            bool isMusl =
                operatingSystem == HostOs.Linux
                && RuntimeInformation.RuntimeIdentifier.Contains(
                    "musl",
                    StringComparison.Ordinal);
            return new PlatformFacts(
                operatingSystem,
                RuntimeInformation.ProcessArchitecture,
                isMusl);
        }

        internal static string Resolve(PlatformFacts facts) =>
            facts switch
            {
                (HostOs.MacOs, Architecture.Arm64, false) => "osx-arm64",
                (HostOs.MacOs, Architecture.X64, false) => "osx-x64",
                (HostOs.Linux, Architecture.Arm64, false) => "linux-arm64",
                (HostOs.Linux, Architecture.Arm64, true) =>
                    "linux-musl-arm64",
                (HostOs.Linux, Architecture.X64, false) => "linux-x64",
                (HostOs.Linux, Architecture.X64, true) => "linux-musl-x64",
                (HostOs.Windows, Architecture.X64, false) => "win-x64",
                (HostOs.Windows, Architecture.Arm64, false) => "win-arm64",
                _ => throw new PlatformNotSupportedException(
                    "baml-bridge does not support the detected platform "
                    + $"os={facts.OperatingSystem}, "
                    + $"architecture={facts.Architecture}, "
                    + $"libc={(facts.IsMusl ? "musl" : "default")}. "
                    + $"Supported RIDs: {SupportedRids}."),
            };
    }
}
