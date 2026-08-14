using System.Numerics;

namespace Baml;

public readonly struct BamlUnion<T0, T1>
    : IEquatable<BamlUnion<T0, T1>>
{
    private readonly byte caseIndex;
    private readonly T0 value0;
    private readonly T1 value1;

    private BamlUnion(byte caseIndex, T0 value0, T1 value1)
    {
        this.caseIndex = caseIndex;
        this.value0 = value0;
        this.value1 = value1;
    }

    public bool IsT0 => caseIndex == 1;

    public bool IsT1 => caseIndex == 2;

    public T0 AsT0 => caseIndex == 1
        ? value0
        : throw InvalidCase(0);

    public T1 AsT1 => caseIndex == 2
        ? value1
        : throw InvalidCase(1);

    public static BamlUnion<T0, T1> FromT0(T0 value) =>
        new(1, value, default!);

    public static BamlUnion<T0, T1> FromT1(T1 value) =>
        new(2, default!, value);

    public TResult Match<TResult>(
        Func<T0, TResult> onT0,
        Func<T1, TResult> onT1)
    {
        ArgumentNullException.ThrowIfNull(onT0);
        ArgumentNullException.ThrowIfNull(onT1);
        return caseIndex switch
        {
            1 => onT0(value0),
            2 => onT1(value1),
            _ => throw new InvalidOperationException(
                "The BAML union is uninitialized."),
        };
    }

    public void Switch(
        Action<T0> onT0,
        Action<T1> onT1)
    {
        ArgumentNullException.ThrowIfNull(onT0);
        ArgumentNullException.ThrowIfNull(onT1);
        switch (caseIndex)
        {
            case 1:
                onT0(value0);
                return;
            case 2:
                onT1(value1);
                return;
            default:
                throw new InvalidOperationException(
                    "The BAML union is uninitialized.");
        }
    }

    public static implicit operator BamlUnion<T0, T1>(
        T0 value) =>
        FromT0(value);

    public static implicit operator BamlUnion<T0, T1>(
        T1 value) =>
        FromT1(value);

    public bool Equals(BamlUnion<T0, T1> other) =>
        caseIndex == other.caseIndex
        && caseIndex switch
        {
            0 => true,
            1 => EqualityComparer<T0>.Default.Equals(
                value0,
                other.value0),
            2 => EqualityComparer<T1>.Default.Equals(
                value1,
                other.value1),
            _ => false,
        };

    public override bool Equals(object? obj) =>
        obj is BamlUnion<T0, T1> other && Equals(other);

    public override int GetHashCode() =>
        caseIndex switch
        {
            0 => 0,
            1 => HashCode.Combine(
                1,
                EqualityComparer<T0>.Default.GetHashCode(value0!)),
            2 => HashCode.Combine(
                2,
                EqualityComparer<T1>.Default.GetHashCode(value1!)),
            _ => 0,
        };

    public static bool operator ==(
        BamlUnion<T0, T1> left,
        BamlUnion<T0, T1> right) =>
        left.Equals(right);

    public static bool operator !=(
        BamlUnion<T0, T1> left,
        BamlUnion<T0, T1> right) =>
        !left.Equals(right);

    internal int ActiveCaseForCodec => caseIndex switch
    {
        1 => 0,
        2 => 1,
        _ => throw new InvalidOperationException(
            "The BAML union is uninitialized."),
    };

    internal object? ValueForCodec => caseIndex switch
    {
        1 => value0,
        2 => value1,
        _ => throw new InvalidOperationException(
            "The BAML union is uninitialized."),
    };

    private InvalidOperationException InvalidCase(int requested) =>
        new(
            caseIndex == 0
                ? "The BAML union is uninitialized."
                : $"The BAML union does not contain T{requested}.");
}

internal static class BamlClrTypeBinder
{
    private static readonly object Gate = new();
    private static readonly Dictionary<
        Type,
        BamlTypeDescriptor> Registered = [];

    internal static void Register(
        Type type,
        BamlTypeDescriptor descriptor)
    {
        ArgumentNullException.ThrowIfNull(type);
        ArgumentNullException.ThrowIfNull(descriptor);
        lock (Gate)
        {
            if (!Registered.TryAdd(type, descriptor))
            {
                throw new InvalidOperationException(
                    $"type already registered: {type}");
            }
        }
    }

    internal static BamlTypeDescriptor Describe<T>() =>
        Describe(typeof(T), "$T");

