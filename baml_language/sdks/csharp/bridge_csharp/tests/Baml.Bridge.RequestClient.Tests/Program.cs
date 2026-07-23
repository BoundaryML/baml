using System.Reflection;

using Baml;
using Baml.Generated.V1;

internal static class Program
{
    public static int Main()
    {
        BamlGeneratedCodecContext context = CreateContext();
        VerifyClientTypes(context);
        VerifyRetryPolicy(context);
        VerifyClient(context);
        VerifyRequestFailsClosed(context);
        Console.WriteLine("request_client_codecs=ok");
        return 0;
    }

    private static void VerifyClientTypes(BamlGeneratedCodecContext context)
    {
        foreach ((BamlClientType managed, string wire) in new[]
        {
            (BamlClientType.Primitive, "Primitive"),
            (BamlClientType.Fallback, "Fallback"),
            (BamlClientType.RoundRobin, "RoundRobin"),
        })
        {
            BamlGeneratedValue encoded = context.ClientType(managed);
            Require(
                context.ReadEnum(encoded, "baml.llm.ClientType") == wire
                    && context.ReadClientType(encoded) == managed,
                $"client type {managed} did not round trip");
        }

        BamlGeneratedValue unknown = context.Enum("baml.llm.ClientType", "Future");
        BamlProtocolException error = Expect<BamlProtocolException>(() =>
            _ = context.ReadClientType(unknown));
        Require(
            error.Message.Contains("unknown BAML client type", StringComparison.Ordinal),
            "unknown client type did not fail explicitly");
    }

    private static void VerifyRetryPolicy(BamlGeneratedCodecContext context)
    {
        BamlGeneratedValue carrier = RetryCarrier(
            context,
            maxRetries: 3,
            initialDelay: 25,
            multiplier: 1.75,
            maxDelay: null);
        BamlRetryPolicy retry = context.ReadRetryPolicy(carrier);
        Require(
            retry.MaxRetries == 3
                && retry.InitialDelayMilliseconds == 25
                && retry.Multiplier == 1.75
                && retry.MaxDelayMilliseconds is null,
            "retry-policy decode changed");

        BamlGeneratedValue encoded = context.RetryPolicy(retry);
        IReadOnlyDictionary<string, BamlGeneratedValue> fields = context.ReadClass(
            encoded,
            "baml.llm.RetryPolicy");
        Require(
            fields.Count == 4
                && context.ReadInt(fields["max_retries"]) == 3
                && fields["max_delay_ms"].IsNull
                && context.ReadRetryPolicy(encoded).Equals(retry),
            "retry-policy encode changed");
    }

    private static void VerifyClient(BamlGeneratedCodecContext context)
    {
        BamlGeneratedValue child = ClientCarrier(
            context,
            "openai/gpt-4o",
            BamlClientType.Primitive,
            [],
            retry: null,
            counter: 0);
        BamlGeneratedValue retry = RetryCarrier(
            context,
            maxRetries: 2,
            initialDelay: null,
            multiplier: null,
            maxDelay: 5000);
        BamlGeneratedValue fallback = ClientCarrier(
            context,
            "fallback",
            BamlClientType.Fallback,
            [child],
            retry,
            counter: 7);

        BamlClient client = context.ReadClient(fallback);
        Require(
            client.Name == "fallback"
                && client.ClientType == BamlClientType.Fallback
                && client.SubClients.Count == 1
                && client.SubClients[0].Name == "openai/gpt-4o"
                && client.RetryPolicy is
                {
                    MaxRetries: 2,
                    InitialDelayMilliseconds: null,
                    Multiplier: null,
                    MaxDelayMilliseconds: 5000,
                }
                && client.Counter == 7,
            "client decode changed");

        BamlGeneratedValue encoded = context.Client(client);
        IReadOnlyDictionary<string, BamlGeneratedValue> fields = context.ReadClass(
            encoded,
            "baml.llm.Client");
        Require(
            fields.Count == 5
                && context.ReadString(fields["name"]) == "fallback"
                && context.ReadClient(encoded).Equals(client),
            "client encode changed");

        BamlGeneratedValue malformed = context.Class(
            "baml.llm.Client",
            [new("name", context.String("missing-fields"))]);
        Expect<BamlProtocolException>(() => _ = context.ReadClient(malformed));
    }

