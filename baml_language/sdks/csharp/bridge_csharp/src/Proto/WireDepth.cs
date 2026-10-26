using BamlBridge.Cffi.V1;

namespace Baml.Proto;

/// <summary>
/// How deep the wire nests, measured as the engine's decoder measures it.
/// </summary>
/// <remarks>
/// The decoder refuses a message nested more than <see cref="Limit"/> messages
/// deep, before it reads any key the message carries, so a value deeper than
/// that would be refused with its handle clones stranded. Every value is
/// therefore measured before it is sent, counting each nested message where
/// the schema nests it: a list level costs two (<c>InboundListValue</c>, the
/// item's <c>InboundValue</c>), a map or class level three (the value message,
/// the <c>InboundMapEntry</c>, the entry's <c>InboundValue</c>), and a stated
/// node also nests its statement's <c>BamlTy</c> tree. This mirrors
/// <c>bridge_ctypes::wire_depth</c>, which pins the accounting to the decoder.
/// </remarks>
internal static class WireDepth
{
    /// <summary>How many nested messages the decoder reads before it refuses one.</summary>
    internal const int Limit = 100;

    /// <summary>
    /// The messages that enclose a call argument's value: <c>CallFunctionArgs</c>
    /// and the argument's <c>InboundMapEntry</c>.
    /// </summary>
    internal const int Argument = 2;

    /// <summary>
    /// The message that encloses a host callback's returned or thrown value:
    /// the <c>HostCallCompletion</c>.
    /// </summary>
    internal const int Result = 1;

    /// <summary>
    /// Whether <paramref name="value"/>, enclosed by <paramref name="enclosing"/>
    /// messages, stays within <see cref="Limit"/>. The walk stops at the first
    /// node past the limit, so it never recurses deeper than the wire does.
    /// </summary>
    internal static bool Fits(InboundValue value, int enclosing)
    {
        ArgumentNullException.ThrowIfNull(value);
        return ValueFits(value, enclosing);
    }

    /// <summary>
    /// Refuses <paramref name="value"/> when it nests deeper than the wire
    /// carries, as a value too large to carry: the caller's argument error, a
    /// callback's bridge failure.
    /// </summary>
    internal static void Require(InboundValue value, int enclosing, string path)
    {
        if (!Fits(value, enclosing))
        {
            throw new BamlTypeMappingException(
                typeof(InboundValue),
                "wire value",
                path,
                $"The value at {path} nests deeper than the {Limit} protobuf messages the wire carries.",
                fault: BamlTypeMappingFault.ExceedsLimit);
        }
    }

    // `level` is the number of messages enclosing the node; a node fits when
    // `level` plus the messages nested below it is at most `Limit`.
    private static bool ValueFits(InboundValue value, int level)
    {
        if (level > Limit)
        {
            return false;
        }

        if (value.ValueType is { } statement && !TypeFits(statement, level + 1))
        {
            return false;
        }

        switch (value.ValueCase)
        {
            case InboundValue.ValueOneofCase.None:
            case InboundValue.ValueOneofCase.StringValue:
            case InboundValue.ValueOneofCase.IntValue:
            case InboundValue.ValueOneofCase.FloatValue:
            case InboundValue.ValueOneofCase.BoolValue:
            case InboundValue.ValueOneofCase.Uint8ArrayValue:
            case InboundValue.ValueOneofCase.BigintValue:
            case InboundValue.ValueOneofCase.JsNumberValue:
                return true;
            case InboundValue.ValueOneofCase.EnumValue:
            case InboundValue.ValueOneofCase.Handle:
                return level + 1 <= Limit;
            case InboundValue.ValueOneofCase.ListValue:
                return level + 1 <= Limit
                    && value.ListValue.Values.All(item => ValueFits(item, level + 2));
            case InboundValue.ValueOneofCase.MapValue:
                return level + 1 <= Limit
                    && value.MapValue.Entries.All(entry => EntryFits(entry, level + 2));
            case InboundValue.ValueOneofCase.ClassValue:
                return level + 1 <= Limit
                    && value.ClassValue.Fields.All(entry => EntryFits(entry, level + 2));
            case InboundValue.ValueOneofCase.TyValue:
                return TypeFits(value.TyValue, level + 1);
            case InboundValue.ValueOneofCase.MediaValue:
                return level + 1 + MediaDepth(value.MediaValue) <= Limit;
            case InboundValue.ValueOneofCase.PromptAstValue:
                return PromptFits(value.PromptAstValue, level + 1);
            default:
                throw new BamlProtocolException(
                    "The managed bridge cannot measure an inbound value it does not know.",
                    $"InboundValue case {value.ValueCase} has no wire depth.");
        }
    }

    // An entry nests its value, or an enum key, the one key kind that is a
    // message.
    private static bool EntryFits(InboundMapEntry entry, int level)
    {
        if (level > Limit)
        {
            return false;
        }

        if (entry.KeyCase == InboundMapEntry.KeyOneofCase.EnumKey && level + 1 > Limit)
        {
            return false;
        }

        return entry.Value is null || ValueFits(entry.Value, level + 1);
    }

