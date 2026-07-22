using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

using Baml.Proto;

namespace Baml.Cffi;

internal static unsafe class NativeCallbacks
{
    private static readonly CallbackIdAllocator CallbackIds = new();
    private static readonly ConcurrentDictionary<uint, TaskCompletionSource<byte[]>> Pending = new();
    private static ExceptionDispatchInfo? callbackFailure;
    private static NativeApi? cleanupApi;
    private static int registered;
    private static long lateOrDuplicateResults;

    internal static long LateOrDuplicateResults => Volatile.Read(ref lateOrDuplicateResults);

    internal static int PendingCount => Pending.Count;

    internal static delegate* unmanaged[Cdecl]<uint, byte*, nuint, void> ResultPointer => &OnResult;

    internal static delegate* unmanaged[Cdecl]<ulong, uint, ulong, byte*, nuint, void>
        HostDispatchV2Pointer => &OnHostDispatchV2;

    internal static delegate* unmanaged[Cdecl]<uint, void> HostCancelPointer => &OnHostCancel;

    internal static delegate* unmanaged[Cdecl]<ulong, void> HostReleasePointer => &OnHostRelease;

    internal static void Register(NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(api);
        if (Interlocked.Exchange(ref registered, 1) != 0)
        {
            return;
        }

        cleanupApi = api;
        api.Table->RegisterCallback(&OnResult);
        api.Table->RegisterHostReleaseCallback(&OnHostRelease);
        api.Table->RegisterHostDispatchCallback(&OnHostDispatchV1);
        if (api.SupportsHostCallables)
        {
            api.Table->RegisterHostDispatchCallbackV2(&OnHostDispatchV2);
            api.Table->RegisterHostCancelCallback(&OnHostCancel);
        }
    }

    internal static (uint Id, Task<byte[]> Task) AddPending()
    {
        uint id = CallbackIds.Next();
        var completion = new TaskCompletionSource<byte[]>(
            TaskCreationOptions.RunContinuationsAsynchronously);
        if (!Pending.TryAdd(id, completion))
        {
            throw new BamlProtocolException(
                "A managed callback identifier was unexpectedly reused.",
                $"Callback identifier {id} already exists in the pending registry.");
        }

        return (id, completion.Task);
    }

    internal static bool TryDiscard(uint id) => Pending.TryRemove(id, out _);

    internal static bool TryCancel(uint id, CancellationToken cancellationToken) =>
        Pending.TryRemove(id, out TaskCompletionSource<byte[]>? completion)
        && completion.TrySetCanceled(cancellationToken);

    internal static bool IsPending(uint id) => Pending.ContainsKey(id);

    internal static void ThrowIfCallbackFailed() =>
        Volatile.Read(ref callbackFailure)?.Throw();

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnResult(uint callbackId, byte* content, nuint length)
    {
        TaskCompletionSource<byte[]>? completion = null;
        try
        {
            if (callbackId == 0)
            {
                throw new InvalidDataException("A native result callback supplied identifier zero.");
            }

            if (length > int.MaxValue || (length != 0 && content is null))
            {
                throw new InvalidDataException(
                    $"Native result callback {callbackId} supplied an invalid borrowed buffer.");
            }

            byte[] copy = length == 0
                ? []
                : new ReadOnlySpan<byte>(content, checked((int)length)).ToArray();
            if (!Pending.TryRemove(callbackId, out completion))
            {
                Interlocked.Increment(ref lateOrDuplicateResults);
                NativeApi? api = Volatile.Read(ref cleanupApi);
                if (api is not null)
                {
                    PrimitiveProtocol.ReleaseOwnedCallResult(copy, api);
                }
                return;
            }

            _ = completion.TrySetResult(copy);
        }
        catch (Exception error)
        {
            var protocolError = new BamlProtocolException(
                "The native bridge returned an invalid result callback.",
                error.Message);
            if (completion is not null)
            {
                _ = completion.TrySetException(protocolError);
            }

            Interlocked.CompareExchange(
                ref callbackFailure,
                ExceptionDispatchInfo.Capture(protocolError),
                null);

            foreach (uint id in Pending.Keys)
            {
                if (Pending.TryRemove(id, out TaskCompletionSource<byte[]>? removed))
                {
                    _ = removed.TrySetException(protocolError);
                }
            }
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostDispatchV1(
        ulong hostKey,
        uint hostCallId,
        byte* content,
        nuint length) =>
        OnHostDispatch(hostKey, hostCallId, functionCallId: 0, content, length);

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostDispatchV2(
        ulong hostKey,
        uint hostCallId,
        ulong functionCallId,
        byte* content,
        nuint length) =>
        OnHostDispatch(hostKey, hostCallId, functionCallId, content, length);

    private static void OnHostDispatch(
        ulong hostKey,
        uint hostCallId,
        ulong functionCallId,
        byte* content,
        nuint length)
    {
        NativeApi? api = Volatile.Read(ref cleanupApi);
        if (api is null)
        {
            return;
        }

        try
        {
            if (length > int.MaxValue || (length != 0 && content is null))
            {
                throw new InvalidDataException(
                    $"Native host dispatch {hostCallId} supplied an invalid borrowed buffer.");
            }

            byte[] copy = length == 0
                ? []
                : new ReadOnlySpan<byte>(content, checked((int)length)).ToArray();
            HostInvocation? invocation = HostValueRegistry.Shared.TryStartInvocation(
                hostKey,
                hostCallId,
                functionCallId,
                copy,
                out string? diagnostic);
            if (invocation is null)
            {
                throw new InvalidDataException(diagnostic);
            }

            ThreadPool.UnsafeQueueUserWorkItem(
                static state => _ = HostCallDispatcher.ExecuteAsync(state.Api, state.Invocation),
                new HostDispatchWork(api, invocation),
                preferLocal: false);
        }
        catch (Exception error)
        {
            HostCallDispatcher.QueueBoundaryException(
                api,
                hostCallId,
                functionCallId,
                error);
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostCancel(uint hostCallId)
    {
        try
        {
            HostValueRegistry.Shared.CancelInvocation(hostCallId);
        }
        catch
        {
            // No managed exception may unwind across the unmanaged callback boundary.
        }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostRelease(ulong hostKey)
    {
        try
        {
            HostValueRegistry.Shared.Release(hostKey);
        }
        catch
        {
            // No managed exception may unwind across the unmanaged callback boundary.
        }
    }

    private sealed record HostDispatchWork(NativeApi Api, HostInvocation Invocation);
}
