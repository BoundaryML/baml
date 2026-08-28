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
                nativeState.Api,
                Operation(declaration.Variant)),
            cancellationToken);
        return DecodeResultAsync(
            function.Result,
            declaration.Identity,
            result,
            cancellationToken);
    }

    internal Task<BamlStreamNativeHandle> StartStreamAsync<TPartial, TFinal>(
        BamlGeneratedFunction<TFinal> function,
        BamlGeneratedArguments<TFinal> arguments,
        BamlGeneratedType<TPartial> partialType,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        FunctionDeclaration declaration = registry.RequireFunction(function);
        TypeDeclaration<TPartial> partial = registry.RequireType(partialType);
        if (!StringComparer.Ordinal.Equals(declaration.Variant, "stream")
            || !ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated stream arguments or function token do not belong to this program.");
        }

        return StartStreamAsync(
            declaration.Identity,
            partial,
            function.Result,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api,
                Operation(declaration.Variant)),
            cancellationToken);
    }

    internal Task<BamlStreamNativeHandle> StartStreamAsync<TPartial, TFinal>(
        BamlGeneratedBoundFunction<TFinal> function,
        BamlGeneratedGenericArguments<TFinal> arguments,
        BamlGeneratedType<TPartial> partialType,
        CancellationToken cancellationToken)
    {
        ArgumentNullException.ThrowIfNull(arguments);
        BoundGenericFunctionDeclaration<TFinal> declaration =
            registry.RequireBoundFunction(function);
        TypeDeclaration<TPartial> partial = registry.RequireType(partialType);
        if (!StringComparer.Ordinal.Equals(declaration.Definition.Variant, "stream")
            || !ReferenceEquals(arguments.Registry, registry)
            || !ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated generic stream arguments or function token do not belong to this program.");
        }

        return StartStreamAsync(
            declaration.Definition.Identity,
            partial,
            declaration.Result,
            callId => PrimitiveProtocol.EncodeOwnedCallArguments(
                arguments,
                callId,
                nativeState.Api,
                Operation(declaration.Definition.Variant)),
            cancellationToken);
    }

    private async Task<BamlStreamNativeHandle> StartStreamAsync<TPartial, TFinal>(
        string functionIdentity,
        TypeDeclaration<TPartial> partialType,
        TypeDeclaration<TFinal> finalType,
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
            partialType.Metadata,
            finalType.Metadata,
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
                nativeState.Api,
                Operation(declaration.Definition.Variant)),
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

    private static FunctionOperation Operation(string variant) => variant switch
    {
        "direct" or "call" => FunctionOperation.Direct,
        "spec" => FunctionOperation.Spec,
        "stream" => FunctionOperation.Stream,
        _ => throw new InvalidOperationException(
            $"Generated function token has unsupported semantic operation {variant}."),
    };
}
