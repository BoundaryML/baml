using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace Baml.Bridge;

internal static unsafe class NativeApi
{
    private const uint ExpectedAbiVersion = 1;
    private const uint CSharpBridgeLanguage = 5;
    private const string LibraryOverrideEnvironmentVariable = "BAML_BRIDGE_LIBRARY";

    private static readonly ConcurrentDictionary<uint, PendingCall> PendingCalls = new();
    private static readonly string SdkVersion = BridgeVersion.Current;
    private static readonly nint LibraryHandle;
    private static readonly ApiV1* Api;
    private static int _nextCallbackId;

    static NativeApi()
    {
        try
        {
            LibraryHandle = LoadLibrary();
            var getApiAddress = NativeLibrary.GetExport(LibraryHandle, "baml_get_api_v1");
            var getApi = (delegate* unmanaged[Cdecl]<ApiV1*>)getApiAddress;
            Api = getApi();
            ValidateApi();
            RegisterBridge();
            Api->RegisterCallback(&CompleteCall);
            Api->RegisterHostDispatchCallback(&HostValueRegistry.Dispatch);
            Api->RegisterHostReleaseCallback(&HostValueRegistry.Release);
        }
        catch (BamlBridgeException)
        {
            throw;
        }
        catch (Exception error)
        {
            throw new BamlBridgeException("Failed to initialize the native BAML C API.", error);
        }
    }

    internal static ulong NewFunctionCall() => Api->NewFunctionCall();

    internal static void FlushEvents() => Api->FlushEvents();

    internal static ulong CloneHandle(ulong key, string operation)
    {
        ulong clonedKey = 0;
        ThrowIfFailed(Api->HandleClone(key, &clonedKey), operation);
        if (clonedKey == 0)
        {
            throw new BamlBridgeException($"Native handle operation {operation} returned a zero key.");
        }

        return clonedKey;
    }

    internal static bool ReleaseHandle(ulong key) => Api->HandleRelease(key) == BamlCffiStatus.Ok;

    internal static NativeHandle CreateMedia(
        NativeMediaKind kind,
        NativeMediaSource source,
        string value,
        string? mimeType)
    {
        ArgumentNullException.ThrowIfNull(value);
        var valueBytes = NullTerminatedUtf8(value, nameof(value));
        var mimeTypeBytes = mimeType is null ? null : NullTerminatedUtf8(mimeType, nameof(mimeType));
        ulong key = 0;
        int handleType = 0;
        fixed (byte* valuePointer = valueBytes)
        fixed (byte* mimeTypePointer = mimeTypeBytes)
        {
            var status = source switch
            {
                NativeMediaSource.Url => Api->MediaFromUrl((int)kind, valuePointer, mimeTypePointer, &key, &handleType),
                NativeMediaSource.File => Api->MediaFromFile((int)kind, valuePointer, mimeTypePointer, &key, &handleType),
                NativeMediaSource.Base64 => Api->MediaFromBase64((int)kind, valuePointer, mimeTypePointer, &key, &handleType),
                _ => BamlCffiStatus.UnsupportedHandleType,
            };
            ThrowIfFailed(status, $"create {kind} media from {source}");
        }

        return NativeHandle.FromOwned(key, handleType);
    }

    internal static string? ReadMediaUrl(ulong key, int handleType) =>
        ReadMediaString(Api->MediaUrl, key, handleType, "read media URL", optional: true);

    internal static string? ReadMediaFile(ulong key, int handleType) =>
        ReadMediaString(Api->MediaFile, key, handleType, "read media file", optional: true);

    internal static string ReadMediaBase64(ulong key, int handleType) =>
        ReadMediaString(Api->MediaBase64, key, handleType, "read media base64", optional: false)!;

    internal static string? ReadMediaMimeType(ulong key, int handleType) =>
        ReadMediaString(Api->MediaMimeType, key, handleType, "read media MIME type", optional: true);

    internal static void InitializeRuntime(ReadOnlySpan<byte> bytecode)
    {
        fixed (byte* bytecodePointer = bytecode)
        {
            var result = Api->InitializeRuntimeFromBytecode(bytecodePointer, (nuint)bytecode.Length);
            var error = CopyAndFree(result);
            if (error.Length != 0)
            {
                throw new BamlBridgeException($"Failed to initialize BAML bytecode: {Encoding.UTF8.GetString(error)}");
            }
        }
    }

