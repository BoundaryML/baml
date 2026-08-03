using System.Collections.ObjectModel;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Numerics;
using System.Text;

using Baml.Generated.V1;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml;

public enum BamlValueKind : int
{
    Null = 0,
    Bool = 1,
    Int = 2,
    Float = 3,
    BigInt = 4,
    String = 5,
    Bytes = 6,
    List = 7,
    Map = 8,
    Enum = 9,
    Class = 10,
    Union = 11,
    Media = 12,
    Handle = 13,
}

public enum BamlTypeDescriptorKind : int
{
    Unknown = 0,
    Null = 1,
    Bool = 2,
    Int = 3,
    Float = 4,
    BigInt = 5,
    String = 6,
    Bytes = 7,
    List = 8,
    Map = 9,
    Enum = 10,
    Class = 11,
    Union = 12,
    Media = 13,
    Handle = 14,
}

/// <summary>
/// An immutable, type-erased BAML value.
/// </summary>
public sealed class BamlValue : IEquatable<BamlValue>
{
    private static readonly byte[] UnknownTypeMetadata = new BamlTy
    {
        Unknown = new BamlTyUnknown(),
    }.ToByteArray();
    private static readonly byte[] StringTypeMetadata = new BamlTy
    {
        Primitive = new BamlTyPrimitive
        {
            Kind = BamlTyPrimitiveKind.BamlTyPrimitiveString,
        },
    }.ToByteArray();
    private static readonly BamlValue NullValue = new(BamlGeneratedValue.CreateNull("$"));

    private readonly BamlGeneratedValue value;
    private readonly BamlTypeDescriptor type;

    internal BamlValue(BamlGeneratedValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        this.value = value;
        type = BamlTypeDescriptor.FromGenerated(value);
    }

    public BamlValueKind Kind => ToPublicKind(value.Kind);

    public BamlTypeDescriptor Type => type;

    public static BamlValue Null => NullValue;

    public static BamlValue Bool(bool value) =>
        new(BamlGeneratedValue.CreateBool(value, "$"));

    public static BamlValue Int(long value)
    {
        if (value is < BamlInteger.Minimum or > BamlInteger.Maximum)
        {
            throw Mapping(
                typeof(long),
                "$",
                $"The integer is outside [{BamlInteger.Minimum}, {BamlInteger.Maximum}].");
        }

        return new BamlValue(BamlGeneratedValue.CreateInt(value, "$"));
    }

    public static BamlValue Float(double value)
    {
        if (!double.IsFinite(value))
        {
            throw Mapping(typeof(double), "$", "BAML float values must be finite.");
        }

        return new BamlValue(BamlGeneratedValue.CreateFloat(value, "$"));
    }

    public static BamlValue BigInt(BigInteger value)
    {
        int byteCount = value.GetByteCount(isUnsigned: false);
        if (byteCount > BamlBigIntLimits.MaxHexLength / 2 + 1)
        {
            throw Mapping(typeof(BigInteger), "$", "The bigint exceeds the BAML allocation limit.");
        }

        string hexadecimal = BigInteger.Abs(value).ToString("x", CultureInfo.InvariantCulture);
        if (hexadecimal.Length + (value.Sign < 0 ? 1 : 0) > BamlBigIntLimits.MaxHexLength)
        {
            throw Mapping(typeof(BigInteger), "$", "The bigint exceeds the BAML allocation limit.");
        }

        return new BamlValue(BamlGeneratedValue.CreateBigInt(value, "$"));
    }

    public static BamlValue String(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new BamlValue(BamlGeneratedValue.CreateString(value, "$"));
    }

    public static BamlValue Bytes(ReadOnlyMemory<byte> value)
    {
        BamlValueLimits.RequireBytes(value.Length, "$", typeof(ReadOnlyMemory<byte>));
        return new BamlValue(BamlGeneratedValue.CreateBytes(value.Span, "$"));
    }

    public static BamlValue List(IEnumerable<BamlValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (values.TryGetNonEnumeratedCount(out int count))
        {
            BamlValueLimits.RequireCollection(count, "$", typeof(BamlValue));
        }

        BamlValue[] snapshot = SnapshotBounded(values, "$", typeof(BamlValue));
        if (snapshot.Any(item => item is null))
        {
            throw Mapping(
                typeof(BamlValue),
                "$",
                "Dynamic lists must use BamlValue.Null instead of CLR null.",
                "BamlValue.Null");
        }

        var result = new BamlValue(
            BamlGeneratedValue.CreateList(
                Array.AsReadOnly(snapshot.Select(item => item.value).ToArray()),
                UnknownTypeMetadata,
                "$"));
        BamlValueLimits.ValidateGraph(result);
        return result;
    }

