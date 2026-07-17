using Baml;
using BamlSdk;
using System.Diagnostics;
using System.Numerics;

if (args is ["--exit-probe", var exitCodeText]
    && long.TryParse(exitCodeText, out var exitCode))
{
    Functions.ExitProbe(exitCode);
    Console.Error.WriteLine("baml.sys.exit returned unexpectedly");
    Environment.Exit(99);
}

const long minBamlInteger = -(1L << 62);
const long maxBamlInteger = (1L << 62) - 1;
const string expected = "hello from C#";

AssertEqual(expected, Functions.Echo(expected));
AssertEqual(expected, Functions.Echo(value: expected));
AssertEqual(expected, await Functions.EchoAsync(expected));
AssertEqual(true, Functions.RoundTripBool(true));
AssertEqual(false, Functions.RoundTripBool(false));

foreach (var value in new[] { 0L, 1L, -1L, minBamlInteger, maxBamlInteger })
{
    AssertEqual(value, Functions.RoundTripInt(value));
}

try
{
    _ = BamlGeneratedProgram.Instance.Call<string>(
        "user.generic_type_name",
        [],
        Array.Empty<(string Name, Type Type)>());
    throw new InvalidOperationException("A native call-boundary type mismatch unexpectedly returned.");
}
catch (BamlTypeMismatchException error)
{
    AssertEqual("baml.errors.TypeMismatch", error.ClassName);
    var fields = AssertType<Dictionary<string, object?>>(error.Value);
    if (!fields.TryGetValue("message", out var mismatchMessage)
        || mismatchMessage is not string message
        || !message.Contains("specif", StringComparison.OrdinalIgnoreCase))
    {
        throw new InvalidOperationException("The native type-mismatch payload lost its diagnostic message.");
    }
}

foreach (var value in new[] { minBamlInteger - 1, maxBamlInteger + 1, long.MinValue, long.MaxValue })
{
    try
    {
        Functions.RoundTripInt(value);
        throw new InvalidOperationException($"Out-of-range BAML int {value} was accepted.");
    }
    catch (BamlBridgeException error) when (error.Message.Contains("outside the BAML int range", StringComparison.Ordinal))
    {
    }
}

foreach (var value in new[]
         {
             BigInteger.Zero,
             new BigInteger(128),
             BigInteger.Parse("1208925819614629174706177"),
             BigInteger.Parse("-1208925819614629174706177"),
         })
{
    AssertEqual(value, Functions.RoundTripBigint(value));
}

foreach (var value in new[] { -3.5, 0.0, 3.5 })
{
    AssertEqual(value, Functions.RoundTripFloat(value));
}

var bytes = new byte[] { 0, 1, 2, 127, 128, 255 };
if (!Functions.RoundTripBytes(bytes).SequenceEqual(bytes))
{
    throw new InvalidOperationException("Byte-array round trip failed.");
}

AssertEqual<object?>(null, Functions.RoundTripNull(null));
AssertEqual<object?>("dynamic", Functions.RoundTripUnknown("dynamic"));
var dynamicList = AssertType<List<object?>>(Functions.RoundTripUnknown(new List<long> { 1, 2 }));
AssertEqual(1L, dynamicList[0]);
AssertEqual(2L, dynamicList[1]);
AssertEqual(101L, Functions.RoundTripProbeId(101));
AssertSequenceEqual(
    new[] { "alias", "values" },
    Functions.RoundTripProbeNames(new() { "alias", "values" }));
try
{
    Functions.RoundTripUnknown(new object());
    throw new InvalidOperationException("An unsupported CLR object crossed the bridge.");
}
catch (BamlBridgeException error) when (error.Message.Contains("System.Object", StringComparison.Ordinal))
{
}
var cyclicValue = new List<object?>();
cyclicValue.Add(cyclicValue);
try
{
    Functions.RoundTripUnknown(cyclicValue);
    throw new InvalidOperationException("A cyclic CLR value crossed the bridge.");
}
catch (BamlBridgeException error) when (error.Message.Contains("Cyclic", StringComparison.Ordinal))
{
}
AssertEqual(42L, Functions.GenericIdentity(42L));
AssertEqual("generic", await Functions.GenericIdentityAsync("generic"));
AssertEqual(7L, Functions.GenericChoose(7L, 9L));
AssertEqual("int", Functions.GenericTypeName<long>());
AssertEqual("string", await Functions.GenericTypeNameAsync<string>());
AssertSequenceEqual(
    new long[] { 4, 8, 15 },
    Functions.GenericIdentity(new List<long> { 4, 8, 15 }));
