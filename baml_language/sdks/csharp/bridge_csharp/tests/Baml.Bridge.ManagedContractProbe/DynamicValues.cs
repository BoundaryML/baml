using System.Collections.ObjectModel;
using System.Diagnostics.CodeAnalysis;
using System.Globalization;
using System.Numerics;

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

public sealed class BamlTypeDescriptor
    : IEquatable<BamlTypeDescriptor>
{
    private readonly ReadOnlyCollection<BamlTypeDescriptor> arguments;

    internal BamlTypeDescriptor(
        BamlValueKind kind,
        string? fqn = null,
        IEnumerable<BamlTypeDescriptor>? arguments = null,
        bool isNullable = false,
        string? alias = null,
        string? literal = null)
        : this(
            ToDescriptorKind(kind),
            fqn,
            arguments,
            isNullable,
            alias,
            literal)
    {
    }

    internal BamlTypeDescriptor(
        BamlTypeDescriptorKind kind,
        string? fqn = null,
        IEnumerable<BamlTypeDescriptor>? arguments = null,
        bool isNullable = false,
        string? alias = null,
        string? literal = null)
    {
        if (!Enum.IsDefined(kind))
        {
            throw new ArgumentOutOfRangeException(nameof(kind));
        }

        if (fqn is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(fqn);
        }

        if (alias is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(alias);
        }

        Kind = kind;
        Fqn = fqn;
        this.arguments = new(
            (arguments ?? []).ToArray());
        if (this.arguments.Any(argument => argument is null))
        {
            throw new ArgumentException(
                "descriptor arguments must not contain null",
                nameof(arguments));
        }

        ValidateShape(kind, fqn, this.arguments);
        IsNullable = isNullable;
        Alias = alias;
        Literal = literal;
    }

    public BamlTypeDescriptorKind Kind { get; }

    public string? Fqn { get; }

    public IReadOnlyList<BamlTypeDescriptor> Arguments =>
        arguments;

    public bool IsNullable { get; }

    public string? Alias { get; }

    public string? Literal { get; }

    public bool Equals(BamlTypeDescriptor? other) =>
        other is not null
        && Kind == other.Kind
        && StringComparer.Ordinal.Equals(Fqn, other.Fqn)
        && IsNullable == other.IsNullable
        && StringComparer.Ordinal.Equals(Alias, other.Alias)
        && StringComparer.Ordinal.Equals(Literal, other.Literal)
        && arguments.SequenceEqual(other.arguments);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlTypeDescriptor);

    public override int GetHashCode()
    {
        HashCode hash = new();
        hash.Add(Kind);
        hash.Add(Fqn, StringComparer.Ordinal);
        hash.Add(IsNullable);
        hash.Add(Alias, StringComparer.Ordinal);
        hash.Add(Literal, StringComparer.Ordinal);
        foreach (BamlTypeDescriptor argument in arguments)
        {
            hash.Add(argument);
        }

        return hash.ToHashCode();
    }

    public override string ToString() =>
        Fqn ?? Kind.ToString();

    private static void ValidateShape(
        BamlTypeDescriptorKind kind,
        string? fqn,
        IReadOnlyList<BamlTypeDescriptor> arguments)
    {
        bool requiresFqn = kind is BamlTypeDescriptorKind.Enum
            or BamlTypeDescriptorKind.Class
            or BamlTypeDescriptorKind.Handle;
        if (requiresFqn != (fqn is not null))
        {
            throw new ArgumentException(
                requiresFqn
                    ? $"{kind} descriptors require a BAML FQN"
                    : $"{kind} descriptors do not carry a BAML FQN",
                nameof(fqn));
        }

        bool validArity = kind switch
        {
            BamlTypeDescriptorKind.List => arguments.Count == 1,
            BamlTypeDescriptorKind.Map => arguments.Count == 2,
            BamlTypeDescriptorKind.Union => arguments.Count >= 2,
            BamlTypeDescriptorKind.Class => true,
            _ => arguments.Count == 0,
        };
        if (!validArity)
        {
            throw new ArgumentException(
                $"{kind} descriptor argument arity is invalid",
                nameof(arguments));
        }
    }

    private static BamlTypeDescriptorKind ToDescriptorKind(
        BamlValueKind kind)
    {
        if (!Enum.IsDefined(kind))
        {
            throw new ArgumentOutOfRangeException(nameof(kind));
        }

        return (BamlTypeDescriptorKind)((int)kind + 1);
    }
}

