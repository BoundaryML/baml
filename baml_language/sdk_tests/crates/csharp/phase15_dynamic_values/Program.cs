using System.Numerics;

using Baml;
using CsharpPhase15;

var failures = new List<string>();

// Force registration before constructing context-free generated nominal values.
CheckParity("null", BamlValue.Null);
CheckParity("bool", BamlValue.Bool(true));
CheckParity("int", BamlValue.Int(-17));
CheckParity("bigint", BamlValue.BigInt((BigInteger.One << 255) + 0x80));
CheckParity("float", BamlValue.Float(-123.5));
CheckParity("string", BamlValue.String("text\0雪"));

byte[] byteSource = [0x00, 0x7f, 0x80, 0xff];
BamlValue bytes = BamlValue.Bytes(byteSource);
byteSource.AsSpan().Fill(0xcc);
CheckParity("bytes snapshot", bytes);
Check(
    bytes.As<ReadOnlyMemory<byte>>().Span.SequenceEqual(new byte[] { 0x00, 0x7f, 0x80, 0xff }),
    "bytes did not own their input snapshot");

var listSource = new List<BamlValue> { BamlValue.Int(1), BamlValue.String("two") };
BamlValue heterogeneousList = BamlValue.List(listSource);
listSource.Clear();
CheckDescriptor(
    "heterogeneous list input",
    heterogeneousList.Type,
    BamlTypeDescriptorKind.List,
    [BamlTypeDescriptorKind.Unknown]);
CheckListParity("heterogeneous list snapshot", heterogeneousList);

BamlValue emptyList = BamlValue.List([]);
CheckDescriptor(
    "empty list input",
    emptyList.Type,
    BamlTypeDescriptorKind.List,
    [BamlTypeDescriptorKind.Unknown]);
CheckListParity("empty list", emptyList);

var mapSource = new Dictionary<string, BamlValue>
{
    ["z"] = BamlValue.Bool(false),
    ["a"] = BamlValue.BigInt(BigInteger.Parse("123456789012345678901234567890")),
};
BamlValue heterogeneousMap = BamlValue.Map(mapSource);
mapSource.Clear();
CheckDescriptor(
    "heterogeneous map input",
    heterogeneousMap.Type,
    BamlTypeDescriptorKind.Map,
    [BamlTypeDescriptorKind.String, BamlTypeDescriptorKind.Unknown]);
CheckMapParity("heterogeneous map snapshot", heterogeneousMap);

BamlValue emptyMap = BamlValue.Map([]);
CheckDescriptor(
    "empty map input",
    emptyMap.Type,
    BamlTypeDescriptorKind.Map,
    [BamlTypeDescriptorKind.String, BamlTypeDescriptorKind.Unknown]);
CheckMapParity("empty map", emptyMap);

BamlValue enumValue = BamlValue.From(DynamicColor.Blue);
CheckNominal(
    "enum input",
    enumValue,
    BamlValueKind.Enum,
    BamlTypeDescriptorKind.Enum,
    "user.csharp_phase15.DynamicColor",
    []);
Check(enumValue.TryGetEnumVariant(out string? variant) && variant == "blue", "enum wire variant changed");
CheckParity("enum", enumValue, value =>
    Check(value.As<DynamicColor>() == DynamicColor.Blue, "enum generated projection changed"));

var record = new DynamicRecord { Label = "record", Count = 23 };
BamlValue classValue = BamlValue.From(record);
CheckNominal(
    "class input",
    classValue,
    BamlValueKind.Class,
    BamlTypeDescriptorKind.Class,
    "user.csharp_phase15.DynamicRecord",
    []);
Check(classValue.TryGetClassFields(out var recordFields) && recordFields.Count == 2, "class fields changed");
CheckParity("class", classValue, value =>
{
    DynamicRecord restored = value.As<DynamicRecord>();
    Check(restored.Label == "record" && restored.Count == 23, "class generated projection changed");
});

var boxed = new DynamicBox<long> { Value = 41 };
DynamicBox<long> echoedBox = Functions.EchoBox(boxed);
Check(echoedBox.Value == 41, "closed generic function codec changed");
BamlValue genericClass = BamlValue.From(boxed);
CheckNominal(
    "generic class input",
    genericClass,
    BamlValueKind.Class,
    BamlTypeDescriptorKind.Class,
    "user.csharp_phase15.DynamicBox",
    [BamlTypeDescriptorKind.Int]);
CheckParity("generic class", genericClass, value =>
    Check(value.As<DynamicBox<long>>().Value == 41, "generic class projection changed"));

var envelope = new DynamicEnvelope
{
    Color = DynamicColor.Red,
    Record = record,
    Boxed = boxed,
    Choice = BamlUnion<long, string>.FromT0(7L),
};
BamlValue mixedClosure = BamlValue.From(envelope);
CheckNominal(
    "mixed closure input",
    mixedClosure,
    BamlValueKind.Class,
    BamlTypeDescriptorKind.Class,
    "user.csharp_phase15.DynamicEnvelope",
    []);
