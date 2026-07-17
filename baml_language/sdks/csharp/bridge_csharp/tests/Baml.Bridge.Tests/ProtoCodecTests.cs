using System.CodeDom.Compiler;
using System.Numerics;
using System.Runtime.Serialization;
using Baml;
using Baml.Bridge;
using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Bridge.Tests;

public sealed class ProtoCodecTests
{
    [Fact]
    public void InboundDelegateUsesHostCallableHandle()
    {
        using var context = new ProtoCodec.EncodeContext();
        Func<long, string> callback = value => value.ToString();

        var encoded = ProtoCodec.Encode(callback, context);

        Assert.Equal(InboundValue.ValueOneofCase.Handle, encoded.ValueCase);
        Assert.Equal(BamlHandleType.HostValueCallable, encoded.Handle.HandleType);
        Assert.NotEqual(0UL, encoded.Handle.Key);
    }

    [Fact]
    public void HostArgumentDecoderUsesTheDelegateParameterType()
    {
        var outbound = new BamlOutboundValue { IntValue = 42 };

        Assert.Equal(42L, ProtoCodec.DecodeOutbound(outbound, typeof(long)));
    }

    [Fact]
    public void StreamNextDecoderPreservesPartialNullAndTerminalCases()
    {
        var text = StreamNextResult(new BamlOutboundValue { StringValue = "partial" });
        var nullPartial = StreamNextResult(new BamlOutboundValue { NullValue = new BamlValueNull() });
        var finished = StreamNextResult(new BamlOutboundValue
        {
            ClassValue = new BamlValueClass { Name = "baml.stream.StreamFinished" },
        });

        Assert.Equal("partial", ProtoCodec.DecodeStreamNext<string?>(text.ToByteArray()).AsT0);
        Assert.Null(ProtoCodec.DecodeStreamNext<string?>(nullPartial.ToByteArray()).AsT0);
        Assert.Same(
            BamlStreamFinished.Instance,
            ProtoCodec.DecodeStreamNext<string?>(finished.ToByteArray()).AsT1);
    }

    private static BamlOutboundResult StreamNextResult(BamlOutboundValue value) => new()
    {
        Ok = new BamlOutboundValue
        {
            UnionVariantValue = new BamlValueUnionVariant
            {
                Value = value,
                ValueOptionName = value.ValueCase.ToString(),
            },
        },
    };

    [Fact]
    public void InboundPrimitiveCasesUseCanonicalFields()
    {
        Assert.Equal(InboundValue.ValueOneofCase.None, ProtoCodec.Encode(null).ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.BoolValue, ProtoCodec.Encode(true).ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.IntValue, ProtoCodec.Encode(42L).ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.FloatValue, ProtoCodec.Encode(3.5).ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.StringValue, ProtoCodec.Encode("text").ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.Uint8ArrayValue, ProtoCodec.Encode(new byte[] { 1, 2 }).ValueCase);
    }