public sealed class BamlValue : IEquatable<BamlValue>
{
    private static readonly BamlTypeDescriptor NullDescriptor =
        new(BamlValueKind.Null);
    private static readonly BamlTypeDescriptor BoolDescriptor =
        new(BamlValueKind.Bool);
    private static readonly BamlTypeDescriptor IntDescriptor =
        new(BamlValueKind.Int);
    private static readonly BamlTypeDescriptor FloatDescriptor =
        new(BamlValueKind.Float);
    private static readonly BamlTypeDescriptor BigIntDescriptor =
        new(BamlValueKind.BigInt);
    private static readonly BamlTypeDescriptor StringDescriptor =
        new(BamlValueKind.String);
    private static readonly BamlTypeDescriptor BytesDescriptor =
        new(BamlValueKind.Bytes);
    private static readonly BamlTypeDescriptor UnknownDescriptor =
        new(BamlTypeDescriptorKind.Unknown);

    private readonly object? payload;

    private BamlValue(
        BamlValueKind kind,
        BamlTypeDescriptor type,
        object? payload)
    {
        Kind = kind;
        Type = type;
        this.payload = payload;
    }

    public BamlValueKind Kind { get; }

    public BamlTypeDescriptor Type { get; }

    public static BamlValue Null { get; } =
        new(BamlValueKind.Null, NullDescriptor, payload: null);

    public static BamlValue Bool(bool value) =>
        new(BamlValueKind.Bool, BoolDescriptor, value);

    public static BamlValue Int(long value)
    {
        BamlInteger.Require(value, nameof(value));
        return new BamlValue(
            BamlValueKind.Int,
            IntDescriptor,
            value);
    }

    public static BamlValue Float(double value) =>
        new(BamlValueKind.Float, FloatDescriptor, value);

    public static BamlValue BigInt(BigInteger value) =>
        new(BamlValueKind.BigInt, BigIntDescriptor, value);

    public static BamlValue String(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new BamlValue(
            BamlValueKind.String,
            StringDescriptor,
            value);
    }

    public static BamlValue Bytes(ReadOnlyMemory<byte> value)
    {
        BamlValueLimits.RequireBytes(value.Length, "$");
        return new BamlValue(
            BamlValueKind.Bytes,
            BytesDescriptor,
            value.Span.ToArray());
    }

    public static BamlValue List(
        IEnumerable<BamlValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (values.TryGetNonEnumeratedCount(out int count))
        {
            BamlValueLimits.RequireCollection(count, "$");
        }

        BamlValue[] snapshot = values.ToArray();
        BamlValueLimits.RequireCollection(snapshot.Length, "$");
        if (snapshot.Any(value => value is null))
        {
            throw new BamlTypeMappingException(
                typeof(BamlValue),
                "$",
                replacement: "BamlValue.Null",
                "dynamic lists must not contain CLR null");
        }

        BamlValue result = new(
            BamlValueKind.List,
            new BamlTypeDescriptor(
                BamlValueKind.List,
                arguments: [UnknownDescriptor]),
            new ReadOnlyCollection<BamlValue>(snapshot));
        BamlValueLimits.ValidateGraph(result);
        return result;
    }

