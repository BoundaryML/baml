using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml;

/// <summary>
/// A portable reflected BAML type, including any runtime-created definitions
/// needed to interpret its root type.
/// </summary>
public sealed class BamlType : IEquatable<BamlType>
{
    private readonly BamlTyDef definition;

    internal BamlType(BamlTy root)
        : this(new BamlTyDef { Root = root?.Clone() })
    {
    }

    internal BamlType(BamlTyDef definition)
    {
        ArgumentNullException.ThrowIfNull(definition);
        if (definition.Root is null
            || definition.Root.TyCase == BamlTy.TyOneofCase.None)
        {
            throw new BamlProtocolException(
                "The native bridge returned an empty reflected BAML type.",
                "BamlTyDef.root was absent or had no type case.");
        }

        this.definition = definition.Clone();
    }

    /// <summary>The structural root descriptor for this reflected type.</summary>
    public BamlTypeDescriptor Descriptor =>
        BamlTypeDescriptor.FromMetadata(definition.Root.ToByteArray());

    internal BamlTyDef WireCopy() => definition.Clone();

    public bool Equals(BamlType? other) =>
        other is not null
        && definition.ToByteArray().AsSpan().SequenceEqual(
            other.definition.ToByteArray());

    public override bool Equals(object? obj) => obj is BamlType other && Equals(other);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        foreach (byte item in definition.ToByteArray())
        {
            hash.Add(item);
        }
        return hash.ToHashCode();
    }

    public override string ToString() => Descriptor.ToString();
}
