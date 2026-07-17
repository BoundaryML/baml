using System.Numerics;
using BamlBridge.Cffi.V1;

namespace Baml.Bridge;

internal static class ProtoTypeCodec
{
    private const int MaxTypeDepth = 100;

    internal static BamlTy Encode(Type type)
    {
        ArgumentNullException.ThrowIfNull(type);
        return Encode(type, new HashSet<Type>(), 0);
    }

    private static BamlTy Encode(Type type, HashSet<Type> activeTypes, int depth)
    {
        if (depth > MaxTypeDepth)
        {
            throw new BamlBridgeException($"A CLR type exceeds the BAML descriptor nesting limit of {MaxTypeDepth}.");
        }

        if (!activeTypes.Add(type))
        {
            throw new BamlBridgeException($"Recursive CLR type descriptor {type.FullName} is not supported.");
        }

        try
        {
            if (PrimitiveKind(type) is { } primitive)
            {
                return new BamlTy { Primitive = new BamlTyPrimitive { Kind = primitive } };
            }

            if (type == typeof(object))
            {
                return new BamlTy { Unknown = new BamlTyUnknown() };
            }

            if (type == typeof(BamlHandle))
            {
                return new BamlTy { RustType = new BamlTyRustType() };
            }

            if (type == typeof(BamlStreamFinished))
            {
                return new BamlTy
                {
                    ClassTy = new BamlTyClass { Name = "baml.stream.StreamFinished" },
                };
            }

            if (BuiltinClassName(type) is { } builtinClassName)
            {
                return new BamlTy
                {
                    ClassTy = new BamlTyClass { Name = builtinClassName },
                };
            }

            if (type.IsGenericType
                && type.GetGenericTypeDefinition() == typeof(BamlStream<,>))
            {
                var classType = new BamlTyClass { Name = "baml.llm.Stream" };
                classType.TypeArgs.Add(type.GetGenericArguments().Select(
                    argument => Encode(argument, activeTypes, depth + 1)));
                return new BamlTy { ClassTy = classType };
            }

            if (Nullable.GetUnderlyingType(type) is { } nullableType)
            {
                return Optional(Encode(nullableType, activeTypes, depth + 1));
            }

            if (type.IsGenericType
                && type.GetGenericTypeDefinition() == typeof(BamlNullable<>))
            {
                return Optional(Encode(type.GetGenericArguments()[0], activeTypes, depth + 1));
            }

            if (type.IsArray && type.GetArrayRank() == 1)
            {
                return List(Encode(type.GetElementType()!, activeTypes, depth + 1));
            }

            if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(List<>))
            {
                return List(Encode(type.GetGenericArguments()[0], activeTypes, depth + 1));
            }

            if (type.IsGenericType && type.GetGenericTypeDefinition() == typeof(Dictionary<,>))
            {
                var arguments = type.GetGenericArguments();
                return new BamlTy
                {
                    Map = new BamlTyMap
                    {
                        Key = Encode(arguments[0], activeTypes, depth + 1),
                        Value = Encode(arguments[1], activeTypes, depth + 1),
                    },
                };
            }

            if (type.IsGenericType && typeof(IBamlUnionValue).IsAssignableFrom(type))
            {
                var union = new BamlTyUnion();
                union.Options.Add(type.GetGenericArguments().Select(
                    argument => Encode(argument, activeTypes, depth + 1)));
                return new BamlTy { Union = union };
            }

            if (MediaKind(type) is { } mediaKind)
            {
                return new BamlTy { Media = new BamlTyMedia { Kind = mediaKind } };
            }

            if (type == typeof(BamlClientType))
            {
                return new BamlTy
                {
                    Enum = new BamlTyEnum { Name = "baml.llm.ClientType" },
                };
            }

            if (GeneratedContracts.GetEnum(type) is { } enumContract)
            {
                return new BamlTy { Enum = new BamlTyEnum { Name = enumContract.WireName } };
            }

            if (GeneratedContracts.GetTypeAlias(type) is { } typeAliasContract)
            {
                return new BamlTy
                {
                    TypeAlias = new BamlTyTypeAlias { Name = typeAliasContract.WireName },
                };
            }

            if (GeneratedContracts.GetClass(type) is { } classContract)
            {
                var classType = new BamlTyClass { Name = classContract.WireName };
                if (type.IsGenericType)
                {
                    classType.TypeArgs.Add(type.GetGenericArguments().Select(
                        argument => Encode(argument, activeTypes, depth + 1)));
                }

                return new BamlTy { ClassTy = classType };
            }

            throw new BamlBridgeException(
                $"CLR type {type.FullName} has no BAML type descriptor; generated generic calls cannot bind it.");
        }
        finally
        {
            activeTypes.Remove(type);
        }
    }

    private static BamlTyPrimitiveKind? PrimitiveKind(Type type) => type == typeof(string)
        ? BamlTyPrimitiveKind.BamlTyPrimitiveString
        : type == typeof(long)
            ? BamlTyPrimitiveKind.BamlTyPrimitiveInt
            : type == typeof(double)
                ? BamlTyPrimitiveKind.BamlTyPrimitiveFloat
                : type == typeof(bool)
                    ? BamlTyPrimitiveKind.BamlTyPrimitiveBool
                    : type == typeof(byte[])
                        ? BamlTyPrimitiveKind.BamlTyPrimitiveBytes
                        : type == typeof(BigInteger)
                            ? BamlTyPrimitiveKind.BamlTyPrimitiveBigint
                            : null;

    private static BamlTyMediaKind? MediaKind(Type type) => type == typeof(BamlImage)
        ? BamlTyMediaKind.Image
        : type == typeof(BamlAudio)
            ? BamlTyMediaKind.Audio
            : type == typeof(BamlVideo)
                ? BamlTyMediaKind.Video
                : type == typeof(BamlPdf)
                    ? BamlTyMediaKind.Pdf
                    : null;

    private static string? BuiltinClassName(Type type) => type == typeof(BamlPromptAst)
        ? "baml.llm.PromptAst"
        : type == typeof(BamlPromptMessage)
            ? "baml.llm.PromptMessage"
            : type == typeof(BamlHttpRequest)
                ? "baml.http.Request"
                : type == typeof(BamlHttpResponse)
                    ? "baml.http.Response"
                : type == typeof(BamlFile)
                    ? "baml.fs.File"
                    : type == typeof(BamlSseStream)
                        ? "baml.http.SseStream"
                        : type == typeof(BamlGlob)
                            ? "baml.glob.Glob"
                            : type == typeof(BamlGlobScanOptions)
                                ? "baml.glob.ScanOptions"
                                : type == typeof(BamlCancelToken)
                                    ? "baml.spawn.CancelToken"
                                    : type == typeof(BamlTaskGroup)
                                        ? "baml.spawn.TaskGroup"
                                        : type == typeof(BamlCsvWriter)
                                            ? "baml.csv.CsvWriter"
                                            : type == typeof(BamlCsvReader)
                                                ? "baml.csv.CsvReader"
                                                : type == typeof(BamlCsvRecord)
                                                    ? "baml.csv.CsvRecord"
                                                    : type == typeof(BamlCsvPosition)
                                                        ? "baml.csv.CsvPosition"
                                                        : type == typeof(BamlIteratorDone)
                                                            ? "baml.iter.Done"
                                                            : type == typeof(BamlCsvWriterOptions)
                                                                ? "baml.csv.WriterOptions"
                                                                : type == typeof(BamlCsvReaderOptions)
                                                                    ? "baml.csv.ReaderOptions"
                                                                    : type == typeof(BamlClient)
                                                                        ? "baml.llm.Client"
                                                                        : type == typeof(BamlRetryPolicy)
                                                                            ? "baml.llm.RetryPolicy"
                                                                            : null;

    private static BamlTy Optional(BamlTy inner) => new()
    {
        Optional = new BamlTyOptional { Inner = inner },
    };

    private static BamlTy List(BamlTy item) => new()
    {
        List = new BamlTyList { Item = item },
    };
}
