using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;

namespace Baml.Cffi;

internal enum BamlCffiStatus : uint
{
    Ok = 0,
    InvalidHandle = 1,
    TypeMismatch = 2,
    UnsupportedHandleType = 3,
    InternalError = 4,
    UnexpectedNullPointer = 5,
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct BamlBuffer
{
    internal byte* Pointer;
    internal nuint Length;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct BamlBridgeInfoV1
{
    internal nuint StructSize;
    internal uint Language;
    internal byte* SdkVersion;
    internal nuint SdkVersionLength;
    internal byte* BridgeRuntimeName;
    internal nuint BridgeRuntimeNameLength;
    internal byte* BridgeRuntimeVersion;
    internal nuint BridgeRuntimeVersionLength;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct BamlApiV1
{
    internal uint AbiVersion;
    internal nuint StructSize;
    internal delegate* unmanaged[Cdecl]<BamlBuffer> Version;
    internal delegate* unmanaged[Cdecl]<byte*, nuint, BamlBuffer> InitializeRuntimeFromBytecode;
    internal delegate* unmanaged[Cdecl]<BamlBuffer, void> FreeBuffer;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<uint, byte*, nuint, void>, void> RegisterCallback;
    internal delegate* unmanaged[Cdecl]<byte*, nuint, uint, void> CallFunction;
    internal delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
    internal delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>, void> RegisterHostDispatchCallback;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, void>, void> RegisterHostReleaseCallback;
    internal delegate* unmanaged[Cdecl]<uint, int, byte*, nuint, void> CompleteHostCall;
    internal delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus> HandleClone;
    internal delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus> HandleRelease;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromUrl;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromFile;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromBase64;
    internal delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, BamlCffiStatus> MediaUrl;
    internal delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, BamlCffiStatus> MediaFile;
    internal delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, BamlCffiStatus> MediaBase64;
    internal delegate* unmanaged[Cdecl]<ulong, int, BamlBuffer*, BamlCffiStatus> MediaMimeType;
    internal delegate* unmanaged[Cdecl]<BamlBridgeInfoV1*, BamlBuffer> RegisterBridge;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<sbyte*, nuint, int, void>, void> RegisterUnhandledSpawnErrorCallback;
    internal delegate* unmanaged[Cdecl]<BamlBuffer> ShutdownRuntime;
    internal delegate* unmanaged[Cdecl]<byte*, nuint, byte*, BamlBuffer> InitializeRuntimeFromBytecodeWithMetadata;
}

internal static unsafe class BamlApiV1Layout
{
    internal static readonly nuint RequiredPrefixSize = EndOf(nameof(BamlApiV1.InitializeRuntimeFromBytecodeWithMetadata));

    private static nuint EndOf(string field) =>
        checked((nuint)Marshal.OffsetOf<BamlApiV1>(field) + (nuint)IntPtr.Size);
}

internal static unsafe partial class NativeMethods
{
    internal const string LibraryName = "bridge_cffi";

    [LibraryImport(LibraryName, EntryPoint = "baml_get_api_v1")]
    [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
    internal static partial BamlApiV1* GetApiV1();
}
