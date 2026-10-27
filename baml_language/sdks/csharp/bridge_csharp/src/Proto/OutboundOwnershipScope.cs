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

    private OutboundOwnershipScope(IEnumerable<BamlOutboundValue?> roots, NativeApi? api)
    {
        var handles = new List<BamlOutboundHandle>();
        var types = new List<(BamlTy Type, List<ulong> Keys)>();
        foreach (BamlOutboundValue? root in roots)
        {
            Collect(root, handles, types);
        }
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

        // Each occurrence of an engine key is one reference, and the engine
        // keeps one key per heap object: a value naming one object twice (a
        // runtime class in `W | W[]`, the same instance in two fields) carries
        // its key twice, and each occurrence is owned and released on its own.
        try
        {
            foreach (BamlOutboundHandle handle in handles)
            {
                if (IsHostValue(handle.HandleType))
                {
                    continue;
                }
                owners.Add(handle, api.OwnHandle(handle.Key));
            }

            foreach ((BamlTy type, List<ulong> typeKeys) in types)
            {
                BamlSafeHandle[] rows = typeRows[type];
                for (int occurrence = 0; occurrence < typeKeys.Count; occurrence++)
                {
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
        new(Roots(envelope), api);

    /// <summary>
    /// The references a host invocation's arguments arrive with. The adapter
    /// owns them from dispatch on, so they are released whether or not the
    /// callback ever runs; decoding an argument claims the ones it keeps.
    /// </summary>
    internal static OutboundOwnershipScope Create(BamlToHostCall call, NativeApi api)
    {
        ArgumentNullException.ThrowIfNull(call);
        ArgumentNullException.ThrowIfNull(api);
        return new(call.Args.Select(argument => argument?.Value), api);
    }

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

    private static bool IsHostValue(BamlHandleType handleType) =>
        handleType is BamlHandleType.HostValueCallable or BamlHandleType.HostValueOpaque;

    private static BamlOutboundValue?[] Roots(BamlOutboundResult envelope)
    {
        ArgumentNullException.ThrowIfNull(envelope);
        return envelope.ResultCase switch
        {
            BamlOutboundResult.ResultOneofCase.Ok => [envelope.Ok],
            BamlOutboundResult.ResultOneofCase.Error => [envelope.Error.Value],
            BamlOutboundResult.ResultOneofCase.Panic => [envelope.Panic.Value],
            _ => [],
        };
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
