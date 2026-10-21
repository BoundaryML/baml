using System.Diagnostics;

using Baml.Cffi;
using Baml.Proto;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml;

/// <summary>
/// A portable reflected BAML type. A declaration compiled with the program is
/// named; one the runtime created is held by a reference of this value's own,
/// released when it is disposed or collected.
/// </summary>
public sealed class BamlType : IEquatable<BamlType>, IDisposable
{
    private readonly BamlTy type;
    private readonly LiveRows rows;

    /// <summary>
    /// Adopts <paramref name="rows"/>, the references <paramref name="type"/>'s
    /// <c>live</c> heads arrived with, one per occurrence in visit order.
    /// </summary>
    internal BamlType(BamlTy type, LiveRows rows)
    {
        ArgumentNullException.ThrowIfNull(rows);
        if (type is null || type.TyCase == BamlTy.TyOneofCase.None)
        {
            rows.Dispose();
            throw new BamlProtocolException(
                "The native bridge returned an empty reflected BAML type.",
                "BamlTy.ty was absent.");
        }

        Debug.Assert(
            LiveHeads.Keys(type).SequenceEqual(rows.Keys),
            "a reflected type's rows must be its live heads, in visit order");
        this.type = type.Clone();
        this.rows = rows;
    }

    /// <summary>The structural root descriptor for this reflected type.</summary>
    public BamlTypeDescriptor Descriptor =>
        BamlTypeDescriptor.FromMetadata(type.ToByteArray());

    /// <summary>
    /// This type as the host states it again: each <c>live</c> head carries a
    /// fresh reference the request transfers, so this value keeps its own.
    /// </summary>
    internal BamlTy Restate(EncodedCallArguments? ownership)
    {
        BamlTy wire = type.Clone();
        if (rows.Count == 0)
        {
            return wire;
        }

        if (ownership is null)
        {
            throw new BamlProtocolException(
                "The managed bridge cannot encode an owned value without a native call context.",
                "A reflected type naming a runtime declaration requires an ownership transaction.");
        }

        int occurrence = 0;
        LiveHeads.Map(wire, _ =>
        {
            BamlSafeHandle reference = rows[occurrence++].CloneOwned();
            ownership.AddTransfer(reference);
            return reference.Key;
        });
        return wire;
    }

    public void Dispose() => rows.Dispose();

    // Equality cannot see through `live` heads: two keys may name one
    // declaration, so two such heads are equal only when their keys are.
    public bool Equals(BamlType? other) =>
        other is not null
        && type.ToByteArray().AsSpan().SequenceEqual(other.type.ToByteArray());

    public override bool Equals(object? obj) => obj is BamlType other && Equals(other);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.AddBytes(type.ToByteArray());
        return hash.ToHashCode();
    }

    public override string ToString() => Descriptor.ToString();
}

/// <summary>
/// The engine references a reflected type's runtime declarations are held by,
/// one per <c>live</c> head occurrence, released together exactly once.
/// </summary>
internal sealed class LiveRows : IDisposable
{
    internal static readonly LiveRows None = new([]);

    private readonly BamlSafeHandle[] rows;

    internal LiveRows(BamlSafeHandle[] rows)
    {
        ArgumentNullException.ThrowIfNull(rows);
        this.rows = rows;
    }

    internal int Count => rows.Length;

    internal BamlSafeHandle this[int occurrence] => rows[occurrence];

    internal IEnumerable<ulong> Keys => rows.Select(row => row.Key);

    public void Dispose()
    {
        foreach (BamlSafeHandle row in rows)
        {
            row.Dispose();
        }
    }
}
