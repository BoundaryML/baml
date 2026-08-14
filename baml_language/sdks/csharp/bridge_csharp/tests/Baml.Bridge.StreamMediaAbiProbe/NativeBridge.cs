using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;
using System.Text;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

internal sealed unsafe partial class NativeBridge : IDisposable
{
    private const string NativeLibraryName = "bridge_cffi";
    private const uint ApiVersion = 2;
    private const uint BridgeLanguageCSharp = 5;
    private const uint StatusOk = 0;
    private const uint StatusInvalidHandle = 1;

    private static readonly ConcurrentDictionary<
        uint,
        TaskCompletionSource<byte[]>> PendingCalls = new();

    private static BamlApiV1* s_api;
    private static ExceptionDispatchInfo? s_callbackFailure;
    private static int s_callbackCount;
    private static int s_lateCallbacks;
    private static int s_maxPendingCalls;
    private static int s_nextCallbackId;
    private static int s_releasedBuffers;

    private bool disposed;

    public NativeBridge(
        string nativeLibraryPath,
        string expectedVersion,
        string bytecodePath)
    {
        string? nativePath = StringComparer.Ordinal.Equals(
                nativeLibraryPath,
                "package-default")
            ? null
            : RequireExistingAbsoluteFile(
                nativeLibraryPath,
                "native library");
        string bytecode = RequireExistingAbsoluteFile(
            bytecodePath,
            "bytecode");

        if (nativePath is not null)
        {
            NativeLibrary.SetDllImportResolver(
                typeof(NativeBridge).Assembly,
                (libraryName, assembly, searchPath) =>
                {
                    if (!StringComparer.Ordinal.Equals(
                            libraryName,
                            NativeLibraryName))
                    {
                        return IntPtr.Zero;
                    }

                    return NativeLibrary.Load(
                        nativePath,
                        assembly,
                        searchPath);
                });
        }

        BamlApiV1* api = NativeMethods.GetApiV1();
        Require(api is not null, "baml_get_api_v1 returned null");
        Require(
            api->AbiVersion == ApiVersion,
            $"unexpected API version {api->AbiVersion}");
        Require(
            api->StructSize >= (nuint)sizeof(BamlApiV1),
            $"truncated BamlApiV1: {api->StructSize} < {sizeof(BamlApiV1)}");
        ValidateRequiredFunctions(api);
        s_api = api;

        string actualVersion = ConsumeUtf8(api->Version());
        Require(
            StringComparer.Ordinal.Equals(
                actualVersion,
                expectedVersion),
            $"product version mismatch: native={actualVersion}, expected={expectedVersion}");
        Require(
            RegisterBridge(expectedVersion).Length == 0,
            "C# bridge registration failed");

        byte[] bytes = File.ReadAllBytes(bytecode);
        fixed (byte* pointer = bytes)
        {
            string diagnostic = ConsumeUtf8(
                api->InitializeRuntimeFromBytecode(
                    pointer,
                    (nuint)bytes.Length));
            Require(
                diagnostic.Length == 0,
                $"bytecode initialization failed: {diagnostic}");
        }

        api->RegisterCallback(&OnResult);
        ProductVersion = actualVersion;
        BytecodeLength = bytes.Length;
    }

    public int BytecodeLength { get; }

    public int CallbackCount => Volatile.Read(ref s_callbackCount);

    public int MaxPendingCalls => Volatile.Read(ref s_maxPendingCalls);

    public string ProductVersion { get; }

    public int ReleasedBuffers => Volatile.Read(ref s_releasedBuffers);

    public ulong AllocateCallId()
    {
        ThrowIfDisposed();
        ulong callId = s_api->NewFunctionCall();
        Require(callId != 0, "new_function_call returned zero");
        return callId;
    }

    public int Cancel(ulong callId)
    {
        ThrowIfDisposed();
        return s_api->CancelFunctionCall(callId);
    }

