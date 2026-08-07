using System.Text;

namespace Baml.Cffi;

internal sealed unsafe partial class NativeApi
{
    private const uint AbiVersion = 2;
    private const uint CSharpBridgeLanguage = 5;

    private static readonly Lazy<NativeApi> Current = new(
        Load,
        LazyThreadSafetyMode.ExecutionAndPublication);

    private readonly BamlApiV1* table;

    internal NativeApi(BamlApiV1* table, string productVersion)
    {
        this.table = table;
        ProductVersion = productVersion;
    }

    internal static NativeApi Instance => Current.Value;

    internal string ProductVersion { get; }

    internal BamlApiV1* Table => table;

    internal BamlSafeHandle OwnHandle(ulong key)
    {
        if (key == 0)
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid handle.",
                "An outbound BAML handle used key zero.");
        }

        return new BamlSafeHandle(key, table->HandleClone, table->HandleRelease);
    }

    internal ulong NewFunctionCall()
    {
        return RequireFunctionCallIdentifier(table->NewFunctionCall());
    }

    internal static ulong RequireFunctionCallIdentifier(ulong identifier)
    {
        if (identifier == 0)
        {
            throw new BamlProtocolException(
                "The native function-call identifier space is exhausted.",
                "baml_api_v1.new_function_call returned its permanent zero exhaustion sentinel.");
        }

        return identifier;
    }

    internal void InitializeRuntime(ReadOnlySpan<byte> bytecode, string? embeddedBamlToml)
    {
        fixed (byte* pointer = bytecode)
        {
            byte[]? manifest = embeddedBamlToml is null
                ? null
                : Encoding.UTF8.GetBytes(embeddedBamlToml + "\0");
            BamlBuffer status;
            fixed (byte* manifestPointer = manifest)
            {
                status = embeddedBamlToml is null
                    ? table->InitializeRuntimeFromBytecode(pointer, (nuint)bytecode.Length)
                    : table->InitializeRuntimeFromBytecodeWithMetadata(
                        pointer,
                        (nuint)bytecode.Length,
                        manifestPointer);
            }
            string diagnostic = NativeBuffer.ReadUtf8AndFree(
                table,
                status);
            if (diagnostic.Length != 0)
            {
                throw new BamlProgramIntegrityException(diagnostic);
            }
        }
    }

    internal Task<byte[]> InvokeFunctionAsync(
        string functionIdentity,
        Func<ulong, byte[]> encodeArguments,
        CancellationToken cancellationToken) =>
        InvokeOwnedFunctionAsync(
            functionIdentity,
            callId => new EncodedCallArguments(encodeArguments(callId)),
            cancellationToken);

    internal Task<byte[]> InvokeOwnedFunctionAsync(
        string functionIdentity,
        Func<ulong, EncodedCallArguments> encodeArguments,
        CancellationToken cancellationToken) =>
        NativeCallCompletion.CompleteManagedOperationAsync(StartOwnedFunction(
            functionIdentity,
            encodeArguments,
            cancellationToken));

    internal Task<byte[]> InvokeOwnedHandleAsync(
        ulong handleKey,
        Func<ulong, EncodedCallArguments> encodeArguments,
        CancellationToken cancellationToken) =>
        NativeCallCompletion.CompleteManagedOperationAsync(StartOwnedHandle(
            handleKey,
            encodeArguments,
            cancellationToken));

    internal NativeFunctionCall StartOwnedFunction(
        string functionIdentity,
        Func<ulong, EncodedCallArguments> encodeArguments,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(functionIdentity);
        ArgumentNullException.ThrowIfNull(encodeArguments);
        if (cancellationToken.IsCancellationRequested)
        {
            return new NativeFunctionCall(
                FunctionCallId: 0,
                Task.FromCanceled<byte[]>(cancellationToken));
        }

        NativeCallbacks.ThrowIfCallbackFailed();
        if (functionIdentity.Contains('\0'))
        {
            throw new BamlProtocolException(
                "A generated BAML function identity is invalid.",
                "The function identity contained an interior NUL byte.");
        }

        ulong callId = NewFunctionCall();
        HostValueRegistry.Shared.BeginFunctionCall(callId, cancellationToken);
        try
        {
            using EncodedCallArguments arguments = encodeArguments(callId);
            arguments.SetCallTarget(functionIdentity);
            (uint callbackId, Task<byte[]> completion) = NativeCallbacks.AddPending();
            var cancellation = new CallCancellation(
                this,
                callId,
                callbackId,
                cancellationToken);
            CancellationTokenRegistration registration = default;

            try
            {
                registration = cancellationToken.Register(
                    static state => ((CallCancellation)state!).Cancel(),
                    cancellation);
                fixed (byte* argumentPointer = arguments.Bytes)
                {
                    if (cancellation.Dispatch(
                        argumentPointer,
                        (nuint)arguments.Bytes.Length))
                    {
                        arguments.Commit();
                    }
                }
            }
            catch
            {
                _ = NativeCallbacks.TryDiscard(callbackId);
                registration.Dispose();
                throw;
            }

            return new NativeFunctionCall(
                callId,
                NativeCallCompletion.CompleteAsync(completion, registration));
        }
        catch
        {
            HostValueRegistry.Shared.CompleteFunctionCall(callId);
            throw;
        }
    }

    internal NativeFunctionCall StartOwnedHandle(
        ulong handleKey,
        Func<ulong, EncodedCallArguments> encodeArguments,
        CancellationToken cancellationToken)
    {
        if (handleKey == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(handleKey));
        }

        ArgumentNullException.ThrowIfNull(encodeArguments);
        if (cancellationToken.IsCancellationRequested)
        {
            return new NativeFunctionCall(
                FunctionCallId: 0,
                Task.FromCanceled<byte[]>(cancellationToken));
        }

        NativeCallbacks.ThrowIfCallbackFailed();
        ulong callId = NewFunctionCall();
        HostValueRegistry.Shared.BeginFunctionCall(callId, cancellationToken);
        try
        {
            using EncodedCallArguments arguments = encodeArguments(callId);
            arguments.SetCallTarget(handleKey);
            (uint callbackId, Task<byte[]> completion) = NativeCallbacks.AddPending();
            var cancellation = new CallCancellation(
                this,
                callId,
                callbackId,
                cancellationToken);
            CancellationTokenRegistration registration = default;

            try
            {
                registration = cancellationToken.Register(
                    static state => ((CallCancellation)state!).Cancel(),
                    cancellation);
                fixed (byte* argumentPointer = arguments.Bytes)
                {
                    if (cancellation.Dispatch(
                        argumentPointer,
                        (nuint)arguments.Bytes.Length))
                    {
                        arguments.Commit();
                    }
                }
            }
            catch
            {
                _ = NativeCallbacks.TryDiscard(callbackId);
                registration.Dispose();
                throw;
            }

            return new NativeFunctionCall(
                callId,
                NativeCallCompletion.CompleteAsync(completion, registration));
        }
        catch
        {
            HostValueRegistry.Shared.CompleteFunctionCall(callId);
            throw;
        }
    }

    internal static void ValidateTable(BamlApiV1* api)
    {
        if (api is null)
        {
            throw new BamlNativeLibraryLoadException("baml_get_api_v1 returned null.");
        }

        if (api->AbiVersion != AbiVersion)
        {
            throw new BamlNativeLibraryLoadException(
                $"Expected bridge_cffi ABI {AbiVersion}, received {api->AbiVersion}.");
        }

        if (api->StructSize < BamlApiV1Layout.RequiredPrefixSize)
        {
            throw new BamlNativeLibraryLoadException(
                $"BamlApiV1 is truncated: {api->StructSize} < {BamlApiV1Layout.RequiredPrefixSize}.");
        }

        Require(api->Version is not null, "version");
        Require(api->InitializeRuntimeFromBytecode is not null, "initialize_runtime_from_bytecode");
        Require(api->InitializeRuntimeFromBytecodeWithMetadata is not null, "initialize_runtime_from_bytecode_with_metadata");
        Require(api->FreeBuffer is not null, "free_buffer");
        Require(api->RegisterCallback is not null, "register_callback");
        Require(api->CallFunction is not null, "call_function");
        Require(api->NewFunctionCall is not null, "new_function_call");
        Require(api->CancelFunctionCall is not null, "cancel_function_call");
        Require(api->RegisterHostDispatchCallback is not null, "register_host_dispatch_callback");
        Require(api->RegisterHostReleaseCallback is not null, "register_host_release_callback");
        Require(api->CompleteHostCall is not null, "complete_host_call");
        Require(api->HandleClone is not null, "handle_clone");
        Require(api->HandleRelease is not null, "handle_release");
        Require(api->MediaFromUrl is not null, "media_from_url");
        Require(api->MediaFromFile is not null, "media_from_file");
        Require(api->MediaFromBase64 is not null, "media_from_base64");
        Require(api->MediaUrl is not null, "media_url");
        Require(api->MediaFile is not null, "media_file");
        Require(api->MediaBase64 is not null, "media_base64");
        Require(api->MediaMimeType is not null, "media_mime_type");
        Require(api->RegisterBridge is not null, "register_bridge");
        Require(api->RegisterUnhandledSpawnErrorCallback is not null, "register_unhandled_spawn_error_callback");
        Require(api->ShutdownRuntime is not null, "shutdown_runtime");
    }

    private static NativeApi Load()
    {
        NativeLibraryResolver.EnsureRegistered();
        BamlApiV1* api;
        try
        {
            api = NativeMethods.GetApiV1();
        }
        catch (Exception error)
            when (error is DllNotFoundException
                or BadImageFormatException
                or EntryPointNotFoundException
                or BamlNativeLibraryLoadException)
        {
            throw new BamlNativeLibraryLoadException(
                $"Unable to resolve the canonical {NativeMethods.LibraryName}!baml_get_api_v1 entry point: {error.Message}",
                error);
        }

        ValidateTable(api);
        string version = NativeBuffer.ReadUtf8AndFree(api, api->Version());
        if (!StringComparer.Ordinal.Equals(version, RuntimeIdentity.RequiredBridgeVersion))
        {
            throw new BamlVersionMismatchException(
                $"Native bridge version {version} is incompatible with managed bridge {RuntimeIdentity.RequiredBridgeVersion}.");
        }

        RegisterBridge(api);
        var loaded = new NativeApi(api, version);
        NativeCallbacks.Register(loaded);
        return loaded;
    }

    internal static void RegisterBridge(BamlApiV1* api)
    {
        byte[] toolchainVersion = Encoding.UTF8.GetBytes(RuntimeIdentity.ToolchainVersion);
        byte[] runtimeName = Encoding.UTF8.GetBytes(RuntimeIdentity.RuntimeName);
        byte[] runtimeVersion = Encoding.UTF8.GetBytes(RuntimeIdentity.BridgeRuntimeVersion);
        fixed (byte* toolchainVersionPointer = toolchainVersion)
        fixed (byte* runtimeNamePointer = runtimeName)
        fixed (byte* runtimeVersionPointer = runtimeVersion)
        {
            BamlBridgeInfoV1 info = new()
            {
                StructSize = (nuint)sizeof(BamlBridgeInfoV1),
                Language = CSharpBridgeLanguage,
                SdkVersion = toolchainVersionPointer,
                SdkVersionLength = (nuint)toolchainVersion.Length,
                BridgeRuntimeName = runtimeNamePointer,
                BridgeRuntimeNameLength = (nuint)runtimeName.Length,
                BridgeRuntimeVersion = runtimeVersionPointer,
                BridgeRuntimeVersionLength = (nuint)runtimeVersion.Length,
            };
            string diagnostic = NativeBuffer.ReadUtf8AndFree(api, api->RegisterBridge(&info));
            if (diagnostic.Length != 0)
            {
                throw new BamlVersionMismatchException(diagnostic);
            }
        }
    }

    private static void Require(bool condition, string field)
    {
        if (!condition)
        {
            throw new BamlNativeLibraryLoadException($"BamlApiV1.{field} is null.");
        }
    }

    private sealed class CallCancellation(
        NativeApi api,
        ulong callId,
        uint callbackId,
        CancellationToken cancellationToken)
    {
        private readonly Lock gate = new();
        private bool started;

        internal bool Dispatch(byte* arguments, nuint argumentsLength)
        {
            lock (gate)
            {
                if (!NativeCallbacks.IsPending(callbackId))
                {
                    return false;
                }

                started = true;
                api.table->CallFunction(
                    arguments,
                    argumentsLength,
                    callbackId);
                return true;
            }
        }

        internal void Cancel()
        {
            lock (gate)
            {
                if (!NativeCallbacks.TryCancel(callbackId, cancellationToken))
                {
                    return;
                }

                if (started)
                {
                    _ = api.table->CancelFunctionCall(callId);
                }
            }
        }
    }
}

internal readonly record struct NativeFunctionCall(
    ulong FunctionCallId,
    Task<byte[]> Completion);

internal static class NativeCallCompletion
{
    internal static async Task<byte[]> CompleteManagedOperationAsync(
        NativeFunctionCall call)
    {
        try
        {
            return await call.Completion.ConfigureAwait(false);
        }
        finally
        {
            HostValueRegistry.Shared.CompleteFunctionCall(call.FunctionCallId);
        }
    }

    internal static async Task<byte[]> CompleteAsync(
        Task<byte[]> completion,
        CancellationTokenRegistration registration)
    {
        try
        {
            return await completion.ConfigureAwait(false);
        }
        finally
        {
            await registration.DisposeAsync().ConfigureAwait(false);
        }
    }
}