    [Fact]
    public void GenericTypeDescriptorsPreserveClrShapeAndWireIdentity()
    {
        var integer = ProtoTypeCodec.Encode(typeof(long));
        var optional = ProtoTypeCodec.Encode(typeof(BamlNullable<string>));
        var list = ProtoTypeCodec.Encode(typeof(List<long?>));
        var map = ProtoTypeCodec.Encode(typeof(Dictionary<string, TestGeneratedLabel>));
        var union = ProtoTypeCodec.Encode(typeof(BamlUnion<long, TestGeneratedModel>));
        var media = ProtoTypeCodec.Encode(typeof(BamlImage));
        var handle = ProtoTypeCodec.Encode(typeof(BamlHandle));
        var stream = ProtoTypeCodec.Encode(typeof(BamlStream<string?, string>));
        var streamFinished = ProtoTypeCodec.Encode(typeof(BamlStreamFinished));
        var promptAst = ProtoTypeCodec.Encode(typeof(BamlPromptAst));
        var promptMessage = ProtoTypeCodec.Encode(typeof(BamlPromptMessage));
        var httpRequest = ProtoTypeCodec.Encode(typeof(BamlHttpRequest));
        var httpResponse = ProtoTypeCodec.Encode(typeof(BamlHttpResponse));
        var file = ProtoTypeCodec.Encode(typeof(BamlFile));
        var sseStream = ProtoTypeCodec.Encode(typeof(BamlSseStream));
        var glob = ProtoTypeCodec.Encode(typeof(BamlGlob));
        var globScanOptions = ProtoTypeCodec.Encode(typeof(BamlGlobScanOptions));
        var cancelToken = ProtoTypeCodec.Encode(typeof(BamlCancelToken));
        var taskGroup = ProtoTypeCodec.Encode(typeof(BamlTaskGroup));
        var recursiveAlias = ProtoTypeCodec.Encode(typeof(TestRecursiveAlias));
        var client = ProtoTypeCodec.Encode(typeof(BamlClient));
        var retryPolicy = ProtoTypeCodec.Encode(typeof(BamlRetryPolicy));
        var clientType = ProtoTypeCodec.Encode(typeof(BamlClientType));

        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveInt, integer.Primitive.Kind);
        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveString, optional.Optional.Inner.Primitive.Kind);
        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveInt, list.List.Item.Optional.Inner.Primitive.Kind);
        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveString, map.Map.Key.Primitive.Kind);
        Assert.Equal("user.tests.Label", map.Map.Value.Enum.Name);
        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveInt, union.Union.Options[0].Primitive.Kind);
        Assert.Equal("user.tests.Model", union.Union.Options[1].ClassTy.Name);
        Assert.Equal(BamlTyMediaKind.Image, media.Media.Kind);
        Assert.Equal(BamlTy.TyOneofCase.RustType, handle.TyCase);
        Assert.Equal("baml.llm.Stream", stream.ClassTy.Name);
        Assert.Equal(BamlTyPrimitiveKind.BamlTyPrimitiveString, stream.ClassTy.TypeArgs[0].Primitive.Kind);
        Assert.Equal("baml.stream.StreamFinished", streamFinished.ClassTy.Name);
        Assert.Equal("baml.llm.PromptAst", promptAst.ClassTy.Name);
        Assert.Equal("baml.llm.PromptMessage", promptMessage.ClassTy.Name);
        Assert.Equal("baml.http.Request", httpRequest.ClassTy.Name);
        Assert.Equal("baml.http.Response", httpResponse.ClassTy.Name);
        Assert.Equal("baml.fs.File", file.ClassTy.Name);
        Assert.Equal("baml.http.SseStream", sseStream.ClassTy.Name);
        Assert.Equal("baml.glob.Glob", glob.ClassTy.Name);
        Assert.Equal("baml.glob.ScanOptions", globScanOptions.ClassTy.Name);
        Assert.Equal("baml.spawn.CancelToken", cancelToken.ClassTy.Name);
        Assert.Equal("baml.spawn.TaskGroup", taskGroup.ClassTy.Name);
        Assert.Equal("user.tests.RecursiveAlias", recursiveAlias.TypeAlias.Name);
        Assert.Equal("baml.llm.Client", client.ClassTy.Name);
        Assert.Equal("baml.llm.RetryPolicy", retryPolicy.ClassTy.Name);
        Assert.Equal("baml.llm.ClientType", clientType.Enum.Name);
    }

    [Fact]
    public void OutboundPromptMessageRequiresTheCanonicalStdlibShape()
    {
        var message = new BamlValueClass
        {
            Name = "baml.llm.PromptMessage",
            Fields =
            {
                new BamlOutboundMapEntry
                {
                    Key = "role",
                    Value = new BamlOutboundValue { StringValue = "user" },
                },
                new BamlOutboundMapEntry
                {
                    Key = "content",
                    Value = new BamlOutboundValue { StringValue = "hello" },
                },
            },
        };
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = message },
        }.ToByteArray();

        Assert.Equal(
            new BamlPromptMessage("user", "hello"),
            ProtoCodec.DecodeResult<BamlPromptMessage>(payload));

        message.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "unexpected",
            Value = new BamlOutboundValue { StringValue = "value" },
        });
        payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = message },
        }.ToByteArray();
        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlPromptMessage>(payload));
    }

    [Fact]
    public void HttpRequestUsesTheCanonicalStdlibShapeInBothDirections()
    {
        var sourceHeaders = new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["authorization"] = "Bearer test",
        };
        var request = new BamlHttpRequest("POST", "https://example.com/v1", sourceHeaders, "body");
        sourceHeaders["authorization"] = "mutated";

        var encoded = ProtoCodec.Encode(request);
        Assert.Equal(InboundValue.ValueOneofCase.ClassValue, encoded.ValueCase);
        Assert.Equal("baml.http.Request", encoded.ClassValue.ClassTy.Name);
        Assert.Empty(encoded.ClassValue.ClassTy.TypeArgs);
        var inboundFields = encoded.ClassValue.Fields.ToDictionary(
            static field => field.StringKey,
            StringComparer.Ordinal);
        Assert.Equal(4, inboundFields.Count);
        Assert.Equal("POST", inboundFields["method"].Value.StringValue);
        Assert.Equal("https://example.com/v1", inboundFields["url"].Value.StringValue);
        Assert.Equal("body", inboundFields["body"].Value.StringValue);
        Assert.Equal(
            "Bearer test",
            Assert.Single(inboundFields["headers"].Value.MapValue.Entries).Value.StringValue);

        var outboundHeaders = new BamlValueMap
        {
            KeyType = new BamlTy
            {
                Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveString },
            },
            Entries =
            {
                new BamlOutboundMapEntry
                {
                    Key = "content-type",
                    Value = new BamlOutboundValue { StringValue = "application/json" },
                },
            },
        };
        var outboundClass = new BamlValueClass
        {
            Name = "baml.http.Request",
            Fields =
            {
                new BamlOutboundMapEntry
                {
                    Key = "method",
                    Value = new BamlOutboundValue { StringValue = "POST" },
                },
                new BamlOutboundMapEntry
                {
                    Key = "url",
                    Value = new BamlOutboundValue { StringValue = "https://example.com/v1" },
                },
                new BamlOutboundMapEntry
                {
                    Key = "headers",
                    Value = new BamlOutboundValue { MapValue = outboundHeaders },
                },
                new BamlOutboundMapEntry
                {
                    Key = "body",
                    Value = new BamlOutboundValue { StringValue = "{\"ok\":true}" },
                },
            },
        };
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = outboundClass },
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<BamlHttpRequest>(payload);
        Assert.Equal("POST", decoded.Method);
        Assert.Equal("https://example.com/v1", decoded.Url);
        Assert.Equal("application/json", decoded.Headers["content-type"]);
        Assert.Equal("{\"ok\":true}", decoded.Body);

        outboundClass.Fields.Add(new BamlOutboundMapEntry
        {
            Key = "unexpected",
            Value = new BamlOutboundValue { StringValue = "value" },
        });
        payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = outboundClass },
        }.ToByteArray();
        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlHttpRequest>(payload));
    }

    [Fact]
    public void InboundLlmClientUsesTheCanonicalRecursiveShape()
    {
        var client = new BamlClient(
            "fallback",
            BamlClientType.Fallback,
            [BamlClient.FromShorthand("openai/gpt-4o-mini")],
            new BamlRetryPolicy(3, 100, 2.0, 1_000),
            counter: 4);

        var encoded = ProtoCodec.Encode(client);

        Assert.Equal("baml.llm.Client", encoded.ClassValue.ClassTy.Name);
        var fields = encoded.ClassValue.Fields.ToDictionary(
            static field => field.StringKey,
            StringComparer.Ordinal);
        Assert.Equal(5, fields.Count);
        Assert.Equal("fallback", fields["name"].Value.StringValue);
        Assert.Equal("baml.llm.ClientType", fields["client_type"].Value.EnumValue.Name);
        Assert.Equal("Fallback", fields["client_type"].Value.EnumValue.Value);
        Assert.Equal(
            "baml.llm.Client",
            Assert.Single(fields["sub_clients"].Value.ListValue.Values).ClassValue.ClassTy.Name);
        Assert.Equal("baml.llm.RetryPolicy", fields["retry"].Value.ClassValue.ClassTy.Name);
        Assert.Equal(4, fields["counter"].Value.IntValue);
    }

    [Fact]
    public void OutboundLlmClientReconstructsTheCanonicalManagedModel()
    {
        var retry = new BamlValueClass
        {
            Name = "baml.llm.RetryPolicy",
            Fields =
            {
                new BamlOutboundMapEntry
                {
                    Key = "max_retries",
                    Value = new BamlOutboundValue { IntValue = 3 },
                },
                new BamlOutboundMapEntry
                {
                    Key = "initial_delay_ms",
                    Value = new BamlOutboundValue { IntValue = 100 },
                },
                new BamlOutboundMapEntry
                {
                    Key = "multiplier",
                    Value = new BamlOutboundValue { FloatValue = 2.0 },
                },
                new BamlOutboundMapEntry
                {
                    Key = "max_delay_ms",
                    Value = new BamlOutboundValue { NullValue = new BamlValueNull() },
                },
            },
        };
        var client = new BamlValueClass
        {
            Name = "baml.llm.Client",
            Fields =
            {
                new BamlOutboundMapEntry
                {
                    Key = "name",
                    Value = new BamlOutboundValue { StringValue = "fallback" },
                },
                new BamlOutboundMapEntry
                {
                    Key = "client_type",
                    Value = new BamlOutboundValue
                    {
                        EnumValue = new BamlValueEnum
                        {
                            Name = "baml.llm.ClientType",
                            Value = "Fallback",
                        },
                    },
                },
                new BamlOutboundMapEntry
                {
                    Key = "sub_clients",
                    Value = new BamlOutboundValue { ListValue = new BamlValueList() },
                },
                new BamlOutboundMapEntry
                {
                    Key = "retry",
                    Value = new BamlOutboundValue { ClassValue = retry },
                },
                new BamlOutboundMapEntry
                {
                    Key = "counter",
                    Value = new BamlOutboundValue { IntValue = 4 },
                },
            },
        };
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = client },
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<BamlClient>(payload);

        Assert.Equal("fallback", decoded.Name);
        Assert.Equal(BamlClientType.Fallback, decoded.ClientType);
        Assert.Empty(decoded.SubClients);
        Assert.Equal(3, decoded.Retry?.MaxRetries);
        Assert.Equal(100, decoded.Retry?.InitialDelayMilliseconds);
        Assert.Equal(2.0, decoded.Retry?.Multiplier);
        Assert.Null(decoded.Retry?.MaxDelayMilliseconds);
        Assert.Equal(4, decoded.Counter);
    }

    [Fact]
    public void OutboundSseStreamRejectsAClassWithoutItsNativeHandle()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass
                {
                    Name = "baml.http.SseStream",
                    Fields =
                    {
                        new BamlOutboundMapEntry
                        {
                            Key = "url",
                            Value = new BamlOutboundValue { StringValue = "https://example.com/events" },
                        },
                    },
                },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlSseStream>(payload));
    }

    [Fact]
    public void GlobScanOptionsUseTheCanonicalShapeInBothDirections()
    {
        var encoded = ProtoCodec.Encode(new BamlGlobScanOptions(
            cwd: "/tmp/work",
            dot: true,
            absolute: false,
            followSymlinks: null,
            throwErrorOnBrokenSymlink: true,
            onlyFiles: true));
        var inboundFields = encoded.ClassValue.Fields.ToDictionary(
            static field => field.StringKey,
            StringComparer.Ordinal);

        Assert.Equal("baml.glob.ScanOptions", encoded.ClassValue.ClassTy.Name);
        Assert.Equal(6, inboundFields.Count);
        Assert.Equal("/tmp/work", inboundFields["cwd"].Value.StringValue);
        Assert.True(inboundFields["dot"].Value.BoolValue);
        Assert.False(inboundFields["absolute"].Value.BoolValue);
        Assert.Equal(InboundValue.ValueOneofCase.None, inboundFields["follow_symlinks"].Value.ValueCase);
        Assert.True(inboundFields["throw_error_on_broken_symlink"].Value.BoolValue);
        Assert.True(inboundFields["only_files"].Value.BoolValue);

        var outboundClass = new BamlValueClass
        {
            Name = "baml.glob.ScanOptions",
            Fields =
            {
                new BamlOutboundMapEntry { Key = "cwd", Value = new BamlOutboundValue { StringValue = "/tmp/work" } },
                new BamlOutboundMapEntry { Key = "dot", Value = new BamlOutboundValue { BoolValue = true } },
                new BamlOutboundMapEntry { Key = "absolute", Value = new BamlOutboundValue() },
                new BamlOutboundMapEntry { Key = "follow_symlinks", Value = new BamlOutboundValue { BoolValue = false } },
                new BamlOutboundMapEntry { Key = "throw_error_on_broken_symlink", Value = new BamlOutboundValue() },
                new BamlOutboundMapEntry { Key = "only_files", Value = new BamlOutboundValue { BoolValue = true } },
            },
        };
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = outboundClass },
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<BamlGlobScanOptions>(payload);
        Assert.Equal("/tmp/work", decoded.Cwd);
        Assert.True(decoded.Dot);
        Assert.Null(decoded.Absolute);
        Assert.False(decoded.FollowSymlinks);
        Assert.Null(decoded.ThrowErrorOnBrokenSymlink);
        Assert.True(decoded.OnlyFiles);
    }

    [Fact]
    public void OutboundGlobRejectsAClassWithoutItsNativeHandle()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass { Name = "baml.glob.Glob" },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlGlob>(payload));
    }

    [Fact]
    public void OutboundCancelTokenRejectsAClassWithoutItsNativeHandle()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass { Name = "baml.spawn.CancelToken" },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlCancelToken>(payload));
    }

    [Fact]
    public void OutboundTaskGroupRejectsAClassWithoutItsNativeHandle()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass { Name = "baml.spawn.TaskGroup" },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BamlTaskGroup>(payload));
    }

    [Fact]
    public void GenericTypeDescriptorsRejectUnsupportedAndOpenGenericClrTypes()
    {
        Assert.Throws<BamlBridgeException>(() => ProtoTypeCodec.Encode(typeof(HashSet<long>)));
        Assert.Throws<BamlBridgeException>(() => ProtoTypeCodec.Encode(typeof(RecursiveType<>)));
    }

    [Theory]
    [InlineData(-(1L << 62))]
    [InlineData(-1L)]
    [InlineData(0L)]
    [InlineData((1L << 62) - 1)]
    public void InboundIntegerAcceptsTheBamlI63Range(long value)
    {
        Assert.Equal(value, ProtoCodec.Encode(value).IntValue);
    }

    [Theory]
    [InlineData(long.MinValue)]
    [InlineData(-(1L << 62) - 1)]
    [InlineData(1L << 62)]
    [InlineData(long.MaxValue)]
    public void InboundIntegerRejectsValuesOutsideTheBamlI63Range(long value)
    {
        var error = Assert.Throws<BamlBridgeException>(() => ProtoCodec.Encode(value));

        Assert.Contains("outside the BAML int range", error.Message, StringComparison.Ordinal);
        Assert.Contains("BigInteger", error.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void BigIntegerUsesSignedHexWithoutPrefix()
    {
        Assert.Equal("0", ProtoCodec.Encode(BigInteger.Zero).BigintValue);
        Assert.Equal("80", ProtoCodec.Encode(new BigInteger(128)).BigintValue);
        Assert.Equal("ff", ProtoCodec.Encode(new BigInteger(255)).BigintValue);
        Assert.Equal("-ff", ProtoCodec.Encode(new BigInteger(-255)).BigintValue);
    }

    [Fact]
    public void InboundCollectionsUseRecursiveCanonicalFields()
    {
        var emptyList = ProtoCodec.Encode(new List<long>());
        var list = ProtoCodec.Encode(new List<long?> { 1, null, 3 });
        var emptyMap = ProtoCodec.Encode(new Dictionary<string, long>());
        var map = ProtoCodec.Encode(new Dictionary<string, long?> { ["one"] = 1, ["none"] = null });

        Assert.Equal(InboundValue.ValueOneofCase.ListValue, emptyList.ValueCase);
        Assert.Empty(emptyList.ListValue.Values);
        Assert.Equal(InboundValue.ValueOneofCase.IntValue, list.ListValue.Values[0].ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.None, list.ListValue.Values[1].ValueCase);
        Assert.Equal(InboundValue.ValueOneofCase.MapValue, emptyMap.ValueCase);
        Assert.Empty(emptyMap.MapValue.Entries);
        Assert.Equal("one", map.MapValue.Entries[0].StringKey);
        Assert.Equal(1, map.MapValue.Entries[0].Value.IntValue);
        Assert.Equal(InboundValue.ValueOneofCase.None, map.MapValue.Entries[1].Value.ValueCase);
    }

    [Fact]
    public void InboundMapKeysUseCanonicalOneofCases()
    {
        var intMap = ProtoCodec.Encode(new Dictionary<long, string> { [42] = "int" });
        var boolMap = ProtoCodec.Encode(new Dictionary<bool, string> { [true] = "bool" });
        var enumMap = ProtoCodec.Encode(new Dictionary<TestGeneratedLabel, string>
        {
            [TestGeneratedLabel.Good] = "enum",
        });

        Assert.Equal(InboundMapEntry.KeyOneofCase.IntKey, intMap.MapValue.Entries[0].KeyCase);
        Assert.Equal(42, intMap.MapValue.Entries[0].IntKey);
        Assert.Equal(InboundMapEntry.KeyOneofCase.BoolKey, boolMap.MapValue.Entries[0].KeyCase);
        Assert.True(boolMap.MapValue.Entries[0].BoolKey);
        Assert.Equal(InboundMapEntry.KeyOneofCase.EnumKey, enumMap.MapValue.Entries[0].KeyCase);
        Assert.Equal("user.tests.Label", enumMap.MapValue.Entries[0].EnumKey.Name);
        Assert.Equal("good-wire", enumMap.MapValue.Entries[0].EnumKey.Value);
    }

    [Fact]
    public void InboundCollectionCyclesAndUnsupportedKeysAreRejected()
    {
        var cycle = new List<object?>();
        cycle.Add(cycle);

        var cycleError = Assert.Throws<BamlBridgeException>(() => ProtoCodec.Encode(cycle));
        var keyError = Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.Encode(new Dictionary<BigInteger, string> { [BigInteger.One] = "one" }));

        Assert.Contains("Cyclic", cycleError.Message, StringComparison.Ordinal);
        Assert.Contains("must be string, bool, int", keyError.Message, StringComparison.Ordinal);
    }

    [Fact]
    public void InboundGeneratedClassAndEnumPreserveWireIdentity()
    {
        var encodedClass = ProtoCodec.Encode(new TestGeneratedModel { Value = 42, Label = TestGeneratedLabel.Good });
        var encodedEmptyClass = ProtoCodec.Encode(new TestGeneratedEmptyModel());
        var encodedGenericClass = ProtoCodec.Encode(new TestGeneratedGenericModel<long>
        {
            Value = 42,
        });
        var encodedEnum = ProtoCodec.Encode(TestGeneratedLabel.Bad);

        Assert.Equal("user.tests.Model", encodedClass.ClassValue.ClassTy.Name);
        Assert.Equal("value", encodedClass.ClassValue.Fields[0].StringKey);
        Assert.Equal(42, encodedClass.ClassValue.Fields[0].Value.IntValue);
        Assert.Equal("user.tests.Label", encodedClass.ClassValue.Fields[1].Value.EnumValue.Name);
        Assert.Equal("good-wire", encodedClass.ClassValue.Fields[1].Value.EnumValue.Value);
        Assert.Equal("user.tests.Label", encodedEnum.EnumValue.Name);
        Assert.Equal("bad-wire", encodedEnum.EnumValue.Value);
        Assert.Equal("user.tests.EmptyModel", encodedEmptyClass.ClassValue.ClassTy.Name);
        Assert.Empty(encodedEmptyClass.ClassValue.Fields);
        Assert.Equal("user.tests.GenericModel", encodedGenericClass.ClassValue.ClassTy.Name);
        Assert.Equal(
            BamlTyPrimitiveKind.BamlTyPrimitiveInt,
            encodedGenericClass.ClassValue.ClassTy.TypeArgs[0].Primitive.Kind);
        Assert.Throws<BamlBridgeException>(() => ProtoCodec.Encode((TestGeneratedLabel)0));
    }

    [Fact]
    public void OutboundOkDecodesPrimitiveAndLiteralValues()
    {
        var stringPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { StringValue = "hello" },
        }.ToByteArray();
        var literalPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                LiteralValue = new BamlLiteralValue { IntValue = 42 },
            },
        }.ToByteArray();

        Assert.Equal("hello", ProtoCodec.DecodeResult<string>(stringPayload));
        Assert.Equal(42L, ProtoCodec.DecodeResult<long>(literalPayload));
    }

    [Fact]
    public void OutboundNullAcceptsUnsetAndExplicitWireRepresentations()
    {
        var unsetPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue(),
        }.ToByteArray();
        var explicitPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { NullValue = new BamlValueNull() },
        }.ToByteArray();

        Assert.Null(ProtoCodec.DecodeResult<object?>(unsetPayload));
        Assert.Null(ProtoCodec.DecodeResult<object?>(explicitPayload));
    }

    [Fact]
    public void OutboundCallbackArgumentsCanPreserveSuppliedBamlOptionalValues()
    {
        var supplied = Assert.IsType<BamlOptional<long>>(ProtoCodec.DecodeOutbound(
            new BamlOutboundValue { IntValue = 42 },
            typeof(BamlOptional<long>)));
        var suppliedNull = Assert.IsType<BamlOptional<string?>>(ProtoCodec.DecodeOutbound(
            new BamlOutboundValue(),
            typeof(BamlOptional<string?>)));

        Assert.True(supplied.IsSet);
        Assert.Equal(42, supplied.Value);
        Assert.True(suppliedNull.IsSet);
        Assert.Null(suppliedNull.Value);
    }

    [Fact]
    public void OutboundCollectionsReconstructGeneratedGenericTypes()
    {
        var listPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ListValue = new BamlValueList
                {
                    Items =
                    {
                        new BamlOutboundValue { IntValue = 1 },
                        new BamlOutboundValue(),
                        new BamlOutboundValue { IntValue = 3 },
                    },
                },
            },
        }.ToByteArray();
        var mapPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                MapValue = new BamlValueMap
                {
                    Entries =
                    {
                        new BamlOutboundMapEntry
                        {
                            Key = "values",
                            Value = new BamlOutboundValue
                            {
                                ListValue = new BamlValueList
                                {
                                    Items = { new BamlOutboundValue { IntValue = 42 } },
                                },
                            },
                        },
                    },
                },
            },
        }.ToByteArray();

        Assert.Equal(new long?[] { 1, null, 3 }, ProtoCodec.DecodeResult<List<long?>>(listPayload));
        var map = ProtoCodec.DecodeResult<Dictionary<string, List<long>>>(mapPayload);
        Assert.Equal(new long[] { 42 }, map["values"]);
    }

    [Fact]
    public void RecursiveTypeAliasUsesItsDescriptorAndReconstructsErasedValues()
    {
        var inbound = new TestRecursiveAlias(
            new List<TestRecursiveAlias> { new(1L), new(2L) });
        var encoded = ProtoCodec.Encode(inbound);
        Assert.Equal(InboundValue.ValueOneofCase.ListValue, encoded.ValueCase);
        Assert.Equal(new long[] { 1, 2 }, encoded.ListValue.Values.Select(static value => value.IntValue));

        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ListValue = new BamlValueList
                {
                    Items =
                    {
                        new BamlOutboundValue { IntValue = 3 },
                        new BamlOutboundValue
                        {
                            ListValue = new BamlValueList
                            {
                                Items = { new BamlOutboundValue { IntValue = 4 } },
                            },
                        },
                    },
                },
            },
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<TestRecursiveAlias>(payload).Value.AsT1;
        Assert.Equal(3, decoded[0].Value.AsT0);
        Assert.Equal(4, Assert.Single(decoded[1].Value.AsT1).Value.AsT0);
    }

    [Fact]
    public void NullableRecursiveTypeAliasWrapsNullInItsNominalValue()
    {
        var encoded = ProtoCodec.Encode(new TestNullableRecursiveAlias(null));
        Assert.Equal(InboundValue.ValueOneofCase.None, encoded.ValueCase);

        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue(),
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<TestNullableRecursiveAlias>(payload);
        Assert.NotNull(decoded);
        Assert.Null(decoded.Value);
    }

    [Fact]
    public void OutboundMapKeysUseDeclaredWireTypes()
    {
        var intPayload = MapResult(
            new BamlTy
            {
                Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveInt },
            },
            ("42", "int"));
        var boolPayload = MapResult(
            new BamlTy
            {
                Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveBool },
            },
            ("true", "bool"));
        var enumPayload = MapResult(
            new BamlTy
            {
                Enum = new BamlTyEnum { Name = "user.tests.Label" },
            },
            ("user.tests.Label::good-wire", "enum"));

        Assert.Equal("int", ProtoCodec.DecodeResult<Dictionary<long, string>>(intPayload)[42]);
        Assert.Equal("bool", ProtoCodec.DecodeResult<Dictionary<bool, string>>(boolPayload)[true]);
        Assert.Equal(
            "enum",
            ProtoCodec.DecodeResult<Dictionary<TestGeneratedLabel, string>>(enumPayload)[TestGeneratedLabel.Good]);
    }

    [Fact]
    public void OutboundMapKeyMetadataAndCanonicalTextAreValidated()
    {
        var wrongType = MapResult(
            new BamlTy
            {
                Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveString },
            },
            ("42", "value"));
        var nonCanonical = MapResult(
            new BamlTy
            {
                Primitive = new BamlTyPrimitive { Kind = BamlTyPrimitiveKind.BamlTyPrimitiveInt },
            },
            ("042", "value"));

        Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.DecodeResult<Dictionary<long, string>>(wrongType));
        Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.DecodeResult<Dictionary<long, string>>(nonCanonical));
    }

    [Fact]
    public void OutboundGeneratedClassAndEnumUseGeneratedFactories()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass
                {
                    Name = "user.tests.Model",
                    Fields =
                    {
                        new BamlOutboundMapEntry
                        {
                            Key = "value",
                            Value = new BamlOutboundValue { IntValue = 42 },
                        },
                        new BamlOutboundMapEntry
                        {
                            Key = "label",
                            Value = new BamlOutboundValue
                            {
                                EnumValue = new BamlValueEnum
                                {
                                    Name = "user.tests.Label",
                                    Value = "good-wire",
                                },
                            },
                        },
                    },
                },
            },
        }.ToByteArray();

        var decoded = ProtoCodec.DecodeResult<TestGeneratedModel>(payload);
        Assert.Equal(42, decoded.Value);
        Assert.Equal(TestGeneratedLabel.Good, decoded.Label);
    }

    [Fact]
    public void OutboundEmptyClassDecodesAndUnexpectedTypeArgumentsAreRejected()
    {
        var emptyPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass { Name = "user.tests.EmptyModel" },
            },
        }.ToByteArray();
        var genericPayload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                ClassValue = new BamlValueClass
                {
                    Name = "user.tests.EmptyModel",
                    TypeArgs =
                    {
                        new BamlTy
                        {
                            Primitive = new BamlTyPrimitive
                            {
                                Kind = BamlTyPrimitiveKind.BamlTyPrimitiveString,
                            },
                        },
                    },
                },
            },
        }.ToByteArray();

        Assert.IsType<TestGeneratedEmptyModel>(ProtoCodec.DecodeResult<TestGeneratedEmptyModel>(emptyPayload));
        Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.DecodeResult<TestGeneratedEmptyModel>(genericPayload));
    }

    [Fact]
    public void OutboundGenericClassRequiresMatchingTypeArguments()
    {
        var classValue = new BamlValueClass
        {
            Name = "user.tests.GenericModel",
            Fields =
            {
                new BamlOutboundMapEntry
                {
                    Key = "value",
                    Value = new BamlOutboundValue { IntValue = 42 },
                },
            },
            TypeArgs =
            {
                new BamlTy
                {
                    Primitive = new BamlTyPrimitive
                    {
                        Kind = BamlTyPrimitiveKind.BamlTyPrimitiveInt,
                    },
                },
            },
        };
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { ClassValue = classValue },
        }.ToByteArray();

        Assert.Equal(42, ProtoCodec.DecodeResult<TestGeneratedGenericModel<long>>(payload).Value);
        Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.DecodeResult<TestGeneratedGenericModel<string>>(payload));
    }

    [Fact]
    public void DuplicateOutboundMapKeysAreRejected()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                MapValue = new BamlValueMap
                {
                    Entries =
                    {
                        new BamlOutboundMapEntry { Key = "same", Value = new BamlOutboundValue() },
                        new BamlOutboundMapEntry { Key = "same", Value = new BamlOutboundValue() },
                    },
                },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() =>
            ProtoCodec.DecodeResult<Dictionary<string, object?>>(payload));
    }

    [Fact]
    public void OutboundErrorPreservesValueAndTrace()
    {
        var payload = new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = new BamlOutboundValue { StringValue = "bad input" },
                Trace = { "main.baml:1" },
            },
        }.ToByteArray();

        var error = Assert.Throws<BamlError>(() => ProtoCodec.DecodeResult<string>(payload));
        Assert.Equal("bad input", error.Value);
        Assert.Equal("main.baml:1", Assert.Single(error.BamlTrace));
    }

    [Fact]
    public void OutboundErrorPreservesClassNameAndDynamicFields()
    {
        var payload = new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = new BamlOutboundValue
                {
                    ClassValue = new BamlValueClass
                    {
                        Name = "user.errors.ValidationError",
                        Fields =
                        {
                            new BamlOutboundMapEntry
                            {
                                Key = "code",
                                Value = new BamlOutboundValue { IntValue = 42 },
                            },
                        },
                    },
                },
                Trace = { "errors.baml:7" },
            },
        }.ToByteArray();

        var error = Assert.Throws<BamlError>(() => ProtoCodec.DecodeResult<object?>(payload));
        var fields = Assert.IsType<Dictionary<string, object?>>(error.Value);
        Assert.Equal("user.errors.ValidationError", error.ClassName);
        Assert.Equal(42L, fields["code"]);
    }

    [Fact]
    public void CallBoundaryTypeMismatchUsesArgumentExceptionTaxonomy()
    {
        var payload = new BamlOutboundResult
        {
            Error = new BamlOutboundError
            {
                Value = new BamlOutboundValue
                {
                    ClassValue = new BamlValueClass
                    {
                        Name = "baml.errors.TypeMismatch",
                        Fields =
                        {
                            new BamlOutboundMapEntry
                            {
                                Key = "message",
                                Value = new BamlOutboundValue { StringValue = "expected int, got string" },
                            },
                        },
                    },
                },
                Trace = { "types.baml:4" },
            },
        }.ToByteArray();

        var error = Assert.Throws<BamlTypeMismatchException>(() => ProtoCodec.DecodeResult<object?>(payload));
        var fields = Assert.IsType<Dictionary<string, object?>>(error.Value);
        Assert.IsAssignableFrom<ArgumentException>(error);
        Assert.Equal("baml.errors.TypeMismatch", error.ClassName);
        Assert.Equal("expected int, got string", error.Message);
        Assert.Equal("expected int, got string", fields["message"]);
        Assert.Equal("types.baml:4", Assert.Single(error.BamlTrace));
    }

    [Fact]
    public void EngineCancellationUsesOperationCanceledTaxonomyAndPreservesMetadata()
    {
        var payload = new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic
            {
                Value = new BamlOutboundValue
                {
                    ClassValue = new BamlValueClass
                    {
                        Name = "baml.panics.Cancelled",
                        Fields =
                        {
                            new BamlOutboundMapEntry
                            {
                                Key = "reason",
                                Value = new BamlOutboundValue { StringValue = "deadline" },
                            },
                        },
                    },
                },
                Trace = { "cancel.baml:3" },
            },
        }.ToByteArray();

        var error = Assert.Throws<BamlCancelledException>(() => ProtoCodec.DecodeResult<object?>(payload));
        var fields = Assert.IsType<Dictionary<string, object?>>(error.Value);
        Assert.IsAssignableFrom<OperationCanceledException>(error);
        Assert.Equal("baml.panics.Cancelled", error.ClassName);
        Assert.Equal("deadline", fields["reason"]);
        Assert.Equal("cancel.baml:3", Assert.Single(error.BamlTrace));
        Assert.False(error.CancellationToken.CanBeCanceled);
    }

    [Fact]
    public void NonCancellationPanicRemainsBamlPanic()
    {
        var payload = new BamlOutboundResult
        {
            Panic = new BamlOutboundPanic
            {
                Value = new BamlOutboundValue
                {
                    ClassValue = new BamlValueClass { Name = "baml.panics.Internal" },
                },
                Trace = { "panic.baml:9" },
            },
        }.ToByteArray();

        var error = Assert.Throws<BamlPanic>(() => ProtoCodec.DecodeResult<object?>(payload));
        Assert.Equal("baml.panics.Internal", error.ClassName);
        Assert.Equal("panic.baml:9", Assert.Single(error.BamlTrace));
    }

    [Fact]
    public void ContradictoryExpectedTypeIsRejected()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { IntValue = 42 },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<string>(payload));
    }

    [Theory]
    [InlineData("")]
    [InlineData("-")]
    [InlineData("0x10")]
    [InlineData("1_0")]
    [InlineData(" 10")]
    public void MalformedBigIntegerPayloadIsRejected(string value)
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { BigintValue = value },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<BigInteger>(payload));
    }

    [Fact]
    public void MalformedFloatLiteralPayloadIsRejected()
    {
        var payload = new BamlOutboundResult
        {
            Ok = new BamlOutboundValue
            {
                LiteralValue = new BamlLiteralValue { FloatValue = "not-a-float" },
            },
        }.ToByteArray();

        Assert.Throws<BamlBridgeException>(() => ProtoCodec.DecodeResult<double>(payload));
    }

    private static byte[] MapResult(BamlTy keyType, params (string Key, string Value)[] entries)
    {
        var map = new BamlValueMap { KeyType = keyType };
        map.Entries.Add(entries.Select(static entry => new BamlOutboundMapEntry
        {
            Key = entry.Key,
            Value = new BamlOutboundValue { StringValue = entry.Value },
        }));
        return new BamlOutboundResult
        {
            Ok = new BamlOutboundValue { MapValue = map },
        }.ToByteArray();
    }
}

