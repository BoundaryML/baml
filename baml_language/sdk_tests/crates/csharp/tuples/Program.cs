using System.Collections.ObjectModel;
using Baml;
using Baml.Generated;
using CsharpTuples;

// `dotnet run -- <case>` runs one tuple stream case (see RunStreamCaseAsync);
// with no argument the boundary suite below runs.
if (args.Length > 0)
{
    await RunStreamCaseAsync(args[0]);
    return;
}

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

// Dynamic values: an engine tuple carries its type, so it decodes as the C#
// tuple registered for that identity, or falls back to its slots.
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
Require(
    dynamic.Type.Kind == BamlTypeDescriptorKind.Tuple
        && dynamic.Type.Arguments.Count == 2
        && dynamic.Type.Arguments[0].Kind == BamlTypeDescriptorKind.Int
        && dynamic.Type.Arguments[1].Kind == BamlTypeDescriptorKind.String,
    "an engine tuple carries its type");
Require(
    Functions.EchoUnknown(dynamic).As<(long, string)>() == (1L, "one"),
    "a received tuple is restated with its carried type");
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

Require(
    Functions.EchoUnknown(thrown.ThrownValue).As<(long, string)>() == (19L, "nineteen"),
    "a thrown tuple is restated with its carried type");

// Carried identity: the declared slot types pick the C# tuple, not the
// slot values. `(int | string, bool)` holding `(1, true)` stays that tuple.
BamlValue unionSlot = Functions.MakeUnionSlotTuple();
(BamlUnion<long, string>, bool) unionSlotTuple =
    unionSlot.As<(BamlUnion<long, string>, bool)>();
Require(
    unionSlotTuple.Item1.IsT0 && unionSlotTuple.Item1.AsT0 == 1L && unionSlotTuple.Item2,
    "a carried union-slot tuple decodes by identity");
Require(!unionSlot.TryGet(out (long, bool) _), "a narrower tuple identity is not inferred");
Require(
    Functions.EchoUnknown(unionSlot).As<(BamlUnion<long, string>, bool)>() == unionSlotTuple,
    "a union-slot tuple is restated with its carried type");

// Generic substitutions are part of the carried identity.
Require(
    Functions.MakeGenericPair().As<(long, long)>() == (21L, 21L),
    "a generic tuple carries its substituted slot types");

// No registered tuple type: a list of the join of the carried slot types.
BamlValue unregistered = Functions.MakeUnregisteredTuple();
Require(!unregistered.TryGet(out (string, long, string) _), "an unregistered tuple type");
IReadOnlyList<BamlUnion<long, string>> joined =
    unregistered.As<IReadOnlyList<BamlUnion<long, string>>>();
Require(
    joined.Count == 3
        && joined[0].IsT1 && joined[0].AsT1 == "a"
        && joined[1].IsT0 && joined[1].AsT0 == 2L
        && joined[2].IsT1 && joined[2].AsT1 == "c",
    "a union list of the carried slot types");
Require(
    unregistered.As<IReadOnlyList<BamlValue>>().Count == 3,
    "an unregistered tuple's heterogeneous fallback");
Require(
    !unregistered.TryGet(out IReadOnlyList<BamlUnion<long, string, bool>>? _),
    "a list of a different join is not the fallback");
Require(
    !unregistered.TryGet(out IReadOnlyList<long>? _),
    "a list of one slot type is not the join");
Require(
    Functions.MakeRepeatedTuple().As<IReadOnlyList<long>>().SequenceEqual(new[] { 1L, 2L, 3L }),
    "repeated slot types join to one item type");
IReadOnlyList<BamlUnion<long, string, bool>> wideJoined = Functions.MakeUnionSlotTriple()
    .As<IReadOnlyList<BamlUnion<long, string, bool>>>();
Require(
    wideJoined.Count == 3
        && wideJoined[0].IsT1 && wideJoined[0].AsT1 == "x"
        && wideJoined[1].IsT2 && !wideJoined[1].AsT2
        && wideJoined[2].IsT1 && wideJoined[2].AsT1 == "y",
    "a union slot joins through the engine's selected arm");

// No union list for the join either: the heterogeneous list, every slot kept.
BamlValue unjoined = Functions.MakeUnjoinedTuple();
Require(
    !unjoined.TryGet(out IReadOnlyList<BamlUnion<long, string>>? _),
    "a union list missing a slot type is not the join");
IReadOnlyList<BamlValue> heterogeneous = unjoined.As<IReadOnlyList<BamlValue>>();
Require(
    heterogeneous.Count == 3
        && heterogeneous[0].As<long>() == 1L
        && heterogeneous[1].As<string>() == "b"
        && heterogeneous[2].As<double>() == 2.5,
    "the heterogeneous fallback keeps every slot in order");

// Generic tuples are built from their slots once the call binds `T`: a
// native tuple stays native, and one needing a generated record (which is
// never generic) is the list of its slots' join. Either crosses as the
// declared tuple, so the engine checks every slot.
Require(Functions.PairOf(23L) == (23L, 23L), "a generic native tuple");
Require(
    await Functions.PairOfAsync("x") == ("x", "x"),
    "a generic native tuple over another binding");
