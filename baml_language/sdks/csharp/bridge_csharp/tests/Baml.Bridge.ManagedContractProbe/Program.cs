using System.Buffers;
using System.Collections;
using System.Collections.ObjectModel;
using System.Numerics;
using System.Reflection;
using System.Runtime.InteropServices;
using System.Text.Json;
using System.Text.Json.Nodes;
using Baml;
using Probe.Generated;

internal static class Program
{
    public static async Task<int> Main()
    {
        RegisterGeneratedTypes();
        VerifyOptionalNullableAndPartialState();
        await VerifyMediaAsync().ConfigureAwait(false);
        await VerifyRequestClientAndHandleAsync()
            .ConfigureAwait(false);
        VerifyDynamicValues();
        VerifyGenericBinder();
        VerifyLimitsAndCycles();
        VerifyPublicShapeInvariants();

        Console.WriteLine("optional_nullable=orthogonal_complete");
        Console.WriteLine("stream_state=pending_incomplete_complete");
        Console.WriteLine("media_values=4x_url_bytes_base64_file_owned");
        Console.WriteLine("http_request=immutable_duplicate_headers_fresh_messages");
        Console.WriteLine("client_retry=immutable_structural_checked");
        Console.WriteLine("handle=safe_clone_lease_dispose_identity");
        Console.WriteLine("baml_value=kinds_14_structural_typed");
        Console.WriteLine("descriptor_kinds=unknown_plus_14_value_shapes");
        Console.WriteLine("descriptors=alias_literal_nominal_generic_union");
        Console.WriteLine("dynamic_inspection=enum_class_union_public");
        Console.WriteLine("dynamic_null=explicit_nullable_only");
        Console.WriteLine("collections=owned_readonly_canonical_maps");
        Console.WriteLine("generic_binder=canonical_and_fail_closed");
        Console.WriteLine("limits=depth_collection_bytes_nodes_bigint_cycle");
        Console.WriteLine("partial_projection=semantic_states");
        Console.WriteLine("unsupported_clr=explicit");
        Console.WriteLine("public_contract=audited");
        return 0;
    }

    private static void RegisterGeneratedTypes()
    {
        BamlTypeDescriptor personDescriptor = new(
            BamlValueKind.Class,
            "probe.Person");
        BamlTypeDescriptor colorDescriptor = new(
            BamlValueKind.Enum,
            "probe.Color");
        BamlTypeDescriptor boxOfIntDescriptor = new(
            BamlValueKind.Class,
            "probe.Box",
            [new BamlTypeDescriptor(BamlValueKind.Int)]);
        BamlClrTypeBinder.Register(
            typeof(Person),
            personDescriptor);
        BamlClrTypeBinder.Register(
            typeof(Color),
            colorDescriptor);
        BamlClrTypeBinder.Register(
            typeof(Box<long>),
            boxOfIntDescriptor);
        BamlDynamicRegistry.Register<Person>(
            person => BamlValue.Class(
                "probe.Person",
                typeArguments: [],
                fields:
                [
                    new("name", BamlValue.String(person.Name)),
                    new("age", BamlValue.Int(person.Age)),
                ]),
            value =>
            {
                Require(
                    value.Type.Equals(personDescriptor),
                    "person descriptor mismatch");
                if (!value.TryGetClassFields(out var classFields))
                {
                    throw new InvalidOperationException(
                        "person payload was not publicly inspectable");
                }

                Dictionary<string, BamlValue> fields =
                    classFields.ToDictionary(
                        pair => pair.Key,
                        pair => pair.Value,
                        StringComparer.Ordinal);
                return new Person
                {
                    Name = fields["name"].As<string>(),
                    Age = fields["age"].As<long>(),
                };
            });
        BamlDynamicRegistry.RegisterCanonicalList<long>();
        BamlDynamicRegistry.RegisterCanonicalStringMap<long>();
    }