    public static BamlValue Map(
        IEnumerable<KeyValuePair<string, BamlValue>> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (values.TryGetNonEnumeratedCount(out int count))
        {
            BamlValueLimits.RequireCollection(count, "$");
        }

        KeyValuePair<string, BamlValue>[] snapshot = values
            .Select(
                pair =>
                {
                    ArgumentNullException.ThrowIfNull(pair.Key);
                    if (pair.Value is null)
                    {
                        throw new BamlTypeMappingException(
                            typeof(BamlValue),
                            $"$[{pair.Key}]",
                            replacement: "BamlValue.Null",
                            "dynamic maps must not contain CLR null values");
                    }

                    return pair;
                })
            .OrderBy(pair => pair.Key, StringComparer.Ordinal)
            .ToArray();
        BamlValueLimits.RequireCollection(snapshot.Length, "$");
        for (int index = 1; index < snapshot.Length; index++)
        {
            if (StringComparer.Ordinal.Equals(
                    snapshot[index - 1].Key,
                    snapshot[index].Key))
            {
                throw new BamlTypeMappingException(
                    typeof(string),
                    $"$[{snapshot[index].Key}]",
                    replacement: null,
                    "duplicate canonical map key");
            }
        }

        BamlValue result = new(
            BamlValueKind.Map,
            new BamlTypeDescriptor(
                BamlValueKind.Map,
                arguments:
                [
                    StringDescriptor,
                    UnknownDescriptor,
                ]),
            new ReadOnlyCollection<
                KeyValuePair<string, BamlValue>>(snapshot));
        BamlValueLimits.ValidateGraph(result);
        return result;
    }

    public static BamlValue From<T>(T value) =>
        BamlDynamicRegistry.Encode(value);

    public bool TryGet<T>(
        [MaybeNullWhen(false)] out T value) =>
        BamlDynamicRegistry.TryDecode(this, out value);

    public T As<T>() =>
        TryGet<T>(out T? value)
            ? value!
            : throw new BamlTypeMappingException(
                typeof(T),
                "$",
                replacement: null,
                $"BAML value {Type} cannot decode as {typeof(T)}");

    public bool TryGetEnumVariant(
        [NotNullWhen(true)] out string? wireVariant)
    {
        if (Kind == BamlValueKind.Enum)
        {
            wireVariant = (string)payload!;
            return true;
        }

        wireVariant = null;
        return false;
    }

    public bool TryGetClassFields(
        [NotNullWhen(true)] out
            IReadOnlyList<KeyValuePair<string, BamlValue>>? fields)
    {
        if (Kind == BamlValueKind.Class)
        {
            fields = ((BamlNominalPayload)payload!).Fields;
            return true;
        }

        fields = null;
        return false;
    }

    public bool TryGetUnion(
        out int activeCase,
        [NotNullWhen(true)] out BamlValue? value)
    {
        if (Kind == BamlValueKind.Union)
        {
            BamlUnionPayload union = (BamlUnionPayload)payload!;
            activeCase = union.ActiveCase;
            value = union.Value;
            return true;
        }

        activeCase = 0;
        value = null;
        return false;
    }

