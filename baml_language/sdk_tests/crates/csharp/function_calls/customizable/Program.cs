using Baml;
using BamlSdk.HostCallableTests;

static void AssertEqual<T>(T expected, T actual)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"Expected {expected}, received {actual}.");
    }
}

Func<long, string> syncCallback = value => $"got {value}";
AssertEqual("got 5", await Functions.CallWithCallbackAsync(syncCallback, 5));

var ambient = new AsyncLocal<string?> { Value = "context" };
Func<long, string> contextCallback = value => $"{ambient.Value}:{value}";
AssertEqual("context:6", await Functions.CallWithCallbackAsync(contextCallback, 6));

Func<long, string, string> twoArgs = (value, prefix) => $"{prefix}:{value}";
AssertEqual("answer:7", await Functions.CallWithTwoArgsAsync(twoArgs, 7, "answer"));

Func<long, ValueTask<long>> asyncCallback = async value =>
{
    await Task.Yield();
    return value * 2;
};
AssertEqual(42L, await Functions.CallIntCallbackAsync(asyncCallback, 21));

var person = new Person { Name = "Ada", Age = 37 };
Func<Person, string> classCallback = value => $"{value.Name} is {value.Age}";
AssertEqual("Ada is 37", await Functions.CallWithClassCallbackAsync(classCallback, person));

var invocations = new List<long>();
Func<long, string> repeated = value =>
{
    invocations.Add(value);
    return $"item-{value}";
};
var repeatedResult = await Functions.CallRepeatedlyAsync(repeated, 5);
if (!repeatedResult.SequenceEqual(new[] { "item-0", "item-1", "item-2", "item-3", "item-4" }))
{
    throw new InvalidOperationException("Repeated host-callable results were not preserved.");
}

if (!invocations.SequenceEqual(new long[] { 0, 1, 2, 3, 4 }))
{
    throw new InvalidOperationException("Repeated host-callable invocation order was not preserved.");
}

var optionalUnset = await Functions.CallCallbackWithOptionalArgsAllUnsetAsync(
    (x, y, z) => x + (y.IsSet ? y.Value : 10) + (z.IsSet ? z.Value : 100),
    1);
if (!optionalUnset.SequenceEqual([111L]))
{
    throw new InvalidOperationException(
        $"Optional callback omission was not preserved: {string.Join(", ", optionalUnset)}");
}

var optionalPartial = await Functions.CallCallbackWithOptionalArgsPartiallySetAsync(
    (x, y, z) => x + (y.IsSet ? y.Value : 10) + (z.IsSet ? z.Value : 100),
    1);
if (!optionalPartial.SequenceEqual([103L, 14L]))
{
    throw new InvalidOperationException(
        $"Optional callback named arguments were misrouted: {string.Join(", ", optionalPartial)}");
}

var optionalAll = await Functions.CallCallbackWithOptionalArgsAllSetAsync(
    async (x, y, z) =>
    {
        await Task.Yield();
        return x + y.Value + z.Value;
    },
    1);
if (!optionalAll.SequenceEqual([6L]))
{
    throw new InvalidOperationException(
        $"Async optional callback dispatch returned an invalid result: {string.Join(", ", optionalAll)}");
}

var genericOptional = await Functions.CallGenericCallbackWithOptionalArgAsync(
    (long value, BamlOptional<long> fallback) => fallback.IsSet ? fallback.Value : value,
    7,
    11);
if (!genericOptional.SequenceEqual([7L, 11L]))
{
    throw new InvalidOperationException(
        $"Generic optional callback dispatch returned an invalid result: {string.Join(", ", genericOptional)}");
}

try
{
    _ = Functions.CallWithCallback(syncCallback, 1);
    throw new InvalidOperationException("The generated sync host-callable path did not reject the call.");
}
catch (InvalidOperationException error) when (error.Message.Contains("async", StringComparison.OrdinalIgnoreCase))
{
}

var original = new HostCallbackProbeException("host callback failed");
Func<long, string> throwing = _ => throw original;
try
{
    _ = await Functions.CallWithCallbackAsync(throwing, 1);
    throw new InvalidOperationException("The throwing host callable unexpectedly returned.");
}
catch (HostCallbackProbeException error) when (ReferenceEquals(error, original))
{
}

AssertEqual(
    "caught:HostCallbackProbeException",
    await Functions.CallWithThrowingAsync(throwing, 1));

Func<long, string> typedThrow = _ => throw new BamlError(
    new ValidationError
    {
        Code = 4,
        Message = "bad shape",
        Fields = new() { "name", "age" },
    },
    Array.Empty<string>());
AssertEqual(
    "caught: bad shape",
    await Functions.CallWithTypedThrowsAsync(typedThrow, 1));

using (var cancellation = new CancellationTokenSource(TimeSpan.FromMilliseconds(20)))
{
    Func<long, ValueTask<string>> slow = async value =>
    {
        await Task.Delay(100);
        return $"late-{value}";
    };
    try
    {
        _ = await Functions.CallWithCallbackAsync(slow, 9, cancellation.Token);
        throw new InvalidOperationException("The host-callable call ignored cancellation.");
    }
    catch (OperationCanceledException)
    {
    }
}

await Task.Delay(150);
AssertEqual("got 10", await Functions.CallWithCallbackAsync(syncCallback, 10));

Console.WriteLine("C# host callable integration passed.");

internal sealed class HostCallbackProbeException(string message) : Exception(message);
