using System.ComponentModel;

using Baml.Cffi;
using Baml.Proto;
using Baml.Runtime;
using BamlBridge.Cffi.V1;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedProgram
{
    private readonly BamlGeneratedRegistry registry;
    private readonly ProgramNativeState nativeState;

    internal BamlGeneratedProgram(
        BamlGeneratedRegistry registry,
        ProgramNativeState nativeState)
    {
        this.registry = registry;
        this.nativeState = nativeState;
    }

    internal BamlGeneratedRegistry Registry => registry;

    internal ProgramNativeState NativeState => nativeState;

    public TResult Call<TResult>(
        BamlGeneratedFunction<TResult> function,
        BamlGeneratedArguments<TResult> arguments,
        CancellationToken cancellationToken = default) =>
        CallAsync(function, arguments, cancellationToken).GetAwaiter().GetResult();

    public Task<TResult> CallAsync<TResult>(
        BamlGeneratedFunction<TResult> function,
        BamlGeneratedArguments<TResult> arguments,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        FunctionDeclaration declaration = registry.RequireFunction(function);
        if (!ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated arguments belong to another function or registry.");
        }

        NativeFunctionCall result = nativeState.Api.StartOwnedFunction(
            declaration.Identity,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api),
            cancellationToken);
        return DecodeResultAsync(
            function.Result,
            declaration.Identity,
            result,
            cancellationToken);
    }

    internal Task<BamlStreamNativeHandle> StartStreamAsync<T>(
        BamlGeneratedFunction<T> function,
        BamlGeneratedArguments<T> arguments,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        FunctionDeclaration declaration = registry.RequireFunction(function);
        if (!StringComparer.Ordinal.Equals(declaration.Variant, "stream")
            || !ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated stream arguments or function token do not belong to this program.");
        }

        return StartStreamAsync(
            declaration.Identity,
            function.Result,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api),
            cancellationToken);
    }

    internal Task<BamlStreamNativeHandle> StartStreamAsync<T>(
        BamlGeneratedBoundFunction<T> function,
        BamlGeneratedGenericArguments<T> arguments,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        BoundGenericFunctionDeclaration<T> declaration =
            registry.RequireBoundFunction(function);
        if (!StringComparer.Ordinal.Equals(declaration.Definition.Variant, "stream")
            || !ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated generic stream arguments or function token do not belong to this program.");
        }

        return StartStreamAsync(
            declaration.Definition.Identity,
            declaration.Result,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api),
            cancellationToken);
    }

    private async Task<BamlStreamNativeHandle> StartStreamAsync<T>(
        string functionIdentity,
        TypeDeclaration<T> streamType,
        Func<ulong, EncodedCallArguments> encodeArguments,
        CancellationToken cancellationToken)
    {
        byte[] bytes = await nativeState.Api.InvokeOwnedFunctionAsync(
                functionIdentity,
                encodeArguments,
                cancellationToken)
            .ConfigureAwait(false);
        return PrimitiveProtocol.DecodeStreamHandle(
            bytes,
            streamType.Metadata,
            functionIdentity,
            nativeState.Api);
    }

    internal Task<BamlGeneratedValue> CallRuntimeMethodAsync(
        string functionIdentity,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> arguments,
        CancellationToken cancellationToken)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(functionIdentity);
        ArgumentNullException.ThrowIfNull(arguments);
        NativeFunctionCall call = nativeState.Api.StartOwnedFunction(
            functionIdentity,
            callId => PrimitiveProtocol.EncodeOwnedHandleArguments(
                arguments,
                callId,
                nativeState.Api),
            cancellationToken);
        return DecodeRuntimeMethodResultAsync(
            functionIdentity,
            call,
            cancellationToken);
    }

    public TResult Call<TResult>(
        BamlGeneratedBoundFunction<TResult> function,
        BamlGeneratedGenericArguments<TResult> arguments,
        CancellationToken cancellationToken = default) =>
        CallAsync(function, arguments, cancellationToken).GetAwaiter().GetResult();

    public Task<TResult> CallAsync<TResult>(
        BamlGeneratedBoundFunction<TResult> function,
        BamlGeneratedGenericArguments<TResult> arguments,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        BoundGenericFunctionDeclaration<TResult> declaration =
            registry.RequireBoundFunction(function);
        if (!ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated generic arguments belong to another function or registry.");
        }

        NativeFunctionCall result = nativeState.Api.StartOwnedFunction(
            declaration.Definition.Identity,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api),
            cancellationToken);
        return DecodeResultAsync(
            declaration.Result,
            declaration.Definition.Identity,
            result,
            cancellationToken);
    }

    private async Task<TResult> DecodeResultAsync<TResult>(
        TypeDeclaration<TResult> resultType,
        string functionIdentity,
        NativeFunctionCall call,
        CancellationToken cancellationToken)
    {
        try
        {
            byte[] bytes;
            try
            {
                bytes = await call.Completion.ConfigureAwait(false);
            }
            catch (OperationCanceledException error)
                when (cancellationToken.IsCancellationRequested
                    && error.CancellationToken == cancellationToken)
            {
                throw new BamlOperationCanceledException(
                    "The BAML call was canceled by the caller.",
                    BamlCancellationOrigin.Caller,
                    cancellationToken,
                    functionIdentity,
                    trace: null);
            }

            BamlGeneratedValue value = PrimitiveProtocol.DecodeCallResult(
                bytes,
                functionIdentity,
                nativeState.Api);
            return registry.Decode(resultType, value);
        }
        finally
        {
            HostValueRegistry.Shared.CompleteFunctionCall(call.FunctionCallId);
        }
    }

    private async Task<BamlGeneratedValue> DecodeRuntimeMethodResultAsync(
        string functionIdentity,
        NativeFunctionCall call,
        CancellationToken cancellationToken)
    {
        try
        {
            byte[] bytes;
            try
            {
                bytes = await call.Completion.ConfigureAwait(false);
            }
            catch (OperationCanceledException error)
                when (cancellationToken.IsCancellationRequested
                    && error.CancellationToken == cancellationToken)
            {
                throw new BamlOperationCanceledException(
                    "The BAML call was canceled by the caller.",
                    BamlCancellationOrigin.Caller,
                    cancellationToken,
                    functionIdentity,
                    trace: null);
            }

            return PrimitiveProtocol.DecodeCallResult(
                bytes,
                functionIdentity,
                nativeState.Api);
        }
        finally
        {
            HostValueRegistry.Shared.CompleteFunctionCall(call.FunctionCallId);
        }
    }

}