var genericOptionalValue = Functions.GenericOptionalIdentity<long>(BamlNullable.FromValue(42L));
var genericOptionalNull = await Functions.GenericOptionalIdentityAsync<long>(BamlNullable.Null<long>());
AssertEqual(42L, genericOptionalValue.Value);
AssertEqual(true, genericOptionalNull.IsNull);
AssertEqual(true, Functions.GenericDefaultedOptional<long>().IsNull);
AssertEqual(
    true,
    Functions.GenericDefaultedOptional<long>(BamlNullable.Null<long>()).IsNull);
AssertEqual(
    64L,
    Functions.GenericDefaultedOptional<long>(BamlNullable.FromValue(64L)).Value);
var genericOptionalList = Functions.GenericOptionalList<long>(new()
{
    BamlNullable.FromValue(3L),
    BamlNullable.Null<long>(),
    BamlNullable.FromValue(5L),
});
AssertEqual(3L, genericOptionalList[0].Value);
AssertEqual(true, genericOptionalList[1].IsNull);
AssertEqual(5L, genericOptionalList[2].Value);
AssertEqual<long?>(null, Functions.RoundTripOptionalInt(null));
AssertEqual<long?>(42, Functions.RoundTripOptionalInt(42));
var intUnion = Functions.RoundTripIntOrString(42L);
var stringUnion = await Functions.RoundTripIntOrStringAsync("union");
AssertEqual(42L, intUnion.AsT0);
AssertEqual("union", stringUnion.AsT1);
AssertEqual("int:42", intUnion.Match(static value => $"int:{value}", static value => $"string:{value}"));
AssertEqual("string:union", stringUnion.Match(static value => $"int:{value}", static value => $"string:{value}"));
var boolUnion = Functions.RoundTripIntStringOrBool(true);
AssertEqual(true, boolUnion.AsT2);
AssertEqual(
    "bool:True",
    boolUnion.Match(
        static value => $"int:{value}",
        static value => $"string:{value}",
        static value => $"bool:{value}"));
AssertEqual<BamlUnion<long, string>?>(null, Functions.RoundTripOptionalUnion(null));
var optionalUnion = Functions.RoundTripOptionalUnion(BamlUnion<long, string>.FromT0(7));
AssertEqual(7L, optionalUnion!.Value.AsT0);
var listUnion = Functions.RoundTripListOrString(new List<long> { 2, 4, 6 });
AssertSequenceEqual(new long[] { 2, 4, 6 }, listUnion.AsT0);
AssertEqual(42L, Functions.RoundTripLiteralUnion(42).AsT0);
AssertEqual("literal", Functions.RoundTripLiteralUnion("literal").AsT1);
using (var image = BamlImage.FromUrl("https://example.com/image.png", "image/png"))
using (var imageClone = image.Clone())
using (var decodedImage = Functions.RoundTripImage(image))
using (var dynamicImage = AssertType<BamlImage>(Functions.RoundTripUnknown(image)))
{
    AssertEqual<string?>("https://example.com/image.png", image.Url);
    AssertEqual<string?>("https://example.com/image.png", imageClone.Url);
    AssertEqual<string?>("https://example.com/image.png", decodedImage.Url);
    AssertEqual<string?>("https://example.com/image.png", dynamicImage.Url);
    AssertEqual<string?>("image/png", decodedImage.MimeType);
}

using (var audio = BamlAudio.FromBase64("AQID", "audio/test"))
using (var decodedAudio = await Functions.RoundTripAudioAsync(audio))
{
    AssertEqual("AQID", decodedAudio.Base64);
    AssertEqual<string?>("audio/test", decodedAudio.MimeType);
}

using (var video = BamlVideo.FromUrl("https://example.com/video.mp4"))
using (var decodedVideo = Functions.RoundTripVideo(video))
{
    AssertEqual<string?>("https://example.com/video.mp4", decodedVideo.Url);
    AssertEqual<string?>(null, decodedVideo.MimeType);
}

using (var pdf = BamlPdf.FromFile("/tmp/document.pdf", "application/pdf"))
using (var decodedPdf = Functions.RoundTripPdf(pdf))
{
    AssertEqual<string?>("/tmp/document.pdf", decodedPdf.File);
    AssertEqual<string?>("application/pdf", decodedPdf.MimeType);
}

