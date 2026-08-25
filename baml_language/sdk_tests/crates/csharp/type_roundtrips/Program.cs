using System.Collections.ObjectModel;
using System.Numerics;
using Baml;
using CsharpTypeRoundtrips;

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

BamlUnion<long, string> literalUnionNumber =
    BamlUnion<long, string>.FromT0(7L);
BamlUnion<long, string> literalUnionNumberResult =
    Functions.RoundtripLiteralUnion(literalUnionNumber);
if (!literalUnionNumberResult.IsT0 || literalUnionNumberResult.AsT0 != 7L)
{
    throw new InvalidOperationException("literal union numeric arm changed");
}

BamlUnion<long, string> literalUnionString =
    BamlUnion<long, string>.FromT1("fixed");
BamlUnion<long, string> literalUnionStringResult =
    Functions.RoundtripLiteralUnion(literalUnionString);
if (!literalUnionStringResult.IsT1 || literalUnionStringResult.AsT1 != "fixed")
{
    throw new InvalidOperationException("literal union literal arm changed");
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

Console.WriteLine("csharp_type_roundtrips=ok");

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
