using System.Collections.ObjectModel;

namespace Baml;

public enum BamlClientType
{
    Primitive = 1,
    Fallback = 2,
    RoundRobin = 3,
}

public sealed class BamlRetryPolicy
{
    public BamlRetryPolicy(
        long maxRetries,
        long? initialDelayMilliseconds = null,
        double? multiplier = null,
        long? maxDelayMilliseconds = null)
    {
        MaxRetries = maxRetries;
        InitialDelayMilliseconds = initialDelayMilliseconds;
        Multiplier = multiplier;
        MaxDelayMilliseconds = maxDelayMilliseconds;
    }

    public long MaxRetries { get; }

    public long? InitialDelayMilliseconds { get; }

    public double? Multiplier { get; }

    public long? MaxDelayMilliseconds { get; }
}

public sealed class BamlClient
{
    private readonly IReadOnlyList<BamlClient> _subClients;

    public BamlClient(
        string name,
        BamlClientType clientType,
        IReadOnlyList<BamlClient>? subClients = null,
        BamlRetryPolicy? retry = null,
        long counter = 0)
    {
        Name = name ?? throw new ArgumentNullException(nameof(name));
        if (!Enum.IsDefined(clientType))
        {
            throw new ArgumentOutOfRangeException(nameof(clientType));
        }

        ClientType = clientType;
        Retry = retry;
        Counter = counter;

        var copy = subClients is null ? [] : subClients.ToArray();
        if (copy.Any(static client => client is null))
        {
            throw new ArgumentException("A BAML client's sub-client list cannot contain null.", nameof(subClients));
        }

        _subClients = new ReadOnlyCollection<BamlClient>(copy);
    }

    public string Name { get; }

    public BamlClientType ClientType { get; }

    public IReadOnlyList<BamlClient> SubClients => _subClients;

    public BamlRetryPolicy? Retry { get; }

    public long Counter { get; }

    public static BamlClient FromShorthand(string shorthand) =>
        new(shorthand, BamlClientType.Primitive);
}