Check(
    mixedClosure.TryGetClassFields(out var mixedFields)
        && mixedFields.Single(field => field.Key == "color").Value.Kind == BamlValueKind.Enum
        && mixedFields.Single(field => field.Key == "record").Value.Type.Fqn
            == "user.csharp_phase15.DynamicRecord"
        && mixedFields.Single(field => field.Key == "boxed").Value.Type.Arguments.Count == 1
        && mixedFields.Single(field => field.Key == "choice").Value.Kind == BamlValueKind.Union,
    "mixed closure did not coexist as enum + nominal + closed generic + union");
CheckParity("mixed closure", mixedClosure, value =>
{
    DynamicEnvelope restored = value.As<DynamicEnvelope>();
    Check(
        restored.Color == DynamicColor.Red
            && restored.Record.Label == "record"
            && restored.Boxed.Value == 41
            && restored.Choice.IsT0
            && restored.Choice.AsT0 == 7L,
        "mixed closure generated projection changed");
});

CheckUnionParity(
    "literal union",
    BamlUnion<long, string>.FromT1("fixed"),
    expectedActiveDescriptorCase: 1,
    expectedInteger: null,
    expectedLiteral: "fixed");
CheckUnionParity(
    "integer union",
    BamlUnion<long, string>.FromT0(29L),
    expectedActiveDescriptorCase: 0,
    expectedInteger: 29L,
    expectedLiteral: null);

byte[] mediaSource = [0x89, 0x50, 0x4e, 0x47];
BamlImage image = BamlImage.FromBytes(mediaSource, "image/png");
BamlValue mediaValue = BamlValue.From(image);
mediaSource.AsSpan().Fill(0x00);
Check(mediaValue.Kind == BamlValueKind.Media, "image did not map to dynamic media");
CheckParity("image media", mediaValue, value =>
    Check(value.As<BamlImage>().Equals(image), "image dynamic projection changed"));

int probeCalls = 0;
Func<CancellationToken, Task<long>> probe = _ =>
{
    Interlocked.Increment(ref probeCalls);
    return Task.FromResult(1L);
};
Expect<BamlTypeMismatchException>(
    "wrong-kind projection",
    () => Functions.TouchStringBeforeDispatch(probe, BamlValue.Int(1).As<string>()));
Check(probeCalls == 0, "wrong-kind projection reached native dispatch");
Expect<BamlTypeMappingException>(
    "unsupported CLR kind",
    () => Functions.TouchBeforeDispatch(probe, BamlValue.From(42)));
Check(probeCalls == 0, "unsupported CLR kind reached native dispatch");
var cyclicNode = new DynamicNode { Name = "cycle", Next = null };
typeof(DynamicNode).GetProperty(nameof(DynamicNode.Next))!.SetValue(cyclicNode, cyclicNode);
Expect<BamlTypeMappingException>(
    "dynamic reference cycle",
    () => Functions.TouchBeforeDispatch(probe, BamlValue.From(cyclicNode)));
Check(probeCalls == 0, "dynamic reference cycle reached native dispatch");
Expect<BamlTypeMappingException>(
    "dynamic depth limit",
    () => Functions.TouchBeforeDispatch(probe, TooDeep()));
Check(probeCalls == 0, "dynamic depth limit reached native dispatch");

if (failures.Count != 0)
{
    throw new InvalidOperationException(
        "Phase15 dynamic-value parity failures:\n- " + string.Join("\n- ", failures));
}

Console.WriteLine("csharp_phase15_dynamic_values=ok");
return 0;

void CheckParity(string label, BamlValue input, Action<BamlValue>? inspect = null)
{
    try
    {
        BamlValue output = Functions.EchoUnknown(input);
        Check(
            output.Equals(input),
            $"{label} changed from {Describe(input)} to {Describe(output)}");
        inspect?.Invoke(output);
    }
    catch (Exception error)
    {
        failures.Add($"{label} threw {error.GetType().Name}: {error.Message}");
    }
}

void CheckListParity(string label, BamlValue input)
{
    try
    {
        IReadOnlyList<BamlValue> echoed = Functions.EchoUnknownList(
            input.As<IReadOnlyList<BamlValue>>());
        BamlValue output = BamlValue.List(echoed);
        Check(
            output.Equals(input),
            $"{label} changed from {Describe(input)} to {Describe(output)}");
    }
    catch (Exception error)
    {
        failures.Add($"{label} threw {error.GetType().Name}: {error.Message}");
    }
}

void CheckMapParity(string label, BamlValue input)
{
    try
    {
        IReadOnlyDictionary<string, BamlValue> echoed = Functions.EchoUnknownMap(
            input.As<IReadOnlyDictionary<string, BamlValue>>());
        BamlValue output = BamlValue.Map(echoed);
        Check(
            output.Equals(input),
            $"{label} changed from {Describe(input)} to {Describe(output)}");
    }
    catch (Exception error)
    {
        failures.Add($"{label} threw {error.GetType().Name}: {error.Message}");
    }
}

