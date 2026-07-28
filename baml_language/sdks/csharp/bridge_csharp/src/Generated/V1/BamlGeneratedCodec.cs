using System.Collections.ObjectModel;
using System.ComponentModel;
using System.Numerics;
using System.Text;

using BamlBridge.Cffi.V1;
using Google.Protobuf;

namespace Baml.Generated.V1;

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly partial struct BamlGeneratedCodecContext
{
    private static readonly UTF8Encoding StrictUtf8 = new(
        encoderShouldEmitUTF8Identifier: false,
        throwOnInvalidBytes: true);

    private readonly BamlGeneratedRegistry registry;
    private readonly BamlGeneratedEncodeBudget encodeBudget;

    internal BamlGeneratedCodecContext(BamlGeneratedRegistry registry)
    {
        this.registry = registry;
        encodeBudget = new BamlGeneratedEncodeBudget();
    }

    internal BamlGeneratedCodecContext(
        BamlGeneratedRegistry registry,
        BamlGeneratedEncodeBudget encodeBudget)
    {
        this.registry = registry;
        this.encodeBudget = encodeBudget;
    }

    public BamlGeneratedValue Null() => BamlGeneratedValue.CreateNull();

    public BamlGeneratedValue Bool(bool value) => BamlGeneratedValue.CreateBool(value);

    public BamlGeneratedValue Int(long value) =>
        BamlGeneratedValue.CreateInt(BamlInteger.Require(value, "generated codec"));

    public BamlGeneratedValue Float(double value)
    {
        if (!double.IsFinite(value))
        {
            throw new BamlProtocolException(
                "A generated codec received a non-finite BAML float.",
                $"Generated codec received {value}.");
        }

        return BamlGeneratedValue.CreateFloat(value);
    }

    public BamlGeneratedValue String(string value)
    {
        ArgumentNullException.ThrowIfNull(value);
        try
        {
            _ = StrictUtf8.GetByteCount(value);
        }
        catch (EncoderFallbackException error)
        {
            throw new BamlProtocolException(
                "A generated codec received a string that is not valid Unicode.",
                error.Message);
        }

        return BamlGeneratedValue.CreateString(value);
    }

    public BamlGeneratedValue Bytes(ReadOnlySpan<byte> value) =>
        BamlGeneratedValue.CreateBytes(value);

    public BamlGeneratedValue BigInt(BigInteger value) =>
        BamlGeneratedValue.CreateBigInt(value);

    public BamlGeneratedValue Media(global::Baml.BamlImage value) =>
        BamlGeneratedValue.CreateMedia(value);

    public BamlGeneratedValue Media(global::Baml.BamlAudio value) =>
        BamlGeneratedValue.CreateMedia(value);

    public BamlGeneratedValue Media(global::Baml.BamlVideo value) =>
        BamlGeneratedValue.CreateMedia(value);

    public BamlGeneratedValue Media(global::Baml.BamlPdf value) =>
        BamlGeneratedValue.CreateMedia(value);

    public BamlGeneratedValue Handle(global::Baml.BamlHandle value) =>
        BamlGeneratedValue.CreateHandle(value);

    public BamlGeneratedValue Handle(
        global::Baml.BamlHandle value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        RequireHandleType(value, expectedIdentity, expectedTypeMetadata, inbound: true);
        return BamlGeneratedValue.CreateHandle(value);
    }

    public BamlGeneratedValue Handle(
        global::Baml.BamlHandle value,
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments) =>
        Handle(
            value,
            expectedIdentity,
            ResourceTypeMetadata(expectedIdentity, expectedTypeArguments));

    public BamlGeneratedValue Resource(
        BamlGeneratedResource value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        ArgumentNullException.ThrowIfNull(value);
        BamlGeneratedValue encoded = value.Value;
        _ = ReadResourceFields(encoded, expectedIdentity, expectedTypeMetadata);
        return encoded;
    }

    public BamlGeneratedValue Resource(
        BamlGeneratedResource value,
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments) =>
        Resource(
            value,
            expectedIdentity,
            ResourceTypeMetadata(expectedIdentity, expectedTypeArguments));

    public BamlGeneratedValue StreamState<T>(
        BamlGeneratedType<T> type,
        global::Baml.BamlStreamState<T> value)
    {
        TypeDeclaration<T> declaration = registry.RequireType(type);
        if (value.State == global::Baml.BamlStreamStateKind.Pending
            && !EqualityComparer<T>.Default.Equals(value.Value, default!))
        {
            return Fail<BamlGeneratedValue>(
                "A pending BAML stream state carried a non-default value.",
                $"Pending BamlStreamState<{typeof(T)}> must retain its zero/default value.");
        }

        return Class(
            "StreamState",
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("value", registry.Encode(declaration, value.Value, encodeBudget)),
                new("state", String(value.State.ToString())),
            });
    }

    public BamlGeneratedHostParameter Required<T>(BamlGeneratedType<T> type)
    {
        BamlGeneratedRegistry capturedRegistry = registry;
        TypeDeclaration<T> declaration = capturedRegistry.RequireType(type);
        return new BamlGeneratedHostParameter(
            declaration,
            string.Empty,
            optional: false,
            value => capturedRegistry.Decode(declaration, value),
            static () => throw new InvalidOperationException(
                "A required generated callback parameter cannot be unset."));
    }

    public BamlGeneratedHostParameter Optional<T>(
        string wireIdentity,
        BamlGeneratedType<T> type)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(wireIdentity);
        BamlGeneratedRegistry capturedRegistry = registry;
        TypeDeclaration<T> declaration = capturedRegistry.RequireType(type);
        return new BamlGeneratedHostParameter(
            declaration,
            wireIdentity,
            optional: true,
            value => global::Baml.BamlOptional<T>.FromValue(
                capturedRegistry.Decode(declaration, value)),
            static () => global::Baml.BamlOptional<T>.Unset);
    }

    public BamlGeneratedHostResult Result<T>(BamlGeneratedType<T> type)
    {
        BamlGeneratedRegistry capturedRegistry = registry;
        TypeDeclaration<T> declaration = capturedRegistry.RequireType(type);
        return new BamlGeneratedHostResult(
            declaration,
            value => capturedRegistry.Encode(declaration, (T)value!));
    }

    public BamlGeneratedHostResult VoidResult() => new(type: null, encode: null);

    public BamlGeneratedValue HostCallable(
        Delegate callback,
        IReadOnlyList<BamlGeneratedHostParameter> parameters,
        BamlGeneratedHostResult result,
        BamlGeneratedHostInvoker invoke)
    {
        ArgumentNullException.ThrowIfNull(callback);
        ArgumentNullException.ThrowIfNull(parameters);
        ArgumentNullException.ThrowIfNull(result);
        ArgumentNullException.ThrowIfNull(invoke);
        return BamlGeneratedValue.CreateHostCallable(
            new BamlGeneratedHostCallable(
                callback,
                new BamlGeneratedHostCallableDescriptor(parameters, result, invoke)));
    }

    public BamlGeneratedValue Value(global::Baml.BamlValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return value.GeneratedValue;
    }

    public BamlGeneratedValue List(IEnumerable<BamlGeneratedValue> values)
    {
        ArgumentNullException.ThrowIfNull(values);
        return BamlGeneratedValue.CreateList(SnapshotValues(values, "list"));
    }

    public BamlGeneratedValue List(
        IEnumerable<BamlGeneratedValue> values,
        ReadOnlySpan<byte> itemTypeMetadata)
    {
        ArgumentNullException.ThrowIfNull(values);
        if (itemTypeMetadata.IsEmpty)
        {
            throw new ArgumentException(
                "Generated list item type metadata cannot be empty.",
                nameof(itemTypeMetadata));
        }

        return BamlGeneratedValue.CreateList(
            SnapshotValues(values, "list"),
            itemTypeMetadata);
    }

    public BamlGeneratedValue Map(
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> entries)
    {
        ArgumentNullException.ThrowIfNull(entries);
        return BamlGeneratedValue.CreateMap(SnapshotEntries(entries, "map"));
    }

    public BamlGeneratedValue Map(
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> entries,
        ReadOnlySpan<byte> keyTypeMetadata,
        ReadOnlySpan<byte> valueTypeMetadata)
    {
        ArgumentNullException.ThrowIfNull(entries);
        if (keyTypeMetadata.IsEmpty)
        {
            throw new ArgumentException(
                "Generated map key type metadata cannot be empty.",
                nameof(keyTypeMetadata));
        }
        if (valueTypeMetadata.IsEmpty)
        {
            throw new ArgumentException(
                "Generated map value type metadata cannot be empty.",
                nameof(valueTypeMetadata));
        }

        return BamlGeneratedValue.CreateMap(
            SnapshotEntries(entries, "map"),
            keyTypeMetadata,
            valueTypeMetadata);
    }

    public BamlGeneratedValue Class(
        string identity,
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> fields)
        => Class(identity, fields, Array.Empty<byte[]>());

    public BamlGeneratedValue Class(
        string identity,
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> fields,
        IReadOnlyList<byte[]> typeArguments)
    {
        RequireIdentity(identity, "class");
        ArgumentNullException.ThrowIfNull(fields);
        ArgumentNullException.ThrowIfNull(typeArguments);
        return BamlGeneratedValue.CreateClass(
            identity,
            SnapshotEntries(fields, "class"),
            SnapshotTypeMetadata(typeArguments, "class type argument"));
    }

    public BamlGeneratedValue Class(
        string identity,
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> fields,
        IReadOnlyList<BamlGeneratedTypeArgument> typeArguments)
    {
        ArgumentNullException.ThrowIfNull(typeArguments);
        return Class(
            identity,
            fields,
            typeArguments
                .Select(registry.RequireTypeArgumentMetadata)
                .ToList()
                .AsReadOnly());
    }

    public BamlGeneratedValue Enum(string identity, string wireValue)
    {
        RequireIdentity(identity, "enum");
        ArgumentNullException.ThrowIfNull(wireValue);
        return BamlGeneratedValue.CreateEnum(identity, wireValue, isDynamic: false);
    }

    public BamlGeneratedValue Union(
        ReadOnlySpan<byte> selfType,
        ReadOnlySpan<byte> selectedType,
        string optionName,
        BamlGeneratedValue value)
    {
        if (selfType.IsEmpty)
        {
            throw new ArgumentException(
                "A generated union self type cannot be empty.",
                nameof(selfType));
        }

        if (selectedType.IsEmpty)
        {
            throw new ArgumentException(
                "A generated union selected type cannot be empty.",
                nameof(selectedType));
        }

        ArgumentException.ThrowIfNullOrEmpty(optionName);
        ArgumentNullException.ThrowIfNull(value);
        return BamlGeneratedValue.CreateInboundUnion(
            selfType,
            selectedType,
            optionName,
            value);
    }

    public bool ReadBool(BamlGeneratedValue value) => Require(value).ReadBool();

    public long ReadInt(BamlGeneratedValue value) =>
        BamlInteger.Require(Require(value).ReadInt(), "generated result");

    public double ReadFloat(BamlGeneratedValue value)
    {
        double result = Require(value).ReadFloat();
        if (!double.IsFinite(result))
        {
            throw new BamlProtocolException(
                "The native bridge returned a non-finite BAML float.",
                $"Generated result contained {result}.");
        }

        return result;
    }

    public string ReadString(BamlGeneratedValue value) => Require(value).ReadString();

    public byte[] ReadBytes(BamlGeneratedValue value) => Require(value).ReadBytes();

    public BigInteger ReadBigInt(BamlGeneratedValue value) =>
        Require(value).ReadBigInt();

    public global::Baml.BamlImage ReadImage(BamlGeneratedValue value) =>
        Require(value).ReadMedia<global::Baml.BamlImage>();

    public global::Baml.BamlAudio ReadAudio(BamlGeneratedValue value) =>
        Require(value).ReadMedia<global::Baml.BamlAudio>();

    public global::Baml.BamlVideo ReadVideo(BamlGeneratedValue value) =>
        Require(value).ReadMedia<global::Baml.BamlVideo>();

    public global::Baml.BamlPdf ReadPdf(BamlGeneratedValue value) =>
        Require(value).ReadMedia<global::Baml.BamlPdf>();

    public global::Baml.BamlHandle ReadHandle(BamlGeneratedValue value) =>
        Require(value).ReadHandle();

    public global::Baml.BamlHandle ReadHandle(
        BamlGeneratedValue value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        global::Baml.BamlHandle handle = Require(value).ReadHandle();
        RequireHandleType(handle, expectedIdentity, expectedTypeMetadata, inbound: false);
        return handle;
    }

    public global::Baml.BamlHandle ReadHandle(
        BamlGeneratedValue value,
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments) =>
        ReadHandle(
            value,
            expectedIdentity,
            ResourceTypeMetadata(expectedIdentity, expectedTypeArguments));

    public BamlGeneratedResource ReadResource(
        BamlGeneratedValue value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        _ = ReadResourceFields(value, expectedIdentity, expectedTypeMetadata);
        return new BamlGeneratedResource(Require(value));
    }

    public BamlGeneratedResource ReadResource(
        BamlGeneratedValue value,
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments) =>
        ReadResource(
            value,
            expectedIdentity,
            ResourceTypeMetadata(expectedIdentity, expectedTypeArguments));

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadResourceFields(
        BamlGeneratedResource value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return ToReadOnlyDictionary(value.Value.ReadClassFields(), "resource");
    }

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadResourceFields(
        BamlGeneratedValue value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        BamlGeneratedValue required = Require(value);
        IReadOnlyList<byte[]> typeArguments = ResourceClassTypeArguments(
            expectedIdentity,
            expectedTypeMetadata);
        required.RequireClass(expectedIdentity, typeArguments);
        return ToReadOnlyDictionary(required.ReadClassFields(), "resource");
    }

    public global::Baml.BamlStreamState<T> ReadStreamState<T>(
        BamlGeneratedType<T> type,
        BamlGeneratedValue value)
    {
        TypeDeclaration<T> declaration = registry.RequireType(type);
        IReadOnlyDictionary<string, BamlGeneratedValue> fields =
            ReadClass(value, "StreamState");
        if (fields.Count != 2
            || !fields.TryGetValue("value", out BamlGeneratedValue? encodedValue)
            || !fields.TryGetValue("state", out BamlGeneratedValue? encodedState))
        {
            return Fail<global::Baml.BamlStreamState<T>>(
                "The native bridge returned a malformed BAML stream state.",
                $"StreamState expected exact value/state fields, received {fields.Count} fields.");
        }

        T decoded = registry.Decode(declaration, encodedValue);
        return ReadString(encodedState) switch
        {
            "Pending" when EqualityComparer<T>.Default.Equals(decoded, default!) => default,
            "Pending" => Fail<global::Baml.BamlStreamState<T>>(
                "The native bridge returned a pending stream state with a non-default value.",
                $"Pending StreamState<{typeof(T)}> must carry its zero/default value."),
            "Incomplete" => global::Baml.BamlStreamState<T>.Incomplete(decoded),
            "Complete" => global::Baml.BamlStreamState<T>.Complete(decoded),
            string state => Fail<global::Baml.BamlStreamState<T>>(
                "The native bridge returned an unknown BAML stream state.",
                $"StreamState returned state {state}."),
        };
    }

    public global::Baml.BamlValue ReadValue(BamlGeneratedValue value) =>
        new(Require(value));

    public T Fail<T>(string safeMessage, string diagnostic)
    {
        ArgumentException.ThrowIfNullOrEmpty(safeMessage);
        ArgumentException.ThrowIfNullOrEmpty(diagnostic);
        throw new BamlProtocolException(safeMessage, diagnostic);
    }

    private byte[] ResourceTypeMetadata(
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(expectedIdentity);
        ArgumentNullException.ThrowIfNull(expectedTypeArguments);
        return BamlGeneratedTypeMetadata.Class(
            expectedIdentity,
            expectedTypeArguments
                .Select(registry.RequireTypeArgumentMetadata)
                .ToList()
                .AsReadOnly());
    }

    private static IReadOnlyList<byte[]> ResourceClassTypeArguments(
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(expectedIdentity);
        if (expectedTypeMetadata.IsEmpty)
        {
            throw new InvalidOperationException(
                "A generated resource codec requires exact BAML type metadata.");
        }

        BamlTy expected;
        try
        {
            expected = BamlTy.Parser.ParseFrom(expectedTypeMetadata);
        }
        catch (InvalidProtocolBufferException error)
        {
            throw new InvalidOperationException(
                "A generated resource codec carried malformed BAML type metadata.",
                error);
        }

        if (expected.TyCase != BamlTy.TyOneofCase.ClassTy
            || !StringComparer.Ordinal.Equals(expected.ClassTy.Name, expectedIdentity))
        {
            throw new InvalidOperationException(
                $"Generated resource metadata did not describe {expectedIdentity}.");
        }

        return expected.ClassTy.TypeArgs
            .Select(argument => argument.ToByteArray())
            .ToList()
            .AsReadOnly();
    }

    private static void RequireHandleType(
        global::Baml.BamlHandle value,
        string expectedIdentity,
        ReadOnlySpan<byte> expectedTypeMetadata,
        bool inbound)
    {
        ArgumentNullException.ThrowIfNull(value);
        ArgumentException.ThrowIfNullOrWhiteSpace(expectedIdentity);
        if (expectedTypeMetadata.IsEmpty)
        {
            throw new InvalidOperationException(
                "A generated opaque-resource codec requires exact BAML type metadata.");
        }

        global::Baml.BamlTypeDescriptor expected =
            global::Baml.BamlTypeDescriptor.FromMetadata(expectedTypeMetadata.ToArray());
        bool descriptorMatches = expected.Kind == global::Baml.BamlTypeDescriptorKind.Class
            && StringComparer.Ordinal.Equals(expected.Fqn, expectedIdentity)
            && StringComparer.Ordinal.Equals(value.Type.Fqn, expectedIdentity)
            && value.Type.Arguments.SequenceEqual(expected.Arguments);
        BamlHandleType expectedHandleType = StringComparer.Ordinal.Equals(
            expectedIdentity,
            "baml.llm.PromptAst")
                ? BamlHandleType.AdtPromptAst
                : BamlHandleType.AdtTaggedHeapHandle;
        if (descriptorMatches && value.HandleType == expectedHandleType)
        {
            return;
        }

        string diagnostic =
            $"Expected opaque resource {expectedIdentity} with {expected.Arguments.Count} type arguments and handle kind {expectedHandleType}; received {value.Type} with handle kind {value.HandleType}.";
        if (inbound)
        {
            throw new BamlTypeMappingException(
                typeof(global::Baml.BamlHandle),
                $"opaque resource {expectedIdentity}",
                "$",
                diagnostic);
        }

        throw new BamlProtocolException(
            "The native bridge returned the wrong opaque BAML resource type.",
            diagnostic);
    }

    public IReadOnlyList<BamlGeneratedValue> ReadList(
        BamlGeneratedValue value,
        ReadOnlySpan<byte> expectedItemType)
    {
        BamlGeneratedValue required = Require(value);
        required.RequireCollectionTypeMetadata(
            expectedItemType,
            required.ItemTypeMetadata,
            "list item",
            allowCanaryUnknownFallback: true);
        return required.ReadList();
    }

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadMap(
        BamlGeneratedValue value,
        ReadOnlySpan<byte> expectedKeyType,
        ReadOnlySpan<byte> expectedValueType)
    {
        BamlGeneratedValue required = Require(value);
        required.RequireTypeMetadata(expectedKeyType, required.KeyTypeMetadata, "map key");
        required.RequireCollectionTypeMetadata(
            expectedValueType,
            required.ValueTypeMetadata,
            "map value",
            allowCanaryUnknownFallback: true);
        return ToReadOnlyDictionary(required.ReadMapEntries(), "map");
    }

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadClass(
        BamlGeneratedValue value,
        string expectedIdentity)
        => ReadClass(value, expectedIdentity, Array.Empty<byte[]>());

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadClass(
        BamlGeneratedValue value,
        string expectedIdentity,
        IReadOnlyList<byte[]> expectedTypeArguments)
    {
        RequireIdentity(expectedIdentity, "class");
        ArgumentNullException.ThrowIfNull(expectedTypeArguments);
        BamlGeneratedValue required = Require(value);
        required.RequireClass(expectedIdentity, expectedTypeArguments);
        return ToReadOnlyDictionary(required.ReadClassFields(), "class");
    }

    public IReadOnlyDictionary<string, BamlGeneratedValue> ReadClass(
        BamlGeneratedValue value,
        string expectedIdentity,
        IReadOnlyList<BamlGeneratedTypeArgument> expectedTypeArguments)
    {
        ArgumentNullException.ThrowIfNull(expectedTypeArguments);
        return ReadClass(
            value,
            expectedIdentity,
            expectedTypeArguments
                .Select(registry.RequireTypeArgumentMetadata)
                .ToList()
                .AsReadOnly());
    }

    public string ReadEnum(BamlGeneratedValue value, string expectedIdentity)
    {
        RequireIdentity(expectedIdentity, "enum");
        return Require(value).ReadEnum(expectedIdentity);
    }

    public BamlGeneratedUnionValue ReadUnion(
        BamlGeneratedValue value,
        ReadOnlySpan<byte> expectedSelfType,
        IReadOnlyList<string> expectedOptionNames) =>
        ReadUnionCore(value, expectedSelfType, expectedOptionNames, expectedOptionTypes: null);

    public BamlGeneratedUnionValue ReadUnion(
        BamlGeneratedValue value,
        ReadOnlySpan<byte> expectedSelfType,
        IReadOnlyList<string> expectedOptionNames,
        IReadOnlyList<byte[]> expectedOptionTypes)
    {
        ArgumentNullException.ThrowIfNull(expectedOptionTypes);
        if (expectedOptionTypes.Count != expectedOptionNames.Count)
        {
            throw new ArgumentException(
                "Generated union option names and type descriptors must have equal lengths.",
                nameof(expectedOptionTypes));
        }

        return ReadUnionCore(value, expectedSelfType, expectedOptionNames, expectedOptionTypes);
    }

    private BamlGeneratedUnionValue ReadUnionCore(
        BamlGeneratedValue value,
        ReadOnlySpan<byte> expectedSelfType,
        IReadOnlyList<string> expectedOptionNames,
        IReadOnlyList<byte[]>? expectedOptionTypes)
    {
        ArgumentNullException.ThrowIfNull(expectedOptionNames);
        if (expectedOptionNames.Count < 2 || expectedOptionNames.Count > 32)
        {
            throw new ArgumentOutOfRangeException(
                nameof(expectedOptionNames),
                "A generated BAML union descriptor must contain 2 through 32 options.");
        }

        BamlGeneratedValue required = Require(value);
        required.RequireUnionTypeMetadata(
            expectedSelfType,
            required.UnionSelfTypeMetadata,
            "union self");
        string selected = required.ReadUnionOptionName();
        int selectedIndex = -1;
        var optionIdentities = new HashSet<string>(StringComparer.Ordinal);
        for (int index = 0; index < expectedOptionNames.Count; index++)
        {
            string option = expectedOptionNames[index]
                ?? throw new ArgumentException(
                    $"Generated union option {index} is null.",
                    nameof(expectedOptionNames));
            if (!optionIdentities.Add(option))
            {
                throw new ArgumentException(
                    $"Generated union option identity {option} is duplicated.",
                    nameof(expectedOptionNames));
            }

        }

        byte[]? selectedType = required.UnionSelectedTypeMetadata;
        if (selectedType is { Length: > 0 } && expectedOptionTypes is not null)
        {
            for (int index = 0; index < expectedOptionTypes.Count; index++)
            {
                byte[] optionType = expectedOptionTypes[index]
                    ?? throw new ArgumentException(
                        $"Generated union option type {index} is null.",
                        nameof(expectedOptionTypes));
                if (!selectedType.AsSpan().SequenceEqual(optionType))
                {
                    continue;
                }

                if (selectedIndex >= 0)
                {
                    throw new BamlProtocolException(
                        "The native bridge selected an ambiguous generic BAML union option.",
                        $"Selected option {selected} matched more than one generated concrete type.");
                }

                selectedIndex = index;
            }
        }
        else
        {
            for (int index = 0; index < expectedOptionNames.Count; index++)
            {
                if (StringComparer.Ordinal.Equals(expectedOptionNames[index], selected))
                {
                    selectedIndex = index;
                    break;
                }
            }

            if (selectedIndex < 0 && expectedOptionTypes is not null)
            {
                BamlTypeDescriptor payloadType =
                    BamlTypeDescriptor.FromGenerated(required.ReadUnionPayload());
                for (int index = 0; index < expectedOptionTypes.Count; index++)
                {
                    byte[] optionType = expectedOptionTypes[index]
                        ?? throw new ArgumentException(
                            $"Generated union option type {index} is null.",
                            nameof(expectedOptionTypes));
                    if (!payloadType.Equals(
                            BamlTypeDescriptor.FromMetadata(optionType)))
                    {
                        continue;
                    }

                    if (selectedIndex >= 0)
                    {
                        throw new BamlProtocolException(
                            "The native bridge selected an ambiguous generic BAML union option.",
                            $"Selected option {selected} matched more than one generated payload type.");
                    }

                    selectedIndex = index;
                }
            }
        }

        if (selectedIndex < 0)
        {
            throw new BamlProtocolException(
                "The native bridge selected an unknown BAML union option.",
                $"Selected option {selected} is not present in the generated descriptor.");
        }

        return new BamlGeneratedUnionValue(selectedIndex, required.ReadUnionPayload());
    }

    public BamlGeneratedValue Encode<T>(BamlGeneratedType<T> type, T value) =>
        registry.Encode(type, value, encodeBudget);

    public BamlGeneratedValue EncodeFresh<T>(BamlGeneratedType<T> type, T value) =>
        registry.Encode(type, value);

    public BamlGeneratedValue ForwardEncode<T>(BamlGeneratedType<T> type, T value) =>
        registry.EncodeForwarded(type, value, encodeBudget);

    public T Decode<T>(BamlGeneratedType<T> type, BamlGeneratedValue value) =>
        registry.Decode(type, value);

    public Func<
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>,
        CancellationToken,
        Task<BamlGeneratedValue>> NativeFunction(BamlGeneratedValue value)
    {
        global::Baml.BamlHandle handle = ReadHandle(value);
        if (handle.HandleType != BamlHandleType.FunctionRef)
        {
            throw new BamlProtocolException(
                "The native bridge returned an incompatible BAML handle.",
                $"Expected a function handle, received {handle.HandleType}.");
        }

        return async (arguments, cancellationToken) =>
        {
            ArgumentNullException.ThrowIfNull(arguments);
            Baml.Cffi.NativeApi api = Baml.Cffi.NativeApi.Instance;
            Task<byte[]> completion;
            using (Baml.Cffi.BamlSafeHandleLease lease = handle.Lease())
            {
                completion = api.InvokeOwnedHandleAsync(
                    lease.Key,
                    callId => Baml.Proto.PrimitiveProtocol.EncodeOwnedHandleArguments(
                        arguments,
                        callId,
                        api),
                    cancellationToken);
            }

            byte[] bytes = await completion.ConfigureAwait(false);
            return Baml.Proto.PrimitiveProtocol.DecodeCallResult(
                bytes,
                "<returned BAML closure>",
                api);
        };
    }

    private static IReadOnlyList<BamlGeneratedValue> SnapshotValues(
        IEnumerable<BamlGeneratedValue> values,
        string kind)
    {
        var snapshot = new List<BamlGeneratedValue>();
        foreach (BamlGeneratedValue value in values)
        {
            snapshot.Add(value ?? throw new ArgumentException(
                $"A generated {kind} contains a null carrier.",
                nameof(values)));
        }

        return snapshot.AsReadOnly();
    }

    private static IReadOnlyList<byte[]> SnapshotTypeMetadata(
        IReadOnlyList<byte[]> metadata,
        string description)
    {
        var snapshot = new List<byte[]>(metadata.Count);
        for (int index = 0; index < metadata.Count; index++)
        {
            byte[] item = metadata[index]
                ?? throw new ArgumentException(
                    $"Generated {description} {index} is null.",
                    nameof(metadata));
            if (item.Length == 0)
            {
                throw new ArgumentException(
                    $"Generated {description} {index} is empty.",
                    nameof(metadata));
            }

            snapshot.Add(item.ToArray());
        }

        return snapshot.AsReadOnly();
    }

    private static IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> SnapshotEntries(
        IEnumerable<KeyValuePair<string, BamlGeneratedValue>> entries,
        string kind)
    {
        var snapshot = new List<KeyValuePair<string, BamlGeneratedValue>>();
        var names = new HashSet<string>(StringComparer.Ordinal);
        foreach ((string key, BamlGeneratedValue value) in entries)
        {
            ArgumentNullException.ThrowIfNull(key);
            if (!names.Add(key))
            {
                throw new BamlProtocolException(
                    $"A generated {kind} contains a duplicate key.",
                    $"Duplicate {kind} key {key}.");
            }

            snapshot.Add(new(
                key,
                value ?? throw new ArgumentException(
                    $"A generated {kind} contains a null carrier.",
                    nameof(entries))));
        }

        return snapshot.AsReadOnly();
    }

    private static IReadOnlyDictionary<string, BamlGeneratedValue> ToReadOnlyDictionary(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> entries,
        string kind)
    {
        var result = new Dictionary<string, BamlGeneratedValue>(
            entries.Count,
            StringComparer.Ordinal);
        foreach ((string key, BamlGeneratedValue value) in entries)
        {
            if (!result.TryAdd(key, value))
            {
                throw new BamlProtocolException(
                    $"The native bridge returned a {kind} with a duplicate key.",
                    $"Duplicate {kind} key {key}.");
            }
        }

        return new ReadOnlyDictionary<string, BamlGeneratedValue>(result);
    }

    private static void RequireIdentity(string identity, string kind)
    {
        ArgumentException.ThrowIfNullOrEmpty(identity);
        if (identity.AsSpan().Trim().Length != identity.Length)
        {
            throw new ArgumentException(
                $"A generated BAML {kind} identity cannot have surrounding whitespace.",
                nameof(identity));
        }
    }

    private static BamlGeneratedValue Require(BamlGeneratedValue value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return value;
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public readonly struct BamlGeneratedUnionValue
{
    internal BamlGeneratedUnionValue(int caseIndex, BamlGeneratedValue value)
    {
        CaseIndex = caseIndex;
        Value = value;
    }

    public int CaseIndex { get; }

    public BamlGeneratedValue Value { get; }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedResource : IDisposable
{
    private BamlGeneratedValue? value;

    internal BamlGeneratedResource(BamlGeneratedValue value)
    {
        this.value = value ?? throw new ArgumentNullException(nameof(value));
    }

    internal BamlGeneratedValue Value =>
        Volatile.Read(ref value)
        ?? throw new ObjectDisposedException(nameof(BamlGeneratedResource));

    public bool IsClosed => Volatile.Read(ref value) is null;

    public BamlGeneratedResource Clone()
    {
        BamlGeneratedValue current = Value;
        var owned = new List<IDisposable>();
        try
        {
            return new BamlGeneratedResource(current.SnapshotForDeferredCall(owned));
        }
        catch
        {
            DeferredCallOwnership.DisposeAll(owned);
            throw;
        }
    }

    public void Dispose()
    {
        BamlGeneratedValue? current = Interlocked.Exchange(ref value, null);
        current?.DisposeOwnedHandles(new HashSet<global::Baml.BamlHandle>(
            ReferenceEqualityComparer.Instance));
    }
}

[EditorBrowsable(EditorBrowsableState.Never)]
public sealed class BamlGeneratedValue
{
    private readonly PrimitiveCarrierKind kind;
    private readonly bool boolValue;
    private readonly long intValue;
    private readonly double floatValue;
    private readonly string? stringValue;
    private readonly byte[]? bytesValue;
    private readonly BigInteger bigIntValue;
    private readonly IReadOnlyList<BamlGeneratedValue>? listValue;
    private readonly IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>? entriesValue;
    private readonly string? nominalIdentity;
    private readonly bool isDynamic;
    private readonly IReadOnlyList<byte[]>? typeArguments;
    private readonly byte[]? itemTypeMetadata;
    private readonly byte[]? keyTypeMetadata;
    private readonly byte[]? valueTypeMetadata;
    private readonly byte[]? unionSelfTypeMetadata;
    private readonly byte[]? unionSelectedTypeMetadata;
    private readonly BamlGeneratedValue? unionPayload;
    private readonly object? managedValue;
    private readonly byte[]? occurrenceTypeMetadata;
    private readonly string sourcePath;

    private BamlGeneratedValue(
        PrimitiveCarrierKind kind,
        bool boolValue = default,
        long intValue = default,
        double floatValue = default,
        string? stringValue = default,
        byte[]? bytesValue = default,
        BigInteger bigIntValue = default,
        IReadOnlyList<BamlGeneratedValue>? listValue = default,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>? entriesValue = default,
        string? nominalIdentity = default,
        bool isDynamic = default,
        IReadOnlyList<byte[]>? typeArguments = default,
        byte[]? itemTypeMetadata = default,
        byte[]? keyTypeMetadata = default,
        byte[]? valueTypeMetadata = default,
        byte[]? unionSelfTypeMetadata = default,
        byte[]? unionSelectedTypeMetadata = default,
        BamlGeneratedValue? unionPayload = default,
        object? managedValue = default,
        byte[]? occurrenceTypeMetadata = default,
        string sourcePath = "generated argument")
    {
        this.kind = kind;
        this.boolValue = boolValue;
        this.intValue = intValue;
        this.floatValue = floatValue;
        this.stringValue = stringValue;
        this.bytesValue = bytesValue;
        this.bigIntValue = bigIntValue;
        this.listValue = listValue;
        this.entriesValue = entriesValue;
        this.nominalIdentity = nominalIdentity;
        this.isDynamic = isDynamic;
        this.typeArguments = typeArguments;
        this.itemTypeMetadata = itemTypeMetadata;
        this.keyTypeMetadata = keyTypeMetadata;
        this.valueTypeMetadata = valueTypeMetadata;
        this.unionSelfTypeMetadata = unionSelfTypeMetadata;
        this.unionSelectedTypeMetadata = unionSelectedTypeMetadata;
        this.unionPayload = unionPayload;
        this.managedValue = managedValue;
        this.occurrenceTypeMetadata = occurrenceTypeMetadata;
        this.sourcePath = sourcePath;
    }

    public bool IsNull => kind == PrimitiveCarrierKind.Null;

    internal PrimitiveCarrierKind Kind => kind;

    internal byte[]? ItemTypeMetadata => itemTypeMetadata;

    internal byte[]? KeyTypeMetadata => keyTypeMetadata;

    internal byte[]? ValueTypeMetadata => valueTypeMetadata;

    internal byte[]? UnionSelfTypeMetadata => unionSelfTypeMetadata;

    internal byte[]? UnionSelectedTypeMetadata => unionSelectedTypeMetadata;

    internal byte[]? OccurrenceTypeMetadata => occurrenceTypeMetadata;

    internal static BamlGeneratedValue CreateNull() => new(PrimitiveCarrierKind.Null);

    internal static BamlGeneratedValue CreateNull(string sourcePath) =>
        new(PrimitiveCarrierKind.Null, sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateBool(bool value) =>
        new(PrimitiveCarrierKind.Bool, boolValue: value);

    internal static BamlGeneratedValue CreateBool(bool value, string sourcePath) =>
        new(PrimitiveCarrierKind.Bool, boolValue: value, sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateInt(long value) =>
        new(PrimitiveCarrierKind.Int, intValue: value);

    internal static BamlGeneratedValue CreateInt(long value, string sourcePath) =>
        new(PrimitiveCarrierKind.Int, intValue: value, sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateFloat(double value) =>
        new(PrimitiveCarrierKind.Float, floatValue: value);

    internal static BamlGeneratedValue CreateFloat(double value, string sourcePath) =>
        new(PrimitiveCarrierKind.Float, floatValue: value, sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateString(string value) =>
        new(PrimitiveCarrierKind.String, stringValue: value);

    internal static BamlGeneratedValue CreateString(string value, string sourcePath) =>
        new(PrimitiveCarrierKind.String, stringValue: value, sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateBytes(ReadOnlySpan<byte> value) =>
        new(PrimitiveCarrierKind.Bytes, bytesValue: value.ToArray());

    internal static BamlGeneratedValue CreateBytes(ReadOnlySpan<byte> value, string sourcePath) =>
        new(
            PrimitiveCarrierKind.Bytes,
            bytesValue: value.ToArray(),
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateBigInt(BigInteger value) =>
        new(PrimitiveCarrierKind.BigInt, bigIntValue: value);

    internal static BamlGeneratedValue CreateBigInt(BigInteger value, string sourcePath) =>
        new(
            PrimitiveCarrierKind.BigInt,
            bigIntValue: value,
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateList(
        IReadOnlyList<BamlGeneratedValue> values,
        ReadOnlySpan<byte> itemTypeMetadata = default,
        string sourcePath = "generated argument") =>
        new(
            PrimitiveCarrierKind.List,
            listValue: values,
            itemTypeMetadata: itemTypeMetadata.ToArray(),
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateMap(
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> entries,
        ReadOnlySpan<byte> keyTypeMetadata = default,
        ReadOnlySpan<byte> valueTypeMetadata = default,
        string sourcePath = "generated argument") =>
        new(
            PrimitiveCarrierKind.Map,
            entriesValue: entries,
            keyTypeMetadata: keyTypeMetadata.ToArray(),
            valueTypeMetadata: valueTypeMetadata.ToArray(),
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateClass(
        string identity,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> fields,
        IReadOnlyList<byte[]> typeArguments,
        string sourcePath = "generated argument") =>
        new(
            PrimitiveCarrierKind.Class,
            entriesValue: fields,
            nominalIdentity: identity,
            typeArguments: typeArguments,
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateEnum(
        string identity,
        string wireValue,
        bool isDynamic,
        string sourcePath = "generated argument") =>
        new(
            PrimitiveCarrierKind.Enum,
            stringValue: wireValue,
            nominalIdentity: identity,
            isDynamic: isDynamic,
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateUnion(
        ReadOnlySpan<byte> selfTypeMetadata,
        byte[]? selectedTypeMetadata,
        string optionName,
        BamlGeneratedValue payload,
        string sourcePath = "generated result") =>
        new(
            PrimitiveCarrierKind.Union,
            stringValue: optionName,
            unionSelfTypeMetadata: selfTypeMetadata.ToArray(),
            unionSelectedTypeMetadata: selectedTypeMetadata?.ToArray(),
            unionPayload: payload,
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateInboundUnion(
        ReadOnlySpan<byte> selfTypeMetadata,
        ReadOnlySpan<byte> selectedTypeMetadata,
        string optionName,
        BamlGeneratedValue payload,
        string sourcePath = "generated argument") =>
        new(
            PrimitiveCarrierKind.Union,
            stringValue: optionName,
            unionSelfTypeMetadata: selfTypeMetadata.ToArray(),
            unionSelectedTypeMetadata: selectedTypeMetadata.ToArray(),
            unionPayload: payload,
            sourcePath: sourcePath);

    internal static BamlGeneratedValue CreateMedia(
        object value,
        string sourcePath = "generated argument")
    {
        ArgumentNullException.ThrowIfNull(value);
        if (value is not global::Baml.BamlImage
            and not global::Baml.BamlAudio
            and not global::Baml.BamlVideo
            and not global::Baml.BamlPdf)
        {
            throw new ArgumentException(
                "A generated media carrier requires a canonical BAML media value.",
                nameof(value));
        }

        return new BamlGeneratedValue(
            PrimitiveCarrierKind.Media,
            managedValue: value,
            sourcePath: sourcePath);
    }

    internal static BamlGeneratedValue CreateHandle(
        global::Baml.BamlHandle value,
        string sourcePath = "generated argument") =>
        CreateHandle(value, null, sourcePath);

    internal static BamlGeneratedValue CreateHandle(
        global::Baml.BamlHandle value,
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>? publicFields,
        string sourcePath)
    {
        ArgumentNullException.ThrowIfNull(value);
        return new BamlGeneratedValue(
            PrimitiveCarrierKind.Handle,
            entriesValue: publicFields,
            managedValue: value,
            sourcePath: sourcePath);
    }

    internal static BamlGeneratedValue CreateHostCallable(
        BamlGeneratedHostCallable value,
        string sourcePath = "generated argument")
    {
        ArgumentNullException.ThrowIfNull(value);
        return new BamlGeneratedValue(
            PrimitiveCarrierKind.HostCallable,
            managedValue: value,
            sourcePath: sourcePath);
    }

    internal BamlGeneratedValue WithOccurrenceType(ReadOnlySpan<byte> metadata) =>
        new(
            kind,
            boolValue,
            intValue,
            floatValue,
            stringValue,
            bytesValue,
            bigIntValue,
            listValue,
            entriesValue,
            nominalIdentity,
            isDynamic,
            typeArguments,
            itemTypeMetadata,
            keyTypeMetadata,
            valueTypeMetadata,
            unionSelfTypeMetadata,
            unionSelectedTypeMetadata,
            unionPayload,
            managedValue,
            metadata.ToArray(),
            sourcePath);

    internal BamlGeneratedValue SnapshotForDeferredCall(
        ICollection<IDisposable> ownedResources)
    {
        ArgumentNullException.ThrowIfNull(ownedResources);
        object? deferredManagedValue = managedValue;
        if (kind == PrimitiveCarrierKind.Handle)
        {
            global::Baml.BamlHandle clone = ReadHandle().Clone();
            ownedResources.Add(clone);
            deferredManagedValue = clone;
        }

        IReadOnlyList<BamlGeneratedValue>? deferredList = listValue?
            .Select(item => item.SnapshotForDeferredCall(ownedResources))
            .ToArray();
        IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>>? deferredEntries =
            entriesValue?
                .Select(entry => new KeyValuePair<string, BamlGeneratedValue>(
                    entry.Key,
                    entry.Value.SnapshotForDeferredCall(ownedResources)))
                .ToArray();
        BamlGeneratedValue? deferredUnion =
            unionPayload?.SnapshotForDeferredCall(ownedResources);
        return new BamlGeneratedValue(
            kind,
            boolValue,
            intValue,
            floatValue,
            stringValue,
            bytesValue?.ToArray(),
            bigIntValue,
            deferredList,
            deferredEntries,
            nominalIdentity,
            isDynamic,
            typeArguments?.Select(argument => argument.ToArray()).ToArray(),
            itemTypeMetadata?.ToArray(),
            keyTypeMetadata?.ToArray(),
            valueTypeMetadata?.ToArray(),
            unionSelfTypeMetadata?.ToArray(),
            unionSelectedTypeMetadata?.ToArray(),
            deferredUnion,
            deferredManagedValue,
            occurrenceTypeMetadata?.ToArray(),
            sourcePath);
    }

    internal void DisposeOwnedHandles(HashSet<global::Baml.BamlHandle> disposed)
    {
        ArgumentNullException.ThrowIfNull(disposed);
        if (kind == PrimitiveCarrierKind.Handle
            && managedValue is global::Baml.BamlHandle handle
            && disposed.Add(handle))
        {
            handle.Dispose();
        }

        if (listValue is not null)
        {
            foreach (BamlGeneratedValue item in listValue)
            {
                item.DisposeOwnedHandles(disposed);
            }
        }

        if (entriesValue is not null)
        {
            foreach (KeyValuePair<string, BamlGeneratedValue> entry in entriesValue)
            {
                entry.Value.DisposeOwnedHandles(disposed);
            }
        }

        unionPayload?.DisposeOwnedHandles(disposed);
    }

    internal BamlGeneratedValue WithDeclaredType(ReadOnlySpan<byte> metadata)
    {
        byte[] snapshot = metadata.ToArray();
        BamlTy declared;
        try
        {
            declared = BamlTy.Parser.ParseFrom(snapshot);
        }
        catch (Google.Protobuf.InvalidProtocolBufferException error)
        {
            throw new global::Baml.BamlProtocolException(
                "Generated BAML type metadata is malformed.",
                error.Message);
        }

        BamlTy structural = declared;
        while (structural.TyCase == BamlTy.TyOneofCase.Optional)
        {
            structural = structural.Optional.Inner;
        }

        byte[]? declaredItem = itemTypeMetadata;
        byte[]? declaredKey = keyTypeMetadata;
        byte[]? declaredValue = valueTypeMetadata;
        IReadOnlyList<byte[]> declaredArguments = typeArguments ?? Array.Empty<byte[]>();
        if (kind == PrimitiveCarrierKind.List
            && structural.TyCase == BamlTy.TyOneofCase.List)
        {
            declaredItem = structural.List.Item.ToByteArray();
        }
        else if (kind == PrimitiveCarrierKind.Map
            && structural.TyCase == BamlTy.TyOneofCase.Map)
        {
            declaredKey = structural.Map.Key.ToByteArray();
            declaredValue = structural.Map.Value.ToByteArray();
        }
        else if (kind == PrimitiveCarrierKind.Class
            && structural.TyCase == BamlTy.TyOneofCase.ClassTy)
        {
            declaredArguments = Array.AsReadOnly(
                structural.ClassTy.TypeArgs.Select(item => item.ToByteArray()).ToArray());
        }

        return new BamlGeneratedValue(
            kind,
            boolValue,
            intValue,
            floatValue,
            stringValue,
            bytesValue,
            bigIntValue,
            listValue,
            entriesValue,
            nominalIdentity,
            isDynamic,
            declaredArguments,
            declaredItem,
            declaredKey,
            declaredValue,
            unionSelfTypeMetadata,
            unionSelectedTypeMetadata,
            unionPayload,
            managedValue,
            snapshot,
            sourcePath);
    }

    internal bool ReadBool()
    {
        RequireKind(PrimitiveCarrierKind.Bool);
        return boolValue;
    }

    internal long ReadInt()
    {
        RequireKind(PrimitiveCarrierKind.Int);
        return intValue;
    }

    internal double ReadFloat()
    {
        RequireKind(PrimitiveCarrierKind.Float);
        return floatValue;
    }

    internal string ReadString()
    {
        RequireKind(PrimitiveCarrierKind.String);
        return stringValue!;
    }

    internal byte[] ReadBytes()
    {
        RequireKind(PrimitiveCarrierKind.Bytes);
        return bytesValue!.ToArray();
    }

    internal BigInteger ReadBigInt()
    {
        RequireKind(PrimitiveCarrierKind.BigInt);
        return bigIntValue;
    }

    internal IReadOnlyList<BamlGeneratedValue> ReadList()
    {
        RequireKind(PrimitiveCarrierKind.List);
        return listValue!;
    }

    internal IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> ReadMapEntries()
    {
        RequireKind(PrimitiveCarrierKind.Map);
        return entriesValue!;
    }

    internal IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> ReadClassFields()
    {
        RequireKind(PrimitiveCarrierKind.Class);
        return entriesValue!;
    }

    internal string ReadClassIdentityForEncode()
    {
        RequireKind(PrimitiveCarrierKind.Class);
        return nominalIdentity!;
    }

    internal string ReadClassIdentity()
    {
        RequireKind(PrimitiveCarrierKind.Class);
        return nominalIdentity!;
    }

    internal IReadOnlyList<byte[]> ReadClassTypeArgumentsForEncode()
    {
        RequireKind(PrimitiveCarrierKind.Class);
        return typeArguments!;
    }

    internal IReadOnlyList<byte[]> ReadClassTypeArguments()
    {
        RequireKind(PrimitiveCarrierKind.Class);
        return typeArguments!;
    }

    internal string ReadEnumIdentityForEncode()
    {
        RequireKind(PrimitiveCarrierKind.Enum);
        return nominalIdentity!;
    }

    internal string ReadEnumIdentity()
    {
        RequireKind(PrimitiveCarrierKind.Enum);
        return nominalIdentity!;
    }

    internal string ReadEnumWireValue()
    {
        RequireKind(PrimitiveCarrierKind.Enum);
        return stringValue!;
    }

    internal bool IsDynamicEnum
    {
        get
        {
            RequireKind(PrimitiveCarrierKind.Enum);
            return isDynamic;
        }
    }

    internal string ReadEnumWireValueForEncode()
    {
        RequireKind(PrimitiveCarrierKind.Enum);
        if (isDynamic)
        {
            throw new BamlProtocolException(
                "A generated static enum codec produced a dynamic enum carrier.",
                $"Enum {nominalIdentity} value {stringValue} was marked dynamic.");
        }

        return stringValue!;
    }

    internal void RequireClass(
        string expectedIdentity,
        IReadOnlyList<byte[]> expectedTypeArguments)
    {
        RequireKind(PrimitiveCarrierKind.Class);
        if (!StringComparer.Ordinal.Equals(nominalIdentity, expectedIdentity))
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML class.",
                $"Expected class {expectedIdentity}, received {nominalIdentity} at {sourcePath}.");
        }

        if (typeArguments!.Count != expectedTypeArguments.Count)
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong generic class arity.",
                $"Class {expectedIdentity} at {sourcePath} expected {expectedTypeArguments.Count} type argument(s), received {typeArguments.Count}.");
        }

        for (int index = 0; index < typeArguments.Count; index++)
        {
            RequireTypeMetadata(
                expectedTypeArguments[index],
                typeArguments[index],
                $"class type argument {index}");
        }
    }

    internal string ReadEnum(string expectedIdentity)
    {
        RequireKind(PrimitiveCarrierKind.Enum);
        if (!StringComparer.Ordinal.Equals(nominalIdentity, expectedIdentity))
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML enum.",
                $"Expected enum {expectedIdentity}, received {nominalIdentity} at {sourcePath}.");
        }

        if (isDynamic)
        {
            throw new BamlProtocolException(
                "The native bridge returned a dynamic enum value for a static generated enum.",
                $"Enum {expectedIdentity} value {stringValue} at {sourcePath} was marked dynamic.");
        }

        return stringValue!;
    }

    internal string ReadUnionOptionName()
    {
        RequireKind(PrimitiveCarrierKind.Union);
        return stringValue!;
    }

    internal BamlGeneratedValue ReadUnionPayload()
    {
        RequireKind(PrimitiveCarrierKind.Union);
        return unionPayload!;
    }

    internal T ReadMedia<T>()
        where T : class
    {
        RequireKind(PrimitiveCarrierKind.Media);
        if (managedValue is not T typed)
        {
            throw new BamlProtocolException(
                "The native bridge returned the wrong BAML media kind.",
                $"Expected managed media {typeof(T)}, received {managedValue?.GetType()} at {sourcePath}.");
        }

        return typed;
    }

    internal object ReadMedia()
    {
        RequireKind(PrimitiveCarrierKind.Media);
        return managedValue!;
    }

    internal global::Baml.BamlHandle ReadHandle()
    {
        RequireKind(PrimitiveCarrierKind.Handle);
        return (global::Baml.BamlHandle)managedValue!;
    }

    internal IReadOnlyList<KeyValuePair<string, BamlGeneratedValue>> ReadHandleFields()
    {
        RequireKind(PrimitiveCarrierKind.Handle);
        return entriesValue ?? Array.Empty<KeyValuePair<string, BamlGeneratedValue>>();
    }

    internal BamlGeneratedHostCallable ReadHostCallable()
    {
        RequireKind(PrimitiveCarrierKind.HostCallable);
        return (BamlGeneratedHostCallable)managedValue!;
    }

    internal string? ReadNominalIdentity() => kind switch
    {
        PrimitiveCarrierKind.Class or PrimitiveCarrierKind.Enum => nominalIdentity,
        PrimitiveCarrierKind.Union => unionPayload!.ReadNominalIdentity(),
        _ => null,
    };

    internal void RequireTypeMetadata(
        ReadOnlySpan<byte> expected,
        byte[]? actual,
        string description)
    {
        if (expected.IsEmpty)
        {
            throw new ArgumentException(
                $"The generated {description} type metadata is empty.",
                nameof(expected));
        }

        if (actual is null || !expected.SequenceEqual(actual))
        {
            throw new BamlProtocolException(
                $"The native bridge returned contradictory {description} type metadata.",
                $"Expected {Convert.ToHexString(expected)}, received "
                    + (actual is null ? "<missing>" : Convert.ToHexString(actual))
                    + $" at {sourcePath}.");
        }
    }

    internal void RequireUnionTypeMetadata(
        ReadOnlySpan<byte> expected,
        byte[]? actual,
        string description)
    {
        if (expected.IsEmpty)
        {
            throw new ArgumentException(
                $"The generated {description} type metadata is empty.",
                nameof(expected));
        }

        if (actual is not null && expected.SequenceEqual(actual))
        {
            return;
        }

        try
        {
            BamlTy expectedType = BamlTy.Parser.ParseFrom(expected);
            BamlTy? actualType = actual is null
                ? null
                : BamlTy.Parser.ParseFrom(actual);
            if (actualType is not null
                && UnionOptionsEqualIgnoringOrder(expectedType, actualType))
            {
                return;
            }
        }
        catch (InvalidProtocolBufferException)
        {
            // Preserve the protocol diagnostic below.
        }

        throw new BamlProtocolException(
            $"The native bridge returned contradictory {description} type metadata.",
            $"Expected {Convert.ToHexString(expected)}, received "
                + (actual is null ? "<missing>" : Convert.ToHexString(actual))
                + $" at {sourcePath}.");
    }

    private static bool UnionOptionsEqualIgnoringOrder(BamlTy expected, BamlTy actual)
    {
        if (expected.TyCase != BamlTy.TyOneofCase.Union
            || actual.TyCase != BamlTy.TyOneofCase.Union
            || expected.Union.Options.Count != actual.Union.Options.Count)
        {
            return false;
        }

        var unmatched = actual.Union.Options
            .Select(option => option.ToByteArray())
            .ToList();
        foreach (BamlTy option in expected.Union.Options)
        {
            byte[] expectedBytes = option.ToByteArray();
            int match = unmatched.FindIndex(candidate =>
                candidate.AsSpan().SequenceEqual(expectedBytes));
            if (match < 0)
            {
                return false;
            }

            unmatched.RemoveAt(match);
        }

        return unmatched.Count == 0;
    }

    internal void RequireCollectionTypeMetadata(
        ReadOnlySpan<byte> expected,
        byte[]? actual,
        string description,
        bool allowCanaryUnknownFallback)
    {
        if (allowCanaryUnknownFallback && actual is { Length: > 0 })
        {
            try
            {
                if (BamlTy.Parser.ParseFrom(actual).TyCase == BamlTy.TyOneofCase.Unknown)
                {
                    return;
                }
            }
            catch (InvalidProtocolBufferException)
            {
                // Preserve the exact metadata diagnostic below.
            }
        }

        RequireTypeMetadata(expected, actual, description);
    }

    internal void RequireKind(PrimitiveCarrierKind expected)
    {
        if (kind != expected)
        {
            throw new BamlProtocolException(
                "The native bridge returned a value with the wrong BAML type.",
                $"Expected generated carrier {expected}, received {kind} at {sourcePath}.");
        }
    }
}

internal enum PrimitiveCarrierKind
{
    Null,
    Bool,
    Int,
    Float,
    String,
    Bytes,
    BigInt,
    List,
    Map,
    Class,
    Enum,
    Union,
    Media,
    Handle,
    HostCallable,
}
