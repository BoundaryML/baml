using System.Numerics;
using System.Collections.ObjectModel;
using Baml;
using CsharpGenerics;

if (Functions.Identity("typed") != "typed")
{
    throw new InvalidOperationException("inferred string generic changed");
}

if (Functions.Identity(42L) != 42L)
{
    throw new InvalidOperationException("inferred integer generic changed");
}

BigInteger huge = BigInteger.Parse(
    "123456789012345678901234567890",
    System.Globalization.CultureInfo.InvariantCulture);
if (await Functions.IdentityAsync(huge) != huge)
{
    throw new InvalidOperationException("async bigint generic changed");
}

IReadOnlyList<long> values = Array.AsReadOnly([1L, 2L, 3L]);
IReadOnlyList<long> valuesResult = Functions.Identity(values);
if (!valuesResult.SequenceEqual(values))
{
    throw new InvalidOperationException("generic list changed");
}
if (Functions.Head(values) != 1L)
{
    throw new InvalidOperationException("nested list inference changed");
}

IReadOnlyDictionary<string, long> scores =
    new ReadOnlyDictionary<string, long>(
        new Dictionary<string, long> { ["typed"] = 9L });
IReadOnlyDictionary<string, long> scoresResult =
    await Functions.IdentityAsync(scores);
if (scoresResult.Count != 1 || scoresResult["typed"] != 9L)
{
    throw new InvalidOperationException("generic map changed");
}
if (!Functions.MapValues(scores).SequenceEqual([9L]))
{
    throw new InvalidOperationException("nested map inference changed");
}

long? nullableInteger = Functions.Identity<long?>(null);
BamlNullable<string> nullableString =
    Functions.Identity(BamlNullable.Null<string>());
if (nullableInteger is not null || !nullableString.IsNull)
{
    throw new InvalidOperationException("generic nullable changed");
}
BamlNullable<long> maybe = Functions.Maybe(
    BamlNullable.FromValue(7L));
BamlNullable<long> omittedMaybe = Functions.OptionalMaybe<long>();
if (maybe.Value != 7L || !omittedMaybe.IsNull)
{
    throw new InvalidOperationException("nested nullable/default changed");
}

var box = new Box<long> { Value = 17L };
Box<long> boxed = Functions.Wrap(18L);
Box<long> identityBox = Functions.Identity(box);
if (boxed.Value != 18L
    || identityBox.Value != 17L
    || Functions.Unbox(box) != 17L)
{
    throw new InvalidOperationException("generic class binding changed");
}

if (box.Get() != 17L || await box.GetAsync() != 17L)
{
    throw new InvalidOperationException("generic instance method binding changed");
}
Box<string> replaced = box.Replace("replaced");
Box<string> asyncReplaced = await box.ReplaceAsync("async replaced");
Box<BigInteger> constructed = Box<long>.New(huge);
Box<string> asyncConstructed = await Box<long>.NewAsync("static");
if (replaced.Value != "replaced"
    || asyncReplaced.Value != "async replaced"
    || constructed.Value != huge
    || asyncConstructed.Value != "static")
{
    throw new InvalidOperationException("generic class method changed");
}

var counter = new Counter { Value = 10L };
Counter madeCounter = Counter.New(12L);
if (counter.Add(5L) != 15L
    || await counter.AddAsync(6L) != 16L
    || madeCounter.Value != 12L
    || (await Counter.NewAsync(13L)).Value != 13L)
{
    throw new InvalidOperationException("nongeneric class method changed");
}

var pair = new Pair<string, long> { First = "left", Second = 21L };
Pair<string, BigInteger> replacedPair = pair.ReplaceSecond(huge);
if (pair.GetSecond() != 21L
    || await pair.GetSecondAsync() != 21L
    || replacedPair.First != "left"
    || replacedPair.Second != huge)
{
    throw new InvalidOperationException("multi-parameter generic method changed");
}

if (Functions.TypeName<string>() != "string"
    || await Functions.TypeNameAsync<long>() != "int")
{
    throw new InvalidOperationException("explicit result-only binding changed");
}

if (Functions.LocalCollision(1L, 2L, 3L, 4L, 5L) != 4L)
{
    throw new InvalidOperationException("generated local allocation changed");
}

Task<long>[] concurrentLongs = Enumerable.Range(0, 32)
    .Select(index => Functions.IdentityAsync((long)index))
    .ToArray();
Task<Box<string>>[] concurrentBoxes = Enumerable.Range(0, 32)
    .Select(index => box.ReplaceAsync(index.ToString(
        System.Globalization.CultureInfo.InvariantCulture)))
    .ToArray();
long[] longResults = await Task.WhenAll(concurrentLongs);
Box<string>[] boxResults = await Task.WhenAll(concurrentBoxes);
if (!longResults.SequenceEqual(Enumerable.Range(0, 32).Select(index => (long)index))
    || boxResults.Where((value, index) => value.Value != index.ToString(
        System.Globalization.CultureInfo.InvariantCulture)).Any())
{
    throw new InvalidOperationException("concurrent generic cache isolation changed");
}

var label = new Label { Text = "nominal" };
Label identityLabel = Functions.Identity(label);
Flavor identityFlavor = Functions.Identity(Flavor.Chocolate);
if (identityLabel.Text != "nominal" || identityFlavor != Flavor.Chocolate)
{
    throw new InvalidOperationException("generic nominal registration changed");
}

if (!BamlValue.From(values).As<IReadOnlyList<long>>().SequenceEqual(values)
    || BamlValue.From(scores).As<IReadOnlyDictionary<string, long>>()["typed"] != 9L
    || BamlValue.From(box).As<Box<long>>().Value != 17L
    || BamlValue.From(label).As<Label>().Text != "nominal"
    || BamlValue.From(identityFlavor).As<Flavor>() != Flavor.Chocolate)
{
    throw new InvalidOperationException("registered dynamic generic codec changed");
}

Expect<BamlTypeMappingException>(() => Functions.Identity(42));
Expect<BamlTypeMappingException>(() => Functions.Identity(new List<long> { 1L }));

Console.WriteLine("csharp_generics=ok");

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
