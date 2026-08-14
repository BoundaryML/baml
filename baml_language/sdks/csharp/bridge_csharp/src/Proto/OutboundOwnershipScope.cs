using Baml.Cffi;
using BamlBridge.Cffi.V1;

namespace Baml.Proto;

internal sealed class OutboundOwnershipScope : IDisposable
{
    private readonly Dictionary<BamlOutboundHandle, BamlSafeHandle> owners =
        new(ReferenceEqualityComparer.Instance);
    private bool disposed;

    private OutboundOwnershipScope(BamlOutboundResult envelope, NativeApi? api)
    {
        var handles = new List<BamlOutboundHandle>();
        Collect(envelope, handles);
        if (handles.Count == 0)
        {
            return;
        }
        if (api is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned an owned handle outside a native call context.",
                $"The result contained {handles.Count} owned handle(s), but no native API owner was supplied.");
        }

        var keys = new HashSet<ulong>();
        try
        {
            foreach (BamlOutboundHandle handle in handles)
            {
                if (IsHostValue(handle.HandleType))
                {
                    continue;
                }
                if (!keys.Add(handle.Key))
                {
                    throw new BamlProtocolException(
                        "The native bridge returned duplicate handle ownership.",
                        $"Outbound handle key {handle.Key} appeared more than once in one result.");
                }

                owners.Add(handle, api.OwnHandle(handle.Key));
            }
        }
        catch
        {
            Dispose();
            throw;
        }
    }

    internal static OutboundOwnershipScope Create(
        BamlOutboundResult envelope,
        NativeApi? api) =>
        new(envelope, api);

    internal BamlSafeHandle Borrow(BamlOutboundHandle wire)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!owners.TryGetValue(wire, out BamlSafeHandle? owner))
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid owned handle.",
                $"Handle key {wire.Key} was missing from the result ownership inventory.");
        }

        return owner;
    }

    internal BamlSafeHandle Claim(BamlOutboundHandle wire)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!owners.Remove(wire, out BamlSafeHandle? owner))
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid owned handle.",
                $"Handle key {wire.Key} was already claimed or missing from the result ownership inventory.");
        }

        return owner;
    }

    public void Dispose()
    {
        if (disposed)
        {
            return;
        }

        disposed = true;
        foreach (BamlSafeHandle owner in owners.Values)
        {
            owner.Dispose();
        }

        owners.Clear();
    }

    private static bool IsHostValue(BamlHandleType handleType) =>
        handleType is BamlHandleType.HostValueCallable or BamlHandleType.HostValueOpaque;

    private static void Collect(BamlOutboundResult envelope, List<BamlOutboundHandle> handles)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        switch (envelope.ResultCase)
        {
            case BamlOutboundResult.ResultOneofCase.Ok:
                Collect(envelope.Ok, handles);
                break;
            case BamlOutboundResult.ResultOneofCase.Error:
                if (envelope.Error.Value is not null)
                {
                    Collect(envelope.Error.Value, handles);
                }
                break;
            case BamlOutboundResult.ResultOneofCase.Panic:
                if (envelope.Panic.Value is not null)
                {
                    Collect(envelope.Panic.Value, handles);
                }
                break;
        }
    }

    private static void Collect(BamlOutboundValue? value, List<BamlOutboundHandle> handles)
    {
        if (value is null)
        {
            return;
        }

        switch (value.ValueCase)
        {
            case BamlOutboundValue.ValueOneofCase.HandleValue:
                handles.Add(value.HandleValue);
                break;
            case BamlOutboundValue.ValueOneofCase.ListValue:
                foreach (BamlOutboundValue item in value.ListValue.Items)
                {
                    Collect(item, handles);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.MapValue:
                foreach (BamlOutboundMapEntry entry in value.MapValue.Entries)
                {
                    Collect(entry.Value, handles);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.ClassValue:
                foreach (BamlOutboundMapEntry field in value.ClassValue.Fields)
                {
                    Collect(field.Value, handles);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.UnionVariantValue:
                Collect(value.UnionVariantValue.Value, handles);
                break;
        }
    }
}