void CheckUnionParity(
    string label,
    BamlUnion<long, string> occurrence,
    int expectedActiveDescriptorCase,
    long? expectedInteger,
    string? expectedLiteral)
{
    BamlValue container = BamlValue.From(new DynamicChoice { Value = occurrence });
    if (!container.TryGetClassFields(out var fields))
    {
        failures.Add($"{label} setup did not encode a class");
        return;
    }

    BamlValue union = fields.Single(field => field.Key == "value").Value;
    Check(union.Kind == BamlValueKind.Union, $"{label} setup kind was {union.Kind}");
    Check(
        union.Type.Kind == BamlTypeDescriptorKind.Union
            && union.Type.Arguments.Count == 2
            && union.Type.Arguments[0].Kind == BamlTypeDescriptorKind.Int
            && union.Type.Arguments[0].Literal is null
            && union.Type.Arguments[1].Kind == BamlTypeDescriptorKind.String
            && union.Type.Arguments[1].Literal == "fixed",
        $"{label} setup descriptor was {DescribeType(union.Type)}");
    Check(
        union.TryGetUnion(out int activeCase, out BamlValue? selected)
            && activeCase == expectedActiveDescriptorCase
            && SelectedOccurrenceMatches(selected, expectedInteger, expectedLiteral),
        $"{label} setup occurrence identity changed");

    DynamicChoice echoed = Functions.EchoChoice(new DynamicChoice { Value = occurrence });
    BamlValue echoedContainer = BamlValue.From(echoed);
    if (!echoedContainer.TryGetClassFields(out var echoedFields))
    {
        failures.Add($"{label} output did not encode a class");
        return;
    }

    BamlValue output = echoedFields.Single(field => field.Key == "value").Value;
    Check(output.Equals(union), $"{label} changed from {Describe(union)} to {Describe(output)}");
    Check(
        output.TryGetUnion(out int outputCase, out BamlValue? outputSelected)
            && outputCase == expectedActiveDescriptorCase
            && SelectedOccurrenceMatches(outputSelected, expectedInteger, expectedLiteral),
        $"{label} selected occurrence changed to {Describe(output)}");
}

static bool SelectedOccurrenceMatches(
    BamlValue selected,
    long? expectedInteger,
    string? expectedLiteral) => expectedInteger is long integer
        ? selected.Kind == BamlValueKind.Int
            && selected.Type.Literal is null
            && selected.As<long>() == integer
        : selected.Kind == BamlValueKind.String
            && selected.Type.Literal == expectedLiteral;

void CheckNominal(
    string label,
    BamlValue value,
    BamlValueKind valueKind,
    BamlTypeDescriptorKind typeKind,
    string fqn,
    IReadOnlyList<BamlTypeDescriptorKind> arguments) => Check(
    value.Kind == valueKind
        && value.Type.Kind == typeKind
        && value.Type.Fqn == fqn
        && value.Type.Arguments.Select(argument => argument.Kind).SequenceEqual(arguments),
    $"{label} descriptor was {Describe(value)}");

void CheckDescriptor(
    string label,
    BamlTypeDescriptor descriptor,
    BamlTypeDescriptorKind kind,
    IReadOnlyList<BamlTypeDescriptorKind> arguments) => Check(
    descriptor.Kind == kind
        && descriptor.Arguments.Select(argument => argument.Kind).SequenceEqual(arguments),
    $"{label} descriptor was {DescribeType(descriptor)}");

void Expect<TException>(string label, Action action)
    where TException : Exception
{
    try
    {
        action();
        failures.Add($"{label} did not throw {typeof(TException).Name}");
    }
    catch (TException)
    {
    }
    catch (Exception error)
    {
        failures.Add($"{label} threw {error.GetType().Name} instead of {typeof(TException).Name}");
    }
}

BamlValue TooDeep()
{
    BamlValue nested = BamlValue.Null;
    for (int depth = 0; depth < 66; depth++)
    {
        nested = BamlValue.List([nested]);
    }

    return nested;
}

void Check(bool condition, string failure)
{
    if (!condition)
    {
        failures.Add(failure);
    }
}

static string Describe(BamlValue value) => $"{value.Kind}/{DescribeType(value.Type)}";

static string DescribeType(BamlTypeDescriptor type)
{
    string identity = type.Fqn is null ? type.Kind.ToString() : $"{type.Kind}({type.Fqn})";
    if (type.Alias is not null)
    {
        identity += $" alias={type.Alias}";
    }
    if (type.Literal is not null)
    {
        identity += $" literal={type.Literal}";
    }
    if (type.IsNullable)
    {
        identity += "?";
    }
    return type.Arguments.Count == 0
        ? identity
        : $"{identity}<{string.Join(",", type.Arguments.Select(DescribeType))}>";
}
