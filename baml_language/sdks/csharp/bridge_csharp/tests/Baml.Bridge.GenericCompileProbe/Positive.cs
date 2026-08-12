using System.Numerics;
using Baml;
using Probe.Generated;

long integer = Generated.Identity(42L);
double floating = Generated.Identity(1.5);
BigInteger bigint = Generated.Identity(
    BigInteger.Parse("12345678901234567890"));
string? nullableReference =
    Generated.Identity<string?>(null);
BamlNullable<string> reifiedNullableReference =
    Generated.Nullable(
        BamlNullable.Null<string>());

BamlOptional<string> unset =
    Generated.Optional<string>();
BamlOptional<string> optional =
    Generated.Optional<string>("value");
BamlOptional<string?> explicitNull =
    Generated.Optional<string?>(null);
BamlNullable<long> nullableValue =
    Generated.Nullable(
        BamlNullable.FromValue(7L));
BamlOptional<BamlNullable<long>> composedUnset =
    Generated.Composed<long>();
BamlOptional<BamlNullable<long>> composedNull =
    Generated.Composed<long>(
        BamlNullable.Null<long>());
BamlOptional<BamlNullable<long>> composedValue =
    Generated.Composed<long>(
        BamlNullable.FromValue(8L));

IReadOnlyList<long> list = [9L, 10L];
IReadOnlyDictionary<string, long> map =
    new Dictionary<string, long>
    {
        ["x"] = 11L,
    };
long head = Generated.Head(list);
long lookup = Generated.Lookup(map, "x");

BamlUnion<string, long> first =
    Generated.Union<string, long>("arm");
BamlUnion<string, long> second =
    Generated.Union<string, long>(
        BamlUnion<string, long>.FromT1(12L));
BamlUnion<string, string> duplicate =
    Generated.Union<string, string>(
        BamlUnion<string, string>.FromT1("second"));

Box<long> box = new() { Value = 13L };
long unboxed = Generated.Unbox(box);
(string owner, long method) =
    new GenericOwner<string>().Method("owner", 14L);
long explicitResult = Generated.ResultOnly<long>();

if (integer != 42
    || floating != 1.5
    || bigint.ToString() != "12345678901234567890"
    || nullableReference is not null
    || !reifiedNullableReference.IsNull
    || unset.IsSet
    || optional.Value != "value"
    || !explicitNull.IsSet
    || explicitNull.Value is not null
    || nullableValue.Value != 7
    || composedUnset.IsSet
    || !composedNull.Value.IsNull
    || composedValue.Value.Value != 8
    || head != 9
    || lookup != 11
    || !first.IsT0
    || !second.IsT1
    || !duplicate.IsT1
    || unboxed != 13
    || owner != "owner"
    || method != 14
    || explicitResult != 0)
{
    throw new InvalidOperationException(
        "positive generic compile/runtime matrix failed");
}

Console.WriteLine("generic_compile_positive=complete");
