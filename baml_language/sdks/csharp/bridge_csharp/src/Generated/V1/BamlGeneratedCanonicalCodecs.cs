using System.Collections.ObjectModel;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Generated.V1;

internal static class BamlGeneratedTypeMetadata
{
    internal static byte[] Class(
        string identity,
        IReadOnlyList<TypeDeclaration> typeArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(identity);
        ArgumentNullException.ThrowIfNull(typeArguments);
        var @class = new BamlTyClass { Name = identity };
        foreach (TypeDeclaration argument in typeArguments)
        {
            @class.TypeArgs.Add(Parse(argument.Metadata, "class type argument"));
        }

        return new BamlTy { ClassTy = @class }.ToByteArray();
    }

    internal static byte[] Class(
        string identity,
        IReadOnlyList<byte[]> typeArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(identity);
        ArgumentNullException.ThrowIfNull(typeArguments);
        var @class = new BamlTyClass { Name = identity };
        foreach (byte[] argument in typeArguments)
        {
            @class.TypeArgs.Add(Parse(argument, "class type argument"));
        }

        return new BamlTy { ClassTy = @class }.ToByteArray();
    }

    internal static byte[] Optional(byte[] inner) =>
        new BamlTy
        {
            Optional = new BamlTyOptional { Inner = Parse(inner, "optional inner") },
        }.ToByteArray();

    internal static byte[] List(byte[] item) =>
        new BamlTy
        {
            List = new BamlTyList { Item = Parse(item, "list item") },
        }.ToByteArray();

    internal static byte[] Map(byte[] key, byte[] value) =>
        new BamlTy
        {
            Map = new BamlTyMap
            {
                Key = Parse(key, "map key"),
                Value = Parse(value, "map value"),
            },
        }.ToByteArray();

    internal static byte[] Union(IReadOnlyList<byte[]> options)
    {
        ArgumentNullException.ThrowIfNull(options);
        if (options.Count < 2 || options.Count > 32)
        {
            throw new ArgumentOutOfRangeException(
                nameof(options),
                "A generated BAML union must contain 2 through 32 options.");
        }

        var union = new BamlTyUnion();
        foreach (byte[] option in options)
        {
            union.Options.Add(Parse(option, "union option"));
        }

        return new BamlTy { Union = union }.ToByteArray();
    }

    internal static string OptionName(byte[] metadata) =>
        Render(Parse(metadata, "union option"));

    private static string Render(BamlTy type) => type.TyCase switch
    {
        BamlTy.TyOneofCase.Primitive => type.Primitive.Kind switch
        {
            BamlTyPrimitiveKind.BamlTyPrimitiveString => "string",
            BamlTyPrimitiveKind.BamlTyPrimitiveInt => "int",
            BamlTyPrimitiveKind.BamlTyPrimitiveFloat => "float",
            BamlTyPrimitiveKind.BamlTyPrimitiveBool => "bool",
            BamlTyPrimitiveKind.BamlTyPrimitiveNull => "null",
            BamlTyPrimitiveKind.BamlTyPrimitiveBytes => "uint8array",
            BamlTyPrimitiveKind.BamlTyPrimitiveBigint => "bigint",
            _ => throw UnsupportedOption(type),
        },
        BamlTy.TyOneofCase.ClassTy => RenderNominal(
            type.ClassTy.Name,
            type.ClassTy.TypeArgs),
        BamlTy.TyOneofCase.Enum => RequireName(type.Enum.Name, type),
        BamlTy.TyOneofCase.List =>
            $"{RenderPostfix(type.List.Item)}[]",
        BamlTy.TyOneofCase.Map =>
            $"map<{Render(type.Map.Key)}, {Render(type.Map.Value)}>",
        BamlTy.TyOneofCase.Optional =>
            $"{Render(type.Optional.Inner)} | null",
        BamlTy.TyOneofCase.Union =>
            string.Join(" | ", type.Union.Options.Select(Render)),
        BamlTy.TyOneofCase.Literal => type.Literal.LiteralCase switch
        {
            BamlTyLiteral.LiteralOneofCase.StringValue =>
                QuoteStringLiteral(type.Literal.StringValue),
            BamlTyLiteral.LiteralOneofCase.IntValue =>
                type.Literal.IntValue.ToString(global::System.Globalization.CultureInfo.InvariantCulture),
            BamlTyLiteral.LiteralOneofCase.BoolValue =>
                type.Literal.BoolValue ? "true" : "false",
            BamlTyLiteral.LiteralOneofCase.BigintValue => $"{type.Literal.BigintValue}n",
            BamlTyLiteral.LiteralOneofCase.FloatValue => type.Literal.FloatValue,
            _ => throw UnsupportedOption(type),
        },
        BamlTy.TyOneofCase.TypeAlias => RequireName(type.TypeAlias.Name, type),
        BamlTy.TyOneofCase.Unknown => "unknown",
        BamlTy.TyOneofCase.Media => type.Media.Kind switch
        {
            BamlTyMediaKind.Image => "image",
            BamlTyMediaKind.Audio => "audio",
            BamlTyMediaKind.Video => "video",
            BamlTyMediaKind.Pdf => "pdf",
            BamlTyMediaKind.Generic => "media",
            _ => throw UnsupportedOption(type),
        },
        BamlTy.TyOneofCase.Interface => RenderNominal(
            type.Interface.Name,
            type.Interface.TypeArgs),
        BamlTy.TyOneofCase.EnumVariant =>
            $"{RequireName(type.EnumVariant.Name, type)}.{RequireName(type.EnumVariant.Variant, type)}",
        BamlTy.TyOneofCase.RustType => "$rust_type",
        BamlTy.TyOneofCase.MetaType => "type",
        BamlTy.TyOneofCase.Resource => "baml.llm.Resource",
        BamlTy.TyOneofCase.PromptAst => "baml.llm.PromptAst",
        BamlTy.TyOneofCase.Void => "void",
        BamlTy.TyOneofCase.TypeVar => RequireName(type.TypeVar.Name, type),
        BamlTy.TyOneofCase.Never => "never",
        _ => throw UnsupportedOption(type),
    };