    internal static Task<NativeCallResult> CallFunctionAsync(
        string functionName,
        byte[] encodedArguments,
        ulong nativeCallId,
        CancellationToken cancellationToken)
    {
        cancellationToken.ThrowIfCancellationRequested();
        var callbackId = AllocateCallbackId();
        var pending = new PendingCall();
        if (!PendingCalls.TryAdd(callbackId, pending))
        {
            throw new BamlBridgeException($"Failed to reserve native callback ID {callbackId}.");
        }

        try
        {
            var functionNameBytes = Encoding.UTF8.GetBytes(functionName + "\0");
            fixed (byte* functionNamePointer = functionNameBytes)
            fixed (byte* argumentsPointer = encodedArguments)
            {
                Api->CallFunction(
                    functionNamePointer,
                    argumentsPointer,
                    (nuint)encodedArguments.Length,
                    callbackId);
            }
        }
        catch (Exception error)
        {
            PendingCalls.TryRemove(callbackId, out _);
            throw new BamlBridgeException($"Failed to invoke BAML function {functionName}.", error);
        }

        if (cancellationToken.CanBeCanceled)
        {
            pending.SetCancellationRegistration(cancellationToken.Register(
                static state =>
                {
                    var cancellation = (CancellationState)state!;
                    if (PendingCalls.TryRemove(cancellation.CallbackId, out var call)
                        && call.TrySetCanceled(cancellation.CancellationToken))
                    {
                        _ = Api->CancelFunctionCall(cancellation.NativeCallId);
                    }
                },
                new CancellationState(callbackId, nativeCallId, cancellationToken)));
        }

        return pending.Task;
    }

