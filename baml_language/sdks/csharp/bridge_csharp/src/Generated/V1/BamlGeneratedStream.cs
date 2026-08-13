using System.ComponentModel;

using Baml.Cffi;
using Baml.Proto;

namespace Baml.Generated.V1;

public static partial class BamlGeneratedContract
{
    [EditorBrowsable(EditorBrowsableState.Never)]
    public static global::Baml.BamlStream<TPartial, TFinal> CreateStream<TPartial, TFinal>(
        Lazy<BamlGeneratedProgram> program,
        BamlGeneratedFunction<TFinal> function,
        BamlGeneratedType<TPartial> partialType,
        BamlGeneratedArguments<TFinal> arguments,
        string partialOptionName,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(program);
        ArgumentNullException.ThrowIfNull(arguments);
        RequireStreamTokens(
            function.Owner,
            function.Declaration.Variant,
            function.Declaration.Result,
            function.Declaration,
            function.Result,
            partialType.Owner,
            partialType.Declaration,
            arguments.Registry.Owner,
            arguments.Function,
            partialOptionName);
        BamlGeneratedArguments<TFinal> snapshot =
            arguments.SnapshotForDeferredCall(out IDisposable ownership);
        try
        {
            var driver = new NativeBamlStreamDriver<TPartial, TFinal>(
                program,
                (activeProgram, token) => activeProgram.StartStreamAsync(
                    function,
                    snapshot,
                    partialType,
                    token),
                ownership,
                partialType.Declaration,
                function.Result,
                partialOptionName,
                function.Declaration.Identity);
            return BamlStreamFactory.Create(
                driver,
                function.Declaration.Identity,
                cancellationToken);
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    [EditorBrowsable(EditorBrowsableState.Never)]
    public static global::Baml.BamlStream<TPartial, TFinal> CreateStream<TPartial, TFinal>(
        Lazy<BamlGeneratedProgram> program,
        BamlGeneratedBoundFunction<TFinal> function,
        BamlGeneratedType<TPartial> partialType,
        BamlGeneratedGenericArguments<TFinal> arguments,
        string partialOptionName,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(program);
        ArgumentNullException.ThrowIfNull(arguments);
        BoundGenericFunctionDeclaration<TFinal> declaration = function.Declaration;
        RequireStreamTokens(
            function.Owner,
            declaration.Definition.Variant,
            declaration.Result,
            declaration.Definition,
            declaration.Result,
            partialType.Owner,
            partialType.Declaration,
            arguments.Registry.Owner,
            arguments.Function.Definition,
            partialOptionName);
        if (!ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated stream arguments belong to another generic binding.");
        }

        BamlGeneratedGenericArguments<TFinal> snapshot =
            arguments.SnapshotForDeferredCall(out IDisposable ownership);
        try
        {
            var driver = new NativeBamlStreamDriver<TPartial, TFinal>(
                program,
                (activeProgram, token) => activeProgram.StartStreamAsync(
                    function,
                    snapshot,
                    partialType,
                    token),
                ownership,
                partialType.Declaration,
                declaration.Result,
                partialOptionName,
                declaration.Definition.Identity);
            return BamlStreamFactory.Create(
                driver,
                declaration.Definition.Identity,
                cancellationToken);
        }
        catch
        {
            ownership.Dispose();
            throw;
        }
    }

    private static void RequireStreamTokens<TPartial, TFinal>(
        RegistryOwner functionOwner,
        string variant,
        TypeDeclaration declaredResult,
        object function,
        TypeDeclaration<TFinal> finalType,
        RegistryOwner partialOwner,
        TypeDeclaration<TPartial> partialType,
        RegistryOwner argumentOwner,
        object argumentFunction,
        string partialOptionName)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(partialOptionName);
        if (!StringComparer.Ordinal.Equals(variant, "stream")
            || !ReferenceEquals(declaredResult, finalType)
            || !ReferenceEquals(functionOwner, partialOwner)
            || !ReferenceEquals(functionOwner, argumentOwner)
            || !ReferenceEquals(function, argumentFunction)
            || partialType.Metadata.Length == 0
            || finalType.Metadata.Length == 0)
        {
            throw new InvalidOperationException(
                "The generated stream function, type, and arguments do not share one valid registry provenance.");
        }
    }
}

internal sealed class NativeBamlStreamDriver<TPartial, TFinal>
    : IBamlStreamDriver<TPartial, TFinal>
{
    private readonly Lazy<BamlGeneratedProgram> deferredProgram;
    private readonly Func<BamlGeneratedProgram, CancellationToken, Task<BamlStreamNativeHandle>> start;
    private readonly TypeDeclaration<TPartial> partialType;
    private readonly TypeDeclaration<TFinal> finalType;
    private readonly string partialOptionName;
    private readonly string streamFunctionIdentity;
    private IDisposable? argumentOwnership;
    private BamlGeneratedProgram? program;
    private BamlStreamNativeHandle? stream;
    private bool started;
    private bool disposed;

    internal NativeBamlStreamDriver(
        Lazy<BamlGeneratedProgram> deferredProgram,
        Func<BamlGeneratedProgram, CancellationToken, Task<BamlStreamNativeHandle>> start,
        IDisposable argumentOwnership,
        TypeDeclaration<TPartial> partialType,
        TypeDeclaration<TFinal> finalType,
        string partialOptionName,
        string streamFunctionIdentity)
    {
        ArgumentNullException.ThrowIfNull(deferredProgram);
        ArgumentNullException.ThrowIfNull(start);
        ArgumentNullException.ThrowIfNull(argumentOwnership);
        ArgumentNullException.ThrowIfNull(partialType);
        ArgumentNullException.ThrowIfNull(finalType);
        ArgumentException.ThrowIfNullOrWhiteSpace(partialOptionName);
        ArgumentException.ThrowIfNullOrWhiteSpace(streamFunctionIdentity);
        this.deferredProgram = deferredProgram;
        this.start = start;
        this.argumentOwnership = argumentOwnership;
        this.partialType = partialType;
        this.finalType = finalType;
        this.partialOptionName = partialOptionName;
        this.streamFunctionIdentity = streamFunctionIdentity;
    }

    public async Task StartAsync(CancellationToken cancellationToken)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (started)
        {
            throw new InvalidOperationException("The native BAML stream driver was started twice.");
        }

        cancellationToken.ThrowIfCancellationRequested();
        BamlGeneratedProgram activeProgram = deferredProgram.Value;
        try
        {
            stream = await start(activeProgram, cancellationToken).ConfigureAwait(false);
            program = activeProgram;
            started = true;
        }
        finally
        {
            Interlocked.Exchange(ref argumentOwnership, null)?.Dispose();
        }
    }