    internal static BamlValue Enum(
        string fqn,
        string value)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fqn);
        ArgumentException.ThrowIfNullOrWhiteSpace(value);
        return new BamlValue(
            BamlValueKind.Enum,
            new BamlTypeDescriptor(BamlValueKind.Enum, fqn),
            value);
    }

    internal static BamlValue Class(
        string fqn,
        IEnumerable<BamlTypeDescriptor> typeArguments,
        IEnumerable<KeyValuePair<string, BamlValue>> fields)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(fqn);
        KeyValuePair<string, BamlValue>[] snapshot =
            fields.ToArray();
        if (snapshot.Any(
                field => global::System.String.IsNullOrWhiteSpace(
                        field.Key)
                    || field.Value is null))
        {
            throw new BamlTypeMappingException(
                typeof(BamlValue),
                "$",
                replacement: null,
                "class fields are malformed");
        }

        return new BamlValue(
            BamlValueKind.Class,
            new BamlTypeDescriptor(
                BamlValueKind.Class,
                fqn,
                typeArguments),
            new BamlNominalPayload(
                new ReadOnlyCollection<
                    KeyValuePair<string, BamlValue>>(
                    snapshot)));
    }

    internal static BamlValue Union(
        IEnumerable<BamlTypeDescriptor> arms,
        int activeCase,
        BamlValue value)
    {
        BamlTypeDescriptor[] descriptors = arms.ToArray();
        if (activeCase < 0 || activeCase >= descriptors.Length)
        {
            throw new ArgumentOutOfRangeException(nameof(activeCase));
        }

        if (!descriptors[activeCase].Equals(value.Type))
        {
            throw new BamlTypeMappingException(
                typeof(BamlValue),
                "$",
                replacement: null,
                "union selected-arm metadata contradicts its payload");
        }

        return new BamlValue(
            BamlValueKind.Union,
            new BamlTypeDescriptor(
                BamlValueKind.Union,
                arguments: descriptors),
            new BamlUnionPayload(activeCase, value));
    }

    internal static BamlValue Alias(
        string alias,
        BamlValue value) =>
        new(
            value.Kind,
            new BamlTypeDescriptor(
                value.Type.Kind,
                value.Type.Fqn,
                value.Type.Arguments,
                value.Type.IsNullable,
                alias,
                value.Type.Literal),
            value.payload);

    internal static BamlValue Literal(
        BamlValue value,
        string canonicalLiteral)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(canonicalLiteral);
        if (value.Kind is not (
            BamlValueKind.Bool
            or BamlValueKind.Int
            or BamlValueKind.Float
            or BamlValueKind.BigInt
            or BamlValueKind.String))
        {
            throw new BamlTypeMappingException(
                typeof(BamlValue),
                "$",
                replacement: null,
                "only scalar BAML values may carry literal metadata");
        }

        string expected = value.Kind switch
        {
            BamlValueKind.Bool =>
                (bool)value.payload! ? "true" : "false",
            BamlValueKind.Int =>
                ((long)value.payload!).ToString(
                    CultureInfo.InvariantCulture),
            BamlValueKind.BigInt =>
                ((BigInteger)value.payload!).ToString(
                    "x",
                    CultureInfo.InvariantCulture),
            BamlValueKind.String => (string)value.payload!,
            _ => canonicalLiteral,
        };
        if (!StringComparer.Ordinal.Equals(
                expected,
                canonicalLiteral))
        {
            throw new BamlTypeMappingException(
                typeof(BamlValue),
                "$",
                replacement: expected,
                "literal metadata contradicts its payload");
        }

        return new BamlValue(
            value.Kind,
            new BamlTypeDescriptor(
                value.Type.Kind,
                value.Type.Fqn,
                value.Type.Arguments,
                value.Type.IsNullable,
                value.Type.Alias,
                canonicalLiteral),
            value.payload);
    }

    internal static BamlValue Media<T>(T value)
        where T : class
    {
        ArgumentNullException.ThrowIfNull(value);
        _ = value switch
        {
            BamlImage => true,
            BamlAudio => true,
            BamlVideo => true,
            BamlPdf => true,
            _ => throw new BamlTypeMappingException(
                typeof(T),
                "$",
                replacement: null,
                "value is not a canonical BAML media type"),
        };
        return new BamlValue(
            BamlValueKind.Media,
            new BamlTypeDescriptor(BamlValueKind.Media),
            value);
    }

    internal static BamlValue Handle(
        BamlHandle value,
        string fqn)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentException.ThrowIfNullOrWhiteSpace(fqn);
        return new BamlValue(
            BamlValueKind.Handle,
            new BamlTypeDescriptor(BamlValueKind.Handle, fqn),
            value);
    }

    internal object? PayloadForProbe => payload;

    public bool Equals(BamlValue? other)
    {
        if (other is null
            || Kind != other.Kind
            || !Type.Equals(other.Type))
        {
            return false;
        }

        return Kind switch
        {
            BamlValueKind.Null => true,
            BamlValueKind.Bytes =>
                ((byte[])payload!).AsSpan().SequenceEqual(
                    (byte[])other.payload!),
            BamlValueKind.List =>
                ((IReadOnlyList<BamlValue>)payload!)
                    .SequenceEqual(
                        (IReadOnlyList<BamlValue>)other.payload!),
            BamlValueKind.Map =>
                ((IReadOnlyList<
                    KeyValuePair<string, BamlValue>>)payload!)
                    .SequenceEqual(
                        (IReadOnlyList<
                            KeyValuePair<string, BamlValue>>)
                        other.payload!),
            BamlValueKind.Handle =>
                ReferenceEquals(payload, other.payload),
            _ => Equals(payload, other.payload),
        };
    }

    public override bool Equals(object? obj) =>
        Equals(obj as BamlValue);

    public override int GetHashCode()
    {
        HashCode hash = new();
        hash.Add(Kind);
        hash.Add(Type);
        switch (Kind)
        {
            case BamlValueKind.Bytes:
                foreach (byte value in (byte[])payload!)
                {
                    hash.Add(value);
                }

                break;
            case BamlValueKind.List:
                foreach (BamlValue value
                    in (IReadOnlyList<BamlValue>)payload!)
                {
                    hash.Add(value);
                }

                break;
            case BamlValueKind.Map:
                foreach ((string key, BamlValue value)
                    in (IReadOnlyList<
                        KeyValuePair<string, BamlValue>>)payload!)
                {
                    hash.Add(key, StringComparer.Ordinal);
                    hash.Add(value);
                }

                break;
            case BamlValueKind.Handle:
                hash.Add(
                    System.Runtime.CompilerServices.RuntimeHelpers
                        .GetHashCode(payload!));
                break;
            default:
                hash.Add(payload);
                break;
        }

        return hash.ToHashCode();
    }

    public override string ToString() =>
        $"{Kind}(<redacted>)";

    private sealed class BamlNominalPayload
        : IEquatable<BamlNominalPayload>
    {
        internal BamlNominalPayload(
            IReadOnlyList<
                KeyValuePair<string, BamlValue>> fields)
        {
            Fields = fields;
        }

        internal IReadOnlyList<
            KeyValuePair<string, BamlValue>> Fields { get; }

        public bool Equals(BamlNominalPayload? other) =>
            other is not null
            && Fields.SequenceEqual(other.Fields);

        public override bool Equals(object? obj) =>
            Equals(obj as BamlNominalPayload);

        public override int GetHashCode()
        {
            HashCode hash = new();
            foreach ((string key, BamlValue value) in Fields)
            {
                hash.Add(key, StringComparer.Ordinal);
                hash.Add(value);
            }

            return hash.ToHashCode();
        }
    }

    private sealed record BamlUnionPayload(
        int ActiveCase,
        BamlValue Value);
}