    private static bool TypeFits(BamlTy type, int level)
    {
        if (level > Limit)
        {
            return false;
        }

        bool Nested(BamlTy? child) => child is null || TypeFits(child, level + 2);
        bool AllNested(IEnumerable<BamlTy> children) => children.All(child => TypeFits(child, level + 2));
        switch (type.TyCase)
        {
            case BamlTy.TyOneofCase.None:
                return true;
            case BamlTy.TyOneofCase.Primitive:
            case BamlTy.TyOneofCase.Enum:
            case BamlTy.TyOneofCase.Literal:
            case BamlTy.TyOneofCase.Unknown:
            case BamlTy.TyOneofCase.Media:
            case BamlTy.TyOneofCase.EnumVariant:
            case BamlTy.TyOneofCase.RustType:
            case BamlTy.TyOneofCase.MetaType:
            case BamlTy.TyOneofCase.Resource:
            case BamlTy.TyOneofCase.PromptAst:
            case BamlTy.TyOneofCase.TypeVar:
            case BamlTy.TyOneofCase.Never:
                return level + 1 <= Limit;
            case BamlTy.TyOneofCase.ClassTy:
                return level + 1 <= Limit && AllNested(type.ClassTy.TypeArgs);
            case BamlTy.TyOneofCase.TypeAlias:
                return level + 1 <= Limit && AllNested(type.TypeAlias.TypeArgs);
            case BamlTy.TyOneofCase.List:
                return level + 1 <= Limit && Nested(type.List.Item);
            case BamlTy.TyOneofCase.Map:
                return level + 1 <= Limit && Nested(type.Map.Key) && Nested(type.Map.Value);
            case BamlTy.TyOneofCase.Optional:
                return level + 1 <= Limit && Nested(type.Optional.Inner);
            case BamlTy.TyOneofCase.Union:
                return level + 1 <= Limit && AllNested(type.Union.Options);
            case BamlTy.TyOneofCase.Interface:
                // A binding is its own message, one level above its type.
                return level + 1 <= Limit
                    && AllNested(type.Interface.TypeArgs)
                    && type.Interface.Bindings.All(binding =>
                        level + 2 <= Limit
                        && (binding.Ty is null || TypeFits(binding.Ty, level + 3)));
            case BamlTy.TyOneofCase.Function:
                // A parameter is its own message, one level above its type.
                return level + 1 <= Limit
                    && type.Function.Params.All(parameter =>
                        level + 2 <= Limit
                        && (parameter.Ty is null || TypeFits(parameter.Ty, level + 3)))
                    && Nested(type.Function.Ret)
                    && Nested(type.Function.Throws);
            case BamlTy.TyOneofCase.Future:
                return level + 1 <= Limit && Nested(type.Future.Value) && Nested(type.Future.Error);
            case BamlTy.TyOneofCase.AssociatedTypeProjection:
                return level + 1 <= Limit
                    && Nested(type.AssociatedTypeProjection.Base)
                    && Nested(type.AssociatedTypeProjection.Interface);
            default:
                throw new BamlProtocolException(
                    "The managed bridge cannot measure a type it does not know.",
                    $"BamlTy case {type.TyCase} has no wire depth.");
        }
    }

    private static int MediaDepth(BamlValueMedia media) =>
        media.ValueCase == BamlValueMedia.ValueOneofCase.FileContent ? 1 : 0;

    private static bool PromptFits(BamlValuePromptAst prompt, int level)
    {
        if (level > Limit)
        {
            return false;
        }

        return prompt.ValueCase switch
        {
            BamlValuePromptAst.ValueOneofCase.None => true,
            BamlValuePromptAst.ValueOneofCase.Simple => SimpleFits(prompt.Simple, level + 1),
            BamlValuePromptAst.ValueOneofCase.Message =>
                level + 1 <= Limit
                && (prompt.Message.Content is null || SimpleFits(prompt.Message.Content, level + 2)),
            BamlValuePromptAst.ValueOneofCase.Multiple =>
                level + 1 <= Limit
                && prompt.Multiple.Items.All(item => PromptFits(item, level + 2)),
            _ => throw new BamlProtocolException(
                "The managed bridge cannot measure a prompt it does not know.",
                $"BamlValuePromptAst case {prompt.ValueCase} has no wire depth."),
        };
    }

    private static bool SimpleFits(BamlValuePromptAstSimple simple, int level)
    {
        if (level > Limit)
        {
            return false;
        }

        return simple.ValueCase switch
        {
            BamlValuePromptAstSimple.ValueOneofCase.None
                or BamlValuePromptAstSimple.ValueOneofCase.String => true,
            BamlValuePromptAstSimple.ValueOneofCase.Media =>
                level + 1 + MediaDepth(simple.Media) <= Limit,
            BamlValuePromptAstSimple.ValueOneofCase.Multiple =>
                level + 1 <= Limit
                && simple.Multiple.Items.All(item => SimpleFits(item, level + 2)),
            _ => throw new BamlProtocolException(
                "The managed bridge cannot measure a prompt it does not know.",
                $"BamlValuePromptAstSimple case {simple.ValueCase} has no wire depth."),
        };
    }
}