    public static BamlValue Map(IEnumerable<KeyValuePair<string, BamlValue>> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (values.TryGetNonEnumeratedCount(out int count))
        {
            BamlValueLimits.RequireCollection(count, "$", typeof(BamlValue));
        }

        KeyValuePair<string, BamlValue>[] snapshot = SnapshotBounded(
            values,
            "$",
            typeof(BamlValue))
            .Select(pair =>
            {
                ArgumentNullException.ThrowIfNull(pair.Key);
                if (pair.Value is null)
                {
                    throw Mapping(
                        typeof(BamlValue),
                        $"$[{pair.Key}]",
                        "Dynamic maps must use BamlValue.Null instead of CLR null.",
                        "BamlValue.Null");
                }

                return pair;
            })
            .OrderBy(pair => pair.Key, StringComparer.Ordinal)
            .ToArray();
        for (int index = 1; index < snapshot.Length; index++)
        {
            if (StringComparer.Ordinal.Equals(snapshot[index - 1].Key, snapshot[index].Key))
            {
                throw Mapping(
                    typeof(string),
                    $"$[{snapshot[index].Key}]",
                    "The dynamic map contains a duplicate canonical key.");
            }
        }

        var entries = snapshot
            .Select(pair => new KeyValuePair<string, BamlGeneratedValue>(pair.Key, pair.Value.value))
            .ToArray();
        var result = new BamlValue(
            BamlGeneratedValue.CreateMap(
                Array.AsReadOnly(entries),
                StringTypeMetadata,
                UnknownTypeMetadata,
                "$"));
        BamlValueLimits.ValidateGraph(result);
        return result;
    }

    public static BamlValue From<T>(T value)
    {
        if (value is null)
        {
            if (Nullable.GetUnderlyingType(typeof(T)) is not null)
            {
                return Null;
            }

            throw Mapping(
                typeof(T),
                "$",
                "CLR null has no context-free BAML descriptor.",
                "BamlValue.Null or an explicit nullable value");
        }

        Type declaredType = typeof(T);
        Type canonicalType = Nullable.GetUnderlyingType(declaredType) ?? declaredType;
        if (declaredType == typeof(BamlValue))
        {
            return (BamlValue)(object)value;
        }

        if (canonicalType == typeof(bool))
        {
            return Bool((bool)(object)value);
        }
        if (canonicalType == typeof(long))
        {
            return Int((long)(object)value);
        }
        if (canonicalType == typeof(double))
        {
            return Float((double)(object)value);
        }
        if (canonicalType == typeof(BigInteger))
        {
            return BigInt((BigInteger)(object)value);
        }
        if (declaredType == typeof(string))
        {
            return String((string)(object)value);
        }
        if (declaredType == typeof(ReadOnlyMemory<byte>))
        {
            return Bytes((ReadOnlyMemory<byte>)(object)value);
        }
        if (declaredType == typeof(BamlImage)
            || declaredType == typeof(BamlAudio)
            || declaredType == typeof(BamlVideo)
            || declaredType == typeof(BamlPdf))
        {
            return new BamlValue(BamlGeneratedValue.CreateMedia(value));
        }
        if (declaredType == typeof(BamlHandle))
        {
            return new BamlValue(BamlGeneratedValue.CreateHandle((BamlHandle)(object)value));
        }
        if (BamlDynamicCodecRegistry.TryEncode(value, out BamlValue? encoded))
        {
            return encoded!;
        }

        throw Mapping(
            declaredType,
            "$",
            "The CLR type has no context-free canonical BAML mapping.",
            CanonicalReplacement(declaredType));
    }

    public bool TryGet<T>([MaybeNullWhen(false)] out T result)
    {
        if (typeof(T) == typeof(BamlValue))
        {
            result = (T)(object)this;
            return true;
        }

        if (Kind == BamlValueKind.Null)
        {
            if (Nullable.GetUnderlyingType(typeof(T)) is not null)
            {
                result = default!;
                return true;
            }

            result = default!;
            return false;
        }

        if (Kind == BamlValueKind.Media
            && IsMediaTarget(typeof(T))
            && value.ReadMedia() is T media)
        {
            result = media;
            return true;
        }
        if (Kind == BamlValueKind.Handle && typeof(T) == typeof(BamlHandle))
        {
            result = (T)(object)value.ReadHandle();
            return true;
        }
        if (BamlDynamicCodecRegistry.TryDecode(this, out result))
        {
            return true;
        }

        Type requestedType = typeof(T);
        Type canonicalType = Nullable.GetUnderlyingType(requestedType) ?? requestedType;
        if (type.Alias is not null || type.Literal is not null)
        {
            result = default!;
            return false;
        }

        object? decoded = (canonicalType, Kind) switch
        {
            ({ } target, BamlValueKind.Bool) when target == typeof(bool) => value.ReadBool(),
            ({ } target, BamlValueKind.Int) when target == typeof(long) => value.ReadInt(),
            ({ } target, BamlValueKind.Float) when target == typeof(double) => value.ReadFloat(),
            ({ } target, BamlValueKind.BigInt) when target == typeof(BigInteger) => value.ReadBigInt(),
            ({ } target, BamlValueKind.String) when target == typeof(string) => value.ReadString(),
            ({ } target, BamlValueKind.Bytes) when target == typeof(ReadOnlyMemory<byte>) =>
                new ReadOnlyMemory<byte>(value.ReadBytes()),
            _ => null,
        };
        if (decoded is T typed)
        {
            result = typed;
            return true;
        }

        result = default!;
        return false;
    }