[GeneratedCode("BAML", "test")]
[DataContract(Name = "user.tests.Model")]
internal sealed class TestGeneratedModel
{
    [DataMember(Name = "value", Order = 0, IsRequired = true)]
    public required long Value { get; init; }

    [DataMember(Name = "label", Order = 1, IsRequired = true)]
    public required TestGeneratedLabel Label { get; init; }
}

[GeneratedCode("BAML", "test")]
[DataContract(Name = "user.tests.EmptyModel")]
internal sealed class TestGeneratedEmptyModel
{
}

[GeneratedCode("BAML", "test")]
[DataContract(Name = "user.tests.GenericModel")]
internal sealed class TestGeneratedGenericModel<T>
{
    [DataMember(Name = "value", Order = 0, IsRequired = true)]
    public required T Value { get; init; }
}

[GeneratedCode("BAML", "test")]
[BamlTypeAlias("user.tests.RecursiveAlias")]
internal sealed class TestRecursiveAlias : IBamlTypeAliasValue
{
    public TestRecursiveAlias(BamlUnion<long, List<TestRecursiveAlias>> value)
    {
        Value = value;
    }

    public BamlUnion<long, List<TestRecursiveAlias>> Value { get; }

    public object? UntypedValue => Value;
}

[GeneratedCode("BAML", "test")]
[BamlTypeAlias("user.tests.NullableRecursiveAlias")]
internal sealed class TestNullableRecursiveAlias : IBamlTypeAliasValue
{
    public TestNullableRecursiveAlias(BamlUnion<long, List<TestNullableRecursiveAlias>>? value)
    {
        Value = value;
    }

    public BamlUnion<long, List<TestNullableRecursiveAlias>>? Value { get; }

    public object? UntypedValue => Value;
}

[GeneratedCode("BAML", "test")]
[DataContract(Name = "user.tests.Label")]
internal enum TestGeneratedLabel : long
{
    [EnumMember(Value = "good-wire")]
    Good = 11,

    [EnumMember(Value = "bad-wire")]
    Bad = 12,
}

internal sealed class RecursiveType<T>
{
}
