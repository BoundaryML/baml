using System.Collections.ObjectModel;
using System.Numerics;
using Baml;
using CsharpPhase5;

var parcel = new Parcel
{
    Label = "parcel-1",
    Count = 3,
    Status = DeliveryStatus.Pending,
    Tags = Array.AsReadOnly(["priority", "fragile"]),
    Scores = new ReadOnlyDictionary<string, long>(
        new Dictionary<string, long>
        {
            ["quality"] = 9,
            ["speed"] = 7,
        }),
};

Parcel synchronousParcel = Functions.RoundtripParcel(parcel);
RequireParcel(synchronousParcel);
Parcel asynchronousParcel = await Functions.RoundtripParcelAsync(parcel);
RequireParcel(asynchronousParcel);

BamlUnion<long, string> number =
    BamlUnion<long, string>.FromT0(42);
BamlUnion<long, string> numberResult = Functions.RoundtripUnion(number);
if (!numberResult.IsT0 || numberResult.AsT0 != 42)
{
    throw new InvalidOperationException("numeric union case changed");
}

BamlUnion<long, string> text =
    BamlUnion<long, string>.FromT1("typed");
BamlUnion<long, string> textResult = await Functions.RoundtripUnionAsync(text);
if (!textResult.IsT1 || textResult.AsT1 != "typed")
{
    throw new InvalidOperationException("string union case changed");
}

BamlUnion<string, string> literalCase =
    BamlUnion<string, string>.FromT0("fixed");
BamlUnion<string, string> literalCaseResult =
    Functions.RoundtripDuplicateProjection(literalCase);
if (!literalCaseResult.IsT0 || literalCaseResult.AsT0 != "fixed")
{
    throw new InvalidOperationException("duplicate projection T0 changed");
}

BamlUnion<string, string> aliasCase =
    BamlUnion<string, string>.FromT1("alias-only");
BamlUnion<string, string> aliasCaseResult =
    Functions.RoundtripDuplicateProjection(aliasCase);
if (!aliasCaseResult.IsT1 || aliasCaseResult.AsT1 != "alias-only")
{
    throw new InvalidOperationException("duplicate projection T1 collapsed into T0");
}

BamlUnion<string, string> overlappingAliasCase =
    BamlUnion<string, string>.FromT1("fixed");
BamlUnion<string, string> overlappingAliasResult =
    Functions.RoundtripDuplicateProjection(overlappingAliasCase);
if (!overlappingAliasResult.IsT0 || overlappingAliasResult.AsT0 != "fixed")
{
    throw new InvalidOperationException(
        "overlapping alias value did not select the exact literal result arm");
}

string literal = Functions.RoundtripLiteral("fixed");
if (literal != "fixed")
{
    throw new InvalidOperationException("literal roundtrip changed");
}
Expect<BamlProtocolException>(() => Functions.RoundtripLiteral("wrong"));

BigInteger huge = BigInteger.Parse(
    "1234567890123456789012345678901234567890",
    System.Globalization.CultureInfo.InvariantCulture);
if (Functions.RoundtripBigint(-huge) != -huge)
{
    throw new InvalidOperationException("bigint roundtrip changed");
}

IReadOnlyDictionary<string, string> statuses =
    new ReadOnlyDictionary<string, string>(
        new Dictionary<string, string>(StringComparer.Ordinal)
        {
            ["pending"] = "queued",
            ["delivered"] = "done",
        });
IReadOnlyDictionary<string, string> statusResult =
    Functions.RoundtripStatusMap(statuses);
if (statusResult.Count != 2
    || statusResult["pending"] != "queued"
    || statusResult["delivered"] != "done")
{
    throw new InvalidOperationException("string-key map roundtrip changed");
}
Expect<NotSupportedException>(() =>
    ((IDictionary<string, string>)statusResult).Add("pending", "mutated"));

IReadOnlyList<string> defaults = Functions.DefaultedValues("required");
RequireSequence(
    defaults,
    ["required", "engine-default", "<null>"],
    "omitted defaults");
IReadOnlyList<string> supplied = Functions.DefaultedValues(
    "required",
    "managed",
    BamlOptional<string?>.FromValue(null));
RequireSequence(
    supplied,
    ["required", "managed", "<null>"],
    "supplied default and explicit null");
IReadOnlyList<string> suppliedNullable = await Functions.DefaultedValuesAsync(
    "required",
    nullable: "present");
RequireSequence(
    suppliedNullable,
    ["required", "engine-default", "present"],
    "omitted middle default and supplied nullable");

Console.WriteLine("csharp_phase5_slice=ok");

static void RequireParcel(Parcel value)
{
    if (value.Label != "parcel-1"
        || value.Count != 3
        || value.Status != DeliveryStatus.Pending
        || !value.Tags.SequenceEqual(["priority", "fragile"])
        || value.Scores.Count != 2
        || value.Scores["quality"] != 9
        || value.Scores["speed"] != 7)
    {
        throw new InvalidOperationException("class roundtrip changed");
    }

    Expect<NotSupportedException>(() =>
        ((IList<string>)value.Tags).Add("mutated"));
    Expect<NotSupportedException>(() =>
        ((IDictionary<string, long>)value.Scores).Add("mutated", 1));
}

static void RequireSequence(
    IReadOnlyList<string> actual,
    IReadOnlyList<string> expected,
    string description)
{
    if (!actual.SequenceEqual(expected))
    {
        throw new InvalidOperationException(
            $"{description} changed: {string.Join(",", actual)}");
    }
}

static void Expect<TException>(Action action)
    where TException : Exception
{
    try
    {
        action();
    }
    catch (TException)
    {
        return;
    }

    throw new InvalidOperationException($"expected {typeof(TException).Name}");
}