    public T As<T>()
    {
        if (TryGet(out T? result))
        {
            return result!;
        }

        if (Kind == BamlValueKind.Null
            && typeof(T) != typeof(BamlValue)
            && Nullable.GetUnderlyingType(typeof(T)) is null)
        {
            throw Mapping(
                typeof(T),
                "$",
                "BAML null requires an explicit canonical nullable target.",
                "BamlValue or an explicit nullable value type");
        }

        if (type.Alias is not null || type.Literal is not null)
        {
            throw Mapping(
                typeof(T),
                "$",
                "A nominal alias or literal occurrence has no context-free canonical CLR projection.",
                "BamlValue or the generated occurrence codec");
        }

        if (!IsCanonicalTarget(typeof(T))
            && !BamlDynamicCodecRegistry.IsRegistered(typeof(T)))
        {
            throw Mapping(
                typeof(T),
                "$",
                "The requested CLR type has no canonical BAML mapping.",
                CanonicalReplacement(typeof(T)));
        }

        throw new BamlTypeMismatchException(
            "The BAML value does not match the requested canonical CLR type.",
            this,
            bamlFunction: null,
            trace: new BamlTrace([]));
    }

    public bool TryGetEnumVariant([NotNullWhen(true)] out string? wireVariant)
    {
        if (value.Kind == PrimitiveCarrierKind.Enum)
        {
            wireVariant = value.ReadEnumWireValue();
            return true;
        }

        wireVariant = null;
        return false;
    }

    public bool TryGetClassFields(
        [NotNullWhen(true)] out IReadOnlyList<KeyValuePair<string, BamlValue>>? fields)
    {
        if (value.Kind == PrimitiveCarrierKind.Class)
        {
            fields = new ReadOnlyCollection<KeyValuePair<string, BamlValue>>(
                value.ReadClassFields()
                    .Select(pair => new KeyValuePair<string, BamlValue>(pair.Key, new BamlValue(pair.Value)))
                    .ToArray());
            return true;
        }

        fields = null;
        return false;
    }

    public bool TryGetUnion(
        out int activeCase,
        [NotNullWhen(true)] out BamlValue? selectedValue)
    {
        if (value.Kind != PrimitiveCarrierKind.Union)
        {
            activeCase = 0;
            selectedValue = null;
            return false;
        }

        BamlGeneratedValue payload = value.ReadUnionPayload();
        bool hasSelectedType = value.UnionSelectedTypeMetadata is { Length: > 0 };
        if (value.UnionSelectedTypeMetadata is { Length: > 0 } selectedType)
        {
            payload = payload.WithOccurrenceType(selectedType);
        }
        BamlValue selected = new(payload);
        int[] matchingCases = value.UnionSelectedTypeMetadata is { Length: > 0 }
            ? Type.Arguments
                .Select((argument, index) => (argument, index))
                .Where(item => item.argument.Equals(selected.Type))
                .Select(item => item.index)
                .ToArray()
            : LiteralOptionMatches(value.ReadUnionOptionName(), Type.Arguments);
        if (matchingCases.Length == 0)
        {
            matchingCases = Type.Arguments
                .Select((argument, index) => (argument, index))
                .Where(item => item.argument.Equals(selected.Type))
                .Select(item => item.index)
                .ToArray();
        }
        if (matchingCases.Length != 1)
        {
            activeCase = 0;
            selectedValue = null;
            return false;
        }

        if (!hasSelectedType
            && UnionOptionMetadata(value.UnionSelfTypeMetadata, matchingCases[0])
                is { } inferredSelectedType)
        {
            selected = new BamlValue(payload.WithOccurrenceType(inferredSelectedType));
        }

        selectedValue = selected;
        activeCase = matchingCases[0];
        return true;
    }

    private static int[] LiteralOptionMatches(
        string optionName,
        IReadOnlyList<BamlTypeDescriptor> arguments) => arguments
        .Select((argument, index) => (argument, index))
        .Where(item => item.argument.Literal is not null
            && StringComparer.Ordinal.Equals(
                optionName,
                item.argument.Kind == BamlTypeDescriptorKind.String
                    ? QuoteStringLiteral(item.argument.Literal)
                    : item.argument.Literal))
        .Select(item => item.index)
        .ToArray();