    private static string RenderNominal(string name, IEnumerable<BamlTy> arguments)
    {
        string required = RequireName(name, null);
        string[] rendered = arguments.Select(Render).ToArray();
        return rendered.Length == 0
            ? required
            : $"{required}<{string.Join(", ", rendered)}>";
    }

    private static string RenderPostfix(BamlTy type) =>
        type.TyCase is BamlTy.TyOneofCase.Union or BamlTy.TyOneofCase.Function
            ? $"({Render(type)})"
            : Render(type);

    private static string RequireName(string name, BamlTy? type)
    {
        if (string.IsNullOrEmpty(name))
        {
            throw type is null
                ? new InvalidOperationException("Generated nominal type metadata has an empty name.")
                : UnsupportedOption(type);
        }

        return name;
    }

    private static string QuoteStringLiteral(string literal)
    {
        var quoted = new global::System.Text.StringBuilder(literal.Length + 2).Append('"');
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
                    quoted.Append("\\u").Append(
                        ((int)character).ToString(
                            "x4",
                            global::System.Globalization.CultureInfo.InvariantCulture));
                    break;
                default:
                    quoted.Append(character);
                    break;
            }
        }

        return quoted.Append('"').ToString();
    }

    private static InvalidOperationException UnsupportedOption(BamlTy type) =>
        new($"Generated union option metadata {type.TyCase} has no canonical BAML name.");

    internal static BamlTy Parse(byte[] metadata, string description)
    {
        ArgumentNullException.ThrowIfNull(metadata);
        if (metadata.Length == 0)
        {
            throw new InvalidOperationException(
                $"Generated {description} type metadata is empty.");
        }

        try
        {
            BamlTy type = BamlTy.Parser.ParseFrom(metadata);
            if (type.TyCase == BamlTy.TyOneofCase.None)
            {
                throw new InvalidOperationException(
                    $"Generated {description} type metadata has no type case.");
            }

            return type;
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new InvalidOperationException(
                $"Generated {description} type metadata is malformed.",
                error);
        }
    }
}

internal sealed class BamlGeneratedFunctionSpecCodec<TFinal>(
    BamlGeneratedType<TFinal> finalType,
    byte[] metadata)
    : IBamlGeneratedCodec<global::Baml.BamlFunctionSpec<TFinal>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        global::Baml.BamlFunctionSpec<TFinal> value) =>
        context.FunctionSpec(value, "ai.FunctionSpec", metadata);

    public global::Baml.BamlFunctionSpec<TFinal> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        context.ReadFunctionSpec(
            value,
            "ai.FunctionSpec",
            metadata,
            finalType);
}

internal sealed class BamlGeneratedNullableValueCodec<T>(BamlGeneratedType<T> inner)
    : IBamlGeneratedCodec<T?>
    where T : struct
{
    public BamlGeneratedValue Encode(BamlGeneratedCodecContext context, T? value) =>
        value.HasValue ? context.Encode(inner, value.Value) : context.Null();

    public T? Decode(BamlGeneratedCodecContext context, BamlGeneratedValue value) =>
        value.IsNull ? null : context.Decode(inner, value);
}

