using System.Collections.ObjectModel;
using System.Net.Http.Headers;

namespace Baml;

public sealed class BamlHttpRequest
{
    private readonly byte[] body;
    private readonly ReadOnlyCollection<KeyValuePair<string, string>> headers;

    internal BamlHttpRequest(
        string id,
        string method,
        string url,
        IEnumerable<KeyValuePair<string, string>> headers,
        string? contentType,
        ReadOnlyMemory<byte> body)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(id);
        ArgumentException.ThrowIfNullOrWhiteSpace(method);
        ArgumentException.ThrowIfNullOrWhiteSpace(url);
        ArgumentNullException.ThrowIfNull(headers);
        if (contentType is not null)
        {
            ArgumentException.ThrowIfNullOrWhiteSpace(contentType);
        }

        Id = id;
        Method = method;
        Url = url;
        this.headers = new ReadOnlyCollection<KeyValuePair<string, string>>(
            headers.Select(pair =>
                {
                    ArgumentException.ThrowIfNullOrWhiteSpace(pair.Key);
                    ArgumentNullException.ThrowIfNull(pair.Value);
                    return pair;
                })
                .ToArray());
        ContentType = contentType;
        this.body = body.Span.ToArray();
    }

    public string Id { get; }

    public string Method { get; }

    public string Url { get; }

    public IReadOnlyList<KeyValuePair<string, string>> Headers => headers;

    public string? ContentType { get; }

    public ReadOnlyMemory<byte> Body => new(body);

    public HttpRequestMessage ToHttpRequestMessage()
    {
        var request = new HttpRequestMessage(new HttpMethod(Method), Url);
        ByteArrayContent? content = body.Length == 0 && ContentType is null
            ? null
            : new ByteArrayContent(body.ToArray());
        if (content is not null && ContentType is not null)
        {
            content.Headers.ContentType = MediaTypeHeaderValue.Parse(ContentType);
        }

        foreach ((string name, string value) in headers)
        {
            if (!request.Headers.TryAddWithoutValidation(name, value))
            {
                content ??= new ByteArrayContent(body.ToArray());
                if (!content.Headers.TryAddWithoutValidation(name, value))
                {
                    request.Dispose();
                    throw new InvalidDataException($"Invalid HTTP header {name}.");
                }
            }
        }

        request.Content = content;
        return request;
    }

    public override string ToString() =>
        $"BamlHttpRequest(Id={Id}, Method={Method}, Url=<redacted>, Headers={headers.Count}, Body=<redacted>)";
}

public enum BamlClientType : long
{
    Primitive = 1,
    Fallback = 2,
    RoundRobin = 3,
}

public sealed class BamlRetryPolicy : IEquatable<BamlRetryPolicy>
{
    internal BamlRetryPolicy(
        long maxRetries,
        long? initialDelayMilliseconds,
        long? maxDelayMilliseconds,
        double? multiplier)
    {
        RequireNonnegative(maxRetries, nameof(maxRetries));
        if (initialDelayMilliseconds is long initial)
        {
            RequireNonnegative(initial, nameof(initialDelayMilliseconds));
        }
        if (maxDelayMilliseconds is long maximum)
        {
            RequireNonnegative(maximum, nameof(maxDelayMilliseconds));
        }
        if (multiplier is double factor && (!double.IsFinite(factor) || factor <= 0))
        {
            throw new ArgumentOutOfRangeException(nameof(multiplier));
        }

        MaxRetries = maxRetries;
        InitialDelayMilliseconds = initialDelayMilliseconds;
        MaxDelayMilliseconds = maxDelayMilliseconds;
        Multiplier = multiplier;
    }

    public long MaxRetries { get; }

    public long? InitialDelayMilliseconds { get; }

    public long? MaxDelayMilliseconds { get; }

    public double? Multiplier { get; }

    public bool Equals(BamlRetryPolicy? other) =>
        other is not null
        && MaxRetries == other.MaxRetries
        && InitialDelayMilliseconds == other.InitialDelayMilliseconds
        && MaxDelayMilliseconds == other.MaxDelayMilliseconds
        && Multiplier == other.Multiplier;

    public override bool Equals(object? obj) => Equals(obj as BamlRetryPolicy);

    public override int GetHashCode() =>
        HashCode.Combine(MaxRetries, InitialDelayMilliseconds, MaxDelayMilliseconds, Multiplier);

    private static void RequireNonnegative(long value, string parameterName)
    {
        if (value is < BamlInteger.Minimum or > BamlInteger.Maximum || value < 0)
        {
            throw new ArgumentOutOfRangeException(parameterName, value, "Value is outside the nonnegative BAML integer domain.");
        }
    }
}

public sealed class BamlClient : IEquatable<BamlClient>
{
    private readonly ReadOnlyCollection<BamlClient> subClients;

    internal BamlClient(
        string name,
        BamlClientType clientType,
        IEnumerable<BamlClient>? subClients,
        BamlRetryPolicy? retryPolicy,
        long counter)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(name);
        if (!Enum.IsDefined(clientType))
        {
            throw new ArgumentOutOfRangeException(nameof(clientType));
        }
        if (counter is < BamlInteger.Minimum or > BamlInteger.Maximum)
        {
            throw new ArgumentOutOfRangeException(nameof(counter));
        }

        Name = name;
        ClientType = clientType;
        this.subClients = new ReadOnlyCollection<BamlClient>((subClients ?? []).ToArray());
        if (this.subClients.Any(client => client is null))
        {
            throw new ArgumentException("Sub-clients must not contain null.", nameof(subClients));
        }
        RetryPolicy = retryPolicy;
        Counter = counter;
    }

    public string Name { get; }

    public BamlClientType ClientType { get; }

    public IReadOnlyList<BamlClient> SubClients => subClients;

    public BamlRetryPolicy? RetryPolicy { get; }

    public long Counter { get; }

    public static BamlClient FromShorthand(string shorthand)
    {
        ArgumentException.ThrowIfNullOrWhiteSpace(shorthand);
        return new BamlClient(
            shorthand,
            BamlClientType.Primitive,
            subClients: null,
            retryPolicy: null,
            counter: 0);
    }

    public bool Equals(BamlClient? other) =>
        other is not null
        && StringComparer.Ordinal.Equals(Name, other.Name)
        && ClientType == other.ClientType
        && Counter == other.Counter
        && EqualityComparer<BamlRetryPolicy?>.Default.Equals(RetryPolicy, other.RetryPolicy)
        && subClients.SequenceEqual(other.subClients);

    public override bool Equals(object? obj) => Equals(obj as BamlClient);

    public override int GetHashCode()
    {
        var hash = new HashCode();
        hash.Add(Name, StringComparer.Ordinal);
        hash.Add(ClientType);
        hash.Add(Counter);
        hash.Add(RetryPolicy);
        foreach (BamlClient client in subClients)
        {
            hash.Add(client);
        }

        return hash.ToHashCode();
    }

    public override string ToString() =>
        $"BamlClient(Name={Name}, Type={ClientType}, SubClients={subClients.Count})";
}
