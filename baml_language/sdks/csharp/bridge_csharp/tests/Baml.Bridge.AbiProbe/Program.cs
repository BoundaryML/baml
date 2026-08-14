using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

internal static unsafe partial class Program
{
    private const string NativeLibraryName = "bridge_cffi";
    private const uint BamlApiV1AbiVersion = 2;
    private const uint BamlBridgeLanguageCSharp = 5;

    public static int Main(string[] args)
    {
        if (args.Length != 2)
        {
            Console.Error.WriteLine(
                "usage: Baml.Bridge.AbiProbe <absolute-native-library-path> <expected-product-version>");
            return 2;
        }

        string nativeLibraryPath = Path.GetFullPath(args[0]);
        if (!Path.IsPathFullyQualified(nativeLibraryPath) || !File.Exists(nativeLibraryPath))
        {
            Console.Error.WriteLine($"native library does not exist: {nativeLibraryPath}");
            return 2;
        }

        NativeLibrary.SetDllImportResolver(
            typeof(Program).Assembly,
            (libraryName, assembly, searchPath) =>
            {
                if (!StringComparer.Ordinal.Equals(libraryName, NativeLibraryName))
                {
                    return IntPtr.Zero;
                }

                return NativeLibrary.Load(nativeLibraryPath, assembly, searchPath);
            });

        BamlApiV1* api = NativeMethods.GetApiV1();
        Require(api is not null, "baml_get_api_v1 returned null");
        Require(api->AbiVersion == BamlApiV1AbiVersion, $"unexpected ABI version {api->AbiVersion}");
        Require(
            api->StructSize >= (nuint)sizeof(BamlApiV1),
            $"truncated BamlApiV1: {api->StructSize} < {sizeof(BamlApiV1)}");
        ValidateRequiredFunctions(api);

        string nativeVersion = ConsumeBuffer(api, api->Version());
        Require(
            StringComparer.Ordinal.Equals(nativeVersion, args[1]),
            $"product version mismatch: native={nativeVersion}, expected={args[1]}");

        byte[] encodedVersion = Encoding.UTF8.GetBytes(args[1]);
        fixed (byte* version = encodedVersion)
        {
            BamlBridgeInfoV1 info = new()
            {
                StructSize = (nuint)sizeof(BamlBridgeInfoV1),
                Language = BamlBridgeLanguageCSharp,
                SdkVersion = version,
                SdkVersionLength = (nuint)encodedVersion.Length,
            };
            string diagnostic = ConsumeBuffer(api, api->RegisterBridge(&info));
            Require(diagnostic.Length == 0, $"bridge registration failed: {diagnostic}");
        }

        Console.WriteLine($"api_v1_size={api->StructSize}");
        Console.WriteLine($"product_version={nativeVersion}");
        Console.WriteLine("csharp_registration=ok");
        return 0;
    }

    private static string ConsumeBuffer(BamlApiV1* api, BamlBuffer buffer)
    {
        try
        {
            if (buffer.Length == 0)
            {
                return string.Empty;
            }

            Require(buffer.Pointer is not null, "non-empty BamlBuffer has a null pointer");
            Require(buffer.Length <= int.MaxValue, $"BamlBuffer is too large: {buffer.Length}");
            return Encoding.UTF8.GetString(
                new ReadOnlySpan<byte>(buffer.Pointer, checked((int)buffer.Length)));
        }
        finally
        {
            api->FreeBuffer(buffer);
        }
    }

    private static void ValidateRequiredFunctions(BamlApiV1* api)
    {
        Require(api->Version is not null, "version is null");
        Require(api->InitializeRuntimeFromBytecode is not null, "initialize_runtime_from_bytecode is null");
        Require(api->FreeBuffer is not null, "free_buffer is null");
        Require(api->RegisterCallback is not null, "register_callback is null");
        Require(api->CallFunction is not null, "call_function is null");
        Require(api->NewFunctionCall is not null, "new_function_call is null");
        Require(api->CancelFunctionCall is not null, "cancel_function_call is null");
        Require(api->RegisterHostDispatchCallback is not null, "register_host_dispatch_callback is null");
        Require(api->RegisterHostReleaseCallback is not null, "register_host_release_callback is null");
        Require(api->CompleteHostCall is not null, "complete_host_call is null");
        Require(api->HandleClone is not null, "handle_clone is null");
        Require(api->HandleRelease is not null, "handle_release is null");
        Require(api->MediaFromUrl is not null, "media_from_url is null");
        Require(api->MediaFromFile is not null, "media_from_file is null");
        Require(api->MediaFromBase64 is not null, "media_from_base64 is null");
        Require(api->MediaUrl is not null, "media_url is null");
        Require(api->MediaFile is not null, "media_file is null");
        Require(api->MediaBase64 is not null, "media_base64 is null");
        Require(api->MediaMimeType is not null, "media_mime_type is null");
        Require(api->RegisterBridge is not null, "register_bridge is null");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlBuffer
    {
        public readonly byte* Pointer;
        public readonly nuint Length;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BamlBridgeInfoV1
    {
        public nuint StructSize;
        public uint Language;
        public byte* SdkVersion;
        public nuint SdkVersionLength;
    }

    [StructLayout(LayoutKind.Sequential)]
    private readonly struct BamlApiV1
    {
        public readonly uint AbiVersion;
        public readonly nuint StructSize;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer> Version;
        public readonly delegate* unmanaged[Cdecl]<byte*, nuint, BamlBuffer> InitializeRuntimeFromBytecode;
        public readonly delegate* unmanaged[Cdecl]<BamlBuffer, void> FreeBuffer;
        public readonly delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<uint, byte*, nuint, void>, void> RegisterCallback;
        public readonly delegate* unmanaged[Cdecl]<byte*, nuint, uint, void> CallFunction;
        public readonly delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>, void> RegisterHostDispatchCallback;
        public readonly delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, void>, void> RegisterHostReleaseCallback;
        public readonly delegate* unmanaged[Cdecl]<uint, int, byte*, nuint, void> CompleteHostCall;
        public readonly delegate* unmanaged[Cdecl]<ulong, ulong*, uint> HandleClone;
        public readonly delegate* unmanaged[Cdecl]<ulong, uint> HandleRelease;
        public readonly delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, uint> MediaFromUrl;
        public readonly delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, uint> MediaFromFile;
        public readonly delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, uint> MediaFromBase64;
        public readonly delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, uint> MediaUrl;
        public readonly delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, uint> MediaFile;
        public readonly delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, uint> MediaBase64;
        public readonly delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, uint> MediaMimeType;
        public readonly delegate* unmanaged[Cdecl]<BamlBridgeInfoV1*, BamlBuffer> RegisterBridge;
    }

    private static partial class NativeMethods
    {
        [LibraryImport(NativeLibraryName, EntryPoint = "baml_get_api_v1")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        internal static partial BamlApiV1* GetApiV1();
    }
}
