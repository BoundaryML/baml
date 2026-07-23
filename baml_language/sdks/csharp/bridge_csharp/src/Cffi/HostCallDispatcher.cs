using System.Runtime.ExceptionServices;

using Baml.Generated.V1;
using Baml.Proto;
using Google.Protobuf;

namespace Baml.Cffi;

internal static class HostCallDispatcher
{
    internal static async Task ExecuteAsync(NativeApi api, HostInvocation invocation)
    {
        ArgumentNullException.ThrowIfNull(api);
        ArgumentNullException.ThrowIfNull(invocation);
        try
        {
            IReadOnlyList<object?> arguments = HostCallableProtocol.BindArguments(
                invocation.Callable.Descriptor,
                invocation.Arguments);
            Task<object?> completion = StartInCapturedContext(invocation, arguments);
            object? result = await completion.ConfigureAwait(false);
            if (invocation.CancellationToken.IsCancellationRequested)
            {
                return;
            }

            BamlGeneratedValue generated = invocation.Callable.Descriptor.Result.Encode(result);
            using EncodedCallArguments encoded = PrimitiveProtocol.EncodeOwnedValue(
                generated,
                api,
                invocation.FunctionCallId);
            Complete(api, invocation.HostCallId, isError: false, encoded.Bytes);
            encoded.Commit();
        }
        catch (OperationCanceledException error)
            when (invocation.CancellationToken.IsCancellationRequested
                && error.CancellationToken == invocation.CancellationToken)
        {
            // The parent function-call cancellation abandons its native host
            // invocation, so Canary has no live V1 completion to receive.
        }
        catch (Exception error)
        {
            CompleteException(
                api,
                invocation.HostCallId,
                invocation.FunctionCallId,
                ExceptionDispatchInfo.Capture(error));
        }
        finally
        {
            invocation.Complete();
        }
    }

    internal static void QueueBoundaryException(
        NativeApi api,
        uint hostCallId,
        ulong functionCallId,
        Exception error)
    {
        ArgumentNullException.ThrowIfNull(api);
        ArgumentNullException.ThrowIfNull(error);
        var work = new BoundaryWork(
            api,
            hostCallId,
            functionCallId,
            ExceptionDispatchInfo.Capture(error));
        ThreadPool.UnsafeQueueUserWorkItem(
            static state => CompleteException(
                state.Api,
                state.HostCallId,
                state.FunctionCallId,
                state.Error),
            work,
            preferLocal: false);
    }

    private static Task<object?> StartInCapturedContext(
        HostInvocation invocation,
        IReadOnlyList<object?> arguments)
    {
        var state = new InvocationStart(invocation, arguments);
        if (invocation.ExecutionContext is null)
        {
            StartWithoutSynchronizationContext(state);
        }
        else
        {
            ExecutionContext.Run(
                invocation.ExecutionContext.CreateCopy(),
                static value => StartWithoutSynchronizationContext((InvocationStart)value!),
                state);
        }

        return state.Completion
            ?? throw new BamlProtocolException(
                "A generated host callback did not produce an asynchronous completion.",
                "The generated host invoker returned a null Task.");
    }

    private static void StartWithoutSynchronizationContext(InvocationStart state)
    {
        SynchronizationContext? previous = SynchronizationContext.Current;
        try
        {
            SynchronizationContext.SetSynchronizationContext(null);
            state.Completion = state.Invocation.Callable.Descriptor.Invoke(
                state.Invocation.Callable.Callback,
                state.Arguments,
                state.Invocation.CancellationToken);
        }
        finally
        {
            SynchronizationContext.SetSynchronizationContext(previous);
        }
    }

    private static void CompleteException(
        NativeApi api,
        uint hostCallId,
        ulong functionCallId,
        ExceptionDispatchInfo error)
    {
        if (hostCallId == 0)
        {
            return;
        }

        using HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterException(error, functionCallId);
        byte[] bytes = HostCallableProtocol.EncodeException(error.SourceException, registration.Key);
        Complete(api, hostCallId, isError: true, bytes);
        registration.Commit();
    }

    private static unsafe void Complete(
        NativeApi api,
        uint hostCallId,
        bool isError,
        byte[] bytes)
    {
        fixed (byte* pointer = bytes)
        {
            api.Table->CompleteHostCall(
                hostCallId,
                isError ? 1 : 0,
                pointer,
                (nuint)bytes.Length);
        }
    }

    private sealed class InvocationStart(
        HostInvocation invocation,
        IReadOnlyList<object?> arguments)
    {
        internal HostInvocation Invocation { get; } = invocation;

        internal IReadOnlyList<object?> Arguments { get; } = arguments;

        internal Task<object?>? Completion { get; set; }
    }

    private sealed record BoundaryWork(
        NativeApi Api,
        uint HostCallId,
        ulong FunctionCallId,
        ExceptionDispatchInfo Error);
}
