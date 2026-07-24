using System.Collections.ObjectModel;
using System.Net.Http.Headers;
using System.Runtime.InteropServices;

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
        this.headers = new(
            headers.Select(
                    pair =>
                    {
                        ArgumentException.ThrowIfNullOrWhiteSpace(
                            pair.Key);
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

    public IReadOnlyList<KeyValuePair<string, string>> Headers =>
        headers;

    public string? ContentType { get; }

    public ReadOnlyMemory<byte> Body =>
        new(body);

    public HttpRequestMessage ToHttpRequestMessage()
    {
        HttpRequestMessage request = new(
            new HttpMethod(Method),
            Url);
        ByteArrayContent? content =
            body.Length == 0 && ContentType is null
                ? null
                : new ByteArrayContent(body.ToArray());
        if (content is not null && ContentType is not null)
        {
            content.Headers.ContentType =
                MediaTypeHeaderValue.Parse(ContentType);
        }

        foreach ((string name, string value) in headers)
        {
            if (!request.Headers.TryAddWithoutValidation(name, value))
            {
                content ??= new ByteArrayContent(body.ToArray());
                Require(
                    content.Headers.TryAddWithoutValidation(
                        name,
                        value),
                    $"invalid header {name}");
            }
        }

        request.Content = content;
        return request;
    }

    public override string ToString() =>
        $"BamlHttpRequest(Id={Id}, Method={Method}, Url=<redacted>, Headers={headers.Count}, Body=<redacted>)";

    private static void Require(bool condition, string message)
    {
        if (!condition)
        {
            throw new InvalidDataException(message);
        }
    }
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
        BamlInteger.Require(maxRetries, nameof(maxRetries));
        if (maxRetries < 0)
        {
            throw new ArgumentOutOfRangeException(nameof(maxRetries));
        }

        if (initialDelayMilliseconds is long initial)
        {
            BamlInteger.Require(
                initial,
                nameof(initialDelayMilliseconds));
            if (initial < 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(initialDelayMilliseconds));
            }
        }

        if (maxDelayMilliseconds is long maximum)
        {
            BamlInteger.Require(
                maximum,
                nameof(maxDelayMilliseconds));
            if (maximum < 0)
            {
                throw new ArgumentOutOfRangeException(
                    nameof(maxDelayMilliseconds));
            }
        }

        if (multiplier is double factor
            && (!Double.IsFinite(factor) || factor <= 0))
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
        && InitialDelayMilliseconds
            == other.InitialDelayMilliseconds
        && MaxDelayMilliseconds == other.MaxDelayMilliseconds
        && Multiplier == other.Multiplier;

    public override bool Equals(object? obj) =>
        Equals(obj as BamlRetryPolicy);

    public override int GetHashCode() =>
        HashCode.Combine(
            MaxRetries,
            InitialDelayMilliseconds,
            MaxDelayMilliseconds,
            Multiplier);
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
        if (!Enum.IsDefined(clientType)
            || clientType == 0)
        {
            throw new ArgumentOutOfRangeException(nameof(clientType));
        }

        BamlInteger.Require(counter, nameof(counter));
        Name = name;
        ClientType = clientType;
        this.subClients = new(
            (subClients ?? []).ToArray());
        if (this.subClients.Any(client => client is null))
        {
            throw new ArgumentException(
                "sub-clients must not contain null",
                nameof(subClients));
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
        && EqualityComparer<BamlRetryPolicy?>.Default.Equals(
            RetryPolicy,
            other.RetryPolicy)
        && subClients.SequenceEqual(other.subClients);

    public override bool Equals(object? obj) =>
        Equals(obj as BamlClient);

    public override int GetHashCode()
    {
        HashCode hash = new();
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

public sealed class BamlHandle : IDisposable
{
    private readonly ProbeSafeHandle handle;

    internal BamlHandle(ProbeSafeHandle handle)
    {
        this.handle = handle;
    }

    public bool IsClosed => handle.IsClosed;

    public BamlHandle Clone()
    {
        bool added = false;
        try
        {
            handle.DangerousAddRef(ref added);
            if (!added)
            {
                throw new ObjectDisposedException(
                    nameof(BamlHandle));
            }

            return new BamlHandle(handle.CloneReference());
        }
        finally
        {
            if (added)
            {
                handle.DangerousRelease();
            }
        }
    }

    public void Dispose() => handle.Dispose();

    internal static BamlHandle CreateForProbe() =>
        new(ProbeSafeHandle.Create());

    internal T LeaseForProbe<T>(Func<long, T> operation)
    {
        ArgumentNullException.ThrowIfNull(operation);
        bool added = false;
        try
        {
            handle.DangerousAddRef(ref added);
            if (!added)
            {
                throw new ObjectDisposedException(nameof(BamlHandle));
            }

            return operation(handle.DangerousGetHandle().ToInt64());
        }
        finally
        {
            if (added)
            {
                handle.DangerousRelease();
            }
        }
    }

    internal sealed class ProbeSafeHandle : SafeHandle
    {
        private ProbeSafeHandle(long identity)
            : base(IntPtr.Zero, ownsHandle: true)
        {
            SetHandle(new IntPtr(identity));
        }

        public override bool IsInvalid => handle == IntPtr.Zero;

        internal static ProbeSafeHandle Create() =>
            new(NativeReferenceTable.Create());

        internal ProbeSafeHandle CloneReference()
        {
            long identity = handle.ToInt64();
            NativeReferenceTable.Clone(identity);
            return new ProbeSafeHandle(identity);
        }

        protected override bool ReleaseHandle()
        {
            NativeReferenceTable.Release(handle.ToInt64());
            return true;
        }
    }
}

internal static class NativeReferenceTable
{
    private static readonly object Gate = new();
    private static readonly Dictionary<long, int> References = [];
    private static long nextIdentity;
    private static int releases;

    internal static int Releases => Volatile.Read(ref releases);

    internal static long Create()
    {
        lock (Gate)
        {
            long identity = checked(++nextIdentity);
            References.Add(identity, 1);
            return identity;
        }
    }

    internal static void Clone(long identity)
    {
        lock (Gate)
        {
            References[identity] = checked(
                References[identity] + 1);
        }
    }

    internal static void Release(long identity)
    {
        lock (Gate)
        {
            int remaining = References[identity] - 1;
            if (remaining == 0)
            {
                References.Remove(identity);
            }
            else
            {
                References[identity] = remaining;
            }
        }

        Interlocked.Increment(ref releases);
    }
}

internal static class BamlInteger
{
    internal const long Min = -4_611_686_018_427_387_904;
    internal const long Max = 4_611_686_018_427_387_903;

    internal static void Require(long value, string parameterName)
    {
        if (value is < Min or > Max)
        {
            throw new ArgumentOutOfRangeException(
                parameterName,
                value,
                "value is outside the BAML integer domain");
        }
    }
}
