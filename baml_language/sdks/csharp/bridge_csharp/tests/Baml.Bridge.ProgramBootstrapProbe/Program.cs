using System.Collections.Concurrent;
using System.Reflection;
using System.Security.Cryptography;
using Baml.Generated;

internal static class Program
{
    public static async Task<int> Main(string[] args)
    {
        if (args.Length == 0)
        {
            PrintUsage();
            return 2;
        }

        switch (args[0])
        {
            case "valid-native":
                Require(
                    args.Length == 4,
                    "valid-native requires bytecode, generated source, and native library");
                await VerifyValidNativeAsync(
                        Path.GetFullPath(args[1]),
                        Path.GetFullPath(args[2]),
                        Path.GetFullPath(args[3]))
                    .ConfigureAwait(false);
                break;
            case "valid-packaged":
                Require(args.Length == 1, "valid-packaged accepts no paths");
                await VerifyValidPackagedAsync().ConfigureAwait(false);
                break;
            case "integrity":
                Require(args.Length == 2, "integrity requires canonical bytecode");
                VerifyIntegrityFailure(Path.GetFullPath(args[1]));
                break;
            case "corrupt-native":
                Require(args.Length == 2, "corrupt-native requires native library");
                await VerifyCorruptNativeAsync(
                        Path.GetFullPath(args[1]))
                    .ConfigureAwait(false);
                break;
            case "boundary":
                Require(
                    args.Length == 3,
                    "boundary requires synthetic bytecode and generated source");
                VerifyBoundaryCarrier(
                    Path.GetFullPath(args[1]),
                    Path.GetFullPath(args[2]));
                break;
            default:
                throw new ArgumentException(
                    $"unknown mode {args[0]}",
                    nameof(args));
        }

        return 0;
    }

    private static async Task VerifyValidNativeAsync(
        string bytecodePath,
        string generatedSourcePath,
        string nativeLibraryPath)
    {
        byte[] canonical = File.ReadAllBytes(bytecodePath);
        VerifyGeneratedCarrier(canonical, generatedSourcePath);
        using NativeBytecodeInitializer native = new(nativeLibraryPath);
        Require(
            StringComparer.Ordinal.Equals(
                native.ProductVersion,
                ProgramProbeRuntime.ManagedVersion),
            $"native version mismatch: {native.ProductVersion}");
        ProgramProbeRuntime.ConfigureInitializer(native.Initialize);
        await VerifyConcurrentInitializationAndReuseAsync(
                native,
                canonical)
            .ConfigureAwait(false);
        Console.WriteLine("carrier_bytes=compiler_exact");
        Console.WriteLine("carrier_sha256=verified");
        Console.WriteLine("carrier_source=one_private_hex_array");
        Console.WriteLine("bootstrap=lazy_concurrent_singleton");
        Console.WriteLine("program_reuse=same_fingerprint");
        Console.WriteLine("program_conflict=before_native");
        Console.WriteLine("multi_file_surfaces=one_program");
        Console.WriteLine($"bytecode_bytes={canonical.Length}");
        Console.WriteLine(
            $"program_fingerprint={BamlGeneratedProgram.ProgramFingerprint}");
        Console.WriteLine($"native_initializations={native.Calls}");
    }

    private static async Task VerifyValidPackagedAsync()
    {
        using NativeBytecodeInitializer native = new(
            nativeLibraryPath: null);
        Require(
            StringComparer.Ordinal.Equals(
                native.ProductVersion,
                ProgramProbeRuntime.ManagedVersion),
            $"native version mismatch: {native.ProductVersion}");
        ProgramProbeRuntime.ConfigureInitializer(native.Initialize);
        await VerifyConcurrentInitializationAndReuseAsync(
                native,
                canonical: null)
            .ConfigureAwait(false);
        Console.WriteLine("packaged_carrier=embedded_managed_source");
        Console.WriteLine("packaged_native=default_resolution");
        Console.WriteLine("packaged_bootstrap=ok");
    }