public sealed class BamlTypeMappingException : Exception
{
    internal BamlTypeMappingException(
        Type clrType,
        string path,
        string? replacement,
        string message)
        : base(message)
    {
        ClrType = clrType;
        Path = path;
        CanonicalReplacement = replacement;
    }

    public Type ClrType { get; }

    public string Path { get; }

    public string? CanonicalReplacement { get; }
}

internal static class BamlValueLimits
{
    internal const int MaxDepth = 64;
    internal const int MaxCollectionItems = 1_000_000;
    internal const int MaxBytes = 64 * 1024 * 1024;
    internal const int MaxNodes = 2_000_000;

    internal static void RequireBytes(int length, string path)
    {
        if (length > MaxBytes)
        {
            throw Limit(path, nameof(MaxBytes));
        }
    }

    internal static void RequireCollection(int count, string path)
    {
        if (count > MaxCollectionItems)
        {
            throw Limit(path, nameof(MaxCollectionItems));
        }
    }

    internal static void ValidateGraph(BamlValue root)
    {
        int nodes = 0;
        Visit(root, "$", depth: 0, ref nodes);
    }

    private static void Visit(
        BamlValue value,
        string path,
        int depth,
        ref int nodes)
    {
        if (depth > MaxDepth)
        {
            throw Limit(path, nameof(MaxDepth));
        }

        nodes++;
        if (nodes > MaxNodes)
        {
            throw Limit(path, nameof(MaxNodes));
        }

        if (value.Kind == BamlValueKind.List)
        {
            IReadOnlyList<BamlValue> values =
                (IReadOnlyList<BamlValue>)value.PayloadForProbe!;
            for (int index = 0; index < values.Count; index++)
            {
                Visit(
                    values[index],
                    $"{path}[{index}]",
                    depth + 1,
                    ref nodes);
            }
        }
        else if (value.Kind == BamlValueKind.Map)
        {
            foreach ((string key, BamlValue child)
                in (IReadOnlyList<
                    KeyValuePair<string, BamlValue>>)
                value.PayloadForProbe!)
            {
                Visit(
                    child,
                    $"{path}[{key}]",
                    depth + 1,
                    ref nodes);
            }
        }
    }

