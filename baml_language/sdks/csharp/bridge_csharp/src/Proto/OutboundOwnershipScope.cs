using Baml.Cffi;
using BamlBridge.Cffi.V1;

namespace Baml.Proto;

internal sealed class OutboundOwnershipScope : IDisposable
{
    private readonly Dictionary<BamlOutboundHandle, BamlSafeHandle> owners =
        new(ReferenceEqualityComparer.Instance);
    // A returned type value's `live` heads, one reference per occurrence.
    private readonly Dictionary<BamlTy, BamlSafeHandle[]> typeRows =
        new(ReferenceEqualityComparer.Instance);
    private bool disposed;

    private OutboundOwnershipScope(BamlOutboundResult envelope, NativeApi? api)
    {
        var handles = new List<BamlOutboundHandle>();
        var types = new List<(BamlTy Type, List<ulong> Keys)>();
        Collect(envelope, handles, types);
        // Every type value is claimed by the decoder, a name-only one with no rows.
        foreach ((BamlTy type, List<ulong> typeKeys) in types)
        {
            typeRows.Add(type, new BamlSafeHandle[typeKeys.Count]);
        }
        int rowCount = types.Sum(type => type.Keys.Count);
        if (handles.Count == 0 && rowCount == 0)
        {
            return;
        }
        if (api is null)
        {
            throw new BamlProtocolException(
                "The native bridge returned an owned handle outside a native call context.",
                $"The result contained {handles.Count + rowCount} owned handle(s), but no native API owner was supplied.");
        }

        // Handles and type rows share the engine's one key space.
        var keys = new HashSet<ulong>();
        try
        {
            foreach (BamlOutboundHandle handle in handles)
            {
                if (IsHostValue(handle.HandleType))
                {
                    continue;
                }
                RequireUnique(keys, handle.Key);
                owners.Add(handle, api.OwnHandle(handle.Key));
            }

            foreach ((BamlTy type, List<ulong> typeKeys) in types)
            {
                BamlSafeHandle[] rows = typeRows[type];
                for (int occurrence = 0; occurrence < typeKeys.Count; occurrence++)
                {
                    RequireUnique(keys, typeKeys[occurrence]);
                    rows[occurrence] = api.OwnHandle(typeKeys[occurrence]);
                }
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

    /// <summary>The references <paramref name="type"/>'s <c>live</c> heads arrived with, now the caller's.</summary>
    internal LiveRows ClaimRows(BamlTy type)
    {
        ObjectDisposedException.ThrowIf(disposed, this);
        if (!typeRows.Remove(type, out BamlSafeHandle[]? rows))
        {
            throw new BamlProtocolException(
                "The native bridge returned an invalid reflected type.",
                "A type value was already claimed or missing from the result ownership inventory.");
        }

        return rows.Length == 0 ? LiveRows.None : new LiveRows(rows);
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
        foreach (BamlSafeHandle[] rows in typeRows.Values)
        {
            foreach (BamlSafeHandle? row in rows)
            {
                // A row is null when owning an earlier one failed.
                row?.Dispose();
            }
        }

        typeRows.Clear();
    }

    private static void RequireUnique(HashSet<ulong> keys, ulong key)
    {
        if (!keys.Add(key))
        {
            throw new BamlProtocolException(
                "The native bridge returned duplicate handle ownership.",
                $"Outbound handle key {key} appeared more than once in one result.");
        }
    }

    private static bool IsHostValue(BamlHandleType handleType) =>
        handleType is BamlHandleType.HostValueCallable or BamlHandleType.HostValueOpaque;

    private static void Collect(
        BamlOutboundResult envelope,
        List<BamlOutboundHandle> handles,
        List<(BamlTy Type, List<ulong> Keys)> types)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        switch (envelope.ResultCase)
        {
            case BamlOutboundResult.ResultOneofCase.Ok:
                Collect(envelope.Ok, handles, types);
                break;
            case BamlOutboundResult.ResultOneofCase.Error:
                if (envelope.Error.Value is not null)
                {
                    Collect(envelope.Error.Value, handles, types);
                }
                break;
            case BamlOutboundResult.ResultOneofCase.Panic:
                if (envelope.Panic.Value is not null)
                {
                    Collect(envelope.Panic.Value, handles, types);
                }
                break;
        }
    }

    private static void Collect(
        BamlOutboundValue? value,
        List<BamlOutboundHandle> handles,
        List<(BamlTy Type, List<ulong> Keys)> types)
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
            case BamlOutboundValue.ValueOneofCase.TyValue:
                types.Add((value.TyValue, LiveHeads.Keys(value.TyValue)));
                break;
            case BamlOutboundValue.ValueOneofCase.ListValue:
                foreach (BamlOutboundValue item in value.ListValue.Items)
                {
                    Collect(item, handles, types);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.MapValue:
                foreach (BamlOutboundMapEntry entry in value.MapValue.Entries)
                {
                    Collect(entry.Value, handles, types);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.ClassValue:
                foreach (BamlOutboundMapEntry field in value.ClassValue.Fields)
                {
                    Collect(field.Value, handles, types);
                }
                break;
            case BamlOutboundValue.ValueOneofCase.UnionVariantValue:
                Collect(value.UnionVariantValue.Value, handles, types);
                break;
        }
    }
}
