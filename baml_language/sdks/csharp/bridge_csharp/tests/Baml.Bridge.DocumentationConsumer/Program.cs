using Baml;
using CsharpBasicCalls;

const string Text = "héllo\0雪";
using var cancellation = new CancellationTokenSource(TimeSpan.FromSeconds(10));

string synchronous = Functions.BasicCalls(
    flag: true,
    count: 42,
    ratio: 1.25,
    text: Text,
    nullable: null,
    cancellation.Token);
string asynchronous = await Functions.BasicCallsAsync(
    flag: false,
    count: -17,
    ratio: -2.5,
    text: Text,
    nullable: "present",
    cancellation.Token);
Require(synchronous == Text && asynchronous == Text, "function example changed");

BamlOptional<string?> omitted = default;
BamlOptional<string?> explicitNull = BamlOptional<string?>.FromValue(null);
Require(!omitted.IsSet && explicitNull.IsSet && explicitNull.Value is null,
    "optional example changed");

BamlNullable<string> genericNull = BamlNullable.Null<string>();
BamlNullable<string> genericValue = BamlNullable.FromValue("value");
Require(genericNull.IsNull && genericValue.Value == "value", "nullable example changed");

BamlUnion<long, string> selected = BamlUnion<long, string>.FromT1("selected");
Require(selected.IsT1 && selected.AsT1 == "selected", "union example changed");

BamlValue dynamicValue = BamlValue.From(42L);
Require(dynamicValue.As<long>() == 42L, "dynamic value example changed");

byte[] source = [0, 1, 2, 255];
BamlImage image = BamlImage.FromBytes(source, "image/png");
source[0] = 99;
Require(
    image.TryGetBytes(out ReadOnlyMemory<byte> imageData, out string? mediaType)
        && imageData.Span[0] == 0
        && mediaType == "image/png",
    "media snapshot example changed");

BamlClient client = BamlClient.FromShorthand("openai/gpt-5");
Require(client.Name == "openai/gpt-5", "client example changed");

Func<long, CancellationToken, Task<long>> callback =
    async (value, token) =>
    {
        await Task.Yield();
        token.ThrowIfCancellationRequested();
        return checked(value * 2);
    };
Require(await callback(21, cancellation.Token) == 42L, "callback example changed");

Func<BamlStream<string?, string>, Func<string?, Task>, CancellationToken, Task<string>>
    streamExample = ConsumeStreamAsync;
Func<BamlHandle, BamlHandle> resourceExample = CloneResource;
GC.KeepAlive(streamExample);
GC.KeepAlive(resourceExample);

IPrimitiveService service = new PrimitiveService();
Require(await service.EchoAsync(Text, cancellation.Token) == Text, "DI example changed");

try
{
    _ = await Functions.BasicCallsAsync(true, 1, 1.0, Text, null, cancellation.Token);
}
catch (BamlTypeMismatchException error)
{
    _ = error.ThrownValue;
    throw;
}
catch (BamlErrorException error)
{
    _ = error.Trace;
    throw;
}
catch (BamlPanicException)
{
    throw;
}
catch (BamlOperationCanceledException error)
    when (error.Origin == BamlCancellationOrigin.Caller)
{
    throw;
}

Console.WriteLine("csharp_documentation_consumer=ok");
return 0;

static async Task<TFinal> ConsumeStreamAsync<TPartial, TFinal>(
    BamlStream<TPartial, TFinal> stream,
    Func<TPartial, Task> onPartial,
    CancellationToken cancellationToken)
{
    await using (stream)
    {
        await foreach (TPartial partial in
            stream.WithCancellation(cancellationToken).ConfigureAwait(false))
        {
            await onPartial(partial).ConfigureAwait(false);
        }

        return await stream.GetFinalResponseAsync(cancellationToken).ConfigureAwait(false);
    }
}

static BamlHandle CloneResource(BamlHandle resource) => resource.Clone();

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

public interface IPrimitiveService
{
    Task<string> EchoAsync(string text, CancellationToken cancellationToken);
}

public sealed class PrimitiveService : IPrimitiveService
{
    public Task<string> EchoAsync(string text, CancellationToken cancellationToken) =>
        Functions.BasicCallsAsync(
            flag: true,
            count: 1,
            ratio: 1.0,
            text,
            nullable: null,
            cancellationToken);
}