    private static byte[]? UnionOptionMetadata(byte[]? selfTypeMetadata, int optionIndex)
    {
        if (selfTypeMetadata is null || selfTypeMetadata.Length == 0)
        {
            return null;
        }

        try
        {
            BamlTy selfType = BamlTy.Parser.ParseFrom(selfTypeMetadata);
            return selfType.TyCase == BamlTy.TyOneofCase.Union
                && optionIndex >= 0
                && optionIndex < selfType.Union.Options.Count
                ? selfType.Union.Options[optionIndex].ToByteArray()
                : null;
        }
        catch (InvalidProtocolBufferException)
        {
            return null;
        }
    }

    private static string QuoteStringLiteral(string literal)
    {
        var quoted = new StringBuilder(literal.Length + 2).Append('"');
        foreach (char character in literal)
        {
            switch (character)
            {
                case '"':
                    quoted.Append("\\\"");
                    break;
                case '\\':
                    quoted.Append("\\\\");
                    break;
                case '\b':
                    quoted.Append("\\b");
                    break;
                case '\f':
                    quoted.Append("\\f");
                    break;
                case '\n':
                    quoted.Append("\\n");
                    break;
                case '\r':
                    quoted.Append("\\r");
                    break;
                case '\t':
                    quoted.Append("\\t");
                    break;
                case < ' ':
                    quoted.Append("\\u").Append(((int)character).ToString("x4", CultureInfo.InvariantCulture));
                    break;
                default:
                    quoted.Append(character);
                    break;
            }
        }

        return quoted.Append('"').ToString();
    }

    internal BamlGeneratedValue GeneratedValue => value;

    internal string? NominalTypeName => value.ReadNominalIdentity();

    public bool Equals(BamlValue? other) =>
        other is not null
        && type.Equals(other.type)
        && ValueEquals(value, other.value);

