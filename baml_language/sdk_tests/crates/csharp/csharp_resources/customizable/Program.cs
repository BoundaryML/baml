using System.Net;
using System.Net.Sockets;
using System.Text;
using Baml;
using Functions = BamlSdk.Functions;

var path = Path.Combine(Path.GetTempPath(), $"baml-csharp-file-{Environment.ProcessId}.txt");
await File.WriteAllTextAsync(path, "0123456789");
try
{
    var original = Functions.OpenFile(path);
    using var file = original.Clone();
    original.Dispose();

    try
    {
        _ = original.Read(1);
        throw new InvalidOperationException("A disposed file remained usable.");
    }
    catch (ObjectDisposedException)
    {
    }

    if (file.Read(3) != "012"
        || await file.ReadAsync(3) != "345"
        || file.SeekFrom("start", 0) != 0
        || !file.ReadBytes(2).SequenceEqual("01"u8.ToArray())
        || file.Text() != "23456789")
    {
        throw new InvalidOperationException("The C# file wrapper did not preserve native cursor state.");
    }

    await file.CloseAsync();
}
finally
{
    File.Delete(path);
}

using var listener = new TcpListener(IPAddress.Loopback, 0);
listener.Start();
var endpoint = (IPEndPoint)listener.LocalEndpoint;
var server = ServeResponse(listener);

var response = await Functions.FetchUrlAsync($"http://127.0.0.1:{endpoint.Port}/resource");
using (var retained = response.Clone())
{
    response.Dispose();
    var body = await retained.TextAsync();
    if (retained.StatusCode != 201
        || !retained.Ok
        || !HeaderEquals(retained, "x-baml-test", "resource")
        || body != "hello from csharp resource fixture")
    {
        throw new InvalidOperationException(
            $"The C# HTTP response wrapper returned invalid data: status={retained.StatusCode}; headers={string.Join(", ", retained.Headers.Select(static header => $"{header.Key}={header.Value}"))}; body={body}");
    }
}

try
{
    _ = response.Text();
    throw new InvalidOperationException("A disposed HTTP response remained usable.");
}
catch (ObjectDisposedException)
{
}

await server.WaitAsync(TimeSpan.FromSeconds(10));
Console.WriteLine("C# resource integration passed.");

static bool HeaderEquals(BamlHttpResponse response, string name, string expected) => response.Headers
    .Any(header => string.Equals(header.Key, name, StringComparison.OrdinalIgnoreCase)
        && string.Equals(header.Value, expected, StringComparison.Ordinal));

static async Task ServeResponse(TcpListener listener)
{
    using var client = await listener.AcceptTcpClientAsync().WaitAsync(TimeSpan.FromSeconds(10));
    await using var stream = client.GetStream();
    var suffix = new Queue<byte>(4);
    var buffer = new byte[1];
    while (true)
    {
        if (await stream.ReadAsync(buffer) == 0)
        {
            throw new IOException("The loopback HTTP client disconnected before sending its headers.");
        }

        suffix.Enqueue(buffer[0]);
        if (suffix.Count > 4)
        {
            suffix.Dequeue();
        }

        if (suffix.SequenceEqual("\r\n\r\n"u8.ToArray()))
        {
            break;
        }
    }

    const string body = "hello from csharp resource fixture";
    var payload = Encoding.ASCII.GetBytes(
        $"HTTP/1.1 201 Created\r\nContent-Length: {Encoding.UTF8.GetByteCount(body)}\r\nContent-Type: text/plain\r\nX-Baml-Test: resource\r\nConnection: close\r\n\r\n{body}");
    await stream.WriteAsync(payload);
}