    private static BamlTypeMappingException Limit(
        string path,
        string limit) =>
        new(
            typeof(BamlValue),
            path,
            replacement: null,
            $"BAML value exceeded {limit}");
}

internal static class BamlBigIntCodec
{
    internal const int MaxHexLength = (1 << 28) / 4 + 2;

    internal static void RequireHexLength(int length)
    {
        if (length > MaxHexLength)
        {
            throw new BamlTypeMappingException(
                typeof(BigInteger),
                "$",
                replacement: null,
                $"bigint hex exceeded {MaxHexLength}");
        }
    }
}

internal static class BamlDynamicRegistry
{
    private static readonly object Gate = new();
    private static readonly Dictionary<
        Type,
        (Func<object?, BamlValue> Encode, Func<BamlValue, object?> Decode)>
        Codecs = [];
    private static readonly Dictionary<
        Type,
        (Func<object?, BamlValue> Encode,
            Func<BamlValue, (bool Success, object? Value)> Decode)>
        CanonicalCollectionCodecs = [];

    internal static void Register<T>(
        Func<T, BamlValue> encode,
        Func<BamlValue, T> decode)
    {
        ArgumentNullException.ThrowIfNull(encode);
        ArgumentNullException.ThrowIfNull(decode);
        lock (Gate)
        {
            if (!Codecs.TryAdd(
                    typeof(T),
                    (
                        value => encode((T)value!),
                        value => decode(value))))
            {
                throw new InvalidOperationException(
                    $"codec already registered for {typeof(T)}");
            }
        }
    }

    internal static void RegisterCanonicalList<TElement>()
    {
        Type collectionType = typeof(IReadOnlyList<TElement>);
        lock (Gate)
        {
            if (!CanonicalCollectionCodecs.TryAdd(
                    collectionType,
                    (
                        value => BamlValue.List(
                            ((IReadOnlyList<TElement>)value!)
                                .Select(BamlValue.From)),
                        value =>
                        {
                            if (value.Kind != BamlValueKind.List)
                            {
                                return (false, null);
                            }

                            IReadOnlyList<BamlValue> encoded =
                                (IReadOnlyList<BamlValue>)
                                value.PayloadForProbe!;
                            TElement[] items =
                                new TElement[encoded.Count];
                            for (int index = 0;
                                index < encoded.Count;
                                index++)
                            {
                                if (!encoded[index].TryGet(
                                        out TElement? item))
                                {
                                    return (false, null);
                                }

                                items[index] = item!;
                            }

                            return (
                                true,
                                new ReadOnlyCollection<TElement>(
                                    items));
                        })))
            {
                throw new InvalidOperationException(
                    $"canonical collection codec already registered for "
                    + $"{collectionType}");
            }
        }
    }