    public NativeCall Dispatch(
        string functionName,
        CallFunctionArgs arguments)
    {
        ThrowIfDisposed();
        ArgumentException.ThrowIfNullOrWhiteSpace(functionName);
        ArgumentNullException.ThrowIfNull(arguments);
        if (arguments.CallId == 0)
        {
            arguments.CallId = AllocateCallId();
        }

        uint callbackId = checked(
            (uint)Interlocked.Increment(ref s_nextCallbackId));
        TaskCompletionSource<byte[]> completion = new(
            TaskCreationOptions.RunContinuationsAsynchronously);
        Require(
            PendingCalls.TryAdd(callbackId, completion),
            $"duplicate callback identity {callbackId}");
        UpdateMaximum(
            ref s_maxPendingCalls,
            PendingCalls.Count);

        arguments.FunctionName = functionName;
        byte[] encodedArguments = arguments.ToByteArray();
        fixed (byte* argumentBytes = encodedArguments)
        {
            s_api->CallFunction(
                argumentBytes,
                (nuint)encodedArguments.Length,
                callbackId);
        }

        return new NativeCall(
            arguments.CallId,
            ParseResultAsync(completion.Task));
    }

    public Task<BamlOutboundResult> CallAsync(
        string functionName,
        CallFunctionArgs arguments)
    {
        NativeCall call = Dispatch(functionName, arguments);
        return call.Completion.WaitAsync(TimeSpan.FromSeconds(30));
    }

    public ulong CloneHandle(ulong key)
    {
        ThrowIfDisposed();
        ulong clone = 0;
        uint status = s_api->HandleClone(key, &clone);
        Require(
            status == StatusOk && clone != 0 && clone != key,
            $"handle clone failed: status={status}, key={key}, clone={clone}");
        return clone;
    }

    public bool IsReleasedHandle(ulong key)
    {
        ThrowIfDisposed();
        ulong clone = 0;
        uint status = s_api->HandleClone(key, &clone);
        if (status == StatusOk)
        {
            _ = s_api->HandleRelease(clone);
            return false;
        }

        Require(
            status == StatusInvalidHandle,
            $"unexpected clone status {status} for released-handle check");
        return true;
    }

    public void ReleaseHandle(ulong key)
    {
        ThrowIfDisposed();
        Require(
            s_api->HandleRelease(key) == StatusOk,
            $"handle release failed for {key}");
    }

    public NativeHandle CreateMedia(
        int mediaKind,
        MediaSource source,
        string value,
        string? mimeType)
    {
        ThrowIfDisposed();
        byte[] encodedValue = NullTerminatedUtf8(value);
        byte[]? encodedMime = mimeType is null
            ? null
            : NullTerminatedUtf8(mimeType);
        ulong key = 0;
        int handleType = 0;
        uint status;
        fixed (byte* valuePointer = encodedValue)
        fixed (byte* mimePointer = encodedMime)
        {
            status = source switch
            {
                MediaSource.Url => s_api->MediaFromUrl(
                    mediaKind,
                    valuePointer,
                    mimePointer,
                    &key,
                    &handleType),
                MediaSource.Base64 => s_api->MediaFromBase64(
                    mediaKind,
                    valuePointer,
                    mimePointer,
                    &key,
                    &handleType),
                MediaSource.File => s_api->MediaFromFile(
                    mediaKind,
                    valuePointer,
                    mimePointer,
                    &key,
                    &handleType),
                _ => throw new ArgumentOutOfRangeException(
                    nameof(source)),
            };
        }

        Require(
            status == StatusOk && key != 0,
            $"media constructor failed: status={status}, key={key}");
        return new NativeHandle(key, handleType, Ty: null);
    }

    public string ReadMedia(
        NativeHandle handle,
        MediaSource source)
    {
        ThrowIfDisposed();
        BamlBuffer buffer = default;
        uint status = source switch
        {
            MediaSource.Url => s_api->MediaUrl(
                handle.Key,
                handle.HandleType,
                &buffer),
            MediaSource.Base64 => s_api->MediaBase64(
                handle.Key,
                handle.HandleType,
                &buffer),
            MediaSource.File => s_api->MediaFile(
                handle.Key,
                handle.HandleType,
                &buffer),
            _ => throw new ArgumentOutOfRangeException(
                nameof(source)),
        };
        Require(
            status == StatusOk,
            $"media accessor failed: source={source}, status={status}");
        return ConsumeUtf8(buffer);
    }