var disposedImage = BamlImage.FromUrl("https://example.com/disposed.png");
disposedImage.Dispose();
try
{
    _ = disposedImage.Url;
    throw new InvalidOperationException("Disposed media remained usable.");
}
catch (ObjectDisposedException)
{
}
try
{
    Functions.RoundTripIntOrString(default);
    throw new InvalidOperationException("An uninitialized BAML union was accepted.");
}
catch (InvalidOperationException error) when (error.Message.Contains("no active case", StringComparison.Ordinal))
{
}
AssertEqual(15L, Functions.AddWithDefault(10));
AssertEqual(17L, Functions.AddWithDefault(10, 7));
AssertEqual(42L, Functions.RoundTripLiteral42(42));

using (var preCanceled = new CancellationTokenSource())
{
    preCanceled.Cancel();
    await AssertCanceled(() => Functions.SleepMsAsync(2_000, preCanceled.Token));
}

using (var inFlight = new CancellationTokenSource(TimeSpan.FromMilliseconds(50)))
{
    var timer = Stopwatch.StartNew();
    await AssertCanceled(() => Functions.SleepMsAsync(2_000, inFlight.Token));
    if (timer.Elapsed >= TimeSpan.FromMilliseconds(500))
    {
        throw new InvalidOperationException($"In-flight cancellation took {timer.Elapsed}.");
    }
}

using (var concurrent = new CancellationTokenSource(TimeSpan.FromMilliseconds(50)))
{
    var calls = Enumerable.Range(0, 8)
        .Select(_ => Functions.SleepMsAsync(2_000, concurrent.Token))
        .ToArray();
    await AssertCanceled(() => Task.WhenAll(calls));
    if (calls.Any(call => !call.IsCanceled))
    {
        throw new InvalidOperationException("Concurrent cancellation left a non-canceled call.");
    }
}

AssertEqual<object?>(null, await Functions.SleepMsAsync(1));

AssertSequenceEqual(new long[] { 1, 2, 3 }, Functions.RoundTripIntList(new List<long> { 1, 2, 3 }));
AssertSequenceEqual(Array.Empty<long>(), Functions.RoundTripIntList(new List<long>()));
AssertSequenceEqual(
    new long?[] { 1, null, 3 },
    Functions.RoundTripOptionalIntList(new List<long?> { 1, null, 3 }));

var map = Functions.RoundTripStringIntMap(new Dictionary<string, long?>
{
    ["one"] = 1,
    ["none"] = null,
});
AssertEqual<long?>(1, map["one"]);
AssertEqual<long?>(null, map["none"]);
AssertEqual(0, Functions.RoundTripStringIntMap(new Dictionary<string, long?>()).Count);

var nested = Functions.RoundTripNestedCollection(new Dictionary<string, List<long?>>
{
    ["values"] = new() { 1, null, 3 },
    ["empty"] = new(),
});
AssertSequenceEqual(new long?[] { 1, null, 3 }, nested["values"]);
AssertSequenceEqual(Array.Empty<long?>(), nested["empty"]);

try
{
    Functions.ThrowProbeError();
    throw new InvalidOperationException("Expected a BAML error.");
}
catch (BamlError error)
{
    AssertEqual("user.ProbeError", error.ClassName);
    var fields = AssertType<Dictionary<string, object?>>(error.Value);
    AssertEqual(42L, fields["code"]);
    AssertEqual("probe failure", fields["detail"]);
    if (error.BamlTrace.Count == 0)
    {
        throw new InvalidOperationException("BAML error trace was empty.");
    }
}

try
{
    Functions.PanicProbe("panic from C#");
    throw new InvalidOperationException("Expected a BAML panic.");
}
catch (BamlPanic panic)
{
    AssertEqual("baml.panics.UserPanic", panic.ClassName);
    var fields = AssertType<Dictionary<string, object?>>(panic.Value);
    AssertEqual("panic from C#", fields["message"]);
    if (panic.BamlTrace.Count == 0)
    {
        throw new InvalidOperationException("BAML panic trace was empty.");
    }
}

AssertEqual(ProbeLabel.Good, Functions.RoundTripProbeLabel(ProbeLabel.Good));
AssertEqual(ProbeLabel.Bad, Functions.RoundTripProbeLabel(ProbeLabel.Bad));
try
{
    Functions.RoundTripProbeLabel((ProbeLabel)0);
    throw new InvalidOperationException("Undefined generated enum value was accepted.");
}
catch (BamlBridgeException error) when (error.Message.Contains("not a declared member", StringComparison.Ordinal))
{
}