    internal static void RegisterCanonicalStringMap<TValue>()
    {
        Type collectionType =
            typeof(IReadOnlyDictionary<string, TValue>);
        lock (Gate)
        {
            if (!CanonicalCollectionCodecs.TryAdd(
                    collectionType,
                    (
                        value => BamlValue.Map(
                            ((IReadOnlyDictionary<string, TValue>)
                                value!)
                            .Select(
                                pair => new KeyValuePair<
                                    string,
                                    BamlValue>(
                                    pair.Key,
                                    BamlValue.From(pair.Value)))),
                        value =>
                        {
                            if (value.Kind != BamlValueKind.Map)
                            {
                                return (false, null);
                            }

                            Dictionary<string, TValue> items =
                                new(StringComparer.Ordinal);
                            foreach ((string key, BamlValue encoded)
                                in (IReadOnlyList<
                                    KeyValuePair<string, BamlValue>>)
                                value.PayloadForProbe!)
                            {
                                if (!encoded.TryGet(
                                        out TValue? item))
                                {
                                    return (false, null);
                                }

                                if (!items.TryAdd(key, item!))
                                {
                                    return (false, null);
                                }
                            }

                            return (
                                true,
                                new ReadOnlyDictionary<
                                    string,
                                    TValue>(items));
                        })))
            {
                throw new InvalidOperationException(
                    $"canonical collection codec already registered for "
                    + $"{collectionType}");
            }
        }
    }

    internal static BamlValue Encode<T>(T value)
    {
        if (value is null)
        {
            if (Nullable.GetUnderlyingType(typeof(T)) is not null)
            {
                return BamlValue.Null;
            }

            throw new BamlTypeMappingException(
                typeof(T),
                "$",
                replacement: "BamlValue.Null or an explicit nullable codec",
                "CLR null has no context-free BAML descriptor");
        }

        return value switch
        {
            bool item => BamlValue.Bool(item),
            long item => BamlValue.Int(item),
            double item => BamlValue.Float(item),
            BigInteger item => BamlValue.BigInt(item),
            string item => BamlValue.String(item),
            ReadOnlyMemory<byte> item => BamlValue.Bytes(item),
            byte[] item => BamlValue.Bytes(item),
            BamlImage item => BamlValue.Media(item),
            BamlAudio item => BamlValue.Media(item),
            BamlVideo item => BamlValue.Media(item),
            BamlPdf item => BamlValue.Media(item),
            BamlHandle item => BamlValue.Handle(
                item,
                "probe.Resource"),
            BamlValue item => item,
            _ => EncodeRegistered(value),
        };
    }

    internal static bool TryDecode<T>(
        BamlValue value,
        [MaybeNullWhen(false)] out T result)
    {
        ArgumentNullException.ThrowIfNull(value);
        if (typeof(T) == typeof(BamlValue))
        {
            result = (T)(object)value;
            return true;
        }

        Type requestedType = typeof(T);
        if (value.Kind == BamlValueKind.Null)
        {
            if (IsCanonicalNullableTarget(requestedType))
            {
                result = default!;
                return true;
            }

            result = default!;
            return false;
        }

        Type canonicalType =
            Nullable.GetUnderlyingType(requestedType)
            ?? requestedType;
        if (TryDecodeCanonical(canonicalType, value, out object? decoded))
        {
            if (decoded is T canonical)
            {
                result = canonical;
                return true;
            }

            result = default!;
            return false;
        }

        lock (Gate)
        {
            if (CanonicalCollectionCodecs.TryGetValue(
                    requestedType,
                    out var collectionCodec))
            {
                if (!DescriptorMatches(requestedType, value.Type))
                {
                    result = default!;
                    return false;
                }

                (bool success, object? collection) =
                    collectionCodec.Decode(value);
                if (success && collection is T typedCollection)
                {
                    result = typedCollection;
                    return true;
                }

                result = default!;
                return false;
            }

            if (!Codecs.TryGetValue(requestedType, out var codec)
                || !DescriptorMatches(requestedType, value.Type))
            {
                result = default!;
                return false;
            }

            decoded = codec.Decode(value);
        }

        if (decoded is T typed)
        {
            result = typed;
            return true;
        }

        result = default!;
        return false;
    }