    public override bool Equals(object? obj) =>
        obj is BamlValue other && Equals(other);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(type);
        AddValueHash(ref hash, value);
        return hash.ToHashCode();
    }

    public override string ToString() => $"BamlValue({Kind})";

    private static BamlValueKind ToPublicKind(PrimitiveCarrierKind kind) => kind switch
    {
        PrimitiveCarrierKind.Null => BamlValueKind.Null,
        PrimitiveCarrierKind.Bool => BamlValueKind.Bool,
        PrimitiveCarrierKind.Int => BamlValueKind.Int,
        PrimitiveCarrierKind.Float => BamlValueKind.Float,
        PrimitiveCarrierKind.BigInt => BamlValueKind.BigInt,
        PrimitiveCarrierKind.String => BamlValueKind.String,
        PrimitiveCarrierKind.Bytes => BamlValueKind.Bytes,
        PrimitiveCarrierKind.List => BamlValueKind.List,
        PrimitiveCarrierKind.Map => BamlValueKind.Map,
        PrimitiveCarrierKind.Enum => BamlValueKind.Enum,
        PrimitiveCarrierKind.Class => BamlValueKind.Class,
        PrimitiveCarrierKind.Union => BamlValueKind.Union,
        PrimitiveCarrierKind.Media => BamlValueKind.Media,
        PrimitiveCarrierKind.Handle => BamlValueKind.Handle,
        _ => throw new BamlProtocolException(
            "The managed bridge encountered an unsupported BAML value kind.",
            $"Generated carrier kind {kind} has no public BamlValueKind."),
    };

    private static bool ValueEquals(BamlGeneratedValue left, BamlGeneratedValue right)
    {
        if (left.Kind != right.Kind)
        {
            return false;
        }

        return left.Kind switch
        {
            PrimitiveCarrierKind.Null => true,
            PrimitiveCarrierKind.Bool => left.ReadBool() == right.ReadBool(),
            PrimitiveCarrierKind.Int => left.ReadInt() == right.ReadInt(),
            PrimitiveCarrierKind.Float => left.ReadFloat().Equals(right.ReadFloat()),
            PrimitiveCarrierKind.String => StringComparer.Ordinal.Equals(
                left.ReadString(),
                right.ReadString()),
            PrimitiveCarrierKind.Bytes => left.ReadBytes().AsSpan().SequenceEqual(right.ReadBytes()),
            PrimitiveCarrierKind.BigInt => left.ReadBigInt() == right.ReadBigInt(),
            PrimitiveCarrierKind.List => SequenceEquals(left.ReadList(), right.ReadList()),
            PrimitiveCarrierKind.Map => MapEquals(left.ReadMapEntries(), right.ReadMapEntries()),
            PrimitiveCarrierKind.Class =>
                StringComparer.Ordinal.Equals(
                    left.ReadClassIdentity(),
                    right.ReadClassIdentity())
                && FieldSequenceEquals(
                    left.ReadClassFields(),
                    right.ReadClassFields()),
            PrimitiveCarrierKind.Enum =>
                StringComparer.Ordinal.Equals(left.ReadEnumIdentity(), right.ReadEnumIdentity())
                && StringComparer.Ordinal.Equals(left.ReadEnumWireValue(), right.ReadEnumWireValue())
                && left.IsDynamicEnum == right.IsDynamicEnum,
            PrimitiveCarrierKind.Union =>
                StringComparer.Ordinal.Equals(
                    left.ReadUnionOptionName(),
                    right.ReadUnionOptionName())
                && ValueEquals(left.ReadUnionPayload(), right.ReadUnionPayload()),
            PrimitiveCarrierKind.Media => left.ReadMedia().Equals(right.ReadMedia()),
            PrimitiveCarrierKind.Handle => ReferenceEquals(left.ReadHandle(), right.ReadHandle()),
            _ => false,
        };
    }

    private static bool SequenceEquals(
        IReadOnlyList<BamlGeneratedValue> left,
        IReadOnlyList<BamlGeneratedValue> right) =>
        left.Count == right.Count
        && left.Zip(right).All(pair => ValueEquals(pair.First, pair.Second));

    private static bool FieldSequenceEquals(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> left,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> right) =>
        left.Count == right.Count
        && left.Zip(right).All(pair =>
            StringComparer.Ordinal.Equals(pair.First.Key, pair.Second.Key)
            && ValueEquals(pair.First.Value, pair.Second.Value));

    private static bool MapEquals(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> left,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> right)
    {
        if (left.Count != right.Count)
        {
            return false;
        }

        KeyValuePair<string, BamlGeneratedValue>[] orderedLeft =
            left.OrderBy(pair => pair.Key, StringComparer.Ordinal).ToArray();
        KeyValuePair<string, BamlGeneratedValue>[] orderedRight =
            right.OrderBy(pair => pair.Key, StringComparer.Ordinal).ToArray();
        return orderedLeft.Zip(orderedRight).All(pair =>
            StringComparer.Ordinal.Equals(pair.First.Key, pair.Second.Key)
            && ValueEquals(pair.First.Value, pair.Second.Value));
    }

    private static void AddValueHash(ref HashCode hash, BamlGeneratedValue value)
    {
        hash.Add(value.Kind);
        switch (value.Kind)
        {
            case PrimitiveCarrierKind.Null:
                break;
            case PrimitiveCarrierKind.Bool:
                hash.Add(value.ReadBool());
                break;
            case PrimitiveCarrierKind.Int:
                hash.Add(value.ReadInt());
                break;
            case PrimitiveCarrierKind.Float:
                hash.Add(value.ReadFloat());
                break;
            case PrimitiveCarrierKind.String:
                hash.Add(value.ReadString(), StringComparer.Ordinal);
                break;
            case PrimitiveCarrierKind.Bytes:
                foreach (byte item in value.ReadBytes())
                {
                    hash.Add(item);
                }
                break;
            case PrimitiveCarrierKind.BigInt:
                hash.Add(value.ReadBigInt());
                break;
            case PrimitiveCarrierKind.List:
                foreach (BamlGeneratedValue item in value.ReadList())
                {
                    AddValueHash(ref hash, item);
                }
                break;
            case PrimitiveCarrierKind.Map:
                foreach ((string key, BamlGeneratedValue item) in value.ReadMapEntries()
                    .OrderBy(pair => pair.Key, StringComparer.Ordinal))
                {
                    hash.Add(key, StringComparer.Ordinal);
                    AddValueHash(ref hash, item);
                }
                break;
            case PrimitiveCarrierKind.Class:
                hash.Add(value.ReadClassIdentity(), StringComparer.Ordinal);
                foreach ((string key, BamlGeneratedValue item) in value.ReadClassFields())
                {
                    hash.Add(key, StringComparer.Ordinal);
                    AddValueHash(ref hash, item);
                }
                break;
            case PrimitiveCarrierKind.Enum:
                hash.Add(value.ReadEnumIdentity(), StringComparer.Ordinal);
                hash.Add(value.ReadEnumWireValue(), StringComparer.Ordinal);
                hash.Add(value.IsDynamicEnum);
                break;
            case PrimitiveCarrierKind.Union:
                hash.Add(value.ReadUnionOptionName(), StringComparer.Ordinal);
                AddValueHash(ref hash, value.ReadUnionPayload());
                break;
            case PrimitiveCarrierKind.Media:
                hash.Add(value.ReadMedia());
                break;
            case PrimitiveCarrierKind.Handle:
                hash.Add(value.ReadHandle(), ReferenceEqualityComparer.Instance);
                break;
        }
    }

    internal IReadOnlyList<BamlValue> ReadListValues() =>
        value.ReadList().Select(item => new BamlValue(item)).ToArray();

    internal IReadOnlyList<KeyValuePair<string, BamlValue>> ReadMapValues() =>
        value.ReadMapEntries()
            .Select(pair => new KeyValuePair<string, BamlValue>(pair.Key, new BamlValue(pair.Value)))
            .ToArray();

    internal IReadOnlyList<KeyValuePair<string, BamlValue>> ReadClassValues() =>
        value.ReadClassFields()
            .Select(pair => new KeyValuePair<string, BamlValue>(pair.Key, new BamlValue(pair.Value)))
            .ToArray();

    internal BamlValue ReadUnionValue() => new(value.ReadUnionPayload());

    private static TItem[] SnapshotBounded<TItem>(
        IEnumerable<TItem> values,
        string path,
        Type clrType)
    {
        var snapshot = new List<TItem>();
        foreach (TItem item in values)
        {
            if (snapshot.Count == BamlValueLimits.MaxCollectionItems)
            {
                BamlValueLimits.RequireCollection(snapshot.Count + 1, path, clrType);
            }

            snapshot.Add(item);
        }

        return snapshot.ToArray();
    }

    private static bool IsCanonicalTarget(Type type)
    {
        Type canonicalType = Nullable.GetUnderlyingType(type) ?? type;
        return canonicalType == typeof(BamlValue)
            || canonicalType == typeof(bool)
            || canonicalType == typeof(long)
            || canonicalType == typeof(double)
            || canonicalType == typeof(BigInteger)
            || canonicalType == typeof(string)
            || canonicalType == typeof(ReadOnlyMemory<byte>)
            || canonicalType == typeof(BamlImage)
            || canonicalType == typeof(BamlAudio)
            || canonicalType == typeof(BamlVideo)
            || canonicalType == typeof(BamlPdf)
            || canonicalType == typeof(BamlHandle);
    }

    private static bool IsMediaTarget(Type type) =>
        type == typeof(BamlImage)
        || type == typeof(BamlAudio)
        || type == typeof(BamlVideo)
        || type == typeof(BamlPdf);

    private static string? CanonicalReplacement(Type type) => type == typeof(int)
        ? "long"
        : type == typeof(float)
            ? "double"
            : type == typeof(decimal)
                ? "double or an explicit BAML model"
                : null;

    private static BamlTypeMappingException Mapping(
        Type clrType,
        string path,
        string diagnostic,
        string? replacement = null) =>
        new(clrType, "dynamic value", path, diagnostic, replacement);
}