    internal static void CompleteHostCall(uint callId, bool isError, ReadOnlySpan<byte> payload)
    {
        fixed (byte* payloadPointer = payload)
        {
            Api->CompleteHostCall(callId, isError ? 1 : 0, (sbyte*)payloadPointer, (nuint)payload.Length);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void CompleteCall(uint callbackId, sbyte* content, nuint length)
    {
        PendingCall? pending = null;
        try
        {
            if (!PendingCalls.TryRemove(callbackId, out pending))
            {
                return;
            }

            if (content == null && length != 0)
            {
                pending.TrySetException(new BamlBridgeException(
                    $"Native callback {callbackId} supplied a null pointer with nonzero length {length}."));
                return;
            }

            if (length > int.MaxValue)
            {
                pending.TrySetException(new BamlBridgeException(
                    $"Native callback {callbackId} payload length {length} exceeds the managed limit."));
                return;
            }

            var payload = length == 0
                ? Array.Empty<byte>()
                : new ReadOnlySpan<byte>(content, checked((int)length)).ToArray();
            Exception? hostException = null;
            try
            {
                var result = global::BamlBridge.Cffi.V1.BamlOutboundResult.Parser.ParseFrom(payload);
                hostException = HostValueRegistry.FindException(result);
            }
            catch (Google.Protobuf.InvalidProtocolBufferException)
            {
                // The normal result decoder reports malformed protobuf with its full context.
            }

            pending.TrySetResult(new NativeCallResult(payload, hostException));
        }
        catch (Exception error)
        {
            if (pending is not null || PendingCalls.TryRemove(callbackId, out pending))
            {
                pending?.TrySetException(new BamlBridgeException(
                    $"Managed callback processing failed for callback ID {callbackId}.", error));
            }
        }
    }

    private static uint AllocateCallbackId()
    {
        for (var attempts = 0; attempts < int.MaxValue; attempts++)
        {
            var candidate = unchecked((uint)Interlocked.Increment(ref _nextCallbackId));
            if (candidate != 0 && !PendingCalls.ContainsKey(candidate))
            {
                return candidate;
            }
        }

        throw new BamlBridgeException("The managed callback ID space is exhausted.");
    }

    private static void RegisterBridge()
    {
        var versionBytes = Encoding.UTF8.GetBytes(SdkVersion);
        fixed (byte* versionPointer = versionBytes)
        {
            var info = new BridgeInfoV1
            {
                StructSize = (nuint)sizeof(BridgeInfoV1),
                Language = CSharpBridgeLanguage,
                SdkVersion = versionPointer,
                SdkVersionLength = (nuint)versionBytes.Length,
            };
            var result = Api->RegisterBridge(&info);
            var error = CopyAndFree(result);
            if (error.Length != 0)
            {
                throw new BamlBridgeException(Encoding.UTF8.GetString(error));
            }
        }
    }

    private static void ValidateApi()
    {
        if (Api == null)
        {
            throw new BamlBridgeException("baml_get_api_v1 returned a null API table.");
        }

        if (Api->AbiVersion != ExpectedAbiVersion)
        {
            throw new BamlBridgeException(
                $"Unsupported BAML C ABI version {Api->AbiVersion}; this bridge requires version {ExpectedAbiVersion}.");
        }

        if (Api->StructSize < (nuint)sizeof(ApiV1))
        {
            throw new BamlBridgeException(
                $"BAML C ABI table is truncated: native size {Api->StructSize}, required size {sizeof(ApiV1)}.");
        }

        if (Api->Version == null
            || Api->InitializeRuntimeFromBytecode == null
            || Api->FreeBuffer == null
            || Api->RegisterCallback == null
            || Api->CallFunction == null
            || Api->NewFunctionCall == null
            || Api->CancelFunctionCall == null
            || Api->RegisterHostDispatchCallback == null
            || Api->RegisterHostReleaseCallback == null
            || Api->CompleteHostCall == null
            || Api->HandleClone == null
            || Api->HandleRelease == null
            || Api->MediaFromUrl == null
            || Api->MediaFromFile == null
            || Api->MediaFromBase64 == null
            || Api->MediaUrl == null
            || Api->MediaFile == null
            || Api->MediaBase64 == null
            || Api->MediaMimeType == null
            || Api->RegisterBridge == null
            || Api->FlushEvents == null)
        {
            throw new BamlBridgeException("BAML C ABI table is missing one or more required function pointers.");
        }
    }

    private static byte[] CopyAndFree(NativeBuffer buffer)
    {
        try
        {
            if (buffer.Pointer == null && buffer.Length != 0)
            {
                throw new BamlBridgeException("The native runtime returned a null buffer pointer with nonzero length.");
            }

            if (buffer.Length > int.MaxValue)
            {
                throw new BamlBridgeException($"Native buffer length {buffer.Length} exceeds the managed limit.");
            }

            return buffer.Length == 0
                ? Array.Empty<byte>()
                : new ReadOnlySpan<byte>(buffer.Pointer, checked((int)buffer.Length)).ToArray();
        }
        finally
        {
            if (buffer.Pointer != null)
            {
                Api->FreeBuffer(buffer);
            }
        }
    }

    private static string? ReadMediaString(
        delegate* unmanaged[Cdecl]<ulong, int, NativeBuffer*, BamlCffiStatus> accessor,
        ulong key,
        int handleType,
        string operation,
        bool optional)
    {
        var buffer = default(NativeBuffer);
        ThrowIfFailed(accessor(key, handleType, &buffer), operation);
        if (buffer.Pointer == null)
        {
            if (buffer.Length != 0)
            {
                throw new BamlBridgeException($"Native handle operation {operation} returned a null pointer with nonzero length.");
            }

            return optional ? null : string.Empty;
        }

        return Encoding.UTF8.GetString(CopyAndFree(buffer));
    }

    private static byte[] NullTerminatedUtf8(string value, string parameterName)
    {
        if (value.Contains('\0', StringComparison.Ordinal))
        {
            throw new ArgumentException("A native media string cannot contain a NUL character.", parameterName);
        }

        return Encoding.UTF8.GetBytes(value + "\0");
    }

    private static void ThrowIfFailed(BamlCffiStatus status, string operation)
    {
        if (status != BamlCffiStatus.Ok)
        {
            throw new BamlBridgeException($"Native handle operation {operation} failed: {status}.");
        }
    }

    private static nint LoadLibrary()
    {
        BridgePlatform.EnsureSupported();
        var explicitPath = Environment.GetEnvironmentVariable(LibraryOverrideEnvironmentVariable);
        if (!string.IsNullOrWhiteSpace(explicitPath))
        {
            try
            {
                return NativeLibrary.Load(Path.GetFullPath(explicitPath));
            }
            catch (Exception error)
            {
                throw new BamlBridgeException(
                    $"Failed to load bridge_cffi from {LibraryOverrideEnvironmentVariable}={explicitPath}.",
                    error);
            }
        }

        if (NativeLibrary.TryLoad(
                "bridge_cffi",
                typeof(NativeApi).Assembly,
                DllImportSearchPath.AssemblyDirectory | DllImportSearchPath.SafeDirectories,
                out var packageHandle))
        {
            return packageHandle;
        }

        var fileName = OperatingSystem.IsWindows()
            ? "bridge_cffi.dll"
            : OperatingSystem.IsMacOS() ? "libbridge_cffi.dylib" : "libbridge_cffi.so";
        foreach (var candidate in DevelopmentCandidates(fileName))
        {
            if (File.Exists(candidate) && NativeLibrary.TryLoad(candidate, out var developmentHandle))
            {
                return developmentHandle;
            }
        }

        throw new BamlBridgeException(
            $"Could not load {fileName}. Install a baml-bridge package containing the current RID, "
            + $"or set {LibraryOverrideEnvironmentVariable} to an absolute development-library path.");
    }

    private static IEnumerable<string> DevelopmentCandidates(string fileName)
    {
        var starts = new[]
        {
            AppContext.BaseDirectory,
            Path.GetDirectoryName(typeof(NativeApi).Assembly.Location),
            Environment.CurrentDirectory,
        };

        foreach (var start in starts.Where(static path => !string.IsNullOrWhiteSpace(path)).Distinct())
        {
            for (var directory = new DirectoryInfo(start!); directory is not null; directory = directory.Parent)
            {
                yield return Path.Combine(directory.FullName, "baml_language", "target", "debug", fileName);
                yield return Path.Combine(directory.FullName, "baml_language", "target", "release", fileName);
                yield return Path.Combine(directory.FullName, "target", "debug", fileName);
                yield return Path.Combine(directory.FullName, "target", "release", fileName);
            }
        }
    }

    // These fields are populated by native memory, not managed assignments.
#pragma warning disable CS0649
    [StructLayout(LayoutKind.Sequential)]
    private struct NativeBuffer
    {
        internal sbyte* Pointer;
        internal nuint Length;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct BridgeInfoV1
    {
        internal nuint StructSize;
        internal uint Language;
        internal byte* SdkVersion;
        internal nuint SdkVersionLength;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct ApiV1
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

    private sealed class PendingCall
    {
        private readonly TaskCompletionSource<NativeCallResult> _completion =
            new(TaskCreationOptions.RunContinuationsAsynchronously);
        private CancellationTokenRegistration _cancellationRegistration;
        private int _terminal;

        internal Task<NativeCallResult> Task => _completion.Task;

        internal bool TrySetResult(NativeCallResult result)
        {
            if (Interlocked.Exchange(ref _terminal, 1) != 0)
            {
                return false;
            }

            _cancellationRegistration.Unregister();
            return _completion.TrySetResult(result);
        }

        internal bool TrySetException(Exception error)
        {
            if (Interlocked.Exchange(ref _terminal, 1) != 0)
            {
                return false;
            }

            _cancellationRegistration.Unregister();
            return _completion.TrySetException(error);
        }

        internal bool TrySetCanceled(CancellationToken cancellationToken)
        {
            if (Interlocked.Exchange(ref _terminal, 1) != 0)
            {
                return false;
            }

            _cancellationRegistration.Unregister();
            return _completion.TrySetCanceled(cancellationToken);
        }

        internal void SetCancellationRegistration(CancellationTokenRegistration registration)
        {
            _cancellationRegistration = registration;
            if (Volatile.Read(ref _terminal) != 0)
            {
                registration.Unregister();
            }
        }
    }

    private sealed record CancellationState(
        uint CallbackId,
        ulong NativeCallId,
        CancellationToken CancellationToken);
}

internal readonly record struct NativeCallResult(ReadOnlyMemory<byte> Payload, Exception? HostException);

internal enum BamlCffiStatus : uint
{
    Ok = 0,
    InvalidHandle = 1,
    TypeMismatch = 2,
    UnsupportedHandleType = 3,
    InternalError = 4,
    UnexpectedNullPointer = 5,
}

internal enum NativeMediaKind
{
    Image = 1,
    Audio = 2,
    Pdf = 3,
    Video = 4,
}

internal enum NativeMediaSource
{
    Url,
    File,
    Base64,
}
