using System.Collections;
using System.Globalization;
using System.Numerics;
using System.Reflection;
using BamlBridge.Cffi.V1;
using Google.Protobuf;
using WireHandle = BamlBridge.Cffi.V1.BamlHandle;

namespace Baml.Bridge;

internal static class ProtoCodec
{
    private const long MinBamlInteger = -(1L << 62);
    private const long MaxBamlInteger = (1L << 62) - 1;
    private const int MaxBigIntegerHexLength = (1 << 28) / 4 + 2;
    private const int MaxValueDepth = 100;
    private const string CancelledPanicClass = "baml.panics.Cancelled";
    private const string TypeMismatchErrorClass = "baml.errors.TypeMismatch";

    internal static InboundValue Encode(object? value)
    {
        using var context = new EncodeContext();
        return Encode(value, context);
    }

    internal static InboundValue Encode(object? value, EncodeContext context) =>
        Encode(value, context, new HashSet<object>(ReferenceEqualityComparer.Instance), 0);

    private static InboundValue Encode(
        object? value,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        if (depth > MaxValueDepth)
        {
            throw new BamlBridgeException($"A C# value exceeds the BAML bridge nesting limit of {MaxValueDepth}.");
        }

        var encoded = new InboundValue();
        switch (value)
        {
            case null:
                return encoded;
            case string text:
                encoded.StringValue = text;
                break;
            case bool boolean:
                encoded.BoolValue = boolean;
                break;
            case byte[] bytes:
                encoded.Uint8ArrayValue = ByteString.CopyFrom(bytes);
                break;
            case sbyte or byte or short or ushort or int or uint or long:
                var integer = Convert.ToInt64(value, CultureInfo.InvariantCulture);
                if (integer is < MinBamlInteger or > MaxBamlInteger)
                {
                    throw new BamlBridgeException(
                        $"C# integer value {integer} is outside the BAML int range [{MinBamlInteger}, {MaxBamlInteger}]. Use BigInteger with a BAML bigint parameter for larger values.");
                }

                encoded.IntValue = integer;
                break;
            case float or double or decimal:
                encoded.FloatValue = Convert.ToDouble(value, CultureInfo.InvariantCulture);
                break;
            case BigInteger bigInteger:
                encoded.BigintValue = FormatBigInteger(bigInteger);
                break;
            case BamlMedia media:
                return EncodeMedia(media, context);
            case BamlPromptAst promptAst:
                return EncodePromptAst(promptAst, context);
            case BamlHttpRequest request:
                return EncodeHttpRequest(request, context, activeReferences, depth);
            case BamlHttpResponse response:
                return EncodeHttpResponse(response, context, activeReferences, depth);
            case BamlFile file:
                return EncodeFile(file, context);
            case BamlSseStream sseStream:
                return EncodeSseStream(sseStream, context, activeReferences, depth);
            case BamlGlob glob:
                return EncodeGlob(glob, context);
            case BamlGlobScanOptions globScanOptions:
                return EncodeGlobScanOptions(globScanOptions, context, activeReferences, depth);
            case BamlCancelToken cancelToken:
                return EncodeCancelToken(cancelToken, context);
            case BamlTaskGroup taskGroup:
                return EncodeTaskGroup(taskGroup, context);
            case BamlIteratorDone:
                return EncodeIteratorDone();
            case BamlCsvPosition csvPosition:
                return EncodeCsvPosition(csvPosition, context, activeReferences, depth);
            case BamlCsvWriterOptions csvWriterOptions:
                return EncodeCsvWriterOptions(
                    csvWriterOptions,
                    context,
                    activeReferences,
                    depth);
            case BamlCsvReaderOptions csvReaderOptions:
                return EncodeCsvReaderOptions(
                    csvReaderOptions,
                    context,
                    activeReferences,
                    depth);
            case BamlCsvRecord csvRecord:
                return EncodeCsvRecord(csvRecord, context);
            case BamlCsvReader csvReader:
                return EncodeCsvReader(csvReader, context, activeReferences, depth);
            case BamlCsvWriter csvWriter:
                return EncodeCsvWriter(csvWriter, context, activeReferences, depth);
            case BamlClient client:
                return EncodeClient(client, context, activeReferences, depth);
            case BamlRetryPolicy retryPolicy:
                return EncodeRetryPolicy(retryPolicy, context, activeReferences, depth);
            case BamlClientType clientType:
                encoded.EnumValue = EncodeClientType(clientType);
                break;
            case BamlHandle handle:
                return EncodeHandle(handle, context);
            case IBamlStreamValue stream:
                return EncodeStream(stream, context);
            case Delegate callback:
                return HostValueRegistry.EncodeCallable(callback, context);
            case IBamlNullableValue nullable:
                return nullable.IsNull
                    ? new InboundValue()
                    : Encode(nullable.Value, context, activeReferences, depth + 1);
            case IBamlTypeAliasValue typeAlias:
                return Encode(typeAlias.UntypedValue, context, activeReferences, depth + 1);
            case IBamlUnionValue union:
                return Encode(union.Value, context, activeReferences, depth + 1);
            case Enum enumValue:
                encoded.EnumValue = EncodeEnum(enumValue);
                break;
            case IDictionary dictionary:
                encoded.MapValue = EncodeMap(dictionary, context, activeReferences, depth);
                break;
            case IEnumerable sequence:
                encoded.ListValue = EncodeList(sequence, context, activeReferences, depth);
                break;
            default:
                var contract = GeneratedContracts.GetClass(value.GetType());
                if (contract is not null)
                {
                    encoded.ClassValue = EncodeClass(value, contract, context, activeReferences, depth);
                    break;
                }

                throw new BamlBridgeException($"C# value type {value.GetType().FullName} is not supported by the BAML bridge yet.");
        }

        return encoded;
    }

    private static InboundEnumValue EncodeEnum(Enum value)
    {
        var contract = GeneratedContracts.GetEnum(value.GetType())
            ?? throw new BamlBridgeException(
                $"C# enum type {value.GetType().FullName} is not a generated BAML enum.");
        var member = contract.FindByValue(value)
            ?? throw new BamlBridgeException(
                $"C# enum value {value} is not a declared member of generated BAML enum {contract.WireName}.");
        return new InboundEnumValue
        {
            Name = contract.WireName,
            Value = member.WireName,
        };
    }

    private static InboundEnumValue EncodeClientType(BamlClientType value) => new()
    {
        Name = "baml.llm.ClientType",
        Value = value switch
        {
            BamlClientType.Primitive => "Primitive",
            BamlClientType.Fallback => "Fallback",
            BamlClientType.RoundRobin => "RoundRobin",
            _ => throw new BamlBridgeException($"Unknown BAML client type {value}."),
        },
    };

    private static InboundClassValue EncodeClass(
        object value,
        GeneratedClassContract contract,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        EnterReference(value, activeReferences);
        try
        {
            var encoded = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = contract.WireName },
            };
            if (contract.Type.IsGenericType)
            {
                encoded.ClassTy.TypeArgs.Add(contract.Type.GetGenericArguments().Select(ProtoTypeCodec.Encode));
            }

            foreach (var property in contract.Properties)
            {
                encoded.Fields.Add(new InboundMapEntry
                {
                    StringKey = property.WireName,
                    Value = Encode(property.Property.GetValue(value), context, activeReferences, depth + 1),
                });
            }

