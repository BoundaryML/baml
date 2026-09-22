using System.ComponentModel;

using Baml.Cffi;
using Baml.Proto;

namespace Baml.Generated.V1;

public static partial class BamlGeneratedContract
{
    [EditorBrowsable(EditorBrowsableState.Never)]
    public static global::Baml.BamlStream<T> CreateStream<T>(
        Lazy<BamlGeneratedProgram> program,
        BamlGeneratedFunction<T> function,
        BamlGeneratedArguments<T> arguments,
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
            arguments.Registry.Owner,
            arguments.Function,
            partialOptionName);
        BamlGeneratedArguments<T> snapshot =
            arguments.SnapshotForDeferredCall(out IDisposable ownership);
        try
        {
            var driver = new NativeBamlStreamDriver<T>(
                program,
                (activeProgram, token) => activeProgram.StartStreamAsync(
                    function,
                    snapshot,
                    token),
                ownership,
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
    public static global::Baml.BamlStream<T> CreateStream<T>(
        Lazy<BamlGeneratedProgram> program,
        BamlGeneratedBoundFunction<T> function,
        BamlGeneratedGenericArguments<T> arguments,
        string partialOptionName,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(program);
        ArgumentNullException.ThrowIfNull(arguments);
        BoundGenericFunctionDeclaration<T> declaration = function.Declaration;
        RequireStreamTokens(
            function.Owner,
            declaration.Definition.Variant,
            declaration.Result,
            declaration.Definition,
            declaration.Result,
            arguments.Registry.Owner,
            arguments.Function.Definition,
            partialOptionName);
        if (!ReferenceEquals(arguments.Function, declaration))
        {
            throw new InvalidOperationException(
                "The generated stream arguments belong to another generic binding.");
        }

        BamlGeneratedGenericArguments<T> snapshot =
            arguments.SnapshotForDeferredCall(out IDisposable ownership);
        try
        {
            var driver = new NativeBamlStreamDriver<T>(
                program,
                (activeProgram, token) => activeProgram.StartStreamAsync(
                    function,
                    snapshot,
                    token),
                ownership,
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

    private static void RequireStreamTokens<T>(
        RegistryOwner functionOwner,
        string variant,
        TypeDeclaration declaredResult,
        object function,
        TypeDeclaration<T> streamType,
        RegistryOwner argumentOwner,
        object argumentFunction,
        string partialOptionName)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(partialOptionName);
        if (!StringComparer.Ordinal.Equals(variant, "stream")
            || !ReferenceEquals(declaredResult, streamType)
            || !ReferenceEquals(functionOwner, argumentOwner)
            || !ReferenceEquals(function, argumentFunction)
            || streamType.Metadata.Length == 0)
        {
            throw new InvalidOperationException(
                "The generated stream function, type, and arguments do not share one valid registry provenance.");
        }
    }
}

/// <summary>
/// Drives one <c>ai.stream.Stream&lt;T&gt;</c>. A partial and the settled value share
/// the one type: a partial is <c>T</c> parsed from the text received so far.
/// </summary>
internal sealed class NativeBamlStreamDriver<T>
    : IBamlStreamDriver<T>
{
    private readonly Lazy<BamlGeneratedProgram> deferredProgram;
    private readonly Func<BamlGeneratedProgram, CancellationToken, Task<BamlStreamNativeHandle>> start;
    private readonly TypeDeclaration<T> streamType;
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
        TypeDeclaration<T> streamType,
        string partialOptionName,
        string streamFunctionIdentity)
    {
        ArgumentNullException.ThrowIfNull(deferredProgram);
        ArgumentNullException.ThrowIfNull(start);
        ArgumentNullException.ThrowIfNull(argumentOwnership);
        ArgumentNullException.ThrowIfNull(streamType);
        ArgumentException.ThrowIfNullOrWhiteSpace(partialOptionName);
        ArgumentException.ThrowIfNullOrWhiteSpace(streamFunctionIdentity);
        this.deferredProgram = deferredProgram;
        this.start = start;
        this.argumentOwnership = argumentOwnership;
        this.streamType = streamType;
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

    public async Task<BamlStreamPull<T>> PullAsync(
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
            streamType.Metadata,
            partialOptionName,
            nextIdentity,
            activeProgram.NativeState.Api);
        return wire.HasPartial
            ? BamlStreamPull<T>.FromPartial(
                activeProgram.Registry.Decode(streamType, wire.Partial))
            : BamlStreamPull<T>.Finished;
    }

    public async Task<T> GetFinalResponseAsync(
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
        return activeProgram.Registry.Decode(streamType, value);
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
