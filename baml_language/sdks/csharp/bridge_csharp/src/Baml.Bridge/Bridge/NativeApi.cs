using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.InteropServices;
using System.Text;

namespace Baml.Bridge;

internal static unsafe class NativeApi
{
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
            NativeApiContract.Validate(Api);
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
            var error = NativeBufferMarshaller.CopyAndFree(result, Api->FreeBuffer);
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
                Language = NativeBridgeLanguage.CSharp,
                SdkVersion = versionPointer,
                SdkVersionLength = (nuint)versionBytes.Length,
            };
            var result = Api->RegisterBridge(&info);
            var error = NativeBufferMarshaller.CopyAndFree(result, Api->FreeBuffer);
            if (error.Length != 0)
            {
                throw new BamlBridgeException(Encoding.UTF8.GetString(error));
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
        return NativeBufferMarshaller.ReadUtf8AndFree(buffer, optional, Api->FreeBuffer);
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

        if (NativeLibrary.TryLoad(
                "bridge_cffi",
                typeof(NativeApi).Assembly,
                DllImportSearchPath.AssemblyDirectory | DllImportSearchPath.SafeDirectories,
                out var packageHandle))
        {
            return packageHandle;
        }

        var explicitPath = NativeLibraryOverride.Resolve(Environment.GetEnvironmentVariable);
        if (explicitPath is not null)
        {
            try
            {
                return NativeLibrary.Load(Path.GetFullPath(explicitPath.Value.Path));
            }
            catch (Exception error)
            {
                throw new BamlBridgeException(
                    $"Failed to load bridge_cffi from {explicitPath.Value.Variable}={explicitPath.Value.Path}.",
                    error);
            }
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
            + $"or set {NativeLibraryOverride.CanonicalVariable} to an absolute development-library path.");
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
    Unspecified = 0,
    Image = 1,
    Audio = 2,
    Pdf = 3,
    Video = 4,
    Generic = 5,
}

internal enum NativeMediaSource
{
    Url,
    File,
    Base64,
}
