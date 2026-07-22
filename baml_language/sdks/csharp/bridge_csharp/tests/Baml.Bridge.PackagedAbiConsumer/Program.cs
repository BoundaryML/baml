using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

internal static unsafe partial class Program
{
    private const string NativeLibraryName = "bridge_cffi";
    private const string OverrideVariable =
        "BAML_BRIDGE_CSHARP_NATIVE_LIBRARY";
    private const uint ExpectedAbiVersion = 1;

    private static readonly Lazy<string?> ConfiguredOverride = new(
        () => Environment.GetEnvironmentVariable(OverrideVariable),
        LazyThreadSafetyMode.ExecutionAndPublication);

    private static bool _usedOverride;

    [ModuleInitializer]
    internal static void RegisterResolver()
    {
        NativeLibrary.SetDllImportResolver(
            typeof(Program).Assembly,
            ResolveNativeLibrary);
    }

    public static int Main(string[] args)
    {
        if (args.Length != 2
            || (args[0] != "success"
                && args[0] != "failure"
                && args[0] != "version-mismatch"))
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.PackagedAbiConsumer <success|failure|version-mismatch> <expected-version-or-diagnostic-fragment>");
            return 2;
        }

        if (args[0] == "failure")
        {
            try
            {
                string unexpectedlyLoaded = GetNativeVersion();
                throw new InvalidOperationException(
                    $"expected native loading to fail with {args[1]}, but loaded {unexpectedlyLoaded}");
            }
            catch (NativeProbeLoadException error)
            {
                RecordExpectedFailure(error, args[1]);
                return 0;
            }
        }

        string version = GetNativeVersion();
        if (args[0] == "version-mismatch")
        {
            if (StringComparer.Ordinal.Equals(version, args[1]))
            {
                throw new InvalidOperationException(
                    $"expected a product-version mismatch, but loaded {version}");
            }

            RecordExpectedFailure(
                new NativeProbeLoadException(
                    $"native product version mismatch: expected {args[1]}, received {version}"),
                "native product version mismatch");
            return 0;
        }

