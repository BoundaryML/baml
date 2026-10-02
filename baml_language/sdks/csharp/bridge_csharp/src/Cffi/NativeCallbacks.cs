using System.Collections.Concurrent;
using System.Runtime.CompilerServices;
using System.Runtime.ExceptionServices;
using System.Runtime.InteropServices;

using Baml.Proto;
using Google.Protobuf;

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

    internal static delegate* unmanaged[Cdecl]<ulong, uint, byte*, nuint, void>
        HostDispatchPointer => &OnHostDispatchV1;

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
        api.Table->RegisterHostDispatchV2(&OnHostDispatchV2);
        api.Table->RegisterHostCancelCallback(&OnHostCancel);
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
    private static void OnHostCancel(uint callbackId)
    {
        try { HostValueRegistry.Shared.CancelInvocation(callbackId); } catch { }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostDispatchV2(byte* content, nuint length)
    {
        NativeApi? api = Volatile.Read(ref cleanupApi);
        if (api is null) return;
        global::BamlBridge.Cffi.V1.HostInvocation? wire = null;
        BamlSafeHandle? state = null;
        OutboundOwnershipScope? controls = null;
        HostInvocation? invocation = null;
        try
        {
            if (length > int.MaxValue || (length != 0 && content is null))
                throw new InvalidDataException("Invalid host invocation buffer.");
            wire = global::BamlBridge.Cffi.V1.HostInvocation.Parser.ParseFrom(new ReadOnlySpan<byte>(content, (int)length));
            state = api.OwnHandle(wire.EffectiveState);
            controls = OutboundOwnershipScope.Create(new global::BamlBridge.Cffi.V1.BamlOutboundResult { Ok = wire.Cancel }, api);
            if (wire.CallbackId == 0 || wire.HostValueKey == 0 || wire.Cancel is null)
                throw new InvalidDataException("Incomplete host invocation controls.");
            invocation = HostValueRegistry.Shared.TryStartInvocation(
                wire.HostValueKey, wire.CallbackId, wire.ApplicationArgs.ToByteArray(), out string? diagnostic,
                wire.HostEnvironment) ?? throw new InvalidDataException(diagnostic);
            invocation.EffectiveState = state;
            invocation.Controls = controls;
            state = null;
            controls = null;
            var ownedCancel = CloneControl(wire.Cancel, api);
            var cancelValue = PrimitiveProtocol.DecodeCallResult(new global::BamlBridge.Cffi.V1.BamlOutboundResult { Ok = ownedCancel }.ToByteArray(), "<effective cancellation>", api);
            invocation.Frame = new global::Baml.Generated.V1.BamlInvocationCapture(invocation.EffectiveState.CloneOwned(), cancelValue, invocation.CancellationToken);
            ThreadPool.UnsafeQueueUserWorkItem(
                static work => _ = HostCallDispatcher.ExecuteAsync(work.Api, work.Invocation),
                new HostDispatchWork(api, invocation), preferLocal: false);
        }
        catch (Exception error)
        {
            if (invocation is not null)
            {
                invocation.Frame?.State.Dispose();
                invocation.Complete();
            }
            state?.Dispose();
            controls?.Dispose();
            if (wire is not null) HostCallDispatcher.QueueBoundaryException(api, wire.CallbackId, wire.HostEnvironment, error);
        }
    }

    private static global::BamlBridge.Cffi.V1.BamlOutboundValue CloneControl(global::BamlBridge.Cffi.V1.BamlOutboundValue value, NativeApi api)
    {
        var result = value.Clone(); var owners = new List<BamlSafeHandle>();
        void Clone(global::BamlBridge.Cffi.V1.BamlOutboundValue item)
        {
            switch (item.ValueCase)
            {
                case global::BamlBridge.Cffi.V1.BamlOutboundValue.ValueOneofCase.HandleValue:
                    if (item.HandleValue.HandleType is global::BamlBridge.Cffi.V1.BamlHandleType.HostValueCallable or global::BamlBridge.Cffi.V1.BamlHandleType.HostValueOpaque) break;
                    ulong key = 0;
                    if (api.Table->HandleClone(item.HandleValue.Key, &key) != BamlCffiStatus.Ok || key == 0) throw new InvalidDataException("Inactive invocation control handle.");
                    owners.Add(api.OwnHandle(key)); item.HandleValue.Key = key; break;
                case global::BamlBridge.Cffi.V1.BamlOutboundValue.ValueOneofCase.ClassValue:
                    foreach (var field in item.ClassValue.Fields) Clone(field.Value); break;
                case global::BamlBridge.Cffi.V1.BamlOutboundValue.ValueOneofCase.ListValue:
                    foreach (var field in item.ListValue.Items) Clone(field); break;
                case global::BamlBridge.Cffi.V1.BamlOutboundValue.ValueOneofCase.MapValue:
                    foreach (var field in item.MapValue.Entries) Clone(field.Value); break;
                case global::BamlBridge.Cffi.V1.BamlOutboundValue.ValueOneofCase.UnionVariantValue:
                    Clone(item.UnionVariantValue.Value); break;
            }
        }
        try { Clone(result); foreach (var owner in owners) owner.TransferOwnership(); return result; }
        finally { foreach (var owner in owners) owner.Dispose(); }
    }

    [UnmanagedCallersOnly(CallConvs = [typeof(CallConvCdecl)])]
    private static void OnHostDispatchV1(
        ulong hostKey,
        uint hostCallId,
        byte* content,
        nuint length) =>
        OnHostDispatch(hostKey, hostCallId, content, length);

    private static void OnHostDispatch(
        ulong hostKey,
        uint hostCallId,
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
                functionCallId: 0,
                error);
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
