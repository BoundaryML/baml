using System.Runtime.InteropServices;

namespace Baml.Bridge;

#pragma warning disable CS0649
[StructLayout(LayoutKind.Sequential)]
internal unsafe struct NativeBuffer
{
    internal sbyte* Pointer;
    internal nuint Length;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct BridgeInfoV1
{
    internal nuint StructSize;
    internal NativeBridgeLanguage Language;
    internal byte* SdkVersion;
    internal nuint SdkVersionLength;
}

[StructLayout(LayoutKind.Sequential)]
internal unsafe struct ApiV1
{
    internal uint AbiVersion;
    internal nuint StructSize;
    internal delegate* unmanaged[Cdecl]<NativeBuffer> Version;
    internal delegate* unmanaged[Cdecl]<byte*, nuint, NativeBuffer> InitializeRuntimeFromBytecode;
    internal delegate* unmanaged[Cdecl]<NativeBuffer, void> FreeBuffer;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<uint, sbyte*, nuint, void>, void> RegisterCallback;
    internal delegate* unmanaged[Cdecl]<byte*, byte*, nuint, uint, void> CallFunction;
    internal delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
    internal delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>, void> RegisterHostDispatchCallback;
    internal delegate* unmanaged[Cdecl]<delegate* unmanaged[Cdecl]<ulong, void>, void> RegisterHostReleaseCallback;
    internal delegate* unmanaged[Cdecl]<uint, int, sbyte*, nuint, void> CompleteHostCall;
    internal delegate* unmanaged[Cdecl]<ulong, ulong*, BamlCffiStatus> HandleClone;
    internal delegate* unmanaged[Cdecl]<ulong, BamlCffiStatus> HandleRelease;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromUrl;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromFile;
    internal delegate* unmanaged[Cdecl]<int, byte*, byte*, ulong*, int*, BamlCffiStatus> MediaFromBase64;
    internal delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> MediaUrl;
    internal delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> MediaFile;
    internal delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> MediaBase64;
    internal delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> MediaMimeType;
    internal delegate* unmanaged[Cdecl]<BridgeInfoV1*, NativeBuffer> RegisterBridge;
    internal delegate* unmanaged[Cdecl]<void> FlushEvents;
}
#pragma warning restore CS0649

internal static unsafe class NativeApiContract
{
    internal const uint ExpectedAbiVersion = 1;
    internal static nuint RequiredSize => (nuint)sizeof(ApiV1);

    internal static void Validate(ApiV1* api)
    {
        if (api == null)
        {
            throw new BamlBridgeException("baml_get_api_v1 returned a null API table.");
        }

        if (api->AbiVersion != ExpectedAbiVersion)
        {
            throw new BamlBridgeException(
                $"Unsupported BAML C ABI version {api->AbiVersion}; this bridge requires version {ExpectedAbiVersion}.");
        }

        if (api->StructSize < RequiredSize)
        {
            throw new BamlBridgeException(
                $"BAML C ABI table is truncated: native size {api->StructSize}, required size {RequiredSize}.");
        }

        if (api->Version == null
            || api->InitializeRuntimeFromBytecode == null
            || api->FreeBuffer == null
            || api->RegisterCallback == null
            || api->CallFunction == null
            || api->NewFunctionCall == null
            || api->CancelFunctionCall == null
            || api->RegisterHostDispatchCallback == null
            || api->RegisterHostReleaseCallback == null
            || api->CompleteHostCall == null
            || api->HandleClone == null
            || api->HandleRelease == null
            || api->MediaFromUrl == null
            || api->MediaFromFile == null
            || api->MediaFromBase64 == null
            || api->MediaUrl == null
            || api->MediaFile == null
            || api->MediaBase64 == null
            || api->MediaMimeType == null
            || api->RegisterBridge == null
            || api->FlushEvents == null)
        {
            throw new BamlBridgeException("BAML C ABI table is missing one or more required function pointers.");
        }
    }
}

internal static unsafe class NativeBufferMarshaller
{
    internal static byte[] CopyAndFree(
        NativeBuffer buffer,
        delegate* unmanaged[Cdecl]<NativeBuffer, void> freeBuffer)
    {
        try
        {
            if (buffer.Pointer == null && buffer.Length != 0)
            {
                throw new BamlBridgeException(
                    "The native runtime returned a null buffer pointer with nonzero length.");
            }

            if (buffer.Length > int.MaxValue)
            {
                throw new BamlBridgeException(
                    $"Native buffer length {buffer.Length} exceeds the managed limit.");
            }

            return buffer.Length == 0
                ? Array.Empty<byte>()
                : new ReadOnlySpan<byte>(buffer.Pointer, checked((int)buffer.Length)).ToArray();
        }
        finally
        {
            freeBuffer(buffer);
        }
    }

    internal static string? ReadUtf8AndFree(
        NativeBuffer buffer,
        bool optional,
        delegate* unmanaged[Cdecl]<NativeBuffer, void> freeBuffer)
    {
        var absent = optional && buffer.Length == 0;
        var content = CopyAndFree(buffer, freeBuffer);
        return absent ? null : System.Text.Encoding.UTF8.GetString(content);
    }
}

internal static class NativeLibraryOverride
{
    internal const string CanonicalVariable = "BAML_RUNTIME_PATH";
    internal const string LibraryCompatibilityVariable = "BAML_LIBRARY_PATH";
    internal const string CSharpCompatibilityVariable = "BAML_BRIDGE_LIBRARY";

    internal static (string Variable, string Path)? Resolve(
        Func<string, string?> getEnvironmentVariable)
    {
        var configured = new[]
        {
            (Variable: CanonicalVariable, Path: getEnvironmentVariable(CanonicalVariable)),
            (Variable: LibraryCompatibilityVariable, Path: getEnvironmentVariable(LibraryCompatibilityVariable)),
            (Variable: CSharpCompatibilityVariable, Path: getEnvironmentVariable(CSharpCompatibilityVariable)),
        }
        .Where(static value => !string.IsNullOrWhiteSpace(value.Path))
        .Select(static value => (value.Variable, Path: value.Path!.Trim()))
        .ToArray();

        if (configured.Length == 0)
        {
            return null;
        }

        var first = configured[0];
        if (configured.Any(value => !string.Equals(value.Path, first.Path, StringComparison.Ordinal)))
        {
            var names = string.Join(", ", configured.Select(static value => value.Variable));
            throw new BamlBridgeException(
                $"Conflicting native runtime paths are configured through {names}. Set only "
                + $"{CanonicalVariable}, or give every compatibility alias the same value.");
        }

        return first;
    }
}

internal enum NativeBridgeLanguage : uint
{
    NodeJs = 1,
    Python = 2,
    Go = 3,
    Rust = 4,
    CSharp = 5,
    Cpp = 6,
}

internal enum NativeHandleType
{
    Unspecified = 0,
    UntaggedRustData = 1,
    UntaggedBexHeap = 2,
    FunctionRef = 5,
    MediaImage = 6,
    MediaAudio = 7,
    MediaVideo = 8,
    MediaPdf = 9,
    MediaGeneric = 10,
    PromptAst = 11,
    Collector = 12,
    Type = 13,
    TaggedHeapHandle = 14,
    HostValueCallable = 15,
    HostValueOpaque = 16,
}