internal static class BamlBigIntLimits
{
    internal const int MaxHexLength = (1 << 28) / 4 + 2;
}

public sealed class BamlTypeDescriptor : IEquatable<BamlTypeDescriptor>
{
    private readonly IReadOnlyList<BamlTypeDescriptor> arguments;

    private BamlTypeDescriptor(
        BamlTypeDescriptorKind kind,
        string? fqn = null,
        IEnumerable<BamlTypeDescriptor>? arguments = null,
        bool isNullable = false,
        string? alias = null,
        string? literal = null)
    {
        Kind = kind;
        Fqn = fqn;
        this.arguments = Array.AsReadOnly(arguments?.ToArray() ?? []);
        IsNullable = isNullable;
        Alias = alias;
        Literal = literal;
    }

    public BamlTypeDescriptorKind Kind { get; }

    public string? Fqn { get; }

    public IReadOnlyList<BamlTypeDescriptor> Arguments => arguments;

    public bool IsNullable { get; }

    public string? Alias { get; }

    public string? Literal { get; }

    public bool Equals(BamlTypeDescriptor? other) =>
        other is not null
        && Kind == other.Kind
        && StringComparer.Ordinal.Equals(Fqn, other.Fqn)
        && arguments.SequenceEqual(other.arguments)
        && IsNullable == other.IsNullable
        && StringComparer.Ordinal.Equals(Alias, other.Alias)
        && StringComparer.Ordinal.Equals(Literal, other.Literal);