    private static void VerifyGeneratedCarrier(
        byte[] canonical,
        string generatedSourcePath)
    {
        const string ArrayDeclaration =
            "private static readonly byte[] s_bytecode =";
        int arrayDeclarations = 0;
        int byteArrayDeclarations = 0;
        bool alternateCarrier = false;
        using (StreamReader source = File.OpenText(generatedSourcePath))
        {
            while (source.ReadLine() is { } line)
            {
                if (line.Contains(
                        ArrayDeclaration,
                        StringComparison.Ordinal))
                {
                    arrayDeclarations++;
                }

                if (line.Contains("byte[]", StringComparison.Ordinal))
                {
                    byteArrayDeclarations++;
                }

                alternateCarrier |= line.Contains(
                        "Convert.FromBase64String",
                        StringComparison.Ordinal)
                    || line.Contains(
                        "GetManifestResourceStream",
                        StringComparison.Ordinal);
            }
        }

        Require(
            arrayDeclarations == 1
            && byteArrayDeclarations == 1,
            "generated source did not contain exactly one private byte array");
        Require(
            !alternateCarrier,
            "generated source used a forbidden alternate carrier");
        FieldInfo field = typeof(BamlGeneratedProgram).GetField(
                "s_bytecode",
                BindingFlags.NonPublic | BindingFlags.Static)
            ?? throw new InvalidOperationException(
                "compiled generated carrier field is absent");
        byte[] generated = field.GetValue(obj: null) as byte[]
            ?? throw new InvalidOperationException(
                "compiled generated carrier is not a byte array");
        Require(
            generated.AsSpan().SequenceEqual(canonical),
            "compiled generated byte array differs from canonical bytecode");
        string actualFingerprint = Convert.ToHexString(
                SHA256.HashData(generated))
            .ToLowerInvariant();
        Require(
            StringComparer.Ordinal.Equals(
                actualFingerprint,
                BamlGeneratedProgram.ProgramFingerprint),
            "generated fingerprint differs from byte array SHA-256");
        Require(
            StringComparer.Ordinal.Equals(
                BamlGeneratedProgram.GeneratedWithVersion,
                ProgramProbeRuntime.ManagedVersion)
            && StringComparer.Ordinal.Equals(
                BamlGeneratedProgram.RequiredBridgeVersion,
                ProgramProbeRuntime.ManagedVersion),
            "generated version metadata differs from runtime");
    }

    private static void VerifyBoundaryCarrier(
        string bytecodePath,
        string generatedSourcePath)
    {
        byte[] canonical = File.ReadAllBytes(bytecodePath);
        VerifyGeneratedCarrier(canonical, generatedSourcePath);
        string fingerprint = Convert.ToHexString(
                SHA256.HashData(canonical))
            .ToLowerInvariant();
        Console.WriteLine($"boundary_bytes={canonical.Length}");
        Console.WriteLine($"boundary_sha256={fingerprint}");
        Console.WriteLine(
            $"boundary_source_bytes={new FileInfo(generatedSourcePath).Length}");
        Console.WriteLine("boundary_compiled_carrier=executed");
        Console.WriteLine("boundary_source=one_private_hex_array");
        Console.WriteLine("boundary_alternate_carriers=absent");
    }