    private static void VerifyOptionalNullableAndPartialState()
    {
        BamlOptional<long> unset = default;
        Require(
            !unset.IsSet
            && !new BamlOptional<long>().IsSet
            && !BamlOptional<long>.Unset.IsSet
            && !unset.TryGetValue(out _),
            "optional unset state changed");
        Expect<InvalidOperationException>(() => _ = unset.Value);
        BamlOptional<long> setZero =
            BamlOptional<long>.FromValue(0);
        Require(
            setZero.IsSet
            && setZero.Value == 0
            && setZero.TryGetValue(out long zero)
            && zero == 0,
            "optional explicit default collapsed to unset");
        BamlOptional<string?> explicitNull =
            BamlOptional<string?>.FromValue(null);
        Require(
            explicitNull.IsSet
            && explicitNull.Value is null
            && explicitNull != default,
            "optional explicit null collapsed to unset");

        BamlNullable<long> nullLong = default;
        Require(
            nullLong.IsNull
            && new BamlNullable<long>().IsNull
            && BamlNullable<long>.Null.IsNull
            && BamlNullable.Null<long>().IsNull
            && !nullLong.TryGetValue(out _),
            "nullable zero state changed");
        Expect<InvalidOperationException>(
            () => _ = nullLong.Value);
        BamlNullable<long> valueZero =
            BamlNullable.FromValue(0L);
        BamlNullable<string> nullString =
            BamlNullable<string>.FromValue(null!);
        Require(
            !valueZero.IsNull
            && valueZero.Value == 0
            && nullString.IsNull
            && valueZero.Match(
                () => -1L,
                value => value) == 0,
            "nullable value/null classification failed");
        Expect<ArgumentNullException>(
            () => nullLong.Match<long>(
                null!,
                value => value));
        Expect<ArgumentNullException>(
            () => valueZero.Match<long>(
                () => 0,
                null!));

        BamlOptional<BamlNullable<long>> composedUnset =
            default;
        BamlOptional<BamlNullable<long>> composedNull =
            BamlNullable.Null<long>();
        BamlOptional<BamlNullable<long>> composedValue =
            BamlNullable.FromValue(42L);
        Require(
            !composedUnset.IsSet
            && composedNull.IsSet
            && composedNull.Value.IsNull
            && composedValue.IsSet
            && composedValue.Value.Value == 42,
            "optional/nullable composition lost a state");

        BamlStreamState<string?> pending = default;
        BamlStreamState<string?> incomplete =
            BamlStreamState<string?>.Incomplete("par");
        BamlStreamState<string?> complete =
            BamlStreamState<string?>.Complete(null);
        Require(
            pending.State == BamlStreamStateKind.Pending
            && pending.Value is null
            && !pending.IsComplete
            && incomplete.State
                == BamlStreamStateKind.Incomplete
            && incomplete.Value == "par"
            && complete.IsComplete
            && complete.Value is null
            && complete == BamlStreamState<string?>.Complete(null),
            "stream state semantics failed");

        ResumePartial partial = new()
        {
            RequiredWhenReady = null,
            DoneField = null,
            NonNullPartial = new ResumePartialDetails
            {
                Text = "ready",
            },
            WithState = incomplete,
        };
        Require(
            partial.RequiredWhenReady is null
            && partial.DoneField is null
            && partial.NonNullPartial.Text == "ready"
            && partial.WithState.State
                == BamlStreamStateKind.Incomplete,
            "semantic partial projection collapsed markers");
    }

    private static async Task VerifyMediaAsync()
    {
        byte[] mutable = [0x00, 0xff, 0x7f, 0x80];
        BamlImage image = BamlImage.FromBytes(
            mutable,
            "image/png");
        mutable[0] = 0x42;
        Require(
            image.TryGetBytes(
                out ReadOnlyMemory<byte> imageBytes,
                out string? imageType)
            && imageBytes.Span[0] == 0x00
            && imageType == "image/png",
            "media bytes were not copied");
        BamlAudio audio = BamlAudio.FromBase64(
            Convert.ToBase64String([1, 2, 3]),
            "audio/wav");
        BamlVideo video = BamlVideo.FromUrl(
            "https://example.com/video.mp4?token=secret#fragment",
            "video/mp4");
        Require(
            video.TryGetUrl(out string? videoUrl)
            && videoUrl.Contains(
                "token=secret",
                StringComparison.Ordinal)
            && !video.ToString().Contains(
                "token=secret",
                StringComparison.Ordinal)
            && !video.ToString().Contains(
                "fragment",
                StringComparison.Ordinal),
            "media URL redaction changed structured value or leaked display");

        string file = Path.Combine(
            Path.GetTempPath(),
            $"baml-managed-media-{Environment.ProcessId}.pdf");
        await File.WriteAllBytesAsync(
                file,
                [0x25, 0x50, 0x44, 0x46])
            .ConfigureAwait(false);
        BamlPdf pdf;
        try
        {
            pdf = await BamlPdf.FromFileAsync(
                    file,
                    "application/pdf")
                .ConfigureAwait(false);
            File.Delete(file);
            Require(
                pdf.TryGetBytes(
                    out ReadOnlyMemory<byte> pdfBytes,
                    out string? pdfType)
                && pdfBytes.Span.SequenceEqual(
                    new byte[] { 0x25, 0x50, 0x44, 0x46 })
                && pdfType == "application/pdf",
                "file media was not eagerly owned");
        }
        finally
        {
            if (File.Exists(file))
            {
                File.Delete(file);
            }
        }

        Require(
            image.Equals(
                BamlImage.FromBase64(
                    Convert.ToBase64String(
                        [0x00, 0xff, 0x7f, 0x80]),
                    "image/png"))
            && audio.Equals(
                BamlAudio.FromBytes(
                    new byte[] { 1, 2, 3 },
                    "audio/wav"))
            && !video.Equals(
                BamlVideo.FromBytes(
                    new byte[] { 1 },
                    "video/mp4"))
            && pdf!.GetHashCode()
                == BamlPdf.FromBytes(
                    new byte[] { 0x25, 0x50, 0x44, 0x46 },
                    "application/pdf").GetHashCode(),
            "media structural equality/hash failed");
    }

