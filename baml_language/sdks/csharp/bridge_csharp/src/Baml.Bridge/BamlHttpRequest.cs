using System.Collections.ObjectModel;

namespace Baml;

public sealed class BamlHttpRequest
{
    private readonly IReadOnlyDictionary<string, string> _headers;

    public BamlHttpRequest(
        string method,
        string url,
        IReadOnlyDictionary<string, string> headers,
        string body)
    {
        ArgumentNullException.ThrowIfNull(headers);

        Method = method ?? throw new ArgumentNullException(nameof(method));
        Url = url ?? throw new ArgumentNullException(nameof(url));
        Body = body ?? throw new ArgumentNullException(nameof(body));

        var copy = new Dictionary<string, string>(headers.Count, StringComparer.Ordinal);
        foreach (var (name, value) in headers)
        {
            ArgumentNullException.ThrowIfNull(name);
            ArgumentNullException.ThrowIfNull(value);
            copy.Add(name, value);
        }

        _headers = new ReadOnlyDictionary<string, string>(copy);
    }

    public string Method { get; }

    public string Url { get; }

    public IReadOnlyDictionary<string, string> Headers => _headers;

    public string Body { get; }
}