    public string ReadMediaMimeType(NativeHandle handle)
    {
        ThrowIfDisposed();
        BamlBuffer buffer = default;
        uint status = s_api->MediaMimeType(
            handle.Key,
            handle.HandleType,
            &buffer);
        Require(
            status == StatusOk,
            $"media MIME accessor failed with {status}");
        return ConsumeUtf8(buffer);
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        s_callbackFailure?.Throw();
        Require(
            PendingCalls.IsEmpty,
            "native callback registry was not drained");
        Require(
            Volatile.Read(ref s_lateCallbacks) == 0,
            "native delivered an unknown/duplicate callback");
    }

    private static Task<BamlOutboundResult> ParseResultAsync(
        Task<byte[]> bytesTask) =>
        bytesTask.ContinueWith(
            static completed =>
            {
                byte[] bytes = completed.GetAwaiter().GetResult();
                try
                {
                    return BamlOutboundResult.Parser.ParseFrom(bytes);
                }
                catch (InvalidProtocolBufferException exception)
                {
                    throw new InvalidDataException(
                        "native returned a malformed BamlOutboundResult",
                        exception);
                }
            },
            CancellationToken.None,
            TaskContinuationOptions.ExecuteSynchronously,
            TaskScheduler.Default);

    private static string RegisterBridge(string version)
    {
        byte[] encodedVersion = Encoding.UTF8.GetBytes(version);
        fixed (byte* versionPointer = encodedVersion)
        {
            BamlBridgeInfoV1 info = new()
            {
                StructSize = (nuint)sizeof(BamlBridgeInfoV1),
                Language = BridgeLanguageCSharp,
                SdkVersion = versionPointer,
                SdkVersionLength = (nuint)encodedVersion.Length,
            };
            return ConsumeUtf8(s_api->RegisterBridge(&info));
        }
    }

    private static string ConsumeUtf8(BamlBuffer buffer) =>
        new UTF8Encoding(
            encoderShouldEmitUTF8Identifier: false,
            throwOnInvalidBytes: true).GetString(ConsumeBuffer(buffer));

    private static byte[] ConsumeBuffer(BamlBuffer buffer)
    {
        try
        {
            if (buffer.Length == 0)
            {
                return [];
            }

            Require(
                buffer.Pointer is not null,
                "non-empty native buffer has a null pointer");
            Require(
                buffer.Length <= Int32.MaxValue,
                $"native buffer is too large: {buffer.Length}");
            return new ReadOnlySpan<byte>(
                    buffer.Pointer,
                    checked((int)buffer.Length))
                .ToArray();
        }
        finally
        {
            s_api->FreeBuffer(buffer);
            Interlocked.Increment(ref s_releasedBuffers);
        }
    }

    private static byte[] NullTerminatedUtf8(string value)
    {
        byte[] bytes = new byte[Encoding.UTF8.GetByteCount(value) + 1];
        Encoding.UTF8.GetBytes(value, bytes);
        return bytes;
    }

    private static string RequireExistingAbsoluteFile(
        string value,
        string description)
    {
        if (!Path.IsPathFullyQualified(value))
        {
            throw new FileNotFoundException(
                $"{description} does not exist at an absolute path",
                value);
        }

        string path = Path.GetFullPath(value);
        if (!File.Exists(path))
        {
            throw new FileNotFoundException(
                $"{description} does not exist at an absolute path",
                path);
        }

        return path;
    }

