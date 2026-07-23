namespace Baml.Generated.V1;

public readonly partial struct BamlGeneratedCodecContext
{
    private const string HttpRequestIdentity = "baml.http.Request";
    private const string ClientIdentity = "baml.llm.Client";
    private const string RetryPolicyIdentity = "baml.llm.RetryPolicy";
    private const string ClientTypeIdentity = "baml.llm.ClientType";

    public global::Baml.BamlHttpRequest ReadHttpRequest(BamlGeneratedValue value)
    {
        _ = ReadClass(value, HttpRequestIdentity);
        return Fail<global::Baml.BamlHttpRequest>(
            "The native bridge returned an HTTP request that cannot be represented exactly.",
            "baml.http.Request omits the required request ID and exposes headers as map<string, string> and the body as string; the BamlHttpRequest contract requires an exact correlation ID, ordered duplicate headers, and raw body bytes.");
    }

    public BamlGeneratedValue Client(global::Baml.BamlClient value)
    {
        ArgumentNullException.ThrowIfNull(value);
        var subClients = new List<BamlGeneratedValue>(value.SubClients.Count);
        foreach (global::Baml.BamlClient subClient in value.SubClients)
        {
            subClients.Add(Client(subClient));
        }

        return Class(
            ClientIdentity,
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("name", String(value.Name)),
                new("client_type", ClientType(value.ClientType)),
                new("sub_clients", List(subClients)),
                new(
                    "retry",
                    value.RetryPolicy is null ? Null() : RetryPolicy(value.RetryPolicy)),
                new("counter", Int(value.Counter)),
            });
    }

    public global::Baml.BamlClient ReadClient(BamlGeneratedValue value)
    {
        IReadOnlyDictionary<string, BamlGeneratedValue> fields = ReadExactClass(
            value,
            ClientIdentity,
            ["name", "client_type", "sub_clients", "retry", "counter"]);
        IReadOnlyList<BamlGeneratedValue> encodedSubClients =
            Require(fields["sub_clients"]).ReadList();
        var subClients = new global::Baml.BamlClient[encodedSubClients.Count];
        for (int index = 0; index < subClients.Length; index++)
        {
            subClients[index] = ReadClient(encodedSubClients[index]);
        }

        BamlGeneratedValue retry = fields["retry"];
        return new global::Baml.BamlClient(
            ReadString(fields["name"]),
            ReadClientType(fields["client_type"]),
            subClients,
            retry.IsNull ? null : ReadRetryPolicy(retry),
            ReadInt(fields["counter"]));
    }

    public BamlGeneratedValue RetryPolicy(global::Baml.BamlRetryPolicy value)
    {
        ArgumentNullException.ThrowIfNull(value);
        return Class(
            RetryPolicyIdentity,
            new KeyValuePair<string, BamlGeneratedValue>[]
            {
                new("max_retries", Int(value.MaxRetries)),
                new("initial_delay_ms", NullableInt(value.InitialDelayMilliseconds)),
                new("multiplier", NullableFloat(value.Multiplier)),
                new("max_delay_ms", NullableInt(value.MaxDelayMilliseconds)),
            });
    }

    public global::Baml.BamlRetryPolicy ReadRetryPolicy(BamlGeneratedValue value)
    {
        IReadOnlyDictionary<string, BamlGeneratedValue> fields = ReadExactClass(
            value,
            RetryPolicyIdentity,
            ["max_retries", "initial_delay_ms", "multiplier", "max_delay_ms"]);
        return new global::Baml.BamlRetryPolicy(
            ReadInt(fields["max_retries"]),
            ReadNullableInt(fields["initial_delay_ms"]),
            ReadNullableInt(fields["max_delay_ms"]),
            ReadNullableFloat(fields["multiplier"]));
    }

    public BamlGeneratedValue ClientType(global::Baml.BamlClientType value) =>
        Enum(
            ClientTypeIdentity,
            value switch
            {
                global::Baml.BamlClientType.Primitive => "Primitive",
                global::Baml.BamlClientType.Fallback => "Fallback",
                global::Baml.BamlClientType.RoundRobin => "RoundRobin",
                _ => throw new ArgumentOutOfRangeException(
                    nameof(value),
                    value,
                    "Unknown BAML client type."),
            });

    public global::Baml.BamlClientType ReadClientType(BamlGeneratedValue value) =>
        ReadEnum(value, ClientTypeIdentity) switch
        {
            "Primitive" => global::Baml.BamlClientType.Primitive,
            "Fallback" => global::Baml.BamlClientType.Fallback,
            "RoundRobin" => global::Baml.BamlClientType.RoundRobin,
            string variant => Fail<global::Baml.BamlClientType>(
                "The native bridge returned an unknown BAML client type.",
                $"Enum {ClientTypeIdentity} returned variant {variant}."),
        };

    private IReadOnlyDictionary<string, BamlGeneratedValue> ReadExactClass(
        BamlGeneratedValue value,
        string identity,
        IReadOnlyList<string> expectedFields)
    {
        IReadOnlyDictionary<string, BamlGeneratedValue> fields = ReadClass(value, identity);
        if (fields.Count != expectedFields.Count)
        {
            return Fail<IReadOnlyDictionary<string, BamlGeneratedValue>>(
                "The native bridge returned malformed BAML client metadata.",
                $"Class {identity} expected {expectedFields.Count} fields, received {fields.Count}.");
        }

        foreach (string field in expectedFields)
        {
            if (!fields.ContainsKey(field))
            {
                return Fail<IReadOnlyDictionary<string, BamlGeneratedValue>>(
                    "The native bridge omitted required BAML client metadata.",
                    $"Class {identity} omitted field {field}.");
            }
        }

        return fields;
    }

    private BamlGeneratedValue NullableInt(long? value) =>
        value is long present ? Int(present) : Null();

    private BamlGeneratedValue NullableFloat(double? value) =>
        value is double present ? Float(present) : Null();

    private long? ReadNullableInt(BamlGeneratedValue value) =>
        value.IsNull ? null : ReadInt(value);

    private double? ReadNullableFloat(BamlGeneratedValue value) =>
        value.IsNull ? null : ReadFloat(value);
}