IReadOnlyList<BamlUnion<long, string>> tagged = Functions.GenericTagged<long>(
    new BamlUnion<long, string>[] { 24L, "tag" });
Require(
    tagged.Count == 2
        && tagged[0].IsT0 && tagged[0].AsT0 == 24L
        && tagged[1].IsT1 && tagged[1].AsT1 == "tag",
    "a generic tuple needing a record round-trips as its join list");
ExpectAny(
    () => Functions.GenericTagged<long>(new BamlUnion<long, string>[] { 24L, "other" }),
    "a join list element outside its literal slot");
ExpectAny(
    () => Functions.GenericTagged<long>(new BamlUnion<long, string>[] { "tag", 24L }),
    "a join list element in another slot's arm");
ExpectAny(
    () => Functions.GenericTagged<long>(new BamlUnion<long, string>[] { 24L }),
    "a join list of the wrong arity");
(IReadOnlyList<BamlUnion<string, string>>, long) nested = Functions.GenericTaggedNested<string>(
    (new[] { BamlUnion<string, string>.FromT0("x"), BamlUnion<string, string>.FromT1("tag") }, 25L));
Require(
    nested.Item2 == 25L
        && nested.Item1.Count == 2
        && nested.Item1[0].IsT0 && nested.Item1[0].AsT0 == "x"
        && nested.Item1[1].IsT1 && nested.Item1[1].AsT1 == "tag",
    "a join list nested in a generic native tuple");

// A host callback echoing a dynamic tuple restates its carried type.
BamlValue? callbackArgument = null;
BamlValue callbackResult = Functions.CallUnknownCallback(
    BamlCallback.FromSync<BamlValue, BamlValue>(argument =>
    {
        callbackArgument = argument;
        return argument;
    }));
Require(
    callbackArgument!.As<(long, string)>() == (22L, "twenty-two")
        && callbackResult.As<(long, string)>() == (22L, "twenty-two"),
    "a callback echoes a tuple with its carried type");

Console.WriteLine("csharp_tuples=ok");

// Tuples as stream items: the scripted clients replay fixed deltas, so the
// partials are exact. Every partial and the final value decode as the C#
// tuple the stream's item type projects to.
static async Task RunStreamCaseAsync(string streamCase)
{
    using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(60));
    switch (streamCase)
    {
        case "stream_items":
        {
            var partials = new List<IReadOnlyList<(string, long)>>();
            BamlStream<IReadOnlyList<(string, long)>> stream = Functions.StreamTupleItemsStream("x");
            await using (((IAsyncDisposable)stream).ConfigureAwait(false))
            {
                await foreach (IReadOnlyList<(string, long)> partial in
                    stream.WithCancellation(timeout.Token).ConfigureAwait(false))
                {
                    partials.Add(partial);
                }

                IReadOnlyList<(string, long)> final =
                    await stream.GetFinalResponseAsync(timeout.Token).ConfigureAwait(false);
                (string, long)[] expected = { ("a", 1L), ("b", 2L) };
                Require(final.SequenceEqual(expected), "final tuple stream list");
                Require(
                    partials.Count > 0 && partials[0].SequenceEqual(new[] { ("a", 1L) }),
                    "first tuple stream partial");
                Require(
                    partials.All(partial =>
                        partial.Count <= expected.Length
                            && partial.SequenceEqual(expected.Take(partial.Count))),
                    "tuple stream partials are prefixes of the final list");
            }

            Require(
                Functions.StreamTupleItems("x").SequenceEqual(new[] { ("a", 1L), ("b", 2L) }),
                "sync call of the streamed tuple-list function");
            Require(
                (await Functions.StreamTupleItemsAsync("x")).SequenceEqual(
                    new[] { ("a", 1L), ("b", 2L) }),
                "async call of the streamed tuple-list function");
            Console.WriteLine("csharp_tuples_stream_items=ok");
            break;
        }
        case "stream_final":
        {
            var partials = new List<(long, string)>();
            BamlStream<(long, string)> stream = Functions.StreamTupleFinalStream("x");
            await using (((IAsyncDisposable)stream).ConfigureAwait(false))
            {
                await foreach ((long, string) partial in
                    stream.WithCancellation(timeout.Token).ConfigureAwait(false))
                {
                    partials.Add(partial);
                }

                (long, string) final =
                    await stream.GetFinalResponseAsync(timeout.Token).ConfigureAwait(false);
                Require(final == (1L, "a"), "final tuple stream value");
                Require(
                    partials.Count > 0
                        && partials[^1] == (1L, "a")
                        && partials.All(partial =>
                            partial.Item1 == 1L && "a".StartsWith(partial.Item2, StringComparison.Ordinal)),
                    "tuple stream partials");
            }

            Require(Functions.StreamTupleFinal("x") == (1L, "a"), "sync call of the streamed tuple function");
            Require(
                await Functions.StreamTupleFinalAsync("x") == (1L, "a"),
                "async call of the streamed tuple function");
            Console.WriteLine("csharp_tuples_stream_final=ok");
            break;
        }
        default:
            throw new ArgumentException($"unknown tuple stream case {streamCase}");
    }
}

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
