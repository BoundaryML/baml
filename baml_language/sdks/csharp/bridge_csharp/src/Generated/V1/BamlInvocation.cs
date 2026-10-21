using System.ComponentModel;
using Baml.Cffi;
using Baml.Proto;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public interface IBamlInvocationOptions
{
    BamlHandle? TraceHandle { get; }
    BamlGeneratedValue? CancelValue { get; }
    long? TimeoutMs { get; }
    CancellationToken CancellationToken { get; }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed unsafe class BamlInvocationPreparation : IDisposable
{
    [ThreadStatic] private static BamlInvocationPreparation? current;
    private readonly BamlInvocationPreparation? previous;
    public static IBamlInvocationOptions? CurrentOptions => current?.Options;
    private IBamlInvocationOptions? Options { get; }
    private bool taken;
    private bool disposed;
    internal ulong CallId { get; }
    internal InvocationOptions Wire { get; }
    internal EncodedCallArguments Ownership { get; } = new([]);
    private BamlInvocationPreparation(IBamlInvocationOptions? options, bool install)
    {
        Options = options;
        NativeApi api = NativeApi.Instance;
        CallId = api.NewFunctionCall();
        Wire = new InvocationOptions { InheritedState = options is BamlInvocationSnapshot snapshot ? snapshot.InheritedState : InvocationFrame.Current.Value?.State.Key ?? 0, HostEnvironment = CallId };
        if (options is BamlInvocationSnapshot frozen && frozen.DeadlineNs is ulong deadline) Wire.DeadlineNs = deadline;
        try
        {
            if (options?.TimeoutMs is long timeout)
            {
                if (timeout < 0 || timeout > int.MaxValue) throw new ArgumentOutOfRangeException(nameof(options), "TimeoutMs must be between 0 and 2147483647.");
                Wire.DeadlineNs = checked(api.InvocationClockNs(CallId) + (ulong)timeout * 1_000_000);
            }
            if (options?.TraceHandle is BamlHandle trace)
            {
                using var lease = trace.Lease();
                BamlBuffer buffer = default; ulong reservation = 0;
                if (api.Table->TraceSelection(CallId, lease.Key, &buffer, &reservation) != BamlCffiStatus.Ok)
                    throw new ArgumentException("Invalid tracing selection.", nameof(options));
                if (reservation != 0) Ownership.AddTransfer(api.OwnHandle(reservation));
                Wire.Trace = TraceSelection.Parser.ParseFrom(NativeBuffer.CopyAndFree(api.Table, buffer));
            }
            if (options?.CancelValue is BamlGeneratedValue cancel)
                Wire.Cancel = PrimitiveProtocol.Encode(cancel, api, Ownership, CallId);
        }
        catch { Ownership.Dispose(); _ = api.Table->ReleaseFunctionCall(CallId); throw; }
        previous = current;
        if (install) current = this;
    }
    public static BamlInvocationPreparation Begin(IBamlInvocationOptions? options) => new(options, true);
    internal static BamlInvocationPreparation Take(IBamlInvocationOptions? options)
    {
        var prepared = current is { taken: false } active ? active : new BamlInvocationPreparation(options, false);
        prepared.taken = true;
        return prepared;
    }
    public void Dispose()
    {
        if (disposed) return;
        disposed = true;
        if (ReferenceEquals(current, this)) current = previous;
        Ownership.Dispose();
        if (!taken) _ = NativeApi.Instance.Table->ReleaseFunctionCall(CallId);
    }
    public static BamlGeneratedValue CurrentTraceContext()
    {
        NativeApi api = NativeApi.Instance; BamlBuffer buffer = default;
        if (api.Table->InvocationContext(InvocationFrame.Current.Value?.State.Key ?? 0, &buffer) != BamlCffiStatus.Ok)
            throw new InvalidOperationException("Invalid active invocation.");
        var value = BamlOutboundValue.Parser.ParseFrom(NativeBuffer.CopyAndFree(api.Table, buffer));
        return PrimitiveProtocol.DecodeCallResult(new BamlOutboundResult { Ok = value }.ToByteArray(), "trace.current_context", api);
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlInvocationCapture
{
    internal BamlSafeHandle State { get; }
    public BamlGeneratedValue CancelValue { get; }
    public CancellationToken CancellationToken { get; }
    internal BamlInvocationCapture(BamlSafeHandle state, BamlGeneratedValue cancel, CancellationToken token)
    { State = state; CancelValue = cancel; CancellationToken = token; }
    public static BamlInvocationCapture? Current => InvocationFrame.Current.Value;
}

internal sealed unsafe class BamlInvocationSnapshot : IBamlInvocationOptions, IDisposable
{
    private readonly List<IDisposable> owners = [];
    private readonly BamlSafeHandle? state;
    public BamlHandle? TraceHandle { get; }
    public BamlGeneratedValue? CancelValue { get; }
    public long? TimeoutMs => null;
    public CancellationToken CancellationToken { get; }
    internal ulong? DeadlineNs { get; }
    internal ulong InheritedState => state?.Key ?? 0;
    internal BamlInvocationSnapshot(IBamlInvocationOptions? options)
    {
        try {
            state = InvocationFrame.Current.Value?.State.CloneOwned(); if (state is not null) owners.Add(state);
            TraceHandle = options?.TraceHandle?.Clone(); if (TraceHandle is not null) owners.Add(TraceHandle);
            CancelValue = options?.CancelValue?.SnapshotForDeferredCall(owners);
            CancellationToken = options?.CancellationToken ?? default;
            using var prepared = BamlInvocationPreparation.Begin(options);
            if (prepared.Wire.HasDeadlineNs) DeadlineNs = prepared.Wire.DeadlineNs;
        } catch { Dispose(); throw; }
    }
    public void Dispose() { foreach (var owner in owners) owner.Dispose(); owners.Clear(); }
}