var model = new ProbeModel
{
    Name = "model",
    Count = null,
    Label = ProbeLabel.Good,
    Tags = new() { "one", "two" },
    Scores = new() { ["first"] = 1, ["second"] = 2 },
};
var decodedModel = Functions.RoundTripProbeModel(model);
var genericModel = Functions.GenericIdentity(model);
AssertEqual("model", decodedModel.Name);
AssertEqual("model", genericModel.Name);
AssertEqual<long?>(null, decodedModel.Count);
AssertEqual(ProbeLabel.Good, decodedModel.Label);
AssertSequenceEqual(new[] { "one", "two" }, decodedModel.Tags);
AssertEqual(1L, decodedModel.Scores["first"]);
AssertEqual(2L, decodedModel.Scores["second"]);
var dynamicModel = AssertType<Dictionary<string, object?>>(Functions.RoundTripUnknown(model));
AssertEqual("model", dynamicModel["name"]);
AssertEqual("Good", dynamicModel["label"]);
AssertType<EmptyProbe>(Functions.RoundTripEmptyProbe(new EmptyProbe()));
var genericProbe = Functions.RoundTripGenericProbe(new GenericProbe<long>
{
    Value = 42,
    History = new() { 1, 2, 3 },
});
AssertEqual(42L, genericProbe.Value);
AssertSequenceEqual(new long[] { 1, 2, 3 }, genericProbe.History);
var createdGenericProbe = GenericProbe<long>.Create(84);
AssertEqual(84L, createdGenericProbe.Value);
AssertSequenceEqual(new long[] { 84 }, createdGenericProbe.History);
AssertEqual(21L, genericProbe.Echo(21));
AssertEqual("converted", genericProbe.Convert("converted"));
AssertEqual(34L, await genericProbe.EchoAsync(34));
AssertEqual(true, await genericProbe.ConvertAsync(true));
AssertEqual(ProbeLabel.Good, Functions.RoundTripLabelOrModel(ProbeLabel.Good).AsT0);
AssertEqual("model", Functions.RoundTripLabelOrModel(model).AsT1.Name);

var createdModel = ProbeModel.Create("created");
AssertEqual("created", createdModel.Name);
AssertEqual("created", createdModel.Who());
AssertEqual("echo", createdModel.Echo("echo"));
AssertEqual(1L, createdModel.AddToCount());
AssertEqual(7L, createdModel.AddToCount(7));

var asyncCreatedModel = await ProbeModel.CreateAsync("async-created");
AssertEqual("async-created", await asyncCreatedModel.WhoAsync());
AssertEqual("async-echo", await asyncCreatedModel.EchoAsync("async-echo"));
AssertEqual(1L, await asyncCreatedModel.AddToCountAsync());
AssertEqual(9L, await asyncCreatedModel.AddToCountAsync(9));

await AssertHardExit(0);
await AssertHardExit(23);

static void AssertEqual<T>(T expected, T actual)
{
    if (!EqualityComparer<T>.Default.Equals(expected, actual))
    {
        throw new InvalidOperationException($"Expected {expected}, got {actual}.");
    }
}

static void AssertSequenceEqual<T>(IEnumerable<T> expected, IEnumerable<T> actual)
{
    if (!expected.SequenceEqual(actual))
    {
        throw new InvalidOperationException("Sequence values differ.");
    }
}

static T AssertType<T>(object? value)
{
    return value is T typed
        ? typed
        : throw new InvalidOperationException($"Expected {typeof(T)}, got {value?.GetType()}.");
}

static async Task AssertCanceled(Func<Task> action)
{
    try
    {
        await action();
        throw new InvalidOperationException("Expected the BAML call to be canceled.");
    }
    catch (OperationCanceledException)
    {
    }
}

static async Task AssertHardExit(int expectedCode)
{
    var executable = Environment.ProcessPath
        ?? throw new InvalidOperationException("The current executable path is unavailable.");
    using var process = Process.Start(new ProcessStartInfo
    {
        FileName = executable,
        ArgumentList = { "--exit-probe", expectedCode.ToString() },
        RedirectStandardOutput = true,
        RedirectStandardError = true,
        UseShellExecute = false,
    }) ?? throw new InvalidOperationException("Failed to start the hard-exit child process.");

    var stdout = process.StandardOutput.ReadToEndAsync();
    var stderr = process.StandardError.ReadToEndAsync();
    await process.WaitForExitAsync();
    if (process.ExitCode != expectedCode)
    {
        throw new InvalidOperationException(
            $"Hard-exit child returned {process.ExitCode}, expected {expectedCode}. "
            + $"stdout={await stdout}; stderr={await stderr}");
    }

    if ((await stderr).Contains("returned unexpectedly", StringComparison.Ordinal))
    {
        throw new InvalidOperationException("baml.sys.exit returned instead of terminating the child.");
    }
}