    public async Task<BamlStreamPull<TPartial>> PullAsync(
        CancellationToken cancellationToken)
    {
        (BamlGeneratedProgram activeProgram, BamlStreamNativeHandle activeStream) = RequireStarted();
        string nextIdentity = $"{activeStream.ClassIdentity}.next";
        Task<byte[]> completion = activeProgram.NativeState.Api.InvokeOwnedFunctionAsync(
            nextIdentity,
            callId => PrimitiveProtocol.EncodeStreamHandleArguments(activeStream.Handle, callId),
            cancellationToken);
        byte[] bytes = await completion.ConfigureAwait(false);
        BamlStreamPull<BamlGeneratedValue> wire = PrimitiveProtocol.DecodeStreamPull(
            bytes,
            partialType.Metadata,
            partialOptionName,
            nextIdentity,
            activeProgram.NativeState.Api);
        return wire.HasPartial
            ? BamlStreamPull<TPartial>.FromPartial(
                activeProgram.Registry.Decode(partialType, wire.Partial))
            : BamlStreamPull<TPartial>.Finished;
    }

    public async Task<TFinal> GetFinalResponseAsync(
        CancellationToken cancellationToken)
    {
        (BamlGeneratedProgram activeProgram, BamlStreamNativeHandle activeStream) = RequireStarted();
        string finalIdentity = $"{activeStream.ClassIdentity}.final";
        Task<byte[]> completion = activeProgram.NativeState.Api.InvokeOwnedFunctionAsync(
            finalIdentity,
            callId => PrimitiveProtocol.EncodeStreamHandleArguments(activeStream.Handle, callId),
            cancellationToken);
        byte[] bytes = await completion.ConfigureAwait(false);
        BamlGeneratedValue value = PrimitiveProtocol.DecodeCallResult(
            bytes,
            streamFunctionIdentity,
            activeProgram.NativeState.Api);
        return activeProgram.Registry.Decode(finalType, value);
    }

    public ValueTask DisposeAsync()
    {
        if (disposed)
        {
            return ValueTask.CompletedTask;
        }

        disposed = true;
        stream?.Dispose();
        Interlocked.Exchange(ref argumentOwnership, null)?.Dispose();
        return ValueTask.CompletedTask;
    }

    private (BamlGeneratedProgram Program, BamlStreamNativeHandle Stream) RequireStarted()
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!started || program is null || stream is null)
        {
            throw new InvalidOperationException("The native BAML stream driver has not started.");
        }

        return (program, stream);
    }
}
