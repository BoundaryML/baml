using BamlBridge.Cffi.V1;

namespace Baml.Proto;

/// <summary>
/// The <c>live</c> heads of a wire type: each is a row of the engine's handle
/// table naming a runtime declaration, and each occurrence is one reference.
/// </summary>
internal static class LiveHeads
{
    /// <summary>Every <c>live</c> key under <paramref name="type"/>, once per occurrence, in visit order.</summary>
    internal static List<ulong> Keys(BamlTy type)
    {
        var keys = new List<ulong>();
        Map(type, key =>
        {
            keys.Add(key);
            return key;
        });
        return keys;
    }

    /// <summary>
    /// Replaces every <c>live</c> key under <paramref name="type"/> in place with
    /// <paramref name="map"/>'s result, visiting occurrences in the order
    /// <see cref="Keys"/> lists them.
    /// </summary>
    internal static void Map(BamlTy? type, Func<ulong, ulong> map)
    {
        if (type is null)
        {
            return;
        }

        switch (type.TyCase)
        {
            case BamlTy.TyOneofCase.None:
            case BamlTy.TyOneofCase.Primitive:
            case BamlTy.TyOneofCase.Literal:
            case BamlTy.TyOneofCase.Unknown:
            case BamlTy.TyOneofCase.Media:
            case BamlTy.TyOneofCase.RustType:
            case BamlTy.TyOneofCase.MetaType:
            case BamlTy.TyOneofCase.Resource:
            case BamlTy.TyOneofCase.PromptAst:
            case BamlTy.TyOneofCase.TypeVar:
            case BamlTy.TyOneofCase.Never:
                break;
            case BamlTy.TyOneofCase.ClassTy:
                if (type.ClassTy.HeadCase == BamlTyClass.HeadOneofCase.Live)
                {
                    type.ClassTy.Live = map(type.ClassTy.Live);
                }
                MapAll(type.ClassTy.TypeArgs, map);
                break;
            case BamlTy.TyOneofCase.TypeAlias:
                if (type.TypeAlias.HeadCase == BamlTyTypeAlias.HeadOneofCase.Live)
                {
                    type.TypeAlias.Live = map(type.TypeAlias.Live);
                }
                MapAll(type.TypeAlias.TypeArgs, map);
                break;
            case BamlTy.TyOneofCase.Enum:
                if (type.Enum.HeadCase == BamlTyEnum.HeadOneofCase.Live)
                {
                    type.Enum.Live = map(type.Enum.Live);
                }
                break;
            case BamlTy.TyOneofCase.EnumVariant:
                if (type.EnumVariant.HeadCase == BamlTyEnumVariant.HeadOneofCase.Live)
                {
                    type.EnumVariant.Live = map(type.EnumVariant.Live);
                }
                break;
            case BamlTy.TyOneofCase.Interface:
                if (type.Interface.HeadCase == BamlTyInterface.HeadOneofCase.Live)
                {
                    type.Interface.Live = map(type.Interface.Live);
                }
                MapAll(type.Interface.TypeArgs, map);
                foreach (BamlTyAssociatedBinding binding in type.Interface.Bindings)
                {
                    Map(binding.Ty, map);
                }
                break;
            case BamlTy.TyOneofCase.List:
                Map(type.List.Item, map);
                break;
            case BamlTy.TyOneofCase.Map:
                Map(type.Map.Key, map);
                Map(type.Map.Value, map);
                break;
            case BamlTy.TyOneofCase.Optional:
                Map(type.Optional.Inner, map);
                break;
            case BamlTy.TyOneofCase.Union:
                MapAll(type.Union.Options, map);
                break;
            case BamlTy.TyOneofCase.Function:
                foreach (BamlTyFunctionParam parameter in type.Function.Params)
                {
                    Map(parameter.Ty, map);
                }
                Map(type.Function.Ret, map);
                Map(type.Function.Throws, map);
                break;
            case BamlTy.TyOneofCase.Future:
                Map(type.Future.Value, map);
                Map(type.Future.Error, map);
                break;
            case BamlTy.TyOneofCase.AssociatedTypeProjection:
                Map(type.AssociatedTypeProjection.Base, map);
                Map(type.AssociatedTypeProjection.Interface, map);
                break;
            default:
                // A case this SDK cannot walk could hide a reference it owns.
                throw new BamlProtocolException(
                    "The native bridge returned unsupported BAML type metadata.",
                    $"BamlTy case {type.TyCase} cannot be searched for runtime declarations.");
        }
    }

    private static void MapAll(IEnumerable<BamlTy> types, Func<ulong, ulong> map)
    {
        foreach (BamlTy type in types)
        {
            Map(type, map);
        }
    }
}
