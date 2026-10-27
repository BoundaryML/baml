using System.Runtime.ExceptionServices;

using Baml.Generated.V1;
using Baml.Proto;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Cffi;

/// <summary>
/// Runs a host callback and completes its call with exactly one outcome.
/// </summary>
/// <remarks>
/// Each fault is completed by whose it is:
/// <list type="bullet">
/// <item>the callback returned: <c>returned</c>, its value encoded against the
/// declared return type; a value that type does not hold is the callable's
/// <c>contract_violation</c>.</item>
/// <item>the callback threw a BAML error (<see cref="BamlErrorException"/>, its
/// own or one a BAML call it made let through): <c>thrown</c>, restated under
/// the declared throws type; a value that type does not hold is a
/// <c>contract_violation</c>.</item>
/// <item>the callback let a BAML panic through (<see cref="BamlPanicException"/>,
/// or the engine's cancellation): <c>thrown</c> as the value it is, a panic
/// whatever the callable declares.</item>
/// <item>any other exception is the host's own: <c>raised</c>, and the engine
/// builds the <c>baml.errors.HostCallable</c> around it.</item>
/// <item>everything the bridge itself could not do (decode or bind the
/// arguments, start a callback whose call was already canceled, clone a handle,
/// carry a value deeper or larger than it or the wire can) is a
/// <c>bridge_failure</c>.</item>
/// </list>
/// </remarks>
internal static class HostCallDispatcher
{
    internal static async Task ExecuteAsync(NativeApi api, HostInvocation invocation)
    {
        ArgumentNullException.ThrowIfNull(api);
        ArgumentNullException.ThrowIfNull(invocation);
        try
        {
            IReadOnlyList<object?> arguments;
            try
            {
                arguments = HostCallableProtocol.BindArguments(
                    invocation.Callable.Descriptor,
                    invocation.Arguments);
            }
            catch (Exception bindFailure)
            {
                CompleteBridgeFailure(
                    api,
                    invocation.HostCallId,
                    $"the callback's arguments could not be bound: {bindFailure.Message}");
                return;
            }
            finally
            {
                // The bound arguments own what they kept; release the rest
                // now rather than when the callback completes.
                invocation.Arguments.Dispose();
            }

            if (invocation.CancellationToken.IsCancellationRequested)
            {
                // The engine already canceled the call and drops whatever
                // completes it.
                CompleteBridgeFailure(
                    api,
                    invocation.HostCallId,
                    "the callback was invoked after its call was canceled");
                return;
            }

            object? result;
            try
            {
                result = await StartInCapturedContext(invocation, arguments).ConfigureAwait(false);
            }
            catch (Exception fault)
            {
                CompleteFault(api, invocation, ExceptionDispatchInfo.Capture(fault));
                return;
            }

            CompleteReturned(api, invocation, result);
        }
        catch (Exception bridgeFault)
        {
            // Completing the call itself failed; the call must still complete.
            CompleteBridgeFailure(
                api,
                invocation.HostCallId,
                $"the bridge failed while completing the callback: {bridgeFault.Message}");
        }
        finally
        {
            invocation.Complete();
        }
    }

    /// <summary>
    /// A dispatch the bridge could not start (an unparsable invocation, a stale
    /// or unknown callable key, a control handle it could not clone) completes
    /// as the bridge's own failure.
    /// </summary>
    internal static void QueueBoundaryFailure(
        NativeApi api,
        uint hostCallId,
        Exception error)
    {
        ArgumentNullException.ThrowIfNull(api);
        ArgumentNullException.ThrowIfNull(error);
        var work = new BoundaryWork(api, hostCallId, error.Message);
        ThreadPool.UnsafeQueueUserWorkItem(
            static state => CompleteBridgeFailure(
                state.Api,
                state.HostCallId,
                $"the bridge could not start the callback: {state.Diagnostic}"),
            work,
            preferLocal: false);
    }

    private static void CompleteReturned(NativeApi api, HostInvocation invocation, object? result)
    {
        BamlGeneratedValue generated;
        try
        {
            generated = invocation.Callable.Descriptor.Result.Encode(result);
        }
        catch (BamlTypeMappingException tooLarge)
            when (tooLarge.Fault == BamlTypeMappingFault.ExceedsLimit)
        {
            CompleteBridgeFailure(
                api,
                invocation.HostCallId,
                $"the callable's return value is too large to carry: {tooLarge.Message}");
            return;
        }
        catch (Exception doesNotFit) when (IsRefusalOfTheValue(doesNotFit))
        {
            // The callable returned a value that does not fit the return type
            // it was bound to: its broken contract, a panic the engine builds.
            CompleteContractViolation(
                api,
                invocation.HostCallId,
                $"the callable's return value does not inhabit its declared type: {doesNotFit.Message}");
            return;
        }

        CompleteValue(api, invocation, generated, Outcome.Returned);
    }