            return encoded;
        }
        finally
        {
            activeReferences.Remove(value);
        }
    }

    private static InboundListValue EncodeList(
        IEnumerable sequence,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        EnterReference(sequence, activeReferences);
        try
        {
            var encoded = new InboundListValue();
            foreach (var item in sequence)
            {
                encoded.Values.Add(Encode(item, context, activeReferences, depth + 1));
            }

            return encoded;
        }
        finally
        {
            activeReferences.Remove(sequence);
        }
    }

    private static InboundMapValue EncodeMap(
        IDictionary dictionary,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        EnterReference(dictionary, activeReferences);
        try
        {
            var encoded = new InboundMapValue();
            foreach (DictionaryEntry entry in dictionary)
            {
                var encodedEntry = new InboundMapEntry
                {
                    Value = Encode(entry.Value, context, activeReferences, depth + 1),
                };
                switch (entry.Key)
                {
                    case string key:
                        encodedEntry.StringKey = key;
                        break;
                    case bool key:
                        encodedEntry.BoolKey = key;
                        break;
                    case BamlClientType key:
                        encodedEntry.EnumKey = EncodeClientType(key);
                        break;
                    case Enum key:
                        encodedEntry.EnumKey = EncodeEnum(key);
                        break;
                    case sbyte or byte or short or ushort or int or uint or long:
                        var integer = Convert.ToInt64(entry.Key, CultureInfo.InvariantCulture);
                        if (integer is < MinBamlInteger or > MaxBamlInteger)
                        {
                            throw new BamlBridgeException(
                                $"C# map key {integer} is outside the BAML int range [{MinBamlInteger}, {MaxBamlInteger}].");
                        }

                        encodedEntry.IntKey = integer;
                        break;
                    default:
                        throw new BamlBridgeException(
                            $"C# dictionary key type {entry.Key?.GetType().FullName ?? "null"} is not supported; BAML map keys must be string, bool, int, or a generated BAML enum.");
                }

                encoded.Entries.Add(encodedEntry);
            }

            return encoded;
        }
        finally
        {
            activeReferences.Remove(dictionary);
        }
    }

    private static void EnterReference(object value, HashSet<object> activeReferences)
    {
        if (!activeReferences.Add(value))
        {
            throw new BamlBridgeException("Cyclic C# values cannot cross the BAML bridge.");
        }
    }

    private static InboundValue EncodeMedia(BamlMedia media, EncodeContext context)
    {
        var (key, handleType) = media.CloneForWire();
        context.TrackHandle(key);
        var wireName = media switch
        {
            BamlImage => "baml.media.Image",
            BamlAudio => "baml.media.Audio",
            BamlVideo => "baml.media.Video",
            BamlPdf => "baml.media.Pdf",
            _ => throw new BamlBridgeException($"Unsupported BAML media wrapper {media.GetType().FullName}."),
        };
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = wireName },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_data",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeHandle(BamlHandle handle, EncodeContext context)
    {
        var (key, handleType) = handle.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            Handle = new WireHandle
            {
                Key = key,
                HandleType = (BamlHandleType)handleType,
            },
        };
    }

    private static InboundValue EncodePromptAst(BamlPromptAst promptAst, EncodeContext context)
    {
        var (key, handleType) = promptAst.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.llm.PromptAst" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_data",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeHttpRequest(
        BamlHttpRequest request,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.http.Request" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "method",
                        Value = Encode(request.Method, context, activeReferences, depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "url",
                        Value = Encode(request.Url, context, activeReferences, depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "headers",
                        Value = EncodeStringMap(request.Headers, context, activeReferences, depth),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "body",
                        Value = Encode(request.Body, context, activeReferences, depth + 1),
                    },
                },
            },
        };
    }

    private static InboundValue EncodeHttpResponse(
        BamlHttpResponse response,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        var (key, handleType) = response.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.http.Response" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "status_code",
                        Value = Encode(response.StatusCode, context, activeReferences, depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "headers",
                        Value = EncodeStringMap(response.Headers, context, activeReferences, depth),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "url",
                        Value = Encode(response.Url, context, activeReferences, depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_body",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeFile(BamlFile file, EncodeContext context)
    {
        var (key, handleType) = file.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.fs.File" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeSseStream(
        BamlSseStream stream,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        var (key, handleType) = stream.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.http.SseStream" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "url",
                        Value = Encode(stream.Url, context, activeReferences, depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeGlob(BamlGlob glob, EncodeContext context)
    {
        var (key, handleType) = glob.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.glob.Glob" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeCancelToken(BamlCancelToken token, EncodeContext context)
    {
        var (key, handleType) = token.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.spawn.CancelToken" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeTaskGroup(BamlTaskGroup group, EncodeContext context)
    {
        var (key, handleType) = group.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.spawn.TaskGroup" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                },
            },
        };
    }

    private static InboundValue EncodeCsvWriter(
        BamlCsvWriter writer,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        var (key, handleType) = writer.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.csv.CsvWriter" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_file",
                        Value = Encode(
                            writer.BackingFile,
                            context,
                            activeReferences,
                            depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_owns_file",
                        Value = Encode(
                            writer.OwnsFile,
                            context,
                            activeReferences,
                            depth + 1),
                    },
                },
            },
        };
    }

    private static InboundValue EncodeIteratorDone() => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = "baml.iter.Done" },
        },
    };

    private static InboundValue EncodeCsvPosition(
        BamlCsvPosition position,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = "baml.csv.CsvPosition" },
            Fields =
            {
                new InboundMapEntry
                {
                    StringKey = "byte",
                    Value = Encode(position.ByteOffset, context, activeReferences, depth + 1),
                },
                new InboundMapEntry
                {
                    StringKey = "line",
                    Value = Encode(position.Line, context, activeReferences, depth + 1),
                },
                new InboundMapEntry
                {
                    StringKey = "record",
                    Value = Encode(position.Record, context, activeReferences, depth + 1),
                },
            },
        },
    };

    private static InboundValue EncodeCsvWriterOptions(
        BamlCsvWriterOptions options,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = "baml.csv.WriterOptions" },
            Fields =
            {
                EncodeClassField("delimiter", options.Delimiter, context, activeReferences, depth),
                EncodeClassField("quote", options.Quote, context, activeReferences, depth),
                EncodeClassField("quote_style", options.QuoteStyle, context, activeReferences, depth),
                EncodeClassField("escape", options.Escape, context, activeReferences, depth),
                EncodeClassField("terminator", options.Terminator, context, activeReferences, depth),
                EncodeClassField("write_header", options.WriteHeader, context, activeReferences, depth),
                EncodeClassField(
                    "headers",
                    options.Headers?.ToList(),
                    context,
                    activeReferences,
                    depth),
                EncodeClassField("null_value", options.NullValue, context, activeReferences, depth),
                EncodeClassField("bom", options.Bom, context, activeReferences, depth),
                EncodeClassField(
                    "sanitize_formulas",
                    options.SanitizeFormulas,
                    context,
                    activeReferences,
                    depth),
            },
        },
    };

    private static InboundValue EncodeCsvReaderOptions(
        BamlCsvReaderOptions options,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = "baml.csv.ReaderOptions" },
            Fields =
            {
                EncodeClassField("delimiter", options.Delimiter, context, activeReferences, depth),
                EncodeClassField("quote", options.Quote, context, activeReferences, depth),
                EncodeClassField("quoting", options.Quoting, context, activeReferences, depth),
                EncodeClassField("escape", options.Escape, context, activeReferences, depth),
                EncodeClassField("has_header", options.HasHeader, context, activeReferences, depth),
                EncodeClassField(
                    "headers",
                    options.Headers?.ToList(),
                    context,
                    activeReferences,
                    depth),
                EncodeClassField("comment", options.Comment, context, activeReferences, depth),
                EncodeClassField("trim", options.Trim, context, activeReferences, depth),
                EncodeClassField("skip_lines", options.SkipLines, context, activeReferences, depth),
                EncodeClassField(
                    "skip_blank_records",
                    options.SkipBlankRecords,
                    context,
                    activeReferences,
                    depth),
                EncodeClassField("ragged", options.Ragged, context, activeReferences, depth),
                EncodeClassField(
                    "null_values",
                    options.NullValues?.ToList(),
                    context,
                    activeReferences,
                    depth),
                EncodeClassField("encoding", options.Encoding, context, activeReferences, depth),
                EncodeClassField("bom", options.Bom, context, activeReferences, depth),
                EncodeClassField("on_error", options.OnError, context, activeReferences, depth),
                EncodeClassField("on_skip", options.OnSkip, context, activeReferences, depth),
                EncodeClassField(
                    "max_skipped",
                    options.MaxSkipped,
                    context,
                    activeReferences,
                    depth),
                EncodeClassField("limit", options.Limit, context, activeReferences, depth),
            },
        },
    };

    private static InboundMapEntry EncodeClassField(
        string name,
        object? value,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) => new()
    {
        StringKey = name,
        Value = Encode(value, context, activeReferences, depth + 1),
    };

    private static InboundValue EncodeCsvRecord(BamlCsvRecord record, EncodeContext context)
    {
        var (key, handleType) = record.CloneForWire();
        context.TrackHandle(key);
        return EncodeSingleHandleClass("baml.csv.CsvRecord", "_handle", key, handleType);
    }

    private static InboundValue EncodeCsvReader(
        BamlCsvReader reader,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        var (key, handleType) = reader.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.csv.CsvReader" },
                Fields =
                {
                    new InboundMapEntry
                    {
                        StringKey = "_handle",
                        Value = new InboundValue
                        {
                            Handle = new WireHandle
                            {
                                Key = key,
                                HandleType = (BamlHandleType)handleType,
                            },
                        },
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_file",
                        Value = Encode(
                            reader.BackingFile,
                            context,
                            activeReferences,
                            depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_on_skip",
                        Value = Encode(
                            reader.OnSkip,
                            context,
                            activeReferences,
                            depth + 1),
                    },
                    new InboundMapEntry
                    {
                        StringKey = "_owns_file",
                        Value = Encode(
                            reader.OwnsFile,
                            context,
                            activeReferences,
                            depth + 1),
                    },
                },
            },
        };
    }

    private static InboundValue EncodeSingleHandleClass(
        string className,
        string fieldName,
        ulong key,
        int handleType) => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = className },
            Fields =
            {
                new InboundMapEntry
                {
                    StringKey = fieldName,
                    Value = new InboundValue
                    {
                        Handle = new WireHandle
                        {
                            Key = key,
                            HandleType = (BamlHandleType)handleType,
                        },
                    },
                },
            },
        },
    };

    private static InboundValue EncodeGlobScanOptions(
        BamlGlobScanOptions options,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) =>
        new()
        {
            ClassValue = new InboundClassValue
            {
                ClassTy = new BamlTyClass { Name = "baml.glob.ScanOptions" },
                Fields =
                {
                    Field("cwd", options.Cwd, context, activeReferences, depth),
                    Field("dot", options.Dot, context, activeReferences, depth),
                    Field("absolute", options.Absolute, context, activeReferences, depth),
                    Field("follow_symlinks", options.FollowSymlinks, context, activeReferences, depth),
                    Field(
                        "throw_error_on_broken_symlink",
                        options.ThrowErrorOnBrokenSymlink,
                        context,
                        activeReferences,
                        depth),
                    Field("only_files", options.OnlyFiles, context, activeReferences, depth),
                },
            },
        };

    private static InboundMapEntry Field(
        string name,
        object? value,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) =>
        new()
        {
            StringKey = name,
            Value = Encode(value, context, activeReferences, depth + 1),
        };

    private static InboundValue EncodeStringMap(
        IReadOnlyDictionary<string, string> values,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        var map = new InboundMapValue();
        foreach (var (key, value) in values)
        {
            map.Entries.Add(new InboundMapEntry
            {
                StringKey = key,
                Value = Encode(value, context, activeReferences, depth + 1),
            });
        }

        return new InboundValue { MapValue = map };
    }

    private static InboundValue EncodeClient(
        BamlClient client,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth)
    {
        EnterReference(client, activeReferences);
        try
        {
            return new InboundValue
            {
                ClassValue = new InboundClassValue
                {
                    ClassTy = new BamlTyClass { Name = "baml.llm.Client" },
                    Fields =
                    {
                        new InboundMapEntry
                        {
                            StringKey = "name",
                            Value = Encode(client.Name, context, activeReferences, depth + 1),
                        },
                        new InboundMapEntry
                        {
                            StringKey = "client_type",
                            Value = Encode(client.ClientType, context, activeReferences, depth + 1),
                        },
                        new InboundMapEntry
                        {
                            StringKey = "sub_clients",
                            Value = Encode(client.SubClients, context, activeReferences, depth + 1),
                        },
                        new InboundMapEntry
                        {
                            StringKey = "retry",
                            Value = Encode(client.Retry, context, activeReferences, depth + 1),
                        },
                        new InboundMapEntry
                        {
                            StringKey = "counter",
                            Value = Encode(client.Counter, context, activeReferences, depth + 1),
                        },
                    },
                },
            };
        }
        finally
        {
            activeReferences.Remove(client);
        }
    }

    private static InboundValue EncodeRetryPolicy(
        BamlRetryPolicy retry,
        EncodeContext context,
        HashSet<object> activeReferences,
        int depth) => new()
    {
        ClassValue = new InboundClassValue
        {
            ClassTy = new BamlTyClass { Name = "baml.llm.RetryPolicy" },
            Fields =
            {
                new InboundMapEntry
                {
                    StringKey = "max_retries",
                    Value = Encode(retry.MaxRetries, context, activeReferences, depth + 1),
                },
                new InboundMapEntry
                {
                    StringKey = "initial_delay_ms",
                    Value = Encode(retry.InitialDelayMilliseconds, context, activeReferences, depth + 1),
                },
                new InboundMapEntry
                {
                    StringKey = "multiplier",
                    Value = Encode(retry.Multiplier, context, activeReferences, depth + 1),
                },
                new InboundMapEntry
                {
                    StringKey = "max_delay_ms",
                    Value = Encode(retry.MaxDelayMilliseconds, context, activeReferences, depth + 1),
                },
            },
        },
    };

    private static InboundValue EncodeStream(IBamlStreamValue stream, EncodeContext context)
    {
        var (key, handleType) = stream.CloneForWire();
        context.TrackHandle(key);
        return new InboundValue
        {
            Handle = new WireHandle
            {
                Key = key,
                HandleType = (BamlHandleType)handleType,
            },
        };
    }

    internal static T DecodeResult<T>(ReadOnlyMemory<byte> payload)
    {
        BamlOutboundResult result;
        try
        {
            result = BamlOutboundResult.Parser.ParseFrom(payload.Span);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlBridgeException("The native runtime returned an invalid BamlOutboundResult payload.", error);
        }

        return result.ResultCase switch
        {
            BamlOutboundResult.ResultOneofCase.Ok => DecodeOk<T>(result.Ok),
            BamlOutboundResult.ResultOneofCase.Error => throw DecodeError(result.Error),
            BamlOutboundResult.ResultOneofCase.Panic => DecodePanic<T>(result.Panic),
            _ => throw new BamlBridgeException("The native runtime returned an empty BamlOutboundResult envelope."),
        };
    }

    internal static BamlUnion<TPartial, BamlStreamFinished> DecodeStreamNext<TPartial>(
        ReadOnlyMemory<byte> payload)
    {
        BamlOutboundResult result;
        try
        {
            result = BamlOutboundResult.Parser.ParseFrom(payload.Span);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new BamlBridgeException("The native runtime returned an invalid stream-next payload.", error);
        }

        if (result.ResultCase == BamlOutboundResult.ResultOneofCase.Error)
        {
            throw DecodeError(result.Error);
        }

        if (result.ResultCase == BamlOutboundResult.ResultOneofCase.Panic)
        {
            return DecodePanic<BamlUnion<TPartial, BamlStreamFinished>>(result.Panic);
        }

        if (result.ResultCase != BamlOutboundResult.ResultOneofCase.Ok)
        {
            throw new BamlBridgeException("The native runtime returned an empty stream-next result envelope.");
        }

        var decoded = DecodeValue(result.Ok);
        try
        {
            if (decoded is not DecodedUnion union)
            {
                throw new BamlBridgeException("The native runtime returned a non-union value from Stream.next.");
            }

            if (union.Value is DecodedClass terminal
                && string.Equals(terminal.Name, "baml.stream.StreamFinished", StringComparison.Ordinal)
                && terminal.TypeArguments.Count == 0
                && terminal.Fields.Count == 0)
            {
                return BamlUnion<TPartial, BamlStreamFinished>.FromT1(BamlStreamFinished.Instance);
            }

            if (union.Value is null)
            {
                var type = typeof(TPartial);
                if (type.IsValueType && Nullable.GetUnderlyingType(type) is null)
                {
                    throw new BamlBridgeException(
                        $"The native runtime returned a null stream partial, but C# expected {type.FullName}.");
                }

                return BamlUnion<TPartial, BamlStreamFinished>.FromT0(default!);
            }

            return BamlUnion<TPartial, BamlStreamFinished>.FromT0(
                ConvertDecoded<TPartial>(union.Value));
        }
        finally
        {
            DisposeDecodedHandles(decoded);
        }
    }

    internal static object? DecodeOutbound(BamlOutboundValue value, Type targetType)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentNullException.ThrowIfNull(targetType);
        var decoded = DecodeValue(value);
        try
        {
            return ConvertDecoded(decoded, targetType, 0);
        }
        finally
        {
            DisposeDecodedHandles(decoded);
        }
    }

    private static Exception DecodeError(BamlOutboundError error)
    {
        var decoded = DecodeValue(error.Value);
        try
        {
            var className = FindClassName(error.Value);
            if (className == TypeMismatchErrorClass)
            {
                return new BamlTypeMismatchException(ToDynamicValue(decoded), error.Trace.ToArray())
                {
                    ClassName = className,
                };
            }

            return new BamlError(ToDynamicValue(decoded), error.Trace.ToArray())
            {
                ClassName = className,
            };
        }
        finally
        {
            DisposeDecodedHandles(decoded);
        }
    }

    private static T DecodeOk<T>(BamlOutboundValue value)
    {
        var decoded = DecodeValue(value);
        try
        {
            return ConvertDecoded<T>(decoded);
        }
        finally
        {
            DisposeDecodedHandles(decoded);
        }
    }

    private static T DecodePanic<T>(BamlOutboundPanic panic)
    {
        if (panic.IsExitPanic)
        {
            NativeApi.FlushEvents();
            Environment.Exit(checked((int)panic.ExitCode));
        }

        var decoded = DecodeValue(panic.Value);
        try
        {
            var className = FindClassName(panic.Value);
            if (className == CancelledPanicClass)
            {
                throw new BamlCancelledException(ToDynamicValue(decoded), panic.Trace.ToArray())
                {
                    ClassName = className,
                };
            }

            throw new BamlPanic(ToDynamicValue(decoded), panic.Trace.ToArray())
            {
                ClassName = className,
            };
        }
        finally
        {
            DisposeDecodedHandles(decoded);
        }
    }

    private static object? DecodeValue(BamlOutboundValue? value, int depth = 0)
    {
        if (depth > MaxValueDepth)
        {
            throw new BamlBridgeException($"A native value exceeds the BAML bridge nesting limit of {MaxValueDepth}.");
        }

        if (value is null)
        {
            return null;
        }

        return value.ValueCase switch
        {
            BamlOutboundValue.ValueOneofCase.None => null,
            BamlOutboundValue.ValueOneofCase.NullValue => null,
            BamlOutboundValue.ValueOneofCase.StringValue => value.StringValue,
            BamlOutboundValue.ValueOneofCase.IntValue => value.IntValue,
            BamlOutboundValue.ValueOneofCase.FloatValue => value.FloatValue,
            BamlOutboundValue.ValueOneofCase.BoolValue => value.BoolValue,
            BamlOutboundValue.ValueOneofCase.Uint8ArrayValue => value.Uint8ArrayValue.ToByteArray(),
            BamlOutboundValue.ValueOneofCase.BigintValue => ParseBigInteger(value.BigintValue),
            BamlOutboundValue.ValueOneofCase.LiteralValue => DecodeLiteral(value.LiteralValue),
            BamlOutboundValue.ValueOneofCase.ListValue => value.ListValue.Items
                .Select(item => DecodeValue(item, depth + 1))
                .ToList(),
            BamlOutboundValue.ValueOneofCase.MapValue => new DecodedMap(
                value.MapValue.KeyType,
                DecodeEntries(value.MapValue.Entries, depth)),
            BamlOutboundValue.ValueOneofCase.ClassValue => new DecodedClass(
                value.ClassValue.Name,
                value.ClassValue.TypeArgs.ToArray(),
                DecodeEntries(value.ClassValue.Fields, depth)),
            BamlOutboundValue.ValueOneofCase.EnumValue => new DecodedEnum(
                value.EnumValue.Name,
                value.EnumValue.Value),
            BamlOutboundValue.ValueOneofCase.UnionVariantValue => new DecodedUnion(
                value.UnionVariantValue.SelfType,
                value.UnionVariantValue.ValueOptionName,
                DecodeValue(value.UnionVariantValue.Value, depth + 1)),
            BamlOutboundValue.ValueOneofCase.HandleValue => new DecodedHandle(
                NativeHandle.FromOwned(value.HandleValue.Key, (int)value.HandleValue.HandleType),
                value.HandleValue.Ty),
            _ => throw new BamlBridgeException($"Outbound BAML value case {value.ValueCase} is not supported by the C# bridge yet."),
        };
    }

    private static Dictionary<string, object?> DecodeEntries(
        IEnumerable<BamlOutboundMapEntry> entries,
        int depth)
    {
        var decoded = new Dictionary<string, object?>(StringComparer.Ordinal);
        foreach (var entry in entries)
        {
            if (!decoded.TryAdd(entry.Key, DecodeValue(entry.Value, depth + 1)))
            {
                throw new BamlBridgeException($"The native runtime returned duplicate map key {entry.Key}.");
            }
        }

        return decoded;
    }

    private static object? ToDynamicValue(object? value) => value switch
    {
        DecodedClass classValue when MediaTargetType(classValue.Name) is { } mediaType =>
            ConvertMediaClass(classValue, mediaType),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.llm.PromptAst",
            StringComparison.Ordinal) => ConvertPromptAstClass(classValue),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.http.Request",
            StringComparison.Ordinal) => ConvertHttpRequest(classValue, 0),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.http.Response",
            StringComparison.Ordinal) => ConvertHttpResponse(classValue, 0),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.fs.File",
            StringComparison.Ordinal) => ConvertFile(classValue),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.http.SseStream",
            StringComparison.Ordinal) => ConvertSseStream(classValue),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.llm.Client",
            StringComparison.Ordinal) => ConvertClient(classValue, 0),
        DecodedClass classValue when string.Equals(
            classValue.Name,
            "baml.llm.RetryPolicy",
            StringComparison.Ordinal) => ConvertRetryPolicy(classValue, 0),
        DecodedClass classValue => classValue.Fields.ToDictionary(
            static entry => entry.Key,
            static entry => ToDynamicValue(entry.Value),
            StringComparer.Ordinal),
        DecodedEnum enumValue => enumValue.Variant,
        DecodedUnion union => ToDynamicValue(union.Value),
        DecodedHandle handle when MediaTargetType(handle.HandleType) is { } mediaType =>
            TakeMediaHandle(handle, mediaType),
        DecodedHandle handle when (BamlHandleType)handle.HandleType == BamlHandleType.AdtPromptAst =>
            TakePromptAstHandle(handle),
        DecodedHandle handle => BamlHandle.FromOwnedHandle(handle.Take()),
        IReadOnlyList<object?> list => list.Select(ToDynamicValue).ToList(),
        DecodedMap map => map.Values.ToDictionary(
            static entry => entry.Key,
            static entry => ToDynamicValue(entry.Value),
            StringComparer.Ordinal),
        _ => value,
    };

    private static string? FindClassName(BamlOutboundValue? value)
    {
        while (value?.ValueCase == BamlOutboundValue.ValueOneofCase.UnionVariantValue)
        {
            value = value.UnionVariantValue.Value;
        }

        return value?.ValueCase == BamlOutboundValue.ValueOneofCase.ClassValue
            ? value.ClassValue.Name
            : null;
    }

    private static object DecodeLiteral(BamlLiteralValue value) => value.LiteralCase switch
    {
        BamlLiteralValue.LiteralOneofCase.StringValue => value.StringValue,
        BamlLiteralValue.LiteralOneofCase.IntValue => value.IntValue,
        BamlLiteralValue.LiteralOneofCase.BoolValue => value.BoolValue,
        BamlLiteralValue.LiteralOneofCase.BigintValue => ParseBigInteger(value.BigintValue),
        BamlLiteralValue.LiteralOneofCase.FloatValue => ParseFloatLiteral(value.FloatValue),
        _ => throw new BamlBridgeException("The native runtime returned an empty BAML literal value."),
    };

    private static T ConvertDecoded<T>(object? value)
    {
        var converted = ConvertDecoded(value, typeof(T), 0);
        return converted is null ? default! : (T)converted;
    }

    private static object? ConvertDecoded(object? value, Type targetType, int depth)
    {
        if (depth > MaxValueDepth)
        {
            throw new BamlBridgeException($"A decoded value exceeds the BAML bridge nesting limit of {MaxValueDepth}.");
        }

        if (targetType.IsGenericType
            && targetType.GetGenericTypeDefinition() == typeof(BamlOptional<>))
        {
            var inner = ConvertDecoded(value, targetType.GetGenericArguments()[0], depth + 1);
            return targetType.GetMethod(nameof(BamlOptional<object>.FromValue), BindingFlags.Public | BindingFlags.Static)!
                .Invoke(null, new[] { inner });
        }

        if (targetType.IsGenericType
            && targetType.GetGenericTypeDefinition() == typeof(BamlNullable<>))
        {
            if (value is null)
            {
                return Activator.CreateInstance(targetType);
            }

            var inner = ConvertDecoded(value, targetType.GetGenericArguments()[0], depth + 1);
            return targetType.GetMethod(nameof(BamlNullable<object>.FromValue), BindingFlags.Public | BindingFlags.Static)!
                .Invoke(null, new[] { inner });
        }

        if (GeneratedContracts.GetTypeAlias(targetType) is { } typeAliasContract)
        {
            if (value is DecodedUnion
                {
                    SelfType.TyCase: BamlTy.TyOneofCase.TypeAlias,
                } aliasUnion
                && !string.Equals(
                    aliasUnion.SelfType.TypeAlias.Name,
                    typeAliasContract.WireName,
                    StringComparison.Ordinal))
            {
                throw new BamlBridgeException(
                    $"The native runtime returned BAML type alias {aliasUnion.SelfType.TypeAlias.Name}, but generated C# code expected {typeAliasContract.WireName}.");
            }

            var valueType = typeAliasContract.ValueProperty.PropertyType;
            var converted = value is not null
                && value is not DecodedUnion
                && valueType.IsGenericType
                && typeof(IBamlUnionValue).IsAssignableFrom(valueType)
                    ? ConvertErasedAliasUnion(value, valueType, depth + 1)
                    : ConvertDecoded(value, valueType, depth + 1);
            return typeAliasContract.Constructor.Invoke([converted]);
        }

        if (value is null)
        {
            if (!targetType.IsValueType || Nullable.GetUnderlyingType(targetType) is not null)
            {
                return null;
            }

            throw ExpectedTypeError(value, targetType);
        }

        if (targetType == typeof(object))
        {
            return ToDynamicValue(value);
        }

        if (targetType.IsInstanceOfType(value))
        {
            return value;
        }

        if (targetType.IsEnum && value is DecodedEnum enumValue)
        {
            if (targetType == typeof(BamlClientType))
            {
                return ConvertClientType(enumValue);
            }

            var contract = GeneratedContracts.GetEnum(targetType)
                ?? throw ExpectedTypeError(value, targetType);
            if (!string.Equals(contract.WireName, enumValue.Name, StringComparison.Ordinal))
            {
                throw new BamlBridgeException(
                    $"The native runtime returned BAML enum {enumValue.Name}, but generated C# code expected {contract.WireName}.");
            }

            return contract.FindByWireName(enumValue.Variant)?.Value
                ?? throw new BamlBridgeException(
                    $"The native runtime returned unknown variant {enumValue.Variant} for BAML enum {enumValue.Name}.");
        }

        if (value is DecodedClass mediaClass && typeof(BamlMedia).IsAssignableFrom(targetType))
        {
            return ConvertMediaClass(mediaClass, targetType);
        }

        if (value is DecodedClass streamFinishedClass
            && targetType == typeof(BamlStreamFinished))
        {
            if (!string.Equals(streamFinishedClass.Name, "baml.stream.StreamFinished", StringComparison.Ordinal)
                || streamFinishedClass.TypeArguments.Count != 0
                || streamFinishedClass.Fields.Count != 0)
            {
                throw ExpectedTypeError(value, targetType);
            }

            return BamlStreamFinished.Instance;
        }

        if (value is DecodedClass promptAstClass
            && targetType == typeof(BamlPromptAst))
        {
            return ConvertPromptAstClass(promptAstClass);
        }

        if (value is DecodedClass promptMessageClass
            && targetType == typeof(BamlPromptMessage))
        {
            return ConvertPromptMessage(promptMessageClass);
        }

        if (value is DecodedClass httpRequestClass
            && targetType == typeof(BamlHttpRequest))
        {
            return ConvertHttpRequest(httpRequestClass, depth);
        }

        if (value is DecodedClass httpResponseClass
            && targetType == typeof(BamlHttpResponse))
        {
            return ConvertHttpResponse(httpResponseClass, depth);
        }

        if (value is DecodedClass fileClass
            && targetType == typeof(BamlFile))
        {
            return ConvertFile(fileClass);
        }

        if (value is DecodedClass sseStreamClass
            && targetType == typeof(BamlSseStream))
        {
            return ConvertSseStream(sseStreamClass);
        }

        if (value is DecodedClass globClass
            && targetType == typeof(BamlGlob))
        {
            return ConvertGlob(globClass);
        }

        if (value is DecodedClass globScanOptionsClass
            && targetType == typeof(BamlGlobScanOptions))
        {
            return ConvertGlobScanOptions(globScanOptionsClass);
        }

        if (value is DecodedClass cancelTokenClass
            && targetType == typeof(BamlCancelToken))
        {
            return ConvertCancelToken(cancelTokenClass);
        }

        if (value is DecodedClass taskGroupClass
            && targetType == typeof(BamlTaskGroup))
        {
            return ConvertTaskGroup(taskGroupClass);
        }

        if (value is DecodedClass iteratorDoneClass
            && targetType == typeof(BamlIteratorDone))
        {
            return ConvertIteratorDone(iteratorDoneClass);
        }

        if (value is DecodedClass csvPositionClass
            && targetType == typeof(BamlCsvPosition))
        {
            return ConvertCsvPosition(csvPositionClass);
        }

        if (value is DecodedClass csvWriterOptionsClass
            && targetType == typeof(BamlCsvWriterOptions))
        {
            return ConvertCsvWriterOptions(csvWriterOptionsClass, depth);
        }

        if (value is DecodedClass csvReaderOptionsClass
            && targetType == typeof(BamlCsvReaderOptions))
        {
            return ConvertCsvReaderOptions(csvReaderOptionsClass, depth);
        }

        if (value is DecodedClass csvRecordClass
            && targetType == typeof(BamlCsvRecord))
        {
            return ConvertCsvRecord(csvRecordClass);
        }

        if (value is DecodedClass csvReaderClass
            && targetType == typeof(BamlCsvReader))
        {
            return ConvertCsvReader(csvReaderClass, depth);
        }

        if (value is DecodedClass csvWriterClass
            && targetType == typeof(BamlCsvWriter))
        {
            return ConvertCsvWriter(csvWriterClass, depth);
        }

        if (value is DecodedClass clientClass
            && targetType == typeof(BamlClient))
        {
            return ConvertClient(clientClass, depth);
        }

        if (value is DecodedClass retryPolicyClass
            && targetType == typeof(BamlRetryPolicy))
        {
            return ConvertRetryPolicy(retryPolicyClass, depth);
        }

        if (value is DecodedClass classValue
            && GeneratedContracts.GetClass(targetType) is { } classContract)
        {
            if (!string.Equals(classContract.WireName, classValue.Name, StringComparison.Ordinal))
            {
                throw new BamlBridgeException(
                    $"The native runtime returned BAML class {classValue.Name}, but generated C# code expected {classContract.WireName}.");
            }

            var expectedTypeArguments = targetType.IsGenericType
                ? targetType.GetGenericArguments()
                : Type.EmptyTypes;
            if (classValue.TypeArguments.Count != expectedTypeArguments.Length)
            {
                throw new BamlBridgeException(
                    $"The native runtime returned {classValue.TypeArguments.Count} generic type argument(s) for BAML class {classValue.Name}, but generated C# code expected {expectedTypeArguments.Length}.");
            }

            for (var index = 0; index < expectedTypeArguments.Length; index++)
            {
                var expected = ProtoTypeCodec.Encode(expectedTypeArguments[index]);
                if (!expected.Equals(classValue.TypeArguments[index]))
                {
                    throw new BamlBridgeException(
                        $"The native runtime returned generic type argument {FormatBamlType(classValue.TypeArguments[index])} at index {index} for BAML class {classValue.Name}, but generated C# code expected {FormatBamlType(expected)}.");
                }
            }

            var instance = Activator.CreateInstance(targetType)
                ?? throw new BamlBridgeException($"Could not construct generated BAML class {targetType.FullName}.");
            var remaining = new HashSet<string>(classValue.Fields.Keys, StringComparer.Ordinal);
            foreach (var property in classContract.Properties)
            {
                if (!classValue.Fields.TryGetValue(property.WireName, out var fieldValue))
                {
                    throw new BamlBridgeException(
                        $"The native runtime omitted required field {property.WireName} from BAML class {classValue.Name}.");
                }

                property.Property.SetValue(
                    instance,
                    ConvertDecoded(fieldValue, property.Property.PropertyType, depth + 1));
                remaining.Remove(property.WireName);
            }

            if (remaining.Count != 0)
            {
                throw new BamlBridgeException(
                    $"The native runtime returned unknown field {remaining.Order(StringComparer.Ordinal).First()} for BAML class {classValue.Name}.");
            }

            return instance;
        }

        if (value is DecodedUnion unionValue
            && targetType.IsGenericType
            && typeof(IBamlUnionValue).IsAssignableFrom(targetType))
        {
            var typeArguments = targetType.GetGenericArguments();
            var activeCase = ResolveUnionCase(unionValue, typeArguments);
            var converted = ConvertDecoded(unionValue.Value, typeArguments[activeCase - 1], depth + 1);
            var factory = targetType.GetMethod(
                $"FromT{activeCase - 1}",
                BindingFlags.Public | BindingFlags.Static)
                ?? throw new BamlBridgeException(
                    $"Generated BAML union type {targetType.FullName} has no factory for active case {activeCase}.");
            return factory.Invoke(null, new[] { converted })!;
        }

        if (value is DecodedHandle handleValue && typeof(BamlMedia).IsAssignableFrom(targetType))
        {
            return TakeMediaHandle(handleValue, targetType);
        }

        if (value is DecodedHandle opaqueHandle && targetType == typeof(BamlHandle))
        {
            return BamlHandle.FromOwnedHandle(opaqueHandle.Take());
        }

        if (value is DecodedHandle promptHandle && targetType == typeof(BamlPromptAst))
        {
            return TakePromptAstHandle(promptHandle);
        }

        if (value is DecodedHandle streamHandle
            && targetType.IsGenericType
            && targetType.GetGenericTypeDefinition() == typeof(BamlStream<,>))
        {
            ValidateStreamHandle(streamHandle, targetType);
            return Activator.CreateInstance(
                targetType,
                BindingFlags.Instance | BindingFlags.NonPublic,
                binder: null,
                args: [streamHandle.Take()],
                culture: null)!;
        }

        var nullableType = Nullable.GetUnderlyingType(targetType);
        if (nullableType is not null)
        {
            return ConvertDecoded(value, nullableType, depth + 1);
        }

        if (targetType.IsArray
            && targetType.GetArrayRank() == 1
            && value is IReadOnlyList<object?> arrayValues)
        {
            var elementType = targetType.GetElementType()!;
            var array = Array.CreateInstance(elementType, arrayValues.Count);
            for (var index = 0; index < arrayValues.Count; index++)
            {
                array.SetValue(ConvertDecoded(arrayValues[index], elementType, depth + 1), index);
            }

            return array;
        }

        if (targetType.IsGenericType
            && targetType.GetGenericTypeDefinition() == typeof(List<>)
            && value is IReadOnlyList<object?> listValues)
        {
            var elementType = targetType.GetGenericArguments()[0];
            var list = (IList)Activator.CreateInstance(targetType)!;
            foreach (var item in listValues)
            {
                list.Add(ConvertDecoded(item, elementType, depth + 1));
            }

            return list;
        }

        if (targetType.IsGenericType
            && targetType.GetGenericTypeDefinition() == typeof(Dictionary<,>)
            && value is DecodedMap map)
        {
            var typeArguments = targetType.GetGenericArguments();
            var dictionary = (IDictionary)Activator.CreateInstance(targetType)!;
            foreach (var (wireKey, item) in map.Values)
            {
                var key = ConvertMapKey(wireKey, typeArguments[0], map.KeyType);
                if (dictionary.Contains(key))
                {
                    throw new BamlBridgeException(
                        $"The native runtime returned duplicate map key {wireKey} after conversion to {typeArguments[0].FullName}.");
                }

                dictionary.Add(key, ConvertDecoded(item, typeArguments[1], depth + 1));
            }

            return dictionary;
        }

        throw ExpectedTypeError(value, targetType);
    }

    private static object ConvertErasedAliasUnion(object value, Type unionType, int depth)
    {
        var typeArguments = unionType.GetGenericArguments();
        var candidates = typeArguments
            .Select((type, index) => (Type: type, Index: index))
            .Where(candidate => ErasedValueMatchesType(value, candidate.Type))
            .ToArray();
        if (candidates.Length != 1)
        {
            throw new BamlBridgeException(
                $"The native runtime returned an erased recursive-alias value matching {candidates.Length} arm(s) of {unionType.FullName}; exactly one is required.");
        }

        var selected = candidates[0];
        var converted = ConvertDecoded(value, selected.Type, depth + 1);
        var factory = unionType.GetMethod(
            $"FromT{selected.Index}",
            BindingFlags.Public | BindingFlags.Static)
            ?? throw new BamlBridgeException(
                $"Generated BAML union type {unionType.FullName} has no factory for active case {selected.Index + 1}.");
        return factory.Invoke(null, [converted])!;
    }

    private static bool ErasedValueMatchesType(object value, Type targetType)
    {
        if (targetType.IsInstanceOfType(value))
        {
            return true;
        }

        if (GeneratedContracts.GetTypeAlias(targetType) is { } alias)
        {
            return ErasedValueMatchesType(value, alias.ValueProperty.PropertyType);
        }

        if (targetType.IsEnum)
        {
            return value is DecodedEnum;
        }

        if (GeneratedContracts.GetClass(targetType) is not null
            || BuiltinClassName(targetType) is not null)
        {
            return value is DecodedClass;
        }

        if (targetType.IsGenericType)
        {
            var definition = targetType.GetGenericTypeDefinition();
            if (definition == typeof(List<>))
            {
                return value is IReadOnlyList<object?>;
            }

            if (definition == typeof(Dictionary<,>))
            {
                return value is DecodedMap;
            }

            if (typeof(IBamlUnionValue).IsAssignableFrom(targetType))
            {
                return targetType.GetGenericArguments().Count(type => ErasedValueMatchesType(value, type)) == 1;
            }
        }

        return false;
    }

    private static BamlBridgeException ExpectedTypeError(object? value, Type targetType) => new(
        $"The native runtime returned {value?.GetType().FullName ?? "null"}, but generated C# code expected {targetType.FullName}.");

    private static object ConvertMapKey(string wireKey, Type targetType, BamlTy? keyType)
    {
        if (targetType == typeof(string))
        {
            ValidatePrimitiveMapKeyType(
                keyType,
                BamlTyPrimitiveKind.BamlTyPrimitiveString,
                targetType,
                allowMissing: true);
            return wireKey;
        }

        if (targetType == typeof(long))
        {
            ValidatePrimitiveMapKeyType(
                keyType,
                BamlTyPrimitiveKind.BamlTyPrimitiveInt,
                targetType,
                allowMissing: false);
            if (!long.TryParse(wireKey, NumberStyles.AllowLeadingSign, CultureInfo.InvariantCulture, out var value)
                || value is < MinBamlInteger or > MaxBamlInteger
                || !string.Equals(value.ToString(CultureInfo.InvariantCulture), wireKey, StringComparison.Ordinal))
            {
                throw new BamlBridgeException($"The native runtime returned invalid BAML int map key {wireKey}.");
            }

            return value;
        }

        if (targetType == typeof(bool))
        {
            ValidatePrimitiveMapKeyType(
                keyType,
                BamlTyPrimitiveKind.BamlTyPrimitiveBool,
                targetType,
                allowMissing: false);
            return wireKey switch
            {
                "true" => true,
                "false" => false,
                _ => throw new BamlBridgeException($"The native runtime returned invalid BAML bool map key {wireKey}."),
            };
        }

        if (targetType.IsEnum)
        {
            if (targetType == typeof(BamlClientType))
            {
                if (keyType?.TyCase != BamlTy.TyOneofCase.Enum
                    || !string.Equals(keyType.Enum.Name, "baml.llm.ClientType", StringComparison.Ordinal))
                {
                    throw MapKeyTypeError(keyType, targetType);
                }

                const string clientTypePrefix = "baml.llm.ClientType::";
                if (!wireKey.StartsWith(clientTypePrefix, StringComparison.Ordinal))
                {
                    throw new BamlBridgeException(
                        $"The native runtime returned invalid key {wireKey} for BAML enum baml.llm.ClientType.");
                }

                return ConvertClientType(new DecodedEnum(
                    "baml.llm.ClientType",
                    wireKey[clientTypePrefix.Length..]));
            }

            var contract = GeneratedContracts.GetEnum(targetType)
                ?? throw new BamlBridgeException(
                    $"Decoded C# dictionary key enum {targetType.FullName} is not a generated BAML enum.");
            if (keyType?.TyCase != BamlTy.TyOneofCase.Enum
                || !string.Equals(keyType.Enum.Name, contract.WireName, StringComparison.Ordinal))
            {
                throw MapKeyTypeError(keyType, targetType);
            }

            var prefix = $"{contract.WireName}::";
            if (!wireKey.StartsWith(prefix, StringComparison.Ordinal))
            {
                throw new BamlBridgeException(
                    $"The native runtime returned invalid key {wireKey} for BAML enum {contract.WireName}.");
            }

            var variant = wireKey[prefix.Length..];
            return contract.FindByWireName(variant)?.Value
                ?? throw new BamlBridgeException(
                    $"The native runtime returned unknown map-key variant {variant} for BAML enum {contract.WireName}.");
        }

        throw new BamlBridgeException(
            $"Decoded C# dictionary key type {targetType.FullName} is not supported; BAML map keys must be string, bool, long, or a generated BAML enum.");
    }

    private static void ValidatePrimitiveMapKeyType(
        BamlTy? keyType,
        BamlTyPrimitiveKind expected,
        Type targetType,
        bool allowMissing)
    {
        if (allowMissing && keyType is null)
        {
            return;
        }

        if (keyType?.TyCase != BamlTy.TyOneofCase.Primitive || keyType.Primitive.Kind != expected)
        {
            throw MapKeyTypeError(keyType, targetType);
        }
    }

    private static BamlBridgeException MapKeyTypeError(BamlTy? keyType, Type targetType) => new(
        $"The native runtime returned map key type {keyType?.TyCase.ToString() ?? "missing"}, but generated C# code expected {targetType.FullName}.");

    private static int ResolveUnionCase(DecodedUnion value, IReadOnlyList<Type> targetTypes)
    {
        var selfType = value.SelfType;
        if (selfType?.TyCase == BamlTy.TyOneofCase.Optional)
        {
            selfType = selfType.Optional.Inner;
        }

        if (selfType?.TyCase == BamlTy.TyOneofCase.TypeAlias)
        {
            var aliasMatches = targetTypes
                .Select((type, index) => (Type: ProtoTypeCodec.Encode(type), Index: index))
                .Where(item => SelectedOptionMatches(item.Type, value.SelectedOption))
                .ToArray();
            if (aliasMatches.Length != 1 || value.Value is null)
            {
                throw new BamlBridgeException(
                    $"The native runtime selected unknown or ambiguous type-alias option {value.SelectedOption}.");
            }

            return aliasMatches[0].Index + 1;
        }

        if (selfType?.TyCase != BamlTy.TyOneofCase.Union)
        {
            throw new BamlBridgeException(
                $"The native runtime returned invalid union metadata for expected C# type BamlUnion<{string.Join(", ", targetTypes.Select(static type => type.FullName))}>.");
        }

        var options = selfType.Union.Options
            .Where(static option => !IsNullType(option))
            .ToArray();
        if (options.Length != targetTypes.Count)
        {
            throw new BamlBridgeException(
                $"The native runtime returned {options.Length} non-null union options, but generated C# code expected {targetTypes.Count}.");
        }

        var matches = options
            .Select((option, index) => (Option: option, Index: index))
            .Where(item => SelectedOptionMatches(item.Option, value.SelectedOption))
            .ToArray();
        if (matches.Length != 1)
        {
            throw new BamlBridgeException(
                $"The native runtime selected unknown or ambiguous union option {value.SelectedOption}.");
        }

        for (var index = 0; index < targetTypes.Count; index++)
        {
            ValidateUnionArmType(options[index], targetTypes[index]);
        }

        if (value.Value is null)
        {
            throw new BamlBridgeException(
                $"The native runtime returned null for non-null union option {value.SelectedOption}.");
        }

        return matches[0].Index + 1;
    }

    private static bool SelectedOptionMatches(BamlTy option, string selectedOption)
    {
        if (string.Equals(FormatBamlType(option), selectedOption, StringComparison.Ordinal))
        {
            return true;
        }

        var name = option.TyCase switch
        {
            BamlTy.TyOneofCase.ClassTy => option.ClassTy.Name,
            BamlTy.TyOneofCase.Enum => option.Enum.Name,
            BamlTy.TyOneofCase.TypeAlias => option.TypeAlias.Name,
            _ => null,
        };
        return name is not null
            && string.Equals(name[(name.LastIndexOf('.') + 1)..], selectedOption, StringComparison.Ordinal);
    }

    private static bool IsNullType(BamlTy type) => type.TyCase == BamlTy.TyOneofCase.Primitive
        && type.Primitive.Kind == BamlTyPrimitiveKind.BamlTyPrimitiveNull;

    private static void ValidateUnionArmType(BamlTy option, Type targetType)
    {
        var matches = option.TyCase switch
        {
            BamlTy.TyOneofCase.Primitive => option.Primitive.Kind switch
            {
                BamlTyPrimitiveKind.BamlTyPrimitiveString => targetType == typeof(string),
                BamlTyPrimitiveKind.BamlTyPrimitiveInt => targetType == typeof(long),
                BamlTyPrimitiveKind.BamlTyPrimitiveFloat => targetType == typeof(double),
                BamlTyPrimitiveKind.BamlTyPrimitiveBool => targetType == typeof(bool),
                BamlTyPrimitiveKind.BamlTyPrimitiveBytes => targetType == typeof(byte[]),
                BamlTyPrimitiveKind.BamlTyPrimitiveBigint => targetType == typeof(BigInteger),
                _ => false,
            },
            BamlTy.TyOneofCase.Enum => targetType.IsEnum
                && (targetType == typeof(BamlClientType)
                    ? string.Equals(option.Enum.Name, "baml.llm.ClientType", StringComparison.Ordinal)
                    : GeneratedContracts.GetEnum(targetType) is { } enumContract
                        && string.Equals(option.Enum.Name, enumContract.WireName, StringComparison.Ordinal)),
            BamlTy.TyOneofCase.ClassTy => GeneratedContracts.GetClass(targetType) is not null
                ? option.Equals(ProtoTypeCodec.Encode(targetType))
                : BuiltinClassName(targetType) is { } builtinClassName
                    && string.Equals(option.ClassTy.Name, builtinClassName, StringComparison.Ordinal),
            BamlTy.TyOneofCase.List => targetType.IsGenericType
                && targetType.GetGenericTypeDefinition() == typeof(List<>)
                && UnionArmTypeMatches(option.List.Item, targetType.GetGenericArguments()[0]),
            BamlTy.TyOneofCase.Map => targetType.IsGenericType
                && targetType.GetGenericTypeDefinition() == typeof(Dictionary<,>)
                && UnionArmTypeMatches(option.Map.Key, targetType.GetGenericArguments()[0])
                && UnionArmTypeMatches(option.Map.Value, targetType.GetGenericArguments()[1]),
            BamlTy.TyOneofCase.Literal => option.Literal.LiteralCase switch
            {
                BamlTyLiteral.LiteralOneofCase.StringValue => targetType == typeof(string),
                BamlTyLiteral.LiteralOneofCase.IntValue => targetType == typeof(long),
                BamlTyLiteral.LiteralOneofCase.BoolValue => targetType == typeof(bool),
                BamlTyLiteral.LiteralOneofCase.BigintValue => targetType == typeof(BigInteger),
                BamlTyLiteral.LiteralOneofCase.FloatValue => targetType == typeof(double),
                _ => false,
            },
            BamlTy.TyOneofCase.Media => option.Media.Kind switch
            {
                BamlTyMediaKind.Image => targetType == typeof(BamlImage),
                BamlTyMediaKind.Audio => targetType == typeof(BamlAudio),
                BamlTyMediaKind.Video => targetType == typeof(BamlVideo),
                BamlTyMediaKind.Pdf => targetType == typeof(BamlPdf),
                _ => false,
            },
            BamlTy.TyOneofCase.RustType => targetType == typeof(BamlHandle),
            BamlTy.TyOneofCase.TypeAlias => GeneratedContracts.GetTypeAlias(targetType) is { } alias
                && string.Equals(option.TypeAlias.Name, alias.WireName, StringComparison.Ordinal),
            _ => false,
        };
        if (!matches)
        {
            throw new BamlBridgeException(
                $"The native runtime returned union arm {FormatBamlType(option)}, but generated C# code expected {targetType.FullName}.");
        }
    }

    private static bool UnionArmTypeMatches(BamlTy option, Type targetType)
    {
        try
        {
            ValidateUnionArmType(option, targetType);
            return true;
        }
        catch (BamlBridgeException)
        {
            return false;
        }
    }

    private static string FormatBamlType(BamlTy type) => type.TyCase switch
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
            _ => "<invalid-primitive>",
        },
        BamlTy.TyOneofCase.ClassTy => FormatNominalType(type.ClassTy.Name, type.ClassTy.TypeArgs),
        BamlTy.TyOneofCase.Enum => type.Enum.Name,
        BamlTy.TyOneofCase.TypeAlias => FormatNominalType(type.TypeAlias.Name, type.TypeAlias.TypeArgs),
        BamlTy.TyOneofCase.List => $"{FormatBamlType(type.List.Item)}[]",
        BamlTy.TyOneofCase.Map => $"map<{FormatBamlType(type.Map.Key)}, {FormatBamlType(type.Map.Value)}>",
        BamlTy.TyOneofCase.Union => $"({string.Join(" | ", type.Union.Options.Select(FormatBamlType))})",
        BamlTy.TyOneofCase.Literal => type.Literal.LiteralCase switch
        {
            BamlTyLiteral.LiteralOneofCase.StringValue =>
                System.Text.Json.JsonSerializer.Serialize(type.Literal.StringValue),
            BamlTyLiteral.LiteralOneofCase.IntValue => type.Literal.IntValue.ToString(CultureInfo.InvariantCulture),
            BamlTyLiteral.LiteralOneofCase.BoolValue =>
                type.Literal.BoolValue.ToString().ToLowerInvariant(),
            BamlTyLiteral.LiteralOneofCase.BigintValue => $"{type.Literal.BigintValue}n",
            BamlTyLiteral.LiteralOneofCase.FloatValue => type.Literal.FloatValue,
            _ => "<invalid-literal>",
        },
        BamlTy.TyOneofCase.Media => type.Media.Kind.ToString().ToLowerInvariant(),
        BamlTy.TyOneofCase.RustType => "$rust_type",
        _ => $"<{type.TyCase}>",
    };

    private static string FormatNominalType(string name, IEnumerable<BamlTy> typeArguments)
    {
        var arguments = typeArguments.Select(FormatBamlType).ToArray();
        return arguments.Length == 0 ? name : $"{name}<{string.Join(", ", arguments)}>";
    }

    private static BamlMedia TakeMediaHandle(DecodedHandle decoded, Type targetType)
    {
        var expectedHandleType = targetType == typeof(BamlImage)
            ? BamlHandleType.AdtMediaImage
            : targetType == typeof(BamlAudio)
                ? BamlHandleType.AdtMediaAudio
                : targetType == typeof(BamlVideo)
                    ? BamlHandleType.AdtMediaVideo
                    : targetType == typeof(BamlPdf)
                        ? BamlHandleType.AdtMediaPdf
                        : BamlHandleType.HandleUnspecified;
        if (expectedHandleType == BamlHandleType.HandleUnspecified
            || decoded.HandleType != (int)expectedHandleType)
        {
            throw new BamlBridgeException(
                $"The native runtime returned media handle type {(BamlHandleType)decoded.HandleType}, but generated C# code expected {targetType.FullName}.");
        }

        var handle = decoded.Take();
        return targetType == typeof(BamlImage)
            ? BamlImage.FromOwnedHandle(handle)
            : targetType == typeof(BamlAudio)
                ? BamlAudio.FromOwnedHandle(handle)
                : targetType == typeof(BamlVideo)
                    ? BamlVideo.FromOwnedHandle(handle)
                    : BamlPdf.FromOwnedHandle(handle);
    }

    private static BamlPromptAst TakePromptAstHandle(DecodedHandle decoded)
    {
        if ((BamlHandleType)decoded.HandleType != BamlHandleType.AdtPromptAst)
        {
            throw new BamlBridgeException(
                $"The native runtime returned handle type {(BamlHandleType)decoded.HandleType}, but generated C# code expected {typeof(BamlPromptAst).FullName}.");
        }

        return new BamlPromptAst(decoded.Take());
    }

    private static BamlPromptAst ConvertPromptAstClass(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.llm.PromptAst", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_data", out var data)
            || data is not DecodedHandle handle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed prompt AST class {value.Name}.");
        }

        return TakePromptAstHandle(handle);
    }

    private static BamlPromptMessage ConvertPromptMessage(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.llm.PromptMessage", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 2
            || !value.Fields.TryGetValue("role", out var role)
            || role is not string roleText
            || !value.Fields.TryGetValue("content", out var content)
            || content is not string contentText)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed prompt message class {value.Name}.");
        }

        return new BamlPromptMessage(roleText, contentText);
    }

    private static BamlHttpRequest ConvertHttpRequest(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.http.Request", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 4
            || !value.Fields.TryGetValue("method", out var method)
            || method is not string methodText
            || !value.Fields.TryGetValue("url", out var url)
            || url is not string urlText
            || !value.Fields.TryGetValue("headers", out var headers)
            || headers is not DecodedMap
            || !value.Fields.TryGetValue("body", out var body)
            || body is not string bodyText)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed HTTP request class {value.Name}.");
        }

        var convertedHeaders = (Dictionary<string, string>)ConvertDecoded(
            headers,
            typeof(Dictionary<string, string>),
            depth + 1)!;
        return new BamlHttpRequest(methodText, urlText, convertedHeaders, bodyText);
    }

    private static BamlHttpResponse ConvertHttpResponse(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.http.Response", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 4
            || !value.Fields.TryGetValue("status_code", out var statusCode)
            || statusCode is not long statusCodeValue
            || !value.Fields.TryGetValue("headers", out var headers)
            || headers is not DecodedMap
            || !value.Fields.TryGetValue("url", out var url)
            || url is not string urlText
            || !value.Fields.TryGetValue("_body", out var body)
            || body is not DecodedHandle bodyHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed HTTP response class {value.Name}.");
        }

        ValidateUntaggedRustData(bodyHandle, typeof(BamlHttpResponse));
        var convertedHeaders = (Dictionary<string, string>)ConvertDecoded(
            headers,
            typeof(Dictionary<string, string>),
            depth + 1)!;
        return new BamlHttpResponse(statusCodeValue, convertedHeaders, urlText, bodyHandle.Take());
    }

    private static BamlFile ConvertFile(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.fs.File", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle fileHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed file class {value.Name}.");
        }

        ValidateUntaggedRustData(fileHandle, typeof(BamlFile));
        return new BamlFile(fileHandle.Take());
    }

    private static BamlSseStream ConvertSseStream(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.http.SseStream", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 2
            || !value.Fields.TryGetValue("url", out var url)
            || url is not string urlText
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle streamHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed SSE stream class {value.Name}.");
        }

        ValidateUntaggedRustData(streamHandle, typeof(BamlSseStream));
        return new BamlSseStream(urlText, streamHandle.Take());
    }

    private static BamlGlob ConvertGlob(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.glob.Glob", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle globHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed glob class {value.Name}.");
        }

        ValidateUntaggedRustData(globHandle, typeof(BamlGlob));
        return new BamlGlob(globHandle.Take());
    }

    private static BamlCancelToken ConvertCancelToken(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.spawn.CancelToken", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle tokenHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed cancel token class {value.Name}.");
        }

        ValidateUntaggedRustData(tokenHandle, typeof(BamlCancelToken));
        return new BamlCancelToken(tokenHandle.Take());
    }

    private static BamlTaskGroup ConvertTaskGroup(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.spawn.TaskGroup", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle groupHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed task group class {value.Name}.");
        }

        ValidateUntaggedRustData(groupHandle, typeof(BamlTaskGroup));
        return new BamlTaskGroup(groupHandle.Take());
    }

    private static BamlCsvWriter ConvertCsvWriter(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.csv.CsvWriter", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 3
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle writerHandle
            || !value.Fields.TryGetValue("_file", out var file)
            || file is not null and not DecodedClass
            || !value.Fields.TryGetValue("_owns_file", out var ownsFile)
            || ownsFile is not bool ownsFileValue)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV writer class {value.Name}.");
        }

        ValidateUntaggedRustData(writerHandle, typeof(BamlCsvWriter));
        var convertedFile = file is null
            ? null
            : (BamlFile)ConvertDecoded(file, typeof(BamlFile), depth + 1)!;
        return new BamlCsvWriter(writerHandle.Take(), convertedFile, ownsFileValue);
    }

    private static BamlIteratorDone ConvertIteratorDone(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.iter.Done", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 0)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed iterator completion class {value.Name}.");
        }

        return BamlIteratorDone.Instance;
    }

    private static BamlCsvPosition ConvertCsvPosition(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.csv.CsvPosition", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 3
            || !value.Fields.TryGetValue("byte", out var byteOffset)
            || byteOffset is not long byteOffsetValue
            || !value.Fields.TryGetValue("line", out var line)
            || line is not long lineValue
            || !value.Fields.TryGetValue("record", out var record)
            || record is not long recordValue)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV position class {value.Name}.");
        }

        return new BamlCsvPosition(byteOffsetValue, lineValue, recordValue);
    }

    private static BamlCsvWriterOptions ConvertCsvWriterOptions(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.csv.WriterOptions", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 10
            || !TryOptionalString(value.Fields, "delimiter", out var delimiter)
            || !TryOptionalString(value.Fields, "quote", out var quote)
            || !TryOptionalLiteralString(
                value.Fields,
                "quote_style",
                ["minimal", "all", "never"],
                out var quoteStyle)
            || !TryOptionalString(value.Fields, "escape", out var escape)
            || !TryOptionalLiteralString(
                value.Fields,
                "terminator",
                ["lf", "crlf"],
                out var terminator)
            || !TryOptionalBool(value.Fields, "write_header", out var writeHeader)
            || !value.Fields.TryGetValue("headers", out var headers)
            || headers is not null and not IReadOnlyList<object?>
            || !TryOptionalString(value.Fields, "null_value", out var nullValue)
            || !TryOptionalBool(value.Fields, "bom", out var bom)
            || !TryOptionalBool(value.Fields, "sanitize_formulas", out var sanitizeFormulas))
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV writer options class {value.Name} with fields [{string.Join(", ", value.Fields.OrderBy(static field => field.Key, StringComparer.Ordinal).Select(DescribeDecodedField))}].");
        }

        var convertedHeaders = headers is null
            ? null
            : (List<string>)ConvertDecoded(headers, typeof(List<string>), depth + 1)!;
        return new BamlCsvWriterOptions(
            delimiter,
            quote,
            quoteStyle,
            escape,
            terminator,
            writeHeader,
            convertedHeaders,
            nullValue,
            bom,
            sanitizeFormulas);
    }

    private static BamlCsvReaderOptions ConvertCsvReaderOptions(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.csv.ReaderOptions", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 18
            || !TryOptionalString(value.Fields, "delimiter", out var delimiter)
            || !TryOptionalString(value.Fields, "quote", out var quote)
            || !TryOptionalBool(value.Fields, "quoting", out var quoting)
            || !TryOptionalString(value.Fields, "escape", out var escape)
            || !TryOptionalBool(value.Fields, "has_header", out var hasHeader)
            || !TryOptionalStringList(value.Fields, "headers", depth, out var headers)
            || !TryOptionalString(value.Fields, "comment", out var comment)
            || !TryOptionalLiteralString(
                value.Fields,
                "trim",
                ["none", "headers", "fields", "all"],
                out var trim)
            || !TryOptionalLong(value.Fields, "skip_lines", out var skipLines)
            || !TryOptionalBool(
                value.Fields,
                "skip_blank_records",
                out var skipBlankRecords)
            || !TryOptionalLiteralString(
                value.Fields,
                "ragged",
                ["strict", "pad", "truncate"],
                out var ragged)
            || !TryOptionalStringList(value.Fields, "null_values", depth, out var nullValues)
            || !TryOptionalLiteralString(
                value.Fields,
                "encoding",
                ["utf8", "utf8-lossy"],
                out var encoding)
            || !TryOptionalLiteralString(
                value.Fields,
                "bom",
                ["strip", "keep"],
                out var bom)
            || !TryOptionalLiteralString(
                value.Fields,
                "on_error",
                ["throw", "skip"],
                out var onError)
            || !value.Fields.TryGetValue("on_skip", out var onSkip)
            || onSkip is not null
            || !TryOptionalLong(value.Fields, "max_skipped", out var maxSkipped)
            || !TryOptionalLong(value.Fields, "limit", out var limit))
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV reader options class {value.Name} with fields [{string.Join(", ", value.Fields.OrderBy(static field => field.Key, StringComparer.Ordinal).Select(DescribeDecodedField))}].");
        }

        return new BamlCsvReaderOptions(
            delimiter,
            quote,
            quoting,
            escape,
            hasHeader,
            headers,
            comment,
            trim,
            skipLines,
            skipBlankRecords,
            ragged,
            nullValues,
            encoding,
            bom,
            onError,
            onSkip: null,
            maxSkipped: maxSkipped,
            limit: limit);
    }

    private static BamlCsvRecord ConvertCsvRecord(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.csv.CsvRecord", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle recordHandle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV record class {value.Name}.");
        }

        ValidateUntaggedRustData(recordHandle, typeof(BamlCsvRecord));
        return new BamlCsvRecord(recordHandle.Take());
    }

    private static BamlCsvReader ConvertCsvReader(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.csv.CsvReader", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 4
            || !value.Fields.TryGetValue("_handle", out var handle)
            || handle is not DecodedHandle readerHandle
            || !value.Fields.TryGetValue("_file", out var file)
            || file is not null and not DecodedClass
            || !value.Fields.TryGetValue("_on_skip", out var onSkip)
            || onSkip is not null and not DecodedHandle
            || !value.Fields.TryGetValue("_owns_file", out var ownsFile)
            || ownsFile is not bool ownsFileValue)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed CSV reader class {value.Name}.");
        }

        ValidateUntaggedRustData(readerHandle, typeof(BamlCsvReader));
        var convertedFile = file is null
            ? null
            : (BamlFile)ConvertDecoded(file, typeof(BamlFile), depth + 1)!;
        var convertedOnSkip = onSkip is null
            ? null
            : BamlHandle.FromOwnedHandle(((DecodedHandle)onSkip).Take());
        return new BamlCsvReader(
            readerHandle.Take(),
            convertedFile,
            convertedOnSkip,
            ownsFileValue);
    }

    private static BamlGlobScanOptions ConvertGlobScanOptions(DecodedClass value)
    {
        if (!string.Equals(value.Name, "baml.glob.ScanOptions", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 6
            || !TryOptionalString(value.Fields, "cwd", out var cwd)
            || !TryOptionalBool(value.Fields, "dot", out var dot)
            || !TryOptionalBool(value.Fields, "absolute", out var absolute)
            || !TryOptionalBool(value.Fields, "follow_symlinks", out var followSymlinks)
            || !TryOptionalBool(
                value.Fields,
                "throw_error_on_broken_symlink",
                out var throwErrorOnBrokenSymlink)
            || !TryOptionalBool(value.Fields, "only_files", out var onlyFiles))
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed glob scan options class {value.Name}.");
        }

        return new BamlGlobScanOptions(
            cwd,
            dot,
            absolute,
            followSymlinks,
            throwErrorOnBrokenSymlink,
            onlyFiles);
    }

    private static bool TryOptionalString(
        IReadOnlyDictionary<string, object?> fields,
        string name,
        out string? value)
    {
        if (fields.TryGetValue(name, out var field) && field is null or string)
        {
            value = (string?)field;
            return true;
        }

        value = null;
        return false;
    }

    private static bool TryOptionalLong(
        IReadOnlyDictionary<string, object?> fields,
        string name,
        out long? value)
    {
        if (fields.TryGetValue(name, out var field) && field is null or long)
        {
            value = (long?)field;
            return true;
        }

        value = null;
        return false;
    }

    private static bool TryOptionalStringList(
        IReadOnlyDictionary<string, object?> fields,
        string name,
        int depth,
        out List<string>? value)
    {
        if (!fields.TryGetValue(name, out var field)
            || field is not null and not IReadOnlyList<object?>)
        {
            value = null;
            return false;
        }

        value = field is null
            ? null
            : (List<string>)ConvertDecoded(field, typeof(List<string>), depth + 1)!;
        return true;
    }

    private static bool TryOptionalLiteralString(
        IReadOnlyDictionary<string, object?> fields,
        string name,
        IReadOnlyCollection<string> allowed,
        out string? value)
    {
        if (!fields.TryGetValue(name, out var field))
        {
            value = null;
            return false;
        }

        if (field is null)
        {
            value = null;
            return true;
        }

        if (field is string text && allowed.Contains(text))
        {
            value = text;
            return true;
        }

        if (field is DecodedUnion { Value: string unionText } union
            && allowed.Contains(unionText))
        {
            var selfType = union.SelfType;
            if (selfType?.TyCase == BamlTy.TyOneofCase.Optional)
            {
                selfType = selfType.Optional.Inner;
            }

            if (selfType?.TyCase == BamlTy.TyOneofCase.Union
                && selfType.Union.Options.Count(option =>
                    option.TyCase == BamlTy.TyOneofCase.Literal
                    && option.Literal.LiteralCase == BamlTyLiteral.LiteralOneofCase.StringValue
                    && string.Equals(option.Literal.StringValue, unionText, StringComparison.Ordinal)) == 1)
            {
                value = unionText;
                return true;
            }
        }

        value = null;
        return false;
    }

    private static string DescribeDecodedField(KeyValuePair<string, object?> field) => field.Value switch
    {
        DecodedUnion union =>
            $"{field.Key}:union(selected={union.SelectedOption}, self={FormatBamlType(union.SelfType ?? new BamlTy())}, value={union.Value})",
        null => $"{field.Key}:null",
        _ => $"{field.Key}:{field.Value.GetType().FullName}",
    };

    private static bool TryOptionalBool(
        IReadOnlyDictionary<string, object?> fields,
        string name,
        out bool? value)
    {
        if (fields.TryGetValue(name, out var field) && field is null or bool)
        {
            value = (bool?)field;
            return true;
        }

        value = null;
        return false;
    }

    private static void ValidateUntaggedRustData(DecodedHandle handle, Type targetType)
    {
        if ((BamlHandleType)handle.HandleType != BamlHandleType.UntaggedRustData)
        {
            throw new BamlBridgeException(
                $"The native runtime returned handle type {(BamlHandleType)handle.HandleType}, but generated C# code expected {targetType.FullName}.");
        }
    }

    private static BamlClient ConvertClient(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.llm.Client", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 5
            || !value.Fields.TryGetValue("name", out var name)
            || name is not string nameText
            || !value.Fields.TryGetValue("client_type", out var clientType)
            || clientType is not DecodedEnum
            || !value.Fields.TryGetValue("sub_clients", out var subClients)
            || subClients is not IReadOnlyList<object?>
            || !value.Fields.TryGetValue("retry", out var retry)
            || retry is not null and not DecodedClass
            || !value.Fields.TryGetValue("counter", out var counter)
            || counter is not long counterValue)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed LLM client class {value.Name}.");
        }

        var convertedType = (BamlClientType)ConvertDecoded(
            clientType,
            typeof(BamlClientType),
            depth + 1)!;
        var convertedSubClients = (List<BamlClient>)ConvertDecoded(
            subClients,
            typeof(List<BamlClient>),
            depth + 1)!;
        var convertedRetry = retry is null
            ? null
            : (BamlRetryPolicy)ConvertDecoded(
                retry,
                typeof(BamlRetryPolicy),
                depth + 1)!;
        return new BamlClient(nameText, convertedType, convertedSubClients, convertedRetry, counterValue);
    }

    private static BamlRetryPolicy ConvertRetryPolicy(DecodedClass value, int depth)
    {
        if (!string.Equals(value.Name, "baml.llm.RetryPolicy", StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 4
            || !value.Fields.TryGetValue("max_retries", out var maxRetries)
            || maxRetries is not long maxRetriesValue
            || !value.Fields.TryGetValue("initial_delay_ms", out var initialDelay)
            || initialDelay is not null and not long
            || !value.Fields.TryGetValue("multiplier", out var multiplier)
            || multiplier is not null and not double
            || !value.Fields.TryGetValue("max_delay_ms", out var maxDelay)
            || maxDelay is not null and not long)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed retry policy class {value.Name}.");
        }

        return new BamlRetryPolicy(
            maxRetriesValue,
            (long?)initialDelay,
            (double?)multiplier,
            (long?)maxDelay);
    }

    private static BamlClientType ConvertClientType(DecodedEnum value)
    {
        if (!string.Equals(value.Name, "baml.llm.ClientType", StringComparison.Ordinal))
        {
            throw new BamlBridgeException(
                $"The native runtime returned BAML enum {value.Name}, but generated C# code expected baml.llm.ClientType.");
        }

        return value.Variant switch
        {
            "Primitive" => BamlClientType.Primitive,
            "Fallback" => BamlClientType.Fallback,
            "RoundRobin" => BamlClientType.RoundRobin,
            _ => throw new BamlBridgeException(
                $"The native runtime returned unknown variant {value.Variant} for BAML enum {value.Name}."),
        };
    }

    private static BamlMedia ConvertMediaClass(DecodedClass value, Type targetType)
    {
        var expectedName = targetType == typeof(BamlImage) ? "baml.media.Image"
            : targetType == typeof(BamlAudio) ? "baml.media.Audio"
            : targetType == typeof(BamlVideo) ? "baml.media.Video"
            : targetType == typeof(BamlPdf) ? "baml.media.Pdf"
            : string.Empty;
        if (!string.Equals(value.Name, expectedName, StringComparison.Ordinal)
            || value.TypeArguments.Count != 0
            || value.Fields.Count != 1
            || !value.Fields.TryGetValue("_data", out var data)
            || data is not DecodedHandle handle)
        {
            throw new BamlBridgeException(
                $"The native runtime returned malformed media class {value.Name} for expected C# type {targetType.FullName}.");
        }

        return TakeMediaHandle(handle, targetType);
    }

    private static void ValidateStreamHandle(DecodedHandle handle, Type targetType)
    {
        if ((BamlHandleType)handle.HandleType != BamlHandleType.AdtTaggedHeapHandle
            || handle.Type?.TyCase != BamlTy.TyOneofCase.ClassTy
            || !string.Equals(handle.Type.ClassTy.Name, "baml.llm.Stream", StringComparison.Ordinal))
        {
            throw new BamlBridgeException(
                $"The native runtime returned a non-stream handle for expected CLR type {targetType.FullName}.");
        }

        var targetArguments = targetType.GetGenericArguments();
        if (handle.Type.ClassTy.TypeArgs.Count != targetArguments.Length
            || !handle.Type.ClassTy.TypeArgs
                .Zip(targetArguments, StreamTypeArgumentMatches)
                .All(static matches => matches))
        {
            throw new BamlBridgeException(
                $"The native runtime returned stream type metadata that does not match {targetType.FullName}.");
        }
    }

    private static bool StreamTypeArgumentMatches(BamlTy actual, Type targetType)
    {
        if (actual.Equals(ProtoTypeCodec.Encode(targetType)))
        {
            return true;
        }

        if (!targetType.IsValueType && actual.TyCase == BamlTy.TyOneofCase.Optional)
        {
            return actual.Optional.Inner.Equals(ProtoTypeCodec.Encode(targetType));
        }

        if (!targetType.IsValueType && actual.TyCase == BamlTy.TyOneofCase.Union)
        {
            var nonNull = actual.Union.Options.Where(static option => !IsNullType(option)).ToArray();
            return nonNull.Length == 1 && nonNull[0].Equals(ProtoTypeCodec.Encode(targetType));
        }

        return false;
    }

    private static string? BuiltinClassName(Type targetType) => targetType == typeof(BamlStreamFinished)
        ? "baml.stream.StreamFinished"
        : targetType == typeof(BamlPromptAst)
            ? "baml.llm.PromptAst"
            : targetType == typeof(BamlPromptMessage)
                ? "baml.llm.PromptMessage"
                : targetType == typeof(BamlHttpRequest)
                    ? "baml.http.Request"
                    : targetType == typeof(BamlHttpResponse)
                        ? "baml.http.Response"
                        : targetType == typeof(BamlFile)
                            ? "baml.fs.File"
                            : targetType == typeof(BamlSseStream)
                                ? "baml.http.SseStream"
                                : targetType == typeof(BamlCsvWriter)
                                    ? "baml.csv.CsvWriter"
                                    : targetType == typeof(BamlCsvReader)
                                        ? "baml.csv.CsvReader"
                                        : targetType == typeof(BamlCsvRecord)
                                            ? "baml.csv.CsvRecord"
                                                : targetType == typeof(BamlCsvPosition)
                                                    ? "baml.csv.CsvPosition"
                                                : targetType == typeof(BamlIteratorDone)
                                                    ? "baml.iter.Done"
                                                    : targetType == typeof(BamlCsvWriterOptions)
                                                        ? "baml.csv.WriterOptions"
                                                        : targetType == typeof(BamlCsvReaderOptions)
                                                            ? "baml.csv.ReaderOptions"
                                                            : targetType == typeof(BamlClient)
                                                                ? "baml.llm.Client"
                                                                : targetType == typeof(BamlRetryPolicy)
                                                                    ? "baml.llm.RetryPolicy"
                                                                    : null;

    private static Type? MediaTargetType(string wireName) => wireName switch
    {
        "baml.media.Image" => typeof(BamlImage),
        "baml.media.Audio" => typeof(BamlAudio),
        "baml.media.Video" => typeof(BamlVideo),
        "baml.media.Pdf" => typeof(BamlPdf),
        _ => null,
    };

    private static Type? MediaTargetType(int handleType) => (BamlHandleType)handleType switch
    {
        BamlHandleType.AdtMediaImage => typeof(BamlImage),
        BamlHandleType.AdtMediaAudio => typeof(BamlAudio),
        BamlHandleType.AdtMediaVideo => typeof(BamlVideo),
        BamlHandleType.AdtMediaPdf => typeof(BamlPdf),
        _ => null,
    };

    private static void DisposeDecodedHandles(object? value)
    {
        switch (value)
        {
            case DecodedHandle handle:
                handle.Dispose();
                break;
            case DecodedClass classValue:
                foreach (var field in classValue.Fields.Values)
                {
                    DisposeDecodedHandles(field);
                }

                break;
            case DecodedMap map:
                foreach (var item in map.Values.Values)
                {
                    DisposeDecodedHandles(item);
                }

                break;
            case DecodedUnion union:
                DisposeDecodedHandles(union.Value);
                break;
            case IReadOnlyList<object?> list:
                foreach (var item in list)
                {
                    DisposeDecodedHandles(item);
                }

                break;
        }
    }

    private sealed record DecodedClass(
        string Name,
        IReadOnlyList<BamlTy> TypeArguments,
        IReadOnlyDictionary<string, object?> Fields);

    private sealed record DecodedEnum(string Name, string Variant);

    private sealed record DecodedUnion(BamlTy? SelfType, string SelectedOption, object? Value);

    private sealed class DecodedHandle(NativeHandle handle, BamlTy? type) : IDisposable
    {
        private NativeHandle? _handle = handle;

        internal int HandleType => _handle?.HandleType
            ?? throw new ObjectDisposedException(nameof(DecodedHandle));

        internal BamlTy? Type { get; } = type;

        internal NativeHandle Take() => Interlocked.Exchange(ref _handle, null)
            ?? throw new ObjectDisposedException(nameof(DecodedHandle));

        public void Dispose() => Interlocked.Exchange(ref _handle, null)?.Dispose();
    }

    internal sealed class EncodeContext : IDisposable
    {
        private readonly List<ulong> _handles = new();
        private readonly List<ulong> _hostValues = new();
        private bool _hostValuesTransferred;

        internal void TrackHandle(ulong key) => _handles.Add(key);

        internal void TrackHostValue(ulong key) => _hostValues.Add(key);

        internal void TransferHostValues() => _hostValuesTransferred = true;

        public void Dispose()
        {
            foreach (var key in _handles)
            {
                _ = NativeApi.ReleaseHandle(key);
            }

            _handles.Clear();
            if (!_hostValuesTransferred)
            {
                foreach (var key in _hostValues)
                {
                    HostValueRegistry.RollBack(key);
                }
            }

            _hostValues.Clear();
        }
    }

    private sealed record DecodedMap(BamlTy? KeyType, IReadOnlyDictionary<string, object?> Values);

    private static string FormatBigInteger(BigInteger value)
    {
        var magnitude = BigInteger.Abs(value).ToString("x", CultureInfo.InvariantCulture).TrimStart('0');
        if (magnitude.Length == 0)
        {
            magnitude = "0";
        }

        return value.Sign < 0 ? $"-{magnitude}" : magnitude;
    }

    private static BigInteger ParseBigInteger(string value)
    {
        var negative = value.StartsWith("-", StringComparison.Ordinal);
        var digits = negative ? value[1..] : value;
        if (digits.Length == 0 || digits.Length > MaxBigIntegerHexLength || !digits.All(Uri.IsHexDigit))
        {
            throw new BamlBridgeException("The native runtime returned an invalid bigint hex value.");
        }

        if (!BigInteger.TryParse(
                $"0{digits}",
                NumberStyles.AllowHexSpecifier,
                CultureInfo.InvariantCulture,
                out var parsed))
        {
            throw new BamlBridgeException("The native runtime returned an invalid bigint hex value.");
        }

        return negative ? -parsed : parsed;
    }

    private static double ParseFloatLiteral(string value)
    {
        if (double.TryParse(value, NumberStyles.Float, CultureInfo.InvariantCulture, out var parsed))
        {
            return parsed;
        }

        throw new BamlBridgeException("The native runtime returned an invalid float literal value.");
    }
}