    private static void VerifyRequestFailsClosed(BamlGeneratedCodecContext context)
    {
        BamlGeneratedValue request = context.Class(
            "baml.http.Request",
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("method", context.String("POST")),
                new("url", context.String("https://example.com/?token=sensitive")),
                new(
                    "headers",
                    context.Map(
                        [new("Authorization", context.String("Bearer sensitive"))])),
                new("body", context.String("sensitive body")),
            });
        BamlProtocolException error = Expect<BamlProtocolException>(() =>
            _ = context.ReadHttpRequest(request));
        string diagnostic = SensitiveDiagnostic(error);
        Require(
            error.Message.Contains("cannot be represented exactly", StringComparison.Ordinal)
                && diagnostic.Contains("request ID", StringComparison.Ordinal)
                && diagnostic.Contains("ordered duplicate headers", StringComparison.Ordinal)
                && diagnostic.Contains("raw body bytes", StringComparison.Ordinal)
                && !error.ToString().Contains("sensitive", StringComparison.Ordinal),
            "HTTP request fidelity gap was not explicit and safely redacted");

        Require(
            typeof(BamlGeneratedCodecContext).GetMethod("HttpRequest") is null,
            "BamlHttpRequest must not expose an inbound BAML encoder");
        Expect<BamlProtocolException>(() =>
            _ = context.ReadHttpRequest(
                context.Class("user.Request", Array.Empty<KeyValuePair<string, BamlGeneratedValue>>())));
    }

    private static BamlGeneratedValue RetryCarrier(
        BamlGeneratedCodecContext context,
        long maxRetries,
        long? initialDelay,
        double? multiplier,
        long? maxDelay) =>
        context.Class(
            "baml.llm.RetryPolicy",
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("max_retries", context.Int(maxRetries)),
                new(
                    "initial_delay_ms",
                    initialDelay is long initial ? context.Int(initial) : context.Null()),
                new(
                    "multiplier",
                    multiplier is double factor ? context.Float(factor) : context.Null()),
                new(
                    "max_delay_ms",
                    maxDelay is long maximum ? context.Int(maximum) : context.Null()),
            });

    private static BamlGeneratedValue ClientCarrier(
        BamlGeneratedCodecContext context,
        string name,
        BamlClientType clientType,
        IReadOnlyList<BamlGeneratedValue> subClients,
        BamlGeneratedValue? retry,
        long counter) =>
        context.Class(
            "baml.llm.Client",
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("name", context.String(name)),
                new("client_type", context.ClientType(clientType)),
                new("sub_clients", context.List(subClients)),
                new("retry", retry ?? context.Null()),
                new("counter", context.Int(counter)),
            });

    private static BamlGeneratedCodecContext CreateContext()
    {
        BamlGeneratedRegistry registry = BamlGeneratedContract
            .CreateRegistryBuilder(BamlGeneratedContract.Version)
            .Build();
        ConstructorInfo constructor = typeof(BamlGeneratedCodecContext).GetConstructor(
            BindingFlags.Instance | BindingFlags.NonPublic,
            binder: null,
            [typeof(BamlGeneratedRegistry)],
            modifiers: null)
            ?? throw new InvalidOperationException("generated codec context constructor missing");
        return (BamlGeneratedCodecContext)constructor.Invoke([registry]);
    }

    private static string SensitiveDiagnostic(BamlProtocolException error) =>
        (string)(typeof(BamlProtocolException).GetProperty(
            "SensitiveDiagnostic",
            BindingFlags.Instance | BindingFlags.NonPublic)?.GetValue(error)
            ?? throw new InvalidOperationException("protocol diagnostic missing"));

    private static TException Expect<TException>(Action action)
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

        throw new InvalidOperationException($"Expected {typeof(TException).Name}.");
    }

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidOperationException(message);
        }
    }
}