    private static void ValidateRequiredFunctions(BamlApiV1* api)
    {
        Require(api->Version is not null, "version is null");
        Require(
            api->InitializeRuntimeFromBytecode is not null,
            "initialize_runtime_from_bytecode is null");
        Require(api->FreeBuffer is not null, "free_buffer is null");
        Require(api->RegisterCallback is not null, "register_callback is null");
        Require(api->CallFunction is not null, "call_function is null");
        Require(
            api->NewFunctionCall is not null,
            "new_function_call is null");
        Require(
            api->CancelFunctionCall is not null,
            "cancel_function_call is null");
        Require(api->HandleClone is not null, "handle_clone is null");
        Require(api->HandleRelease is not null, "handle_release is null");
        Require(api->MediaFromUrl is not null, "media_from_url is null");
        Require(api->MediaFromFile is not null, "media_from_file is null");
        Require(
            api->MediaFromBase64 is not null,
            "media_from_base64 is null");
        Require(api->MediaUrl is not null, "media_url is null");
        Require(api->MediaFile is not null, "media_file is null");
        Require(api->MediaBase64 is not null, "media_base64 is null");
        Require(
            api->MediaMimeType is not null,
            "media_mime_type is null");
        Require(api->RegisterBridge is not null, "register_bridge is null");
    }

    private static void UpdateMaximum(ref int target, int candidate)
    {
        int observed = Volatile.Read(ref target);
        while (candidate > observed)
        {
            int prior = Interlocked.CompareExchange(
                ref target,
                candidate,
                observed);
            if (prior == observed)
            {
                return;
            }

            observed = prior;
        }
    }

    private void ThrowIfDisposed() =>
        ObjectDisposedException.ThrowIf(disposed, this);

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnResult(
        uint callbackId,
        byte* content,
        nuint length)
    {
        try
        {
            if (length > Int32.MaxValue
                || (length != 0 && content is null))
            {
                throw new InvalidDataException(
                    $"invalid borrowed result buffer for callback {callbackId}");
            }

            byte[] copy = length == 0
                ? []
                : new ReadOnlySpan<byte>(
                    content,
                    checked((int)length)).ToArray();
            Interlocked.Increment(ref s_callbackCount);
            if (!PendingCalls.TryRemove(
                    callbackId,
                    out TaskCompletionSource<byte[]>? completion))
            {
                Interlocked.Increment(ref s_lateCallbacks);
                return;
            }

            completion.TrySetResult(copy);
        }
        catch (Exception exception)
        {
            Interlocked.CompareExchange(
                ref s_callbackFailure,
                ExceptionDispatchInfo.Capture(exception),
                null);
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
            nuint,
            uint,
            void> CallFunction;
        public readonly delegate* unmanaged[Cdecl]<ulong> NewFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<ulong, int> CancelFunctionCall;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>,
            void> RegisterHostDispatchCallback;
        public readonly delegate* unmanaged[Cdecl]<
            delegate* unmanaged[Cdecl]<ulong, void>,
            void> RegisterHostReleaseCallback;
        public readonly delegate* unmanaged[Cdecl]<
            uint,
            int,
            byte*,
            nuint,
            void> CompleteHostCall;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            ulong*,
            uint> HandleClone;
        public readonly delegate* unmanaged[Cdecl]<ulong, uint> HandleRelease;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromUrl;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromFile;
        public readonly delegate* unmanaged[Cdecl]<
            int,
            byte*,
            byte*,
            ulong*,
            int*,
            uint> MediaFromBase64;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaUrl;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaFile;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaBase64;
        public readonly delegate* unmanaged[Cdecl]<
            ulong,
            int,
            BamlBuffer*,
            uint> MediaMimeType;
        public readonly delegate* unmanaged[Cdecl]<
            BamlBridgeInfoV1*,
            BamlBuffer> RegisterBridge;
    }

    private static partial class NativeMethods
    {
        [LibraryImport(
            NativeLibraryName,
            EntryPoint = "baml_get_api_v1")]
        [UnmanagedCallConv(CallConvs = [typeof(CallConvCdecl)])]
        internal static partial BamlApiV1* GetApiV1();
    }
}

internal enum MediaSource
{
    Url,
    Base64,
    File,
}

internal sealed record NativeCall(
    ulong OperationId,
    Task<BamlOutboundResult> Completion);

internal sealed record NativeHandle(
    ulong Key,
    int HandleType,
    BamlTy? Ty);