    private static BamlValue EncodeRegistered<T>(T value)
    {
        lock (Gate)
        {
            if (CanonicalCollectionCodecs.TryGetValue(
                    typeof(T),
                    out var collectionCodec))
            {
                return collectionCodec.Encode(value);
            }

            if (!Codecs.TryGetValue(
                    typeof(T),
                    out var codec))
            {
                throw new BamlTypeMappingException(
                    typeof(T),
                    "$",
                    replacement: CanonicalReplacement(typeof(T)),
                    "CLR type has no explicit BAML codec");
            }

            return codec.Encode(value);
        }
    }

    private static bool TryDecodeCanonical(
        Type type,
        BamlValue value,
        out object? decoded)
    {
        if (!DescriptorMatches(type, value.Type))
        {
            decoded = null;
            return false;
        }

        decoded = (type, value.Kind) switch
        {
            ({ } candidate, BamlValueKind.Bool)
                when candidate == typeof(bool) =>
                value.PayloadForProbe,
            ({ } candidate, BamlValueKind.Int)
                when candidate == typeof(long) =>
                value.PayloadForProbe,
            ({ } candidate, BamlValueKind.Float)
                when candidate == typeof(double) =>
                value.PayloadForProbe,
            ({ } candidate, BamlValueKind.BigInt)
                when candidate == typeof(BigInteger) =>
                value.PayloadForProbe,
            ({ } candidate, BamlValueKind.String)
                when candidate == typeof(string) =>
                value.PayloadForProbe,
            ({ } candidate, BamlValueKind.Bytes)
                when candidate == typeof(ReadOnlyMemory<byte>) =>
                new ReadOnlyMemory<byte>(
                    ((byte[])value.PayloadForProbe!).ToArray()),
            _ => null,
        };
        return decoded is not null;
    }

    private static bool DescriptorMatches(
        Type type,
        BamlTypeDescriptor actual)
    {
        try
        {
            BamlTypeDescriptor expected =
                BamlClrTypeBinder.Describe(type, "$");
            if (expected.IsNullable)
            {
                expected = new BamlTypeDescriptor(
                    expected.Kind,
                    expected.Fqn,
                    expected.Arguments,
                    isNullable: false,
                    expected.Alias,
                    expected.Literal);
            }

            return DescriptorCompatible(expected, actual);
        }
        catch (BamlTypeMappingException)
        {
            return false;
        }
    }

    private static bool DescriptorCompatible(
        BamlTypeDescriptor expected,
        BamlTypeDescriptor actual)
    {
        if (actual.Kind == BamlTypeDescriptorKind.Unknown)
        {
            return true;
        }

        return expected.Kind == actual.Kind
            && StringComparer.Ordinal.Equals(
                expected.Fqn,
                actual.Fqn)
            && expected.IsNullable == actual.IsNullable
            && StringComparer.Ordinal.Equals(
                expected.Alias,
                actual.Alias)
            && StringComparer.Ordinal.Equals(
                expected.Literal,
                actual.Literal)
            && expected.Arguments.Count == actual.Arguments.Count
            && expected.Arguments
                .Zip(actual.Arguments)
                .All(pair => DescriptorCompatible(
                    pair.First,
                    pair.Second));
    }

    private static bool IsCanonicalNullableTarget(Type type) =>
        Nullable.GetUnderlyingType(type) is not null
        || (type.IsGenericType
            && type.GetGenericTypeDefinition()
                == typeof(BamlNullable<>));

    private static string? CanonicalReplacement(Type type) =>
        type == typeof(int)
            ? "long"
            : type == typeof(float)
                ? "double"
                : type == typeof(decimal)
                    ? "double or an explicit BAML model"
                    : null;
}