    public override bool Equals(object? obj) =>
        obj is BamlTypeDescriptor other && Equals(other);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(Kind);
        hash.Add(Fqn, StringComparer.Ordinal);
        foreach (BamlTypeDescriptor argument in arguments)
        {
            hash.Add(argument);
        }
        hash.Add(IsNullable);
        hash.Add(Alias, StringComparer.Ordinal);
        hash.Add(Literal, StringComparer.Ordinal);
        return hash.ToHashCode();
    }

    public override string ToString() => Fqn is null ? Kind.ToString() : $"{Kind}({Fqn})";

    internal static BamlTypeDescriptor FromGenerated(BamlGeneratedValue value) =>
        value.OccurrenceTypeMetadata is { Length: > 0 } metadata
            ? FromMetadata(metadata)
            : value.Kind switch
        {
            PrimitiveCarrierKind.Null => Leaf(BamlTypeDescriptorKind.Null),
            PrimitiveCarrierKind.Bool => Leaf(BamlTypeDescriptorKind.Bool),
            PrimitiveCarrierKind.Int => Leaf(BamlTypeDescriptorKind.Int),
            PrimitiveCarrierKind.Float => Leaf(BamlTypeDescriptorKind.Float),
            PrimitiveCarrierKind.BigInt => Leaf(BamlTypeDescriptorKind.BigInt),
            PrimitiveCarrierKind.String => Leaf(BamlTypeDescriptorKind.String),
            PrimitiveCarrierKind.Bytes => Leaf(BamlTypeDescriptorKind.Bytes),
            PrimitiveCarrierKind.List => new(
                BamlTypeDescriptorKind.List,
                arguments: [FromMetadata(value.ItemTypeMetadata)]),
            PrimitiveCarrierKind.Map => new(
                BamlTypeDescriptorKind.Map,
                arguments:
                [
                    FromMetadata(value.KeyTypeMetadata),
                    FromMetadata(value.ValueTypeMetadata),
                ]),
            PrimitiveCarrierKind.Class => new(
                BamlTypeDescriptorKind.Class,
                value.ReadClassIdentity(),
                value.ReadClassTypeArguments().Select(FromMetadata)),
            PrimitiveCarrierKind.Enum => new(
                BamlTypeDescriptorKind.Enum,
                value.ReadEnumIdentity()),
            PrimitiveCarrierKind.Union => FromMetadata(value.UnionSelfTypeMetadata),
            PrimitiveCarrierKind.Media => Leaf(BamlTypeDescriptorKind.Media),
            PrimitiveCarrierKind.Handle => value.ReadHandle().Type,
            _ => throw new BamlProtocolException(
                "The managed bridge encountered an unsupported BAML value type.",
                $"Generated carrier kind {value.Kind} has no public type descriptor."),
        };

    internal static BamlTypeDescriptor FromMetadata(byte[]? metadata)
    {
        if (metadata is null || metadata.Length == 0)
        {
            return Leaf(BamlTypeDescriptorKind.Unknown);
        }

        try
        {
            return FromWire(BamlTy.Parser.ParseFrom(metadata), new DescriptorBudget(), depth: 0);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlProtocolException(
                "The native bridge returned malformed BAML type metadata.",
                error.Message);
        }
    }

    private static BamlTypeDescriptor FromWire(
        BamlTy? type,
        DescriptorBudget budget,
        int depth)
    {
        budget.Visit(depth);
        if (type is null || type.TyCase == BamlTy.TyOneofCase.None)
        {
            throw new BamlProtocolException(
                "The native bridge returned empty BAML type metadata.",
                "BamlTy.ty was absent.");
        }

        return type.TyCase switch
        {
            BamlTy.TyOneofCase.Primitive => type.Primitive.Kind switch
            {
                BamlTyPrimitiveKind.BamlTyPrimitiveNull => Leaf(BamlTypeDescriptorKind.Null),
                BamlTyPrimitiveKind.BamlTyPrimitiveBool => Leaf(BamlTypeDescriptorKind.Bool),
                BamlTyPrimitiveKind.BamlTyPrimitiveInt => Leaf(BamlTypeDescriptorKind.Int),
                BamlTyPrimitiveKind.BamlTyPrimitiveFloat => Leaf(BamlTypeDescriptorKind.Float),
                BamlTyPrimitiveKind.BamlTyPrimitiveBigint => Leaf(BamlTypeDescriptorKind.BigInt),
                BamlTyPrimitiveKind.BamlTyPrimitiveString => Leaf(BamlTypeDescriptorKind.String),
                BamlTyPrimitiveKind.BamlTyPrimitiveBytes => Leaf(BamlTypeDescriptorKind.Bytes),
                _ => throw UnsupportedType(type),
            },
            BamlTy.TyOneofCase.ClassTy => Nominal(
                BamlTypeDescriptorKind.Class,
                type.ClassTy.Name,
                type.ClassTy.TypeArgs.Select(item => FromWire(item, budget, depth + 1))),
            BamlTy.TyOneofCase.Enum => Nominal(BamlTypeDescriptorKind.Enum, type.Enum.Name),
            BamlTy.TyOneofCase.List => new(
                BamlTypeDescriptorKind.List,
                arguments: [FromWire(type.List.Item, budget, depth + 1)]),
            BamlTy.TyOneofCase.Map => new(
                BamlTypeDescriptorKind.Map,
                arguments:
                [
                    FromWire(type.Map.Key, budget, depth + 1),
                    FromWire(type.Map.Value, budget, depth + 1),
                ]),
            BamlTy.TyOneofCase.Optional =>
                FromWire(type.Optional.Inner, budget, depth + 1).WithNullable(),
            BamlTy.TyOneofCase.Union => new(
                BamlTypeDescriptorKind.Union,
                arguments: type.Union.Options.Select(
                    item => FromWire(item, budget, depth + 1))),
            BamlTy.TyOneofCase.Literal => FromLiteral(type.Literal),
            BamlTy.TyOneofCase.TypeAlias => CreateAlias(
                type.TypeAlias.Name,
                type.TypeAlias.TypeArgs.Select(
                    item => FromWire(item, budget, depth + 1))),
            BamlTy.TyOneofCase.Unknown => Leaf(BamlTypeDescriptorKind.Unknown),
            BamlTy.TyOneofCase.Media => Leaf(BamlTypeDescriptorKind.Media),
            BamlTy.TyOneofCase.EnumVariant => Nominal(
                BamlTypeDescriptorKind.Enum,
                type.EnumVariant.Name,
                literal: type.EnumVariant.Variant),
            BamlTy.TyOneofCase.Resource => Leaf(BamlTypeDescriptorKind.Handle),
            _ => throw UnsupportedType(type),
        };
    }

    private static BamlTypeDescriptor FromLiteral(BamlTyLiteral literal) =>
        literal.LiteralCase switch
        {
            BamlTyLiteral.LiteralOneofCase.StringValue => new(
                BamlTypeDescriptorKind.String,
                literal: literal.StringValue),
            BamlTyLiteral.LiteralOneofCase.IntValue => new(
                BamlTypeDescriptorKind.Int,
                literal: literal.IntValue.ToString(CultureInfo.InvariantCulture)),
            BamlTyLiteral.LiteralOneofCase.BoolValue => new(
                BamlTypeDescriptorKind.Bool,
                literal: literal.BoolValue ? "true" : "false"),
            BamlTyLiteral.LiteralOneofCase.BigintValue => new(
                BamlTypeDescriptorKind.BigInt,
                literal: literal.BigintValue),
            BamlTyLiteral.LiteralOneofCase.FloatValue => new(
                BamlTypeDescriptorKind.Float,
                literal: literal.FloatValue),
            _ => throw new BamlProtocolException(
                "The native bridge returned empty BAML literal metadata.",
                "BamlTyLiteral.literal was absent."),
        };

    private BamlTypeDescriptor WithNullable() => new(
        Kind,
        Fqn,
        arguments,
        isNullable: true,
        Alias,
        Literal);

    private static BamlTypeDescriptor Leaf(BamlTypeDescriptorKind kind) => new(kind);

    internal static BamlTypeDescriptor CreateHandle(
        string fqn,
        IEnumerable<BamlTypeDescriptor>? arguments = null) =>
        Nominal(BamlTypeDescriptorKind.Handle, fqn, arguments);

    private static BamlTypeDescriptor Nominal(
        BamlTypeDescriptorKind kind,
        string fqn,
        IEnumerable<BamlTypeDescriptor>? arguments = null,
        string? literal = null)
    {
        if (string.IsNullOrEmpty(fqn))
        {
            throw new BamlProtocolException(
                "The native bridge returned nominal type metadata without an identity.",
                $"Baml type kind {kind} supplied an empty FQN.");
        }

        return new(kind, fqn, arguments, literal: literal);
    }

    private static BamlTypeDescriptor CreateAlias(
        string alias,
        IEnumerable<BamlTypeDescriptor> arguments)
    {
        if (string.IsNullOrEmpty(alias))
        {
            throw new BamlProtocolException(
                "The native bridge returned alias metadata without an identity.",
                "BamlTyTypeAlias.name was empty.");
        }

        return new(BamlTypeDescriptorKind.Unknown, arguments: arguments, alias: alias);
    }

    private static BamlProtocolException UnsupportedType(BamlTy type) => new(
        "The native bridge returned unsupported BAML type metadata.",
        $"BamlTy case {type.TyCase} cannot describe a public BamlValue.");

    private sealed class DescriptorBudget
    {
        private int nodes;

        internal void Visit(int depth)
        {
            if (depth > BamlValueLimits.MaxDepth
                || ++nodes > BamlValueLimits.MaxNodes)
            {
                throw new BamlProtocolException(
                    "The native bridge returned BAML type metadata that exceeds managed resource limits.",
                    $"BamlTy exceeded depth {BamlValueLimits.MaxDepth} or node count {BamlValueLimits.MaxNodes}.");
            }
        }
    }
}