    private static async Task VerifyConcurrentInitializationAndReuseAsync(
        NativeBytecodeInitializer native,
        byte[]? canonical)
    {
        Task<ProgramProbe>[] callers = Enumerable.Range(0, 128)
            .Select(
                index => Task.Run(
                    () => index % 2 == 0
                        ? Alpha.Functions.Touch()
                        : Beta.Nested.Functions.Touch()))
            .ToArray();
        ProgramProbe[] programs = await Task.WhenAll(callers)
            .ConfigureAwait(false);
        Require(
            programs.All(
                program => ReferenceEquals(
                    program,
                    programs[0])),
            "concurrent generated callers received different programs");
        Require(
            native.Calls == 1
            && ProgramProbeRuntime.InitializationCount == 1,
            "concurrent generated callers initialized more than once");

        if (canonical is null)
        {
            return;
        }

        ProgramProbe reused = ProgramProbeRuntime.RegisterProgram(
            canonical,
            BamlGeneratedProgram.ProgramFingerprint,
            BamlGeneratedProgram.GeneratedWithVersion);
        Require(
            ReferenceEquals(reused, programs[0])
            && native.Calls == 1,
            "same fingerprint did not reuse the initialized program");

        byte[] conflicting = canonical.ToArray();
        conflicting[0] ^= 0xff;
        string conflictingFingerprint = Convert.ToHexString(
                SHA256.HashData(conflicting))
            .ToLowerInvariant();
        Expect<ProgramProbeConflictException>(
            () => ProgramProbeRuntime.RegisterProgram(
                conflicting,
                conflictingFingerprint,
                BamlGeneratedProgram.GeneratedWithVersion));
        Require(
            native.Calls == 1,
            "conflicting fingerprint reached native initialization");
    }

    private static void VerifyIntegrityFailure(string bytecodePath)
    {
        byte[] edited = File.ReadAllBytes(bytecodePath);
        Require(edited.Length != 0, "canonical bytecode is empty");
        edited[^1] ^= 0xff;
        int initializerCalls = 0;
        ProgramProbeRuntime.ConfigureInitializer(
            _ =>
            {
                Interlocked.Increment(ref initializerCalls);
                return String.Empty;
            });
        ProgramProbeIntegrityException exception =
            Expect<ProgramProbeIntegrityException>(
                () => ProgramProbeRuntime.RegisterProgram(
                    edited,
                    BamlGeneratedProgram.ProgramFingerprint,
                    BamlGeneratedProgram.GeneratedWithVersion));
        Require(
            initializerCalls == 0
            && exception.ExpectedFingerprint
                == BamlGeneratedProgram.ProgramFingerprint
            && exception.ActualFingerprint
                != exception.ExpectedFingerprint,
            "edited bytes did not fail integrity before native initialization");
        Console.WriteLine("edited_byte_integrity=failed_before_native");
    }

    private static async Task VerifyCorruptNativeAsync(
        string nativeLibraryPath)
    {
        using NativeBytecodeInitializer native = new(nativeLibraryPath);
        byte[] corrupt = [0x00];
        string fingerprint = Convert.ToHexString(
                SHA256.HashData(corrupt))
            .ToLowerInvariant();
        ProgramProbeRuntime.ConfigureInitializer(native.Initialize);
        Lazy<ProgramProbe> lazy = new(
            () => ProgramProbeRuntime.RegisterProgram(
                corrupt,
                fingerprint,
                ProgramProbeRuntime.ManagedVersion),
            LazyThreadSafetyMode.ExecutionAndPublication);
        ConcurrentBag<Exception> failures = new();
        Task[] callers = Enumerable.Range(0, 32)
            .Select(
                index => Task.Run(
                    () =>
                    {
                        try
                        {
                            ProgramProbe value = lazy.Value;
                            GC.KeepAlive(value);
                        }
                        catch (Exception exception)
                        {
                            failures.Add(exception);
                        }
                    }))
            .ToArray();
        await Task.WhenAll(callers).ConfigureAwait(false);
        Exception[] observed = failures.ToArray();
        Require(
            observed.Length == 32
            && observed.All(
                exception => exception
                    is ProgramProbeInitializationException)
            && observed.All(
                exception => ReferenceEquals(
                    exception,
                    observed[0])),
            "lazy initialization did not cache one structured native failure");
        Require(
            native.Calls == 1
            && ProgramProbeRuntime.InitializationCount == 1,
            "corrupt bytecode reached native initialization more than once");
        Console.WriteLine("corrupt_matching_fingerprint=native_rejected");
        Console.WriteLine("initialization_failure=single_cached_instance");
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

    private static void PrintUsage() =>
        Console.Error.WriteLine(
            "usage: Baml.Bridge.ProgramBootstrapProbe <valid-native|valid-packaged|integrity|corrupt-native|boundary> [paths]");

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
