using System.Collections.ObjectModel;
using Baml;
using Baml.Generated;
using CsharpTuples;

// Native tuples: sync and async calls, slot order, singletons, nesting.
Require(Functions.EchoPair((1L, "one")) == (1L, "one"), "sync native pair");
Require(await Functions.EchoPairAsync((2L, "two")) == (2L, "two"), "async native pair");
Require(Functions.SwapPair((3L, "three")) == ("three", 3L), "tuple slot order");
ValueTuple<long> single = Functions.EchoSingle(new ValueTuple<long>(4L));
Require(single.Item1 == 4L, "singleton tuple");
Require(
    Functions.EchoNested(((5L, "five"), true)) == ((5L, "five"), true),
    "nested native tuple");

// Generated records: arity above the native limit, literal and
// nullable-reference slots.
var wide = new Tuple8_IntIntIntIntIntIntIntInt(1, 2, 3, 4, 5, 6, 7, 8);
Require(Functions.EchoWide(wide) == wide, "wide tuple record");
Require(
    await Functions.EchoWideAsync(wide) == wide,
    "async wide tuple record");
Require(
    Functions.EchoLiteral(new Tuple2_KTagInt("tag", 9L)) == new Tuple2_KTagInt("tag", 9L),
    "literal-slot tuple record");
ExpectAny(() => Functions.EchoLiteral(new Tuple2_KTagInt("wrong", 9L)), "wrong literal slot");
Tuple2_OptionalStringInt optional = Functions.EchoOptionalSlot(
    new Tuple2_OptionalStringInt(null, 10L));
Require(optional.Item1 is null && optional.Item2 == 10L, "nullable-reference slot");
Expect<BamlProtocolException>(
    () => Functions.EchoWide(null!),
    "a null tuple record");

// Nested in classes, lists, optionals, maps, and methods.
var holder = new TupleHolder
{
    Pair = (11L, "eleven"),
    Pairs = Array.AsReadOnly(new[] { ("a", 1L), ("b", 2L) }),
    Maybe = null,
    Wide = wide,
};
TupleHolder holderResult = Functions.EchoHolder(holder);
Require(
    holderResult.Pair == (11L, "eleven")
        && holderResult.Pairs.SequenceEqual(new[] { ("a", 1L), ("b", 2L) })
        && holderResult.Maybe is null
        && holderResult.Wide == wide,
    "tuples nested in a class");
holderResult = Functions.EchoHolder(new TupleHolder
{
    Pair = holder.Pair,
    Pairs = holder.Pairs,
    Maybe = (12L, true),
    Wide = holder.Wide,
});
Require(holderResult.Maybe == (12L, true), "optional tuple field");
Require(holderResult.First() == (11L, "eleven"), "method returning a tuple");
Require(await holderResult.FirstAsync() == (11L, "eleven"), "async method returning a tuple");
IReadOnlyDictionary<string, (long, string)> map = Functions.EchoTupleMap(
    new ReadOnlyDictionary<string, (long, string)>(
        new Dictionary<string, (long, string)> { ["k"] = (13L, "thirteen") }));
Require(map["k"] == (13L, "thirteen"), "tuple map values");

// Unions of tuples and tuples of unions stay distinct.
BamlUnion<(long, bool), (string, bool)> unionResult = Functions.EchoUnionOfTuples(
    BamlUnion<(long, bool), (string, bool)>.FromT1(("s", true)));
Require(unionResult.IsT1 && unionResult.AsT1 == ("s", true), "union of tuples");
unionResult = Functions.EchoUnionOfTuples(
    BamlUnion<(long, bool), (string, bool)>.FromT0((14L, false)));
Require(unionResult.IsT0 && unionResult.AsT0 == (14L, false), "union of tuples, first arm");
(BamlUnion<long, string>, bool) product = Functions.EchoProductOfUnions(
    (BamlUnion<long, string>.FromT1("x"), false));
Require(product.Item1.IsT1 && product.Item1.AsT1 == "x" && !product.Item2, "tuple of unions");

// Host callback arguments and results.
var callbackSeen = new List<(long, string)>();
(string, long) swapped = Functions.CallPairCallback(
    BamlCallback.FromSync<(long, string), (string, long)>(pair =>
    {
        callbackSeen.Add(pair);
        return (pair.Item2, pair.Item1);
    }),
    (15L, "fifteen"));
Require(
    swapped == ("fifteen", 15L) && callbackSeen.SequenceEqual(new[] { (15L, "fifteen") }),
    "host callback tuple argument and result");
swapped = await Functions.CallPairCallbackAsync(
    async pair =>
    {
        await Task.Yield();
        return (pair.Item2, pair.Item1);
    },
    (16L, "sixteen"));
Require(swapped == ("sixteen", 16L), "async host callback tuple");

// Dynamic values: an engine tuple carries no type yet, so it decodes by the
// requested type's shape, or falls back to its slots.
BamlValue dynamic = Functions.MakeUnknownTuple();
Require(dynamic.Kind == BamlValueKind.Tuple, "dynamic tuple kind");
Require(
    dynamic.TryGetTupleItems(out IReadOnlyList<BamlValue>? items)
        && items.Count == 2
        && items[0].As<long>() == 1L
        && items[1].As<string>() == "one",
    "dynamic tuple slots");
Require(dynamic.As<(long, string)>() == (1L, "one"), "dynamic tuple as registered tuple");
Require(dynamic.As<IReadOnlyList<BamlValue>>().Count == 2, "heterogeneous fallback");
Require(!dynamic.TryGet(out (string, long) _), "wrong slot types are rejected");
Require(
    !dynamic.TryGet(out (long, string, string) _),
    "wrong arity is rejected");
Expect<BamlTypeMappingException>(
    () => Functions.EchoUnknown(dynamic),
    "an uncarried tuple cannot be stated back");
BamlValue stated = BamlValue.From((17L, "seventeen"));
Require(
    stated.Kind == BamlValueKind.Tuple
        && stated.Type.Kind == BamlTypeDescriptorKind.Tuple
        && stated.Type.Arguments.Count == 2,
    "a registered tuple states its type");
BamlValue echoed = Functions.EchoUnknown(stated);
Require(echoed.As<(long, string)>() == (17L, "seventeen"), "dynamic tuple roundtrip");
Require(
    !BamlValue.List(new[] { BamlValue.Int(1) }).TryGet(out (long, string) _),
    "a list of the wrong arity is not a tuple");
Require(
    BamlValue.List(new[] { BamlValue.Int(18), BamlValue.String("x") }).As<(long, string)>()
        == (18L, "x"),
    "a plain list takes a requested tuple shape");

// Thrown values.
BamlErrorException thrown = Expect<BamlErrorException>(
    () => Functions.ThrowPair((19L, "nineteen")),
    "a thrown tuple");
Require(
    thrown.ThrownValue.Kind == BamlValueKind.Tuple
        && thrown.ThrownValue.As<(long, string)>() == (19L, "nineteen"),
    "thrown tuple value");

Console.WriteLine("csharp_tuples=ok");

static void Require(bool condition, string description)
{
    if (!condition)
    {
        throw new InvalidOperationException($"{description} changed");
    }
}

static TException Expect<TException>(Action action, string description)
    where TException : Exception
{
    try
    {
        action();
    }
    catch (TException error)
    {
        return error;
    }

    throw new InvalidOperationException($"{description}: expected {typeof(TException).Name}");
}

static void ExpectAny(Action action, string description)
{
    try
    {
        action();
    }
    catch (Exception)
    {
        return;
    }

    throw new InvalidOperationException($"{description}: expected a failure");
}