internal sealed class BamlGeneratedNullableCodec<T>(BamlGeneratedType<T> inner)
    : IBamlGeneratedCodec<global::Baml.BamlNullable<T>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        global::Baml.BamlNullable<T> value) =>
        value.IsNull ? context.Null() : context.Encode(inner, value.Value);

    public global::Baml.BamlNullable<T> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value) =>
        value.IsNull
            ? global::Baml.BamlNullable<T>.Null
            : global::Baml.BamlNullable<T>.FromValue(context.Decode(inner, value));
}

internal sealed class BamlGeneratedUnionCodec<T0, T1>(
    BamlGeneratedType<T0> type0,
    BamlGeneratedType<T1> type1,
    byte[] selfMetadata,
    byte[] metadata0,
    byte[] metadata1,
    string option0,
    string option1) : IBamlGeneratedCodec<global::Baml.BamlUnion<T0, T1>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        global::Baml.BamlUnion<T0, T1> value) =>
        value.Match(
            item => context.Union(
                selfMetadata,
                metadata0,
                option0,
                context.Encode(type0, item)),
            item => context.Union(
                selfMetadata,
                metadata1,
                option1,
                context.Encode(type1, item)));

    public global::Baml.BamlUnion<T0, T1> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        BamlGeneratedUnionValue selected = context.ReadUnion(
            value,
            selfMetadata,
            new string[] { option0, option1 },
            new byte[][] { metadata0, metadata1 });
        return selected.CaseIndex switch
        {
            0 => global::Baml.BamlUnion<T0, T1>.FromT0(
                context.Decode(type0, selected.Value)),
            1 => global::Baml.BamlUnion<T0, T1>.FromT1(
                context.Decode(type1, selected.Value)),
            _ => context.Fail<global::Baml.BamlUnion<T0, T1>>(
                "The native bridge returned an invalid BAML union case.",
                $"Union case {selected.CaseIndex} was outside the generated descriptor."),
        };
    }
}

internal sealed class BamlGeneratedListCodec<T>(
    BamlGeneratedType<T> item,
    byte[] itemMetadata) : IBamlGeneratedCodec<IReadOnlyList<T>>
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        IReadOnlyList<T> value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var items = new List<BamlGeneratedValue>(value.Count);
        foreach (T element in value)
        {
            items.Add(context.Encode(item, element));
        }

        return context.List(items, itemMetadata);
    }

    public IReadOnlyList<T> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        IReadOnlyList<BamlGeneratedValue> items = context.ReadList(value, itemMetadata);
        var result = new T[items.Count];
        for (int index = 0; index < result.Length; index++)
        {
            result[index] = context.Decode(item, items[index]);
        }

        return Array.AsReadOnly(result);
    }
}

internal sealed class BamlGeneratedMapCodec<TKey, TValue>(
    BamlGeneratedType<TKey> key,
    BamlGeneratedType<TValue> valueType,
    byte[] keyMetadata,
    byte[] valueMetadata) : IBamlGeneratedCodec<IReadOnlyDictionary<TKey, TValue>>
    where TKey : notnull
{
    public BamlGeneratedValue Encode(
        BamlGeneratedCodecContext context,
        IReadOnlyDictionary<TKey, TValue> value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var entries = new List<KeyValuePair<string, BamlGeneratedValue>>(value.Count);
        foreach (KeyValuePair<TKey, TValue> entry in value)
        {
            BamlGeneratedValue encodedKey = context.Encode(key, entry.Key);
            string wireKey = encodedKey.ReadString();
            entries.Add(new(wireKey, context.Encode(valueType, entry.Value)));
        }

        return context.Map(entries, keyMetadata, valueMetadata);
    }

    public IReadOnlyDictionary<TKey, TValue> Decode(
        BamlGeneratedCodecContext context,
        BamlGeneratedValue value)
    {
        IReadOnlyDictionary<string, BamlGeneratedValue> entries =
            context.ReadMap(value, keyMetadata, valueMetadata);
        var result = new Dictionary<TKey, TValue>();
        foreach (KeyValuePair<string, BamlGeneratedValue> entry in entries)
        {
            BamlGeneratedValue encodedKey = BamlGeneratedValue.CreateString(entry.Key);
            TKey decodedKey = context.Decode(key, encodedKey);
            if (!result.TryAdd(decodedKey, context.Decode(valueType, entry.Value)))
            {
                throw new global::Baml.BamlProtocolException(
                    "The native bridge returned duplicate projected BAML map keys.",
                    $"Map key {entry.Key} collided after typed decoding.");
            }
        }

        return new ReadOnlyDictionary<TKey, TValue>(result);
    }
}