        Require(
            StringComparer.Ordinal.Equals(version, args[1]),
            $"native product version mismatch: expected {args[1]}, received {version}");
        Console.WriteLine($"product_version={version}");
        Console.WriteLine(
            _usedOverride
                ? "resolution=absolute-override"
                : "resolution=package-default");
        Console.WriteLine("packaged_getter_table=ok");
        return 0;
    }

    private static void RecordExpectedFailure(
        Exception error,
        string expectedMarker)
    {
        string diagnostic =
            $"{error.GetType().Name}: {error.Message}";
        Require(
            diagnostic.Contains(
                expectedMarker,
                StringComparison.Ordinal),
            $"failure diagnostic did not contain the expected marker: {diagnostic}");
        Console.WriteLine($"failure={diagnostic}");
        Console.WriteLine("invalid_override=fail_closed");
    }

    private static nint ResolveNativeLibrary(
        string libraryName,
        System.Reflection.Assembly assembly,
        DllImportSearchPath? searchPath)
    {
        if (!StringComparer.Ordinal.Equals(
                libraryName,
                NativeLibraryName))
        {
            return 0;
        }

        string? configured = ConfiguredOverride.Value;
        if (string.IsNullOrEmpty(configured))
        {
            return 0;
        }

        if (!Path.IsPathFullyQualified(configured))
        {
            throw new NativeProbeLoadException(
                $"{OverrideVariable} must be an absolute native-library file path; received {configured}.");
        }

        if (!File.Exists(configured))
        {
            throw new NativeProbeLoadException(
                $"{OverrideVariable} file does not exist: {configured}.");
        }

        try
        {
            nint handle = NativeLibrary.Load(
                configured,
                assembly,
                searchPath);
            _usedOverride = true;
            return handle;
        }
        catch (Exception error)
            when (error is DllNotFoundException
                or BadImageFormatException
                or FileLoadException)
        {
            throw new NativeProbeLoadException(
                $"Failed to load the exact {OverrideVariable} file {configured}; packaged fallback is disabled.",
                error);
        }
    }

    private static string GetNativeVersion()
    {
        BamlApiV1* api;
        try
        {
            api = NativeMethods.GetApiV1();
        }
        catch (Exception error)
            when (error is DllNotFoundException
                or BadImageFormatException
                or EntryPointNotFoundException
                or NativeProbeLoadException)
        {
            throw new NativeProbeLoadException(
                $"Unable to resolve the canonical {NativeLibraryName}!baml_get_api_v1 entry point: {error.Message}",
                error);
        }

        Require(api is not null, "baml_get_api_v1 returned null");
        Require(
            api->AbiVersion == ExpectedAbiVersion,
            $"expected ABI {ExpectedAbiVersion}, received {api->AbiVersion}");
        Require(
            api->StructSize >= (nuint)sizeof(BamlApiV1),
            $"BamlApiV1 is truncated: {api->StructSize} < {sizeof(BamlApiV1)}");
        ValidateRequiredFunctions(api);

        BamlBuffer version = api->Version();
        try
        {
            Require(
                version.Length == 0 || version.Pointer is not null,
                "native version buffer has a null pointer with nonzero length");
            Require(
                version.Length <= int.MaxValue,
                $"native version is too large: {version.Length}");
            return version.Length == 0
                ? string.Empty
                : new UTF8Encoding(
                    encoderShouldEmitUTF8Identifier: false,
                    throwOnInvalidBytes: true).GetString(
                    new ReadOnlySpan<byte>(
                        version.Pointer,
                        checked((int)version.Length)));
        }
        finally
        {
            api->FreeBuffer(version);
        }
    }

    private static void ValidateRequiredFunctions(BamlApiV1* api)
    {
        Require(api->Version is not null, "BamlApiV1.version is null");
        Require(
            api->InitializeRuntimeFromBytecode is not null,
            "BamlApiV1.initialize_runtime_from_bytecode is null");
        Require(
            api->FreeBuffer is not null,
            "BamlApiV1.free_buffer is null");
        Require(
            api->RegisterCallback is not null,
            "BamlApiV1.register_callback is null");
        Require(
            api->CallFunction is not null,
            "BamlApiV1.call_function is null");
        Require(
            api->NewFunctionCall is not null,
            "BamlApiV1.new_function_call is null");
        Require(
            api->CancelFunctionCall is not null,
            "BamlApiV1.cancel_function_call is null");
        Require(
            api->RegisterHostDispatchCallback != 0,
            "BamlApiV1.register_host_dispatch_callback is null");
        Require(
            api->RegisterHostReleaseCallback != 0,
            "BamlApiV1.register_host_release_callback is null");
        Require(
            api->CompleteHostCall != 0,
            "BamlApiV1.complete_host_call is null");
        Require(
            api->HandleClone != 0,
            "BamlApiV1.handle_clone is null");
        Require(
            api->HandleRelease != 0,
            "BamlApiV1.handle_release is null");
        Require(
            api->MediaFromUrl != 0,
            "BamlApiV1.media_from_url is null");
        Require(
            api->MediaFromFile != 0,
            "BamlApiV1.media_from_file is null");
        Require(
            api->MediaFromBase64 != 0,
            "BamlApiV1.media_from_base64 is null");
        Require(api->MediaUrl != 0, "BamlApiV1.media_url is null");
        Require(api->MediaFile != 0, "BamlApiV1.media_file is null");
        Require(
            api->MediaBase64 != 0,
            "BamlApiV1.media_base64 is null");
        Require(
            api->MediaMimeType != 0,
            "BamlApiV1.media_mime_type is null");
        Require(
            api->RegisterBridge != 0,
            "BamlApiV1.register_bridge is null");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new NativeProbeLoadException(message);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlBuffer
    {
        public readonly byte* Pointer;
        public readonly nuint Length;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlApiV1
    {
        public readonly uint AbiVersion;
        public readonly nuint StructSize;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer> Version;
        public readonly delegate* unmanaged[Cdecl]<
            byte*,
            nuint,
            BamlBuffer> InitializeRuntimeFromBytecode;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer, void> FreeBuffer;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<uint, byte*, nuint, void>,
            void> RegisterCallback;
        public readonly delegate* unmanaged[Cdecl]<
            byte*,
            byte*,
            nuint,
            uint,
            void> CallFunction;
        public readonly delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
        public readonly nint RegisterHostDispatchCallback;
        public readonly nint RegisterHostReleaseCallback;
        public readonly nint CompleteHostCall;
        public readonly nint HandleClone;
        public readonly nint HandleRelease;
        public readonly nint MediaFromUrl;
        public readonly nint MediaFromFile;
        public readonly nint MediaFromBase64;
        public readonly nint MediaUrl;
        public readonly nint MediaFile;
        public readonly nint MediaBase64;
        public readonly nint MediaMimeType;
        public readonly nint RegisterBridge;
    }

    private static partial class NativeMethods
    {
        [LibraryImport(
            NativeLibraryName,
            EntryPoint = "baml_get_api_v1")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        internal static partial BamlApiV1* GetApiV1();
    }

    private sealed class NativeProbeLoadException : Exception
    {
        internal NativeProbeLoadException(string message)
            : base(message)
        {
        }

        internal NativeProbeLoadException(
            string message,
            Exception innerException)
            : base(message, innerException)
        {
        }
    }
}