    private static void CompleteFault(
        NativeApi api,
        HostInvocation invocation,
        ExceptionDispatchInfo fault)
    {
        switch (fault.SourceException)
        {
            case BamlErrorException error:
                BamlGeneratedValue restated;
                try
                {
                    restated = invocation.Callable.Descriptor.Throws.Restate(
                        error.ThrownValue.GeneratedValue);
                }
                catch (BamlTypeMappingException tooLarge)
                    when (tooLarge.Fault == BamlTypeMappingFault.ExceedsLimit)
                {
                    CompleteBridgeFailure(
                        api,
                        invocation.HostCallId,
                        $"the callable's thrown error is too large to carry: {tooLarge.Message}");
                    return;
                }
                catch (Exception doesNotFit) when (IsRefusalOfTheValue(doesNotFit))
                {
                    CompleteContractViolation(
                        api,
                        invocation.HostCallId,
                        $"the callable threw a BAML error its declared throws type does not hold: {doesNotFit.Message}");
                    return;
                }

                CompleteValue(api, invocation, restated, Outcome.Thrown);
                return;
            case BamlPanicException panic:
                // A panic is a panic whatever the callable declares: it is
                // never held to the throws type, so it crosses as it is.
                CompleteValue(api, invocation, panic.Panic.Value.GeneratedValue, Outcome.Thrown);
                return;
            case BamlOperationCanceledException { PanicValue: { } cancelled }:
                CompleteValue(api, invocation, cancelled.GeneratedValue, Outcome.Thrown);
                return;
            default:
                CompleteRaised(api, invocation, fault);
                return;
        }
    }

    /// <summary>
    /// Whether <paramref name="error"/>, raised while a codec of the declared
    /// type read or wrote the callable's value, is that codec refusing the
    /// value: a CLR value of another shape, a <c>null</c> where none is
    /// allowed, a non-finite float or an integer out of range.
    /// </summary>
    private static bool IsRefusalOfTheValue(Exception error) =>
        error is BamlTypeMappingException
            or BamlProtocolException
            or ArgumentException
            or InvalidCastException
            or NullReferenceException;

    private static void CompleteValue(
        NativeApi api,
        HostInvocation invocation,
        BamlGeneratedValue value,
        Outcome outcome)
    {
        InboundValue encoded;
        EncodedCallArguments ownership;
        try
        {
            (encoded, ownership) = PrimitiveProtocol.EncodeOwnedResult(
                value,
                api,
                invocation.FunctionCallId);
        }
        catch (Exception cannotCarry)
        {
            // The value fits its type; the bridge could not carry it: too deep
            // for the wire, or a handle it could not clone. Every clone taken
            // so far is released.
            CompleteBridgeFailure(
                api,
                invocation.HostCallId,
                $"the bridge could not carry the callable's {Describe(outcome)} value: {cannotCarry.Message}");
            return;
        }

        using (ownership)
        {
            Complete(api, invocation.HostCallId, outcome switch
            {
                Outcome.Returned => new HostCallCompletion { Returned = encoded },
                Outcome.Thrown => new HostCallCompletion { Thrown = encoded },
                _ => throw new InvalidOperationException($"Unknown host call outcome {outcome}."),
            });
            ownership.Commit();
        }
    }

    private static void CompleteRaised(
        NativeApi api,
        HostInvocation invocation,
        ExceptionDispatchInfo error)
    {
        // The host's own exception: the engine builds the
        // `baml.errors.HostCallable` whose `_handle` is this registration, so
        // the same process restores the exception by identity on round trip.
        using HostValueRegistration registration =
            HostValueRegistry.Shared.RegisterException(error, invocation.FunctionCallId);
        Complete(api, invocation.HostCallId, new HostCallCompletion
        {
            Raised = HostCallableProtocol.Raised(error.SourceException, registration.Key),
        });
        registration.Commit();
    }

    private static void CompleteContractViolation(NativeApi api, uint hostCallId, string diagnostic) =>
        Complete(api, hostCallId, new HostCallCompletion { ContractViolation = diagnostic });

    private static void CompleteBridgeFailure(NativeApi api, uint hostCallId, string diagnostic)
    {
        if (hostCallId == 0)
        {
            // No call names this dispatch, so nothing can complete it.
            return;
        }

        Complete(api, hostCallId, new HostCallCompletion { BridgeFailure = diagnostic });
    }

    private static string Describe(Outcome outcome) => outcome switch
    {
        Outcome.Returned => "returned",
        Outcome.Thrown => "thrown",
        _ => throw new InvalidOperationException($"Unknown host call outcome {outcome}."),
    };

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

    private enum Outcome
    {
        Returned,
        Thrown,
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
        string Diagnostic);
}