    internal static BamlTypeDescriptor Describe(
        Type type,
        string path)
    {
        ArgumentNullException.ThrowIfNull(type);
        if (type == typeof(bool))
        {
            return new BamlTypeDescriptor(BamlValueKind.Bool);
        }

        if (type == typeof(long))
        {
            return new BamlTypeDescriptor(BamlValueKind.Int);
        }

        if (type == typeof(double))
        {
            return new BamlTypeDescriptor(BamlValueKind.Float);
        }

        if (type == typeof(BigInteger))
        {
            return new BamlTypeDescriptor(BamlValueKind.BigInt);
        }

        if (type == typeof(string))
        {
            return new BamlTypeDescriptor(BamlValueKind.String);
        }

        if (type == typeof(ReadOnlyMemory<byte>))
        {
            return new BamlTypeDescriptor(BamlValueKind.Bytes);
        }

        if (type == typeof(BamlValue))
        {
            return new BamlTypeDescriptor(
                BamlTypeDescriptorKind.Unknown);
        }

        string? mediaFqn = type == typeof(BamlImage)
            ? "baml.media.Image"
            : type == typeof(BamlAudio)
                ? "baml.media.Audio"
                : type == typeof(BamlVideo)
                    ? "baml.media.Video"
                    : type == typeof(BamlPdf)
                        ? "baml.media.Pdf"
                        : null;
        if (mediaFqn is not null)
        {
            return new BamlTypeDescriptor(
                BamlValueKind.Media);
        }

        if (type == typeof(BamlHandle))
        {
            return new BamlTypeDescriptor(
                BamlValueKind.Handle,
                "probe.Resource");
        }

        lock (Gate)
        {
            if (Registered.TryGetValue(
                    type,
                    out BamlTypeDescriptor? registered))
            {
                return registered;
            }
        }

        if (!type.IsGenericType)
        {
            throw Unsupported(type, path);
        }

        Type definition = type.GetGenericTypeDefinition();
        Type[] arguments = type.GetGenericArguments();
        if (definition == typeof(Nullable<>)
            || definition == typeof(BamlNullable<>))
        {
            Type inner = arguments[0];
            if (inner.IsGenericType
                && inner.GetGenericTypeDefinition()
                    == typeof(BamlNullable<>))
            {
                throw new BamlTypeMappingException(
                    type,
                    path,
                    replacement: null,
                    "redundant BamlNullable wrappers collapse BAML null states");
            }

            BamlTypeDescriptor descriptor =
                Describe(inner, $"{path}.nullable");
            return new BamlTypeDescriptor(
                descriptor.Kind,
                descriptor.Fqn,
                descriptor.Arguments,
                isNullable: true,
                descriptor.Alias,
                descriptor.Literal);
        }

        if (definition == typeof(IReadOnlyList<>))
        {
            return new BamlTypeDescriptor(
                BamlValueKind.List,
                arguments:
                [
                    Describe(arguments[0], $"{path}[item]"),
                ]);
        }

        if (definition == typeof(IReadOnlyDictionary<,>))
        {
            BamlTypeDescriptor key = Describe(
                arguments[0],
                $"{path}[key]");
            if (arguments[0] != typeof(string)
                && key.Kind != BamlTypeDescriptorKind.Enum
                && key.Literal is null)
            {
                throw new BamlTypeMappingException(
                    arguments[0],
                    $"{path}[key]",
                    replacement: "string or a generated BAML enum",
                    "map key has no canonical wire identity");
            }

            return new BamlTypeDescriptor(
                BamlValueKind.Map,
                arguments:
                [
                    key,
                    Describe(arguments[1], $"{path}[value]"),
                ]);
        }

        if (definition == typeof(BamlUnion<,>))
        {
            throw new BamlTypeMappingException(
                type,
                path,
                replacement: null,
                "a context-free union has no occurrence-specific BAML arm descriptor");
        }

        if (definition == typeof(BamlOptional<>))
        {
            throw new BamlTypeMappingException(
                type,
                path,
                replacement: null,
                "BamlOptional is caller presence, not a BAML value type");
        }

        throw Unsupported(type, path);
    }

    private static BamlTypeMappingException Unsupported(
        Type type,
        string path)
    {
        string? replacement = type == typeof(int)
            ? "long"
            : type == typeof(float)
                ? "double"
                : type.IsGenericType
                    && type.GetGenericTypeDefinition()
                        == typeof(List<>)
                    ? "IReadOnlyList<T>"
                    : type.IsGenericType
                        && type.GetGenericTypeDefinition()
                            == typeof(Dictionary<,>)
                        ? "IReadOnlyDictionary<TKey,TValue>"
                        : null;
        return new BamlTypeMappingException(
            type,
            path,
            replacement,
            "CLR type has no canonical supported BAML translation");
    }
}

internal sealed class GeneratedCodecTraversal
{
    private readonly HashSet<object> active =
        new(ReferenceEqualityComparer.Instance);

    internal void Visit<T>(
        T value,
        string path,
        Action<GeneratedCodecTraversal, T> visit)
        where T : class
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(visit);
        if (!active.Add(value))
        {
            throw new BamlTypeMappingException(
                typeof(T),
                path,
                replacement: null,
                "generated value graph contains a reference cycle");
        }

        try
        {
            visit(this, value);
        }
        finally
        {
            active.Remove(value);
        }
    }
}