    private static async Task VerifyRequestClientAndHandleAsync()
    {
        byte[] body = [1, 2, 3];
        List<KeyValuePair<string, string>> headers =
        [
            new("X-Trace", "one"),
            new("X-Trace", "two"),
            new("Content-Language", "en"),
        ];
        BamlHttpRequest snapshot = new(
            "request-17",
            "POST",
            "https://example.com/provider?key=secret",
            headers,
            "application/octet-stream",
            body);
        headers.Clear();
        body[0] = 9;
        using HttpRequestMessage first =
            snapshot.ToHttpRequestMessage();
        using HttpRequestMessage second =
            snapshot.ToHttpRequestMessage();
        Require(
            snapshot.Headers.Count == 3
            && snapshot.Body.Span[0] == 1
            && !ReferenceEquals(first, second)
            && !ReferenceEquals(first.Content, second.Content)
            && first.Headers.GetValues("X-Trace")
                .SequenceEqual(["one", "two"])
            && first.Content!.Headers.ContentType!.MediaType
                == "application/octet-stream"
            && !snapshot.ToString().Contains(
                "secret",
                StringComparison.Ordinal)
            && !snapshot.ToString().Contains(
                "1, 2, 3",
                StringComparison.Ordinal),
            "HTTP request snapshot/conversion failed");
        byte[] firstBody = await first.Content!.ReadAsByteArrayAsync()
            .ConfigureAwait(false);
        first.Dispose();
        byte[] secondBody = await second.Content!
            .ReadAsByteArrayAsync()
            .ConfigureAwait(false);
        Require(
            firstBody.SequenceEqual(new byte[] { 1, 2, 3 })
            && secondBody.SequenceEqual(
                new byte[] { 1, 2, 3 }),
            "HTTP message disposal or mutation affected sibling");

        BamlRetryPolicy retry = new(
            maxRetries: 3,
            initialDelayMilliseconds: 10,
            maxDelayMilliseconds: 100,
            multiplier: 2);
        BamlClient child = BamlClient.FromShorthand("child");
        BamlClient[] sourceChildren = [child];
        BamlClient client = new(
            "fallback",
            BamlClientType.Fallback,
            sourceChildren,
            retry,
            counter: 7);
        sourceChildren[0] = BamlClient.FromShorthand("mutated");
        BamlClient equivalent = new(
            "fallback",
            BamlClientType.Fallback,
            [BamlClient.FromShorthand("child")],
            new BamlRetryPolicy(3, 10, 100, 2),
            7);
        Require(
            client.SubClients[0].Name == "child"
            && client.Equals(equivalent)
            && client.GetHashCode() == equivalent.GetHashCode()
            && BamlClient.FromShorthand("name").ClientType
                == BamlClientType.Primitive,
            "client/retry snapshot or structural equality failed");
        Expect<ArgumentOutOfRangeException>(
            () => _ = new BamlRetryPolicy(
                -1,
                null,
                null,
                null));
        Expect<ArgumentOutOfRangeException>(
            () => _ = new BamlClient(
                "bad",
                (BamlClientType)0,
                null,
                null,
                0));

        int releasesBefore = NativeReferenceTable.Releases;
        BamlHandle original = BamlHandle.CreateForProbe();
        BamlHandle clone = original.Clone();
        Require(
            !ReferenceEquals(original, clone)
            && !original.IsClosed
            && !clone.IsClosed,
            "handle clone did not create a distinct owner");
        long identity = original.LeaseForProbe(value => value);
        Require(
            clone.LeaseForProbe(value => value) == identity,
            "handle clone did not refer to the same resource");
        original.Dispose();
        original.Dispose();
        Require(
            original.IsClosed
            && !clone.IsClosed
            && clone.LeaseForProbe(value => value) == identity,
            "handle dispose was not independent/idempotent");
        Expect<ObjectDisposedException>(
            () => _ = original.Clone());
        clone.Dispose();
        Require(
            NativeReferenceTable.Releases
                == releasesBefore + 2,
            "handle wrappers did not release exactly once");

        BamlHandle racing = BamlHandle.CreateForProbe();
        Task[] leases = Enumerable.Range(0, 64)
            .Select(
                index => Task.Run(
                    () =>
                    {
                        try
                        {
                            long leased = racing.LeaseForProbe(
                                value => value);
                            GC.KeepAlive(leased);
                        }
                        catch (ObjectDisposedException)
                        {
                        }
                    }))
            .Append(Task.Run(racing.Dispose))
            .ToArray();
        await Task.WhenAll(leases).ConfigureAwait(false);
        Require(racing.IsClosed, "handle disposal race did not close");
    }

