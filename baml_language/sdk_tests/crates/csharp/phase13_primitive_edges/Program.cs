using System.Collections.ObjectModel;
using System.Numerics;

using Baml;
using Baml.Cffi;
using CsharpPhase13;

const long BamlIntMinimum = -4_611_686_018_427_387_904;
const long BamlIntMaximum = 4_611_686_018_427_387_903;

Require(Functions.IntMin() == BamlIntMinimum, "native BAML int minimum changed");
Require(await Functions.IntMaxAsync() == BamlIntMaximum, "native BAML int maximum changed");
Require(Functions.EchoInt(BamlIntMinimum) == BamlIntMinimum, "minimum int roundtrip changed");
Require(Functions.EchoInt(BamlIntMaximum) == BamlIntMaximum, "maximum int roundtrip changed");

int primitiveProbeCalls = 0;
Func<CancellationToken, Task<long>> probe = _ =>
{
    Interlocked.Increment(ref primitiveProbeCalls);
    return Task.FromResult(1L);
};
foreach (long overRange in new[] { BamlIntMinimum - 1, BamlIntMaximum + 1 })
{
    Expect<BamlProtocolException>(() =>
        Functions.RejectIntBeforeDispatch(probe, overRange));
    Require(primitiveProbeCalls == 0, "over-range int reached native dispatch");
    RequireRegistryIdle("over-range int encoding");
}

BigInteger highBit = (BigInteger.One << 255) + 0x80;
BigInteger[] bigints =
[
    BigInteger.Zero,
    BigInteger.One,
    highBit,
    -BigInteger.One,
    -highBit,
];
foreach (BigInteger value in bigints)
{
    Require(
        await Functions.EchoBigintAsync(value) == value,
        $"bigint roundtrip changed for {value}");
}

double[] finiteFloats =
[
    0.0,
    -0.0,
    double.Epsilon,
    -double.Epsilon,
    double.MaxValue,
    double.MinValue,
];
foreach (double value in finiteFloats)
{
    double result = Functions.EchoFloat(value);
    Require(
        BitConverter.DoubleToInt64Bits(result) == BitConverter.DoubleToInt64Bits(value),
        $"finite float bits changed for {value:R}");
}
foreach (double nonFinite in new[]
         {
             double.NaN,
             double.PositiveInfinity,
             double.NegativeInfinity,
         })
{
    Expect<BamlProtocolException>(() =>
        Functions.RejectFloatBeforeDispatch(probe, nonFinite));
    Require(primitiveProbeCalls == 0, "non-finite float reached native dispatch");
    RequireRegistryIdle("non-finite float encoding");
}

Require(
    Functions.EchoBytes(ReadOnlyMemory<byte>.Empty).IsEmpty,
    "empty byte memory roundtrip changed");
byte[] backing = [0xaa, 0x00, 0x7f, 0x80, 0xff, 0x00, 0xbb];
byte[] expectedBytes = [0x00, 0x7f, 0x80, 0xff, 0x00];
var inputMemory = new ReadOnlyMemory<byte>(backing, 1, expectedBytes.Length);
Task<ReadOnlyMemory<byte>> byteCall = Functions.EchoBytesAsync(inputMemory);
Array.Fill(backing, (byte)0xcc);
ReadOnlyMemory<byte> returnedMemory = await byteCall;
Require(
    returnedMemory.Span.SequenceEqual(expectedBytes),
    "byte memory was not snapshotted before native dispatch");

Require(Functions.EchoNullableInt(null) is null, "nullable int null changed");
Require(Functions.EchoNullableInt(0L) == 0L, "nullable int zero changed");
Require(Functions.EchoNullableString(null) is null, "nullable string null changed");
Require(
    Functions.EchoNullableString("text\0雪") == "text\0雪",
    "nullable string value changed");
bool nullableBytesInputWasNull = Functions.NullableBytesIsNull(null);
ReadOnlyMemory<byte>? echoedNullBytes = Functions.EchoNullableBytes(null);
ReadOnlyMemory<byte>? literalNullBytes = Functions.ReturnNullBytes();
Require(
    nullableBytesInputWasNull
        && echoedNullBytes is null
        && literalNullBytes is null,
    $"nullable bytes null changed: input-null={nullableBytesInputWasNull}, "
        + $"echo-has-value={echoedNullBytes.HasValue}, "
        + $"literal-has-value={literalNullBytes.HasValue}");
ReadOnlyMemory<byte>? nullableBytes = Functions.EchoNullableBytes(inputMemory);
Require(
    nullableBytes.HasValue
        && nullableBytes.Value.Span.SequenceEqual(
            new byte[] { 0xcc, 0xcc, 0xcc, 0xcc, 0xcc }),
    "nullable bytes value changed");

Require(Functions.EchoNullableList(null) is null, "nullable list null changed");
IReadOnlyList<long> list = Array.AsReadOnly([1L, 2L, 3L]);
IReadOnlyList<long>? nullableList = Functions.EchoNullableList(list);
Require(
    nullableList is not null && nullableList.SequenceEqual(list),
    "nullable list value changed");
IReadOnlyList<long?> nullableElements = Array.AsReadOnly<long?>([1L, null, 3L]);
Require(
    Functions.EchoListOfNullableInts(nullableElements).SequenceEqual(nullableElements),
    "nullable collection elements changed");

Require(Functions.EchoNullableMap(null) is null, "nullable map null changed");
IReadOnlyDictionary<string, string> map =
    new ReadOnlyDictionary<string, string>(
        new Dictionary<string, string> { ["nul\0key"] = "雪" });
IReadOnlyDictionary<string, string>? nullableMap = Functions.EchoNullableMap(map);
Require(
    nullableMap is not null
        && nullableMap.Count == 1
        && nullableMap["nul\0key"] == "雪",
    "nullable map value changed");

BamlNullable<long> genericValue =
    Functions.GenericMaybe(BamlNullable.FromValue(17L));
BamlNullable<string> genericNull =
    Functions.GenericMaybe(BamlNullable.Null<string>());
BamlNullable<IReadOnlyList<long>> genericList =
    Functions.GenericMaybe(BamlNullable.FromValue(list));
Require(
    !genericValue.IsNull
        && genericValue.Value == 17L
        && genericNull.IsNull
        && !genericList.IsNull
        && genericList.Value.SequenceEqual(list),
    "generic nullable semantic position changed");

RequireRegistryIdle("completed primitive calls");
Console.WriteLine("csharp_phase13_primitive_edges=ok");
return 0;

static bool RegistryIdle() =>
    HostValueRegistry.Shared.EntryCount == 0
    && HostValueRegistry.Shared.InvocationCount == 0
    && NativeCallbacks.PendingCount == 0;

static void RequireRegistryIdle(string operation) => Require(
    RegistryIdle(),
    $"{operation} left host values, dispatch leases, or native results pending");

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

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}
