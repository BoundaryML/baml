using System.Runtime.ExceptionServices;

using Baml.Generated.V1;
using Baml.Proto;
using BamlBridge.Cffi.V1;
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
            BamlGeneratedValue generated;
            try
            {
                generated = invocation.Callable.Descriptor.Result.Encode(result);
            }
            catch (BamlTypeMappingException doesNotFit)
            {
                // The callable returned a value that does not fit the return
                // type it was bound to: its broken contract, a panic the
                // engine builds.
                Complete(api, invocation.HostCallId, new HostCallCompletion
                {
                    ContractViolation =
                        $"the callable's return value does not inhabit its declared type: {doesNotFit.Message}",
                });
                return;
            }
            using EncodedCallArguments encoded = PrimitiveProtocol.EncodeOwnedValue(
                generated,
                api,
                invocation.FunctionCallId);
            Complete(api, invocation.HostCallId, new HostCallCompletion
            {
                Returned = InboundValue.Parser.ParseFrom(encoded.Bytes),
            });
            encoded.Commit();
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
        BamlInvocationCapture? previousFrame = InvocationFrame.Current.Value;
        try
        {
            SynchronizationContext.SetSynchronizationContext(null);
            InvocationFrame.Current.Value = state.Invocation.Frame;
            state.Invocation.CancellationToken.ThrowIfCancellationRequested();
            state.Completion = state.Invocation.Callable.Descriptor.Invoke(
                state.Invocation.Callable.Callback,
                state.Arguments,
                state.Invocation.CancellationToken);
        }
        finally
        {
            InvocationFrame.Current.Value = previousFrame;
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

        // The host's own exception: the engine builds the
        // `baml.errors.HostCallable` whose `_handle` is this registration, so
        // the same process restores the exception by identity on round trip.
        using HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterException(error, functionCallId);
        Complete(api, hostCallId, new HostCallCompletion
        {
            Raised = HostCallableProtocol.Raised(error.SourceException, registration.Key),
        });
        registration.Commit();
    }

    private static unsafe void Complete(
        NativeApi api,
        uint hostCallId,
        HostCallCompletion completion)
    {
        byte[] bytes = completion.ToByteArray();
        fixed (byte* pointer = bytes)
        {
            api.Table->CompleteHostCall(hostCallId, pointer, (nuint)bytes.Length);
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