    private static void VerifyDynamicValues()
    {
        BamlValue nullValue = BamlValue.Null;
        BamlValue boolValue = BamlValue.Bool(true);
        BamlValue intValue = BamlValue.Int(BamlInteger.Max);
        BamlValue floatValue = BamlValue.Float(1.5);
        BamlValue bigIntValue = BamlValue.BigInt(
            BigInteger.Parse(
                "123456789012345678901234567890"));
        BamlValue stringValue = BamlValue.String("value");
        byte[] sourceBytes = [1, 2, 3];
        BamlValue bytesValue = BamlValue.Bytes(sourceBytes);
        sourceBytes[0] = 9;
        BamlValue listValue = BamlValue.List(
            [intValue, BamlValue.Int(4)]);
        BamlValue mapA = BamlValue.Map(
            [
                new("z", BamlValue.Int(2)),
                new("a", BamlValue.Int(1)),
            ]);
        BamlValue mapB = BamlValue.Map(
            [
                new("a", BamlValue.Int(1)),
                new("z", BamlValue.Int(2)),
            ]);
        BamlValue enumValue = BamlValue.Enum(
            "probe.Color",
            "RED");
        BamlValue classValue = BamlValue.From(
            new Person
            {
                Name = "Ada",
                Age = 37,
            });
        BamlTypeDescriptor stringDescriptor =
            BamlValue.String(String.Empty).Type;
        BamlTypeDescriptor intDescriptor =
            BamlValue.Int(0).Type;
        BamlValue unionValue = BamlValue.Union(
            [stringDescriptor, intDescriptor],
            activeCase: 1,
            BamlValue.Int(9));
        BamlImage image = BamlImage.FromUrl(
            "https://example.com/a.png",
            "image/png");
        BamlValue mediaValue = BamlValue.Media(image);
        using BamlHandle handle = BamlHandle.CreateForProbe();
        BamlValue handleValue = BamlValue.Handle(
            handle,
            "probe.Resource");

        BamlValue[] kinds =
        [
            nullValue,
            boolValue,
            intValue,
            floatValue,
            bigIntValue,
            stringValue,
            bytesValue,
            listValue,
            mapA,
            enumValue,
            classValue,
            unionValue,
            mediaValue,
            handleValue,
        ];
        Require(
            kinds.Select(value => value.Kind)
                .SequenceEqual(
                    Enum.GetValues<BamlValueKind>()),
            "dynamic value kinds are incomplete or out of order");
        Require(
            ((byte[])bytesValue.PayloadForProbe!)[0] == 1
            && mapA.Equals(mapB)
            && mapA.GetHashCode() == mapB.GetHashCode()
            && listValue.Equals(
                BamlValue.List(
                    [BamlValue.Int(BamlInteger.Max), BamlValue.Int(4)]))
            && unionValue.TryGetUnion(
                out int unionCase,
                out BamlValue? unionPayload)
            && unionCase == 1
            && unionPayload.As<long>() == 9
            && BamlValue.Media(image).Equals(mediaValue)
            && BamlValue.Handle(handle, "probe.Resource")
                .Equals(handleValue),
            "dynamic structural equality/ownership failed");
        using BamlHandle otherHandle = BamlHandle.CreateForProbe();
        Require(
            !BamlValue.Handle(otherHandle, "probe.Resource")
                .Equals(handleValue),
            "dynamic handles used native identity instead of wrapper identity");

        Person decoded = classValue.As<Person>();
        Require(
            enumValue.TryGetEnumVariant(out string? wireVariant)
            && wireVariant == "RED"
            && classValue.TryGetClassFields(out var publicFields)
            && publicFields.Select(pair => pair.Key)
                .SequenceEqual(["name", "age"])
            && !stringValue.TryGetEnumVariant(out string? wrongEnum)
            && wrongEnum is null
            && !stringValue.TryGetClassFields(out var wrongFields)
            && wrongFields is null
            && !stringValue.TryGetUnion(
                out int wrongCase,
                out BamlValue? wrongUnion)
            && wrongCase == 0
            && wrongUnion is null,
            "public dynamic shape inspection failed");
        KeyValuePair<string, BamlValue>[] fieldSource =
        [
            new("wire_name", BamlValue.String("original")),
        ];
        BamlValue ownedClass = BamlValue.Class(
            "probe.Owned",
            typeArguments: [],
            fieldSource);
        fieldSource[0] =
            new("mutated", BamlValue.String("changed"));
        if (!ownedClass.TryGetClassFields(out var ownedFields))
        {
            throw new InvalidOperationException(
                "owned class was not publicly inspectable");
        }

        Require(
            ownedFields.Count == 1
            && ownedFields[0].Key == "wire_name"
            && ownedFields[0].Value.As<string>() == "original"
            && ownedFields is ReadOnlyCollection<
                KeyValuePair<string, BamlValue>>,
            "class inspection did not preserve an owned wire-order snapshot");
        Expect<NotSupportedException>(
            () => ((IList<KeyValuePair<string, BamlValue>>)
                ownedFields).Add(
                    new("forbidden", BamlValue.Null)));
        Require(
            decoded.Name == "Ada"
            && decoded.Age == 37
            && BamlValue.From(42L).As<long>() == 42
            && BamlValue.From("x").As<string>() == "x",
            "registered/canonical dynamic codec failed");

        IReadOnlyList<long> decodedList =
            listValue.As<IReadOnlyList<long>>();
        IReadOnlyDictionary<string, long> decodedMap =
            mapA.As<IReadOnlyDictionary<string, long>>();
        List<long> listSource = [7, 8];
        IReadOnlyList<long> listInput = listSource;
        BamlValue encodedList = BamlValue.From(listInput);
        listSource[0] = 99;
        Dictionary<string, long> mapSource =
            new(StringComparer.Ordinal)
            {
                ["value"] = 5,
            };
        IReadOnlyDictionary<string, long> mapInput = mapSource;
        BamlValue encodedMap = BamlValue.From(mapInput);
        mapSource["value"] = 99;
        Require(
            decodedList is ReadOnlyCollection<long>
            && decodedList.SequenceEqual(
                [BamlInteger.Max, 4L])
            && decodedMap is ReadOnlyDictionary<string, long>
            && decodedMap.Count == 2
            && decodedMap["a"] == 1
            && decodedMap["z"] == 2,
            "canonical list/map decoding lost values or ownership");
        Require(
            encodedList.As<IReadOnlyList<long>>()
                .SequenceEqual([7L, 8L])
            && encodedMap.As<
                IReadOnlyDictionary<string, long>>()["value"] == 5,
            "canonical list/map decoding lost values or ownership");
        Expect<NotSupportedException>(
            () => ((IList<long>)decodedList)[0] = 0);
        Expect<NotSupportedException>(
            () => ((IDictionary<string, long>)decodedMap)
                .Add("new", 3));

        Require(
            nullValue.TryGet(out BamlValue? standaloneNull)
            && ReferenceEquals(standaloneNull, BamlValue.Null)
            && nullValue.TryGet(out long? nullableLong)
            && nullableLong is null
            && nullValue.TryGet(
                out BamlNullable<string> nullableString)
            && nullableString.IsNull
            && !nullValue.TryGet(out object? rejectedObject)
            && rejectedObject is null
            && !nullValue.TryGet(out string? rejectedString)
            && rejectedString is null
            && !nullValue.TryGet(out Person? rejectedPerson)
            && rejectedPerson is null
            && !nullValue.TryGet(
                out IReadOnlyList<long>? rejectedInterfaceList)
            && rejectedInterfaceList is null
            && !nullValue.TryGet(
                out List<long>? rejectedConcreteList)
            && rejectedConcreteList is null,
            "BAML null escaped into an unsupported or nonnullable CLR target");
        Expect<BamlTypeMappingException>(
            () => _ = nullValue.As<object>());
        Expect<BamlTypeMappingException>(
            () => _ = nullValue.As<string>());
        Expect<BamlTypeMappingException>(
            () => _ = nullValue.As<List<long>>());
        Expect<BamlTypeMappingException>(
            () => _ = listValue.As<List<long>>());
        long? nullableInput = null;
        Require(
            ReferenceEquals(
                BamlValue.From(nullableInput),
                BamlValue.Null),
            "nullable value-type null did not map to explicit BAML null");
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.From<object?>(null));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.From<string?>(null));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.From<List<long>?>(null));

        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.From(42));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.From(new { Value = 1 }));

        BamlValue alias = BamlValue.Alias(
            "probe.UserId",
            BamlValue.String("u-1"));
        BamlValue literal = BamlValue.Literal(
            BamlValue.String("fixed"),
            "fixed");
        Require(
            alias.Type.Alias == "probe.UserId"
            && literal.Type.Literal == "fixed"
            && BamlValue.String("fixed").Type.Alias is null
            && BamlValue.String("fixed").Type.Literal is null
            && !literal.Type.Equals(
                BamlValue.Literal(
                    BamlValue.String("other"),
                    "other").Type),
            "alias/literal descriptor identity was lost");
        Require(
            !alias.TryGet(out string? aliasAsString)
            && aliasAsString is null
            && !literal.TryGet(out string? literalAsString)
            && literalAsString is null,
            "context-free decoding guessed an alias/literal occurrence");
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.Literal(
                BamlValue.Int(1),
                "01"));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.Union(
                [stringDescriptor, intDescriptor],
                activeCase: 0,
                BamlValue.Int(9)));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.Map(
                [
                    new("same", BamlValue.Int(1)),
                    new("same", BamlValue.Int(2)),
                ]));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.List(
                [null!]));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.Map(
                [new("null", (BamlValue)null!)]));
        BamlValue emptyList = BamlValue.List([]);
        BamlValue heterogeneousList = BamlValue.List(
            [BamlValue.Int(1), BamlValue.String("one")]);
        BamlValue emptyMap = BamlValue.Map([]);
        BamlValue heterogeneousMap = BamlValue.Map(
            [
                new("int", BamlValue.Int(1)),
                new("string", BamlValue.String("one")),
            ]);
        Require(
            emptyList.Type.Arguments[0].Kind
                == BamlTypeDescriptorKind.Unknown
            && heterogeneousList.Type.Arguments[0].Kind
                == BamlTypeDescriptorKind.Unknown
            && emptyMap.Type.Arguments[1].Kind
                == BamlTypeDescriptorKind.Unknown
            && heterogeneousMap.Type.Arguments[1].Kind
                == BamlTypeDescriptorKind.Unknown
            && ((IReadOnlyList<BamlValue>)
                heterogeneousList.PayloadForProbe!)
                .Select(item => item.Type.Kind)
                .SequenceEqual(
                    [
                        BamlTypeDescriptorKind.Int,
                        BamlTypeDescriptorKind.String,
                    ])
            && emptyList.As<IReadOnlyList<long>>().Count == 0
            && emptyMap.As<
                IReadOnlyDictionary<string, long>>().Count == 0
            && !heterogeneousList.TryGet(
                out IReadOnlyList<long>? wrongTypedList)
            && wrongTypedList is null
            && !heterogeneousMap.TryGet(
                out IReadOnlyDictionary<
                    string,
                    long>? wrongTypedMap)
            && wrongTypedMap is null,
            "dynamic containers inferred a type or encoded unknown as null");
    }

    private static void VerifyGenericBinder()
    {
        BamlTypeDescriptor nullableReference =
            BamlClrTypeBinder.Describe<
                BamlNullable<string>>();
        BamlTypeDescriptor nestedCollection =
            BamlClrTypeBinder.Describe<
                IReadOnlyDictionary<
                    Color,
                    IReadOnlyList<
                        BamlNullable<Person>>>>();
        Require(
            nullableReference.IsNullable
            && nullableReference.Kind
                == BamlTypeDescriptorKind.String
            && nestedCollection.Kind
                == BamlTypeDescriptorKind.Map
            && BamlClrTypeBinder.Describe<BamlValue>().Kind
                == BamlTypeDescriptorKind.Unknown
            && BamlClrTypeBinder.Describe<Box<long>>().Fqn
                == "probe.Box"
            && BamlClrTypeBinder.Describe<long?>().IsNullable,
            "canonical generic binder lost a supported closure");

        Type[] unsupported =
        [
            typeof(short),
            typeof(int),
            typeof(uint),
            typeof(ulong),
            typeof(float),
            typeof(decimal),
            typeof(List<long>),
            typeof(Dictionary<string, long>),
            typeof(long[]),
            typeof(object),
            typeof(JsonElement),
            typeof(JsonNode),
            typeof(JsonDocument),
            typeof(DateTime),
            typeof(DateTimeOffset),
            typeof(DateOnly),
            typeof(Guid),
            typeof(Uri),
            typeof(ValueTuple<long, string>),
            typeof(BamlOptional<long>),
            typeof(BamlUnion<string, long>),
            typeof(BamlNullable<BamlNullable<string>>),
            typeof(IReadOnlyDictionary<long, string>),
        ];
        foreach (Type type in unsupported)
        {
            BamlTypeMappingException exception =
                Expect<BamlTypeMappingException>(
                    () => _ = BamlClrTypeBinder.Describe(
                        type,
                        "$T"));
            Require(
                exception.Path.StartsWith(
                    "$T",
                    StringComparison.Ordinal),
                $"unsupported generic diagnostic lost context for {type}");
        }

        Require(
            Expect<BamlTypeMappingException>(
                () => _ = BamlClrTypeBinder.Describe(
                    typeof(int),
                    "$T")).CanonicalReplacement == "long"
            && Expect<BamlTypeMappingException>(
                () => _ = BamlClrTypeBinder.Describe(
                    typeof(float),
                    "$T")).CanonicalReplacement == "double",
            "noncanonical numeric diagnostics lost replacements");
    }

    private static void VerifyLimitsAndCycles()
    {
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.List(
                new CountOnlyValues(
                    BamlValueLimits.MaxCollectionItems + 1)));
        using OversizeMemory memory = new(
            BamlValueLimits.MaxBytes + 1);
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.Bytes(memory.Oversized));

        BamlValue deep = BamlValue.Null;
        for (int index = 0;
            index < BamlValueLimits.MaxDepth;
            index++)
        {
            deep = BamlValue.List([deep]);
        }

        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.List([deep]));
        BamlValue wide = BamlValue.List(
            Enumerable.Repeat(
                BamlValue.Null,
                BamlValueLimits.MaxCollectionItems));
        Expect<BamlTypeMappingException>(
            () => _ = BamlValue.List([wide, wide]));
        BamlBigIntCodec.RequireHexLength(
            BamlBigIntCodec.MaxHexLength);
        Expect<BamlTypeMappingException>(
            () => BamlBigIntCodec.RequireHexLength(
                BamlBigIntCodec.MaxHexLength + 1));

        Node node = new() { Name = "cycle" };
        node.Next = node;
        GeneratedCodecTraversal traversal = new();
        BamlTypeMappingException cycle =
            Expect<BamlTypeMappingException>(
                () => EncodeNode(
                    traversal,
                    node,
                    "$.next"));
        Require(
            cycle.Path == "$.next"
            && cycle.Message.Contains(
                "cycle",
                StringComparison.Ordinal),
            "cycle diagnostic lost its exact path");
    }

    private static void VerifyPublicShapeInvariants()
    {
        BamlTypeDescriptor[] mutableArguments =
        [
            new BamlTypeDescriptor(BamlValueKind.Int),
        ];
        BamlTypeDescriptor genericDescriptor = new(
            BamlValueKind.Class,
            "probe.Box",
            mutableArguments);
        mutableArguments[0] =
            new BamlTypeDescriptor(BamlValueKind.String);
        Require(
            genericDescriptor.Fqn == "probe.Box"
            && genericDescriptor.Arguments.Count == 1
            && genericDescriptor.Arguments[0].Kind
                == BamlTypeDescriptorKind.Int
            && BamlValue.List([BamlValue.Int(1)])
                .Type.Arguments[0].Kind
                == BamlTypeDescriptorKind.Unknown
            && BamlValue.Map([new("one", BamlValue.Int(1))])
                .Type.Arguments[1].Kind
                == BamlTypeDescriptorKind.Unknown
            && BamlValue.Enum("probe.Color", "RED").Type.Fqn
                == "probe.Color"
            && BamlValue.Media(
                BamlImage.FromUrl("https://example.com"))
                .Type.Fqn is null,
            "descriptor argument/FQN semantics changed");
        Expect<ArgumentException>(
            () => _ = new BamlTypeDescriptor(
                BamlValueKind.Enum));
        Expect<ArgumentException>(
            () => _ = new BamlTypeDescriptor(
                BamlValueKind.String,
                "not.nominal"));
        Expect<ArgumentException>(
            () => _ = new BamlTypeDescriptor(
                BamlValueKind.List,
                arguments: []));
        Expect<ArgumentOutOfRangeException>(
            () => _ = new BamlTypeDescriptor(
                (BamlValueKind)14));
        Expect<ArgumentOutOfRangeException>(
            () => _ = new BamlTypeDescriptor(
                (BamlTypeDescriptorKind)15));

        Require(
            !typeof(IDisposable).IsAssignableFrom(typeof(BamlImage))
            && !typeof(IDisposable).IsAssignableFrom(typeof(BamlAudio))
            && !typeof(IDisposable).IsAssignableFrom(typeof(BamlVideo))
            && !typeof(IDisposable).IsAssignableFrom(typeof(BamlPdf))
            && !typeof(IDisposable).IsAssignableFrom(typeof(BamlHttpRequest))
            && typeof(IDisposable).IsAssignableFrom(typeof(BamlHandle))
            && typeof(BamlImage).IsSealed
            && typeof(BamlValue).IsSealed
            && typeof(BamlTypeDescriptor).IsSealed
            && typeof(BamlClient).IsSealed
            && typeof(BamlHttpRequest).IsSealed,
            "public ownership/sealing contract changed");
        Require(
            typeof(BamlHandle).GetProperties()
                .All(property =>
                    property.PropertyType != typeof(IntPtr)
                    && property.PropertyType
                        != typeof(SafeHandle))
            && typeof(BamlValue).GetProperties()
                .All(property =>
                    property.PropertyType != typeof(object)),
            "public surface exposed a raw handle/object");

        string[] descriptorProperties =
            typeof(BamlTypeDescriptor)
                .GetProperties(
                    BindingFlags.Public
                    | BindingFlags.Instance
                    | BindingFlags.DeclaredOnly)
                .Select(property => property.Name)
                .OrderBy(
                    name => name,
                    StringComparer.Ordinal)
                .ToArray();
        Require(
            descriptorProperties.SequenceEqual(
                new[]
                {
                    "Alias",
                    "Arguments",
                    "Fqn",
                    "IsNullable",
                    "Kind",
                    "Literal",
                })
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.Kind))!
                .PropertyType == typeof(BamlTypeDescriptorKind)
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.Fqn))!
                .PropertyType == typeof(string)
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.Arguments))!
                .PropertyType
                == typeof(IReadOnlyList<BamlTypeDescriptor>)
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.IsNullable))!
                .PropertyType == typeof(bool)
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.Alias))!
                .PropertyType == typeof(string)
            && typeof(BamlTypeDescriptor)
                .GetProperty(nameof(BamlTypeDescriptor.Literal))!
                .PropertyType == typeof(string)
            && typeof(BamlTypeDescriptor)
                .GetConstructors(
                    BindingFlags.Public | BindingFlags.Instance)
                .Length == 0,
            "BamlTypeDescriptor public inspection surface drifted");

        string[] valueProperties =
            typeof(BamlValue)
                .GetProperties(
                    BindingFlags.Public
                    | BindingFlags.Instance
                    | BindingFlags.Static
                    | BindingFlags.DeclaredOnly)
                .Select(property => property.Name)
                .OrderBy(
                    name => name,
                    StringComparer.Ordinal)
                .ToArray();
        string[] valueMethods =
            typeof(BamlValue)
                .GetMethods(
                    BindingFlags.Public
                    | BindingFlags.Instance
                    | BindingFlags.Static
                    | BindingFlags.DeclaredOnly)
                .Where(method => !method.IsSpecialName)
                .Select(method => method.Name)
                .OrderBy(
                    name => name,
                    StringComparer.Ordinal)
                .ToArray();
        Require(
            valueProperties.SequenceEqual(
                new[] { "Kind", "Null", "Type" })
            && valueMethods.SequenceEqual(
                new[]
                {
                    "As",
                    "BigInt",
                    "Bool",
                    "Bytes",
                    "Equals",
                    "Equals",
                    "Float",
                    "From",
                    "GetHashCode",
                    "Int",
                    "List",
                    "Map",
                    "String",
                    "ToString",
                    "TryGet",
                    "TryGetClassFields",
                    "TryGetEnumVariant",
                    "TryGetUnion",
                })
            && typeof(BamlValue)
                .GetConstructors(
                    BindingFlags.Public | BindingFlags.Instance)
                .Length == 0,
            "BamlValue public inspection surface drifted");

        Require(
            Enum.GetUnderlyingType(typeof(BamlValueKind))
                == typeof(int)
            && Enum.GetValues<BamlValueKind>()
                .Select(value => (int)value)
                .SequenceEqual(Enumerable.Range(0, 14))
            && Enum.GetUnderlyingType(
                typeof(BamlTypeDescriptorKind)) == typeof(int)
            && Enum.GetValues<BamlTypeDescriptorKind>()
                .Select(value => (int)value)
                .SequenceEqual(Enumerable.Range(0, 15))
            && Enum.GetUnderlyingType(typeof(BamlStreamStateKind))
                == typeof(int)
            && Enum.GetValues<BamlStreamStateKind>()
                .Select(value => (int)value)
                .SequenceEqual([0, 1, 2])
            && Enum.GetUnderlyingType(typeof(BamlClientType))
                == typeof(long)
            && Enum.GetValues<BamlClientType>()
                .Select(value => (long)value)
                .SequenceEqual([1L, 2L, 3L]),
            "bridge-owned enum ABI numbers drifted");

        BamlUnion<string, long> first =
            BamlUnion<string, long>.FromT0("left");
        BamlUnion<string, long> second =
            BamlUnion<string, long>.FromT1(2);
        long switched = 0;
        second.Switch(
            _ => switched = -1,
            value => switched = value);
        Require(
            first.IsT0
            && !first.IsT1
            && first.AsT0 == "left"
            && second.IsT1
            && !second.IsT0
            && second.AsT1 == 2
            && switched == 2
            && second.ActiveCaseForCodec == 1
            && Equals(second.ValueForCodec, 2L),
            "public/internal union accessors lost case identity");
        Expect<InvalidOperationException>(() => _ = first.AsT1);
        Expect<InvalidOperationException>(() => _ = second.AsT0);
        Require(
            !default(BamlUnion<string, long>).IsT0
            && !default(BamlUnion<string, long>).IsT1
            && typeof(BamlUnion<string, long>)
                .GetProperties(
                    BindingFlags.Public
                    | BindingFlags.Instance
                    | BindingFlags.DeclaredOnly)
                .Select(property => property.Name)
                .OrderBy(
                    name => name,
                    StringComparer.Ordinal)
                .SequenceEqual(
                    new[] { "AsT0", "AsT1", "IsT0", "IsT1" })
            && typeof(BamlUnion<string, long>)
                .GetProperty("CaseIndex") is null
            && typeof(BamlUnion<string, long>)
                .GetProperty("IsValid") is null,
            "union default became valid");
        Expect<InvalidOperationException>(
            () => _ =
                default(BamlUnion<string, long>).AsT0);
        Expect<InvalidOperationException>(
            () => _ =
                default(BamlUnion<string, long>)
                    .ActiveCaseForCodec);
    }

    private static void EncodeNode(
        GeneratedCodecTraversal traversal,
        Node node,
        string path) =>
        traversal.Visit(
            node,
            path,
            (context, current) =>
            {
                if (current.Next is not null)
                {
                    EncodeNode(
                        context,
                        current.Next,
                        $"{path}");
                }
            });

    private static TException Expect<TException>(Action action)
        where TException : Exception
    {
        try
        {
            action();
        }
        catch (TException exception)
        {
            return exception;
        }

        throw new InvalidOperationException(
            $"expected {typeof(TException).Name}");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }

    private sealed class CountOnlyValues
        : ICollection<BamlValue>
    {
        internal CountOnlyValues(int count)
        {
            Count = count;
        }

        public int Count { get; }

        public bool IsReadOnly => true;

        public void Add(BamlValue item) =>
            throw new NotSupportedException();

        public void Clear() =>
            throw new NotSupportedException();

        public bool Contains(BamlValue item) => false;

        public void CopyTo(BamlValue[] array, int arrayIndex)
        {
        }

        public IEnumerator<BamlValue> GetEnumerator() =>
            Enumerable.Empty<BamlValue>().GetEnumerator();

        public bool Remove(BamlValue item) =>
            throw new NotSupportedException();

        IEnumerator IEnumerable.GetEnumerator() =>
            GetEnumerator();
    }

    private sealed class OversizeMemory : MemoryManager<byte>
    {
        private readonly int length;

        internal OversizeMemory(int length)
        {
            this.length = length;
        }

        internal Memory<byte> Oversized =>
            CreateMemory(length);

        public override Span<byte> GetSpan() =>
            throw new InvalidOperationException(
                "oversize memory must be rejected before access");

        public override MemoryHandle Pin(int elementIndex = 0) =>
            throw new NotSupportedException();

        public override void Unpin()
        {
        }

        protected override void Dispose(bool disposing)
        {
        }

    }
}

namespace Probe.Generated
{
    public enum Color : long
    {
        Red = 1,
        Blue = 2,
    }

    public sealed class Person
    {
        public required string Name { get; init; }

        public required long Age { get; init; }
    }

    public sealed class Box<T>
    {
        public required T Value { get; init; }
    }

    public sealed class Node
    {
        public required string Name { get; init; }

        public Node? Next { get; set; }
    }

    public sealed class ResumePartial
    {
        public string? RequiredWhenReady { get; init; }

        public string? DoneField { get; init; }

        public required ResumePartialDetails NonNullPartial { get; init; }

        public BamlStreamState<string?> WithState { get; init; }
    }

    public sealed class ResumePartialDetails
    {
        public required string Text { get; init; }
    }
}
