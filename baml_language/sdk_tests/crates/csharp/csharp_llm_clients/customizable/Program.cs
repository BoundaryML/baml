using Baml;
using Functions = BamlSdk.Functions;

var client = new BamlClient("Offline", BamlClientType.Primitive);
var request = Functions.InspectBuildRequest("typed-client-marker", client);

if (!string.Equals(request.Method, "POST", StringComparison.Ordinal)
    || !HeaderEquals(request, "authorization", "Bearer offline-fixture-key")
    || !request.Body.Contains("typed-client-marker", StringComparison.Ordinal))
{
    throw new InvalidOperationException(
        $"The typed client override produced an invalid request: method={request.Method}; headers={string.Join(", ", request.Headers.Select(static header => $"{header.Key}={header.Value}"))}; body={request.Body}");
}

Console.WriteLine("C# typed LLM client integration passed.");

static bool HeaderEquals(BamlHttpRequest request, string name, string expected) => request.Headers
    .Any(header => string.Equals(header.Key, name, StringComparison.OrdinalIgnoreCase)
        && string.Equals(header.Value, expected, StringComparison.Ordinal));
