using CsharpPhase12;
using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Text;

string tempRoot = Path.Combine(
    Path.GetTempPath(),
    $"baml-csharp-resource-state-{Guid.NewGuid():N}");
string tempPath = Path.Combine(tempRoot, "state.txt");
string binaryPath = Path.Combine(tempRoot, "bytes.bin");
string csvPath = Path.Combine(tempRoot, "rows.csv");
Baml.Fs.Functions.Mkdir(
    tempRoot,
    new Baml.Fs.MkdirOptions { Recursive = false });
try
{
    Require(
        Baml.Fs.Functions.Write(tempPath, "0123456789") == 10,
        "baml.fs.write returned the wrong byte count");
    _ = Expect<ArgumentOutOfRangeException>(
        () => Baml.Fs.Functions.Open(tempPath, "invalid-mode"));

    using Baml.Fs.File reader = Baml.Fs.Functions.Open(tempPath, "r");
    // Load-bearing parity assertion: separate managed-to-native calls must
    // operate on the same native File object and advance one shared cursor.
    Require(reader.Read(3) == "012", "first native File.read changed");
    Require(await reader.ReadAsync(3) == "345", "native File cursor did not persist across calls");
    Require(reader.SeekFrom("start", 0) == 0, "native File.seek_from did not rewind");
    Require(reader.Read(2) == "01", "native File.read after seek changed");
    Require(reader.Text() == "23456789", "native File.text ignored the current cursor");
    _ = reader.Close();

    Require(Baml.Fs.Functions.Size(tempPath) == 10, "baml.fs.size changed");
    Require(await Baml.Fs.Functions.ReadAsync(tempPath) == "0123456789", "baml.fs.read changed");
    Require(
        await Baml.Fs.Functions.WriteBytesAsync(binaryPath, new byte[] { 0, 1, 0xFE, 0xFF }) == 4,
        "baml.fs.write_bytes returned the wrong byte count");
    using (Baml.Fs.File binary = await Baml.Fs.Functions.OpenAsync(binaryPath, "r+"))
    {
        Require(
            binary.ReadBytes(2).Span.SequenceEqual(new byte[] { 0, 1 }),
            "File.read_bytes changed");
        Require(binary.Write("AZ") == 2, "File.write changed");
        Require(binary.SeekFrom("start", 0) == 0, "binary File.seek_from changed");
        Require(
            (await binary.BytesAsync()).Span.SequenceEqual(new byte[] { 0, 1, (byte)'A', (byte)'Z' }),
            "File.bytes did not preserve raw data and cursor state");
        Require(binary.SeekFrom("end", 0) == 4, "binary File end seek changed");
        Require(
            await binary.WriteBytesAsync(new byte[] { 5, 6 }) == 2,
            "File.write_bytes changed");
        _ = await binary.CloseAsync();
    }

    IReadOnlyList<Baml.Fs.DirEntry> entries = Baml.Fs.Functions.ReadDir(tempRoot);
    Require(
        entries.Any(entry => entry.Name == "state.txt" && entry.IsFile && !entry.IsDir)
            && entries.Any(entry => entry.Name == "bytes.bin" && entry.IsFile),
        "baml.fs.read_dir lost structural entry fields");

    using Baml.Glob.Glob scanningGlob = await Baml.Glob.Functions.NewAsync("*.txt");
    Require(
        scanningGlob.Scan(tempRoot).Any(path => path.EndsWith("state.txt", StringComparison.Ordinal)),
        "Glob.scan(string) did not find the native filesystem entry");
    Require(
        (await scanningGlob.ScanAsync(
            new Baml.Glob.ScanOptions
            {
                Cwd = tempRoot,
                Dot = false,
                Absolute = false,
                FollowSymlinks = false,
                ThrowErrorOnBrokenSymlink = true,
                OnlyFiles = true,
            })).Contains("state.txt"),
        "Glob.scan(ScanOptions) changed");

    string nested = Path.Combine(tempRoot, "nested", "child");
    await Baml.Fs.Functions.MkdirAsync(
        nested,
        new Baml.Fs.MkdirOptions { Recursive = true });
    Require(Baml.Fs.Functions.Exists(nested), "recursive baml.fs.mkdir changed");
    _ = Baml.Fs.Functions.RemoveDir(nested);
    _ = await Baml.Fs.Functions.RemoveDirAllAsync(Path.Combine(tempRoot, "nested"));
}
finally
{
    if (Baml.Fs.Functions.Exists(tempRoot))
    {
        _ = Baml.Fs.Functions.RemoveDirAll(tempRoot);
    }
}

Baml.Glob.Glob glob = Baml.Glob.Functions.New("*.txt");
Require(
    glob.Matches("notes.txt")
        && !glob.Matches("notes.rs"),
    "native Glob.matches result changed");

Baml.Glob.Glob clonedGlob = glob.Clone();
glob.Dispose();
Require(glob.IsClosed && !clonedGlob.IsClosed, "resource clone did not own an independent lease");
Require(
    await clonedGlob.MatchesAsync("cloned.txt"),
    "cloned native Glob stopped working after the original was disposed");

Baml.Http.Response response = Baml.Http.Response.New(
    201,
    new Dictionary<string, string>
    {
        ["content-type"] = "text/plain; charset=utf-8",
    },
    Encoding.UTF8.GetBytes("offline response body"));
Require(
    await response.TextAsync() == "offline response body",
    "offline Response.new body did not round trip through its native carrier");
Require(response.Ok(), "native Response.ok changed");
Require(
    response.StatusCode == 201
        && response.Headers["content-type"] == "text/plain; charset=utf-8"
        && response.Url == "",
    "Response public fields were not preserved with the live native handle");
Require(
    response.Bytes().Span.SequenceEqual(Encoding.UTF8.GetBytes("offline response body")),
    "native Response.bytes changed");

using Baml.Http.Response streamingResponse = await Baml.Http.Response.NewStreamingAsync(
    202,
    new Dictionary<string, string>
    {
        ["content-type"] = "application/octet-stream",
    });
_ = await streamingResponse.EndAsync();
_ = streamingResponse.End();
Require(
    streamingResponse.StatusCode == 202
        && streamingResponse.Headers["content-type"] == "application/octet-stream",
    "Response streaming construction, end, idempotence, or public fields changed");
_ = Expect<Baml.BamlErrorException>(
    () => Baml.Http.TlsConfig.New(
        Encoding.UTF8.GetBytes("not a certificate"),
        Encoding.UTF8.GetBytes("not a private key"),
        allowTls12: false,
        handshakeTimeout: Baml.Time.Duration.FromSeconds(1L)));

if (CanCreateLoopbackSockets())
{
    using CancellationTokenSource networkTimeout = new(TimeSpan.FromSeconds(10));
    using System.Net.Sockets.TcpListener systemListener = new(IPAddress.Loopback, 0);
    systemListener.Start();
    int systemListenerPort = ((IPEndPoint)systemListener.LocalEndpoint).Port;
    Task<TcpClient> peerAccept = systemListener.AcceptTcpClientAsync(networkTimeout.Token).AsTask();
    using Baml.Net.TcpStream outbound = await Baml.Net.TcpStream.ConnectAsync(
        $"127.0.0.1:{systemListenerPort}",
        cancellationToken: networkTimeout.Token);
    using TcpClient outboundPeer = await peerAccept;
    using NetworkStream outboundPeerStream = outboundPeer.GetStream();
    await outboundPeerStream.WriteAsync(Encoding.UTF8.GetBytes("from-peer"), networkTimeout.Token);
    Require(
        (await outbound.ReadAsync(cancellationToken: networkTimeout.Token)).Span.SequenceEqual(
            Encoding.UTF8.GetBytes("from-peer")),
        "TcpStream.connect/read did not retain its native socket state");
    _ = outbound.Write(Encoding.UTF8.GetBytes("from-baml"), cancellationToken: networkTimeout.Token);
    byte[] fromBaml = new byte["from-baml".Length];
    await outboundPeerStream.ReadExactlyAsync(fromBaml, networkTimeout.Token);
    Require(
        fromBaml.SequenceEqual(Encoding.UTF8.GetBytes("from-baml")),
        "TcpStream.write changed");
    _ = await outbound.CloseAsync(networkTimeout.Token);
    systemListener.Stop();

    int bamlListenerPort = ReserveTcpPort();
    Baml.Net.TcpListener originalListener = Baml.Net.TcpListener.Bind(
        $"127.0.0.1:{bamlListenerPort}",
        networkTimeout.Token);
    using Baml.Net.TcpListener clonedListener = originalListener.Clone();
    originalListener.Dispose();
    Task<Baml.Net.TcpStream> acceptedStream = clonedListener.AcceptAsync(networkTimeout.Token);
    using TcpClient inboundPeer = new();
    await inboundPeer.ConnectAsync(IPAddress.Loopback, bamlListenerPort, networkTimeout.Token);
    using Baml.Net.TcpStream inbound = await acceptedStream;
    await inboundPeer.GetStream().WriteAsync(Encoding.UTF8.GetBytes("accepted"), networkTimeout.Token);
    Require(
        inbound.Read(cancellationToken: networkTimeout.Token).Span.SequenceEqual(
            Encoding.UTF8.GetBytes("accepted")),
        "TcpListener.accept returned a stream without live native state");
    _ = inbound.Close();
    _ = await clonedListener.CloseAsync(networkTimeout.Token);

    int bamlUdpPort = ReserveUdpPort();
    using UdpClient udpPeer = new(new IPEndPoint(IPAddress.Loopback, 0));
    int udpPeerPort = ((IPEndPoint)udpPeer.Client.LocalEndPoint!).Port;
    Baml.Net.UdpSocket originalUdp = await Baml.Net.UdpSocket.BindAsync(
        $"127.0.0.1:{bamlUdpPort}",
        networkTimeout.Token);
    using Baml.Net.UdpSocket udp = originalUdp.Clone();
    originalUdp.Dispose();
    Require(
        await udp.SendToAsync(
            Encoding.UTF8.GetBytes("datagram-out"),
            $"127.0.0.1:{udpPeerPort}",
            cancellationToken: networkTimeout.Token) == "datagram-out".Length,
        "UdpSocket.send_to returned the wrong byte count");
    UdpReceiveResult outboundDatagram = await udpPeer.ReceiveAsync(networkTimeout.Token);
    Require(
        outboundDatagram.Buffer.SequenceEqual(Encoding.UTF8.GetBytes("datagram-out")),
        "UdpSocket.send_to changed the datagram payload");
    _ = await udpPeer.SendAsync(
        Encoding.UTF8.GetBytes("datagram-in"),
        new IPEndPoint(IPAddress.Loopback, bamlUdpPort),
        networkTimeout.Token);
    Baml.Net.Datagram inboundDatagram = udp.RecvFrom(cancellationToken: networkTimeout.Token);
    Require(
        inboundDatagram.Data.Span.SequenceEqual(Encoding.UTF8.GetBytes("datagram-in"))
            && inboundDatagram.Addr.Contains($":{udpPeerPort}", StringComparison.Ordinal),
        "UdpSocket.recv_from lost payload or sender fields");
    _ = udp.Close();

    using Baml.Http.Response servedResponse = Baml.Http.Response.New(
        200,
        new Dictionary<string, string> { ["content-type"] = "text/plain" },
        Encoding.UTF8.GetBytes("served-by-baml"));
    using Baml.Http.Server bamlServer = await Baml.Http.Server.BindAsync(
        "127.0.0.1:0",
        networkTimeout.Token);
    TaskCompletionSource<Baml.Http.Request> receivedRequest = new(
        TaskCreationOptions.RunContinuationsAsynchronously);
    using CancellationTokenSource serveCancellation = CancellationTokenSource.CreateLinkedTokenSource(
        networkTimeout.Token);
    Task serveTask = bamlServer.ServeAsync(
        (request, _) =>
        {
            receivedRequest.TrySetResult(request);
            return Task.FromResult(servedResponse.Clone());
        },
        headerReadTimeout: Baml.Time.Duration.FromSeconds(2L),
        cancellationToken: serveCancellation.Token);
    using (System.Net.Http.HttpClient client = new())
    {
        using HttpResponseMessage served = await client.GetAsync(
            $"http://{bamlServer.Addr}/resource?q=1",
            networkTimeout.Token);
        string servedBody = await served.Content.ReadAsStringAsync(networkTimeout.Token);
        Require(
            served.IsSuccessStatusCode,
            $"Server.serve returned {(int)served.StatusCode}: {servedBody}");
        Require(servedBody == "served-by-baml", "Server.serve response changed");
    }
    Baml.Http.Request serverRequest = await receivedRequest.Task.WaitAsync(networkTimeout.Token);
    Require(
        serverRequest.Method == "GET"
            && serverRequest.Url == "/resource?q=1"
            && serverRequest.Headers.ContainsKey("host")
            && serverRequest.Body == "",
        "Server.serve lost Request structural fields");
    serveCancellation.Cancel();
    _ = await ExpectAsync<OperationCanceledException>(serveTask);

    using System.Net.Sockets.TcpListener ssePeer = new(IPAddress.Loopback, 0);
    ssePeer.Start();
    int ssePort = ((IPEndPoint)ssePeer.LocalEndpoint).Port;
    Task ssePeerTask = ServeOneSseResponseAsync(ssePeer, networkTimeout.Token);
    string sseUrl = $"http://127.0.0.1:{ssePort}/events";
    using Baml.Http.SseStream sse = await Baml.Http.Functions.FetchSseAsync(
        new Baml.Http.Request
        {
            Method = "GET",
            Url = sseUrl,
            Headers = new Dictionary<string, string>(),
            Body = "",
        },
        networkTimeout.Token);
    string? firstEvent = await sse.NextAsync(networkTimeout.Token);
    Require(
        sse.Url == sseUrl
            && firstEvent is not null
            && firstEvent.Contains("\"data\":\"from-csharp\"", StringComparison.Ordinal)
            && sse.Next(networkTimeout.Token) is null,
        "SseStream url/next/EOF state changed");
    _ = await sse.CloseAsync(networkTimeout.Token);
    await ssePeerTask;
    ssePeer.Stop();
}

const string csvSource = "name,count\nalpha,7\nbeta,8\n";
CsvRow[] typedRows =
{
    new CsvRow { Name = "alpha", Count = 7 },
    new CsvRow { Name = "beta", Count = 8 },
};
IReadOnlyList<IReadOnlyList<string>> parsedRows = Baml.Csv.Functions.Parse(csvSource);
Require(
    parsedRows.Count == 2
        && parsedRows[0].SequenceEqual(new[] { "alpha", "7" })
        && (await Baml.Csv.Functions.ParseAsync(
            new ReadOnlyMemory<byte>(Encoding.UTF8.GetBytes(csvSource)))).Count == 2,
    "baml.csv.parse text/bytes arms changed");
IReadOnlyList<CsvRow> decodedRows = await Baml.Csv.Functions.DecodeAsync<CsvRow>(csvSource);
Require(
    decodedRows.Count == 2
        && decodedRows[1].Name == "beta"
        && decodedRows[1].Count == 8,
    "baml.csv.decode<T> changed");
CsvRow decodedOne = Baml.Csv.Functions.DecodeOne<CsvRow>("name,count\nsolo,9\n");
Require(decodedOne.Name == "solo" && decodedOne.Count == 9, "baml.csv.decode_one<T> changed");
Require(
    (await Baml.Csv.Functions.DecodeOptionalAsync<CsvRow>("name,count\n")).IsNull,
    "baml.csv.decode_optional<T> did not preserve null");
Require(
    Baml.Csv.Functions.Stringify(typedRows) == csvSource
        && (await Baml.Csv.Functions.StringifyAsync(typedRows)) == csvSource,
    "baml.csv.stringify<T> changed");
string typedMarkdown = Baml.Csv.Functions.ToMarkdown(typedRows);
Require(
    typedMarkdown.Contains("| name | count |", StringComparison.OrdinalIgnoreCase)
        && typedMarkdown.Contains("| alpha | 7 |", StringComparison.Ordinal),
    "baml.csv.to_markdown<T> changed");

using (Baml.Csv.CsvWriter rawWriter = await Baml.Csv.Functions.BufferAsync())
{
    _ = await rawWriter.WriteHeaderAsync(new[] { "name", "count" });
    _ = rawWriter.WriteRecord(
        new Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>?[]
        {
            Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>.FromT3("raw"),
            Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>.FromT0(11),
        });
    Require(
        await rawWriter.RecordsWrittenAsync() == 1
            && await rawWriter.TextAsync() == "name,count\nraw,11\n",
        "CsvWriter raw header/record methods changed");
}

IReadOnlyList<IReadOnlyList<Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>?>> rawRecords =
    new IReadOnlyList<Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>?>[]
    {
        new Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>?[]
        {
            Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>.FromT3("gamma"),
            Baml.BamlUnion<long, System.Numerics.BigInteger, double, string, bool>.FromT0(12),
        },
    };
Require(
    await Baml.Csv.Functions.StringifyRecordsAsync(rawRecords) == "gamma,12\n",
    "baml.csv.stringify_records changed");
Require(
    (await Baml.Csv.Functions.ToMarkdownRecordsAsync(
        new IReadOnlyList<string>[] { new[] { "gamma", "12" } },
        new[] { "name", "count" }))
        .Contains("| gamma | 12 |", StringComparison.Ordinal),
    "baml.csv.to_markdown_records changed");

csvPath = Path.Combine(Path.GetTempPath(), $"baml-csharp-csv-{Guid.NewGuid():N}.csv");
string createdCsvPath = Path.Combine(Path.GetTempPath(), $"baml-csharp-csv-created-{Guid.NewGuid():N}.csv");
string wrappedCsvPath = Path.Combine(Path.GetTempPath(), $"baml-csharp-csv-wrapped-{Guid.NewGuid():N}.csv");
try
{
    Require(
        Baml.Csv.Functions.Write(csvPath, typedRows) == Encoding.UTF8.GetByteCount(csvSource),
        "baml.csv.write<T> returned the wrong byte count");
    IReadOnlyList<CsvRow> fileRows = await Baml.Csv.Functions.ReadAsync<CsvRow>(csvPath);
    Require(fileRows.Count == 2 && fileRows[0].Name == "alpha", "baml.csv.read<T> changed");
    using (Baml.Csv.CsvReader openedReader = await Baml.Csv.Functions.OpenAsync(csvPath))
    {
        Require(openedReader.Headers()!.SequenceEqual(new[] { "name", "count" }), "baml.csv.open changed");
        _ = openedReader.Close();
    }

    using (Baml.Csv.CsvWriter createdWriter = Baml.Csv.Functions.Create(createdCsvPath))
    {
        _ = await createdWriter.WriteRowAsync(new CsvRow { Name = "created", Count = 13 });
        _ = await createdWriter.FlushAsync();
        _ = createdWriter.Close();
    }
    Require(
        Baml.Fs.Functions.Read(createdCsvPath) == "name,count\ncreated,13\n",
        "baml.csv.create changed");

    using Baml.Fs.File wrappedFile = Baml.Fs.Functions.Open(wrappedCsvPath, "w+");
    using (Baml.Csv.CsvWriter wrappedWriter = await Baml.Csv.Functions.WriterAsync(wrappedFile))
    {
        _ = wrappedWriter.WriteRow(new CsvRow { Name = "wrapped", Count = 14 });
        _ = await wrappedWriter.CloseAsync();
    }
    Require(!wrappedFile.IsClosed, "baml.csv.writer incorrectly took ownership of the caller's File");
    Require(wrappedFile.SeekFrom("start", 0) == 0, "wrapped CSV file seek changed");
    Require(wrappedFile.Text() == "name,count\nwrapped,14\n", "baml.csv.writer output changed");
}
finally
{
    foreach (string path in new[] { csvPath, createdCsvPath, wrappedCsvPath })
    {
        if (Baml.Fs.Functions.Exists(path))
        {
            _ = Baml.Fs.Functions.Remove(path);
        }
    }
}

using Baml.Csv.CsvReader csvReader = Baml.Csv.Functions.Reader(
    "name,count\nalpha,7\n");
Require(ReferenceEquals(csvReader, csvReader.Iter()), "CsvReader.Iter did not preserve iterator identity");
IReadOnlyList<string>? headers = await csvReader.HeadersAsync();
Require(
    headers is not null && headers.SequenceEqual(new[] { "name", "count" }),
    "native CsvReader.headers changed");

using Baml.Csv.CsvRows<CsvRow> csvRows = csvReader.Rows<CsvRow>();
Require(
    csvRows.Reader.Headers()!.SequenceEqual(new[] { "name", "count" }),
    "CsvRows<CsvRow>.Reader did not preserve the public resource field");
Baml.BamlUnion<Baml.Iter.Done, CsvRow> typedNext = await csvRows.NextAsync();
Require(
    typedNext.IsT1
        && typedNext.AsT1.Name == "alpha"
        && typedNext.AsT1.Count == 7,
    "CsvRows<CsvRow>.Next did not decode the generic row");
Require(csvRows.Next().IsT0, "CsvRows<CsvRow> did not reach Done");
Require(
    ReferenceEquals(csvRows, csvRows.Iter())
        && csvRows.Iter().Reader.Headers()!.SequenceEqual(new[] { "name", "count" }),
    "CsvRows<CsvRow>.Iter lost the closed generic resource carrier");
Require(
    csvReader.Skipped().Count == 0
        && await csvReader.SkippedCountAsync() == 0
        && csvReader.Position().Record == 1,
    "CsvReader skipped/position state methods changed");

using Baml.Csv.CsvReader recordReader = Baml.Csv.Functions.Reader(
    "name,count\nalpha,7\n");
_ = recordReader.Headers();
Baml.BamlUnion<Baml.Csv.CsvRecord, Baml.Iter.Done> rawNext = recordReader.Next();
Require(rawNext.IsT0, "CsvReader.Next did not return a CsvRecord");
using Baml.Csv.CsvRecord record = rawNext.AsT0;
Require(
    record.Length() == 2
        && record.Fields().SequenceEqual(new[] { "alpha", "7" })
        && record.Position().Record == 0,
    "CsvRecord snapshot methods changed");
Require(
    !record.Get<string>("name").IsNull
        && record.Get<string>("name").Value == "alpha"
        && record.GetAt<long>(1).Value == 7,
    "CsvRecord generic cell access changed");
CsvRow decodedRecord = record.Decode<CsvRow>();
Require(
    decodedRecord.Name == "alpha"
        && decodedRecord.Count == 7
        && record.ToMap()["count"] == "7",
    "CsvRecord generic decode or map projection changed");

using Baml.Csv.CsvWriter writer = Baml.Csv.Functions.Buffer();
_ = writer.WriteRow(new CsvRow { Name = "alpha", Count = 7 });
_ = await writer.WriteRowsAsync(
    new[]
    {
        new CsvRow { Name = "beta", Count = 8 },
    });
Require(
    writer.RecordsWritten() == 2
        && writer.Text() == "name,count\nalpha,7\nbeta,8\n",
    "CsvWriter typed writes or buffer text changed");
_ = writer.Flush();
_ = writer.Close();

using Baml.Spawn.TaskGroup group = Baml.Spawn.TaskGroup.New(2, "phase12");
Require(group.Limit() == 2 && group.Name() == "phase12", "TaskGroup construction changed");
_ = group.SetLimit(1);
Require(
    group.Limit() == 1
        && group.ActiveCount() == 0
        && group.QueuedCount() == 0
        && group.Cancel() == 0,
    "TaskGroup state methods changed");

using Baml.Spawn.CancelToken token = Baml.Spawn.CancelToken.New();
using Baml.Spawn.CancelToken combined = Baml.Spawn.CancelToken.Any(new[] { token });
Require(!token.IsCancelled() && !combined.IsCancelled(), "CancelToken started cancelled");
Require(token.Cancel() == 1, "CancelToken.cancel did not transition native state");
await RequireEventuallyAsync(
    () => combined.IsCancelled(),
    TimeSpan.FromSeconds(1),
    "CancelToken.any did not propagate native cancellation");

using Boundary.LocalId localId = Boundary.Functions.Id();
using Boundary.LocalId capturedId = localId.Capture(inputs: true, output: false, error: true);
Require(!capturedId.IsClosed, "boundary.LocalId.capture returned a closed resource");

_ = csvReader.Close();
_ = recordReader.Close();

clonedGlob.Dispose();
ObjectDisposedException disposedClone = Expect<ObjectDisposedException>(() => clonedGlob.Clone());
ObjectDisposedException disposedCall = Expect<ObjectDisposedException>(
    () => clonedGlob.Matches("disposed-must-not-dispatch.txt"));
Require(
    disposedClone.ObjectName is not null
        && disposedCall.ObjectName is not null
        && clonedGlob.IsClosed,
    "disposed opaque resource remained usable or dispatchable");

response.Dispose();

Console.WriteLine("csharp_phase12_resources=ok");

static TException Expect<TException>(Action action)
    where TException : Exception
{
    try
    {
        action();
    }
    catch (TException exception)
    {
        return exception;
    }

    throw new InvalidOperationException($"expected {typeof(TException).Name}");
}

static async Task<TException> ExpectAsync<TException>(Task task)
    where TException : Exception
{
    try
    {
        await task;
    }
    catch (TException exception)
    {
        return exception;
    }

    throw new InvalidOperationException($"expected {typeof(TException).Name}");
}

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}

static async Task RequireEventuallyAsync(
    Func<bool> condition,
    TimeSpan timeout,
    string message)
{
    Stopwatch stopwatch = Stopwatch.StartNew();
    TimeSpan pollInterval = TimeSpan.FromMilliseconds(10);
    while (true)
    {
        TimeSpan remaining = timeout - stopwatch.Elapsed;
        if (remaining <= TimeSpan.Zero)
        {
            throw new InvalidOperationException(message);
        }

        if (condition())
        {
            return;
        }

        remaining = timeout - stopwatch.Elapsed;
        if (remaining <= TimeSpan.Zero)
        {
            throw new InvalidOperationException(message);
        }

        await Task.Delay(remaining < pollInterval ? remaining : pollInterval);
    }
}

static int ReserveTcpPort()
{
    using System.Net.Sockets.TcpListener listener = new(IPAddress.Loopback, 0);
    listener.Start();
    int port = ((IPEndPoint)listener.LocalEndpoint).Port;
    listener.Stop();
    return port;
}

static int ReserveUdpPort()
{
    using UdpClient socket = new(new IPEndPoint(IPAddress.Loopback, 0));
    return ((IPEndPoint)socket.Client.LocalEndPoint!).Port;
}

static bool CanCreateLoopbackSockets()
{
    try
    {
        using Socket socket = new(
            AddressFamily.InterNetwork,
            SocketType.Stream,
            ProtocolType.Tcp);
        return true;
    }
    catch (SocketException error) when (error.SocketErrorCode == SocketError.AccessDenied)
    {
        return false;
    }
}

static async Task ServeOneSseResponseAsync(
    System.Net.Sockets.TcpListener listener,
    CancellationToken cancellationToken)
{
    using TcpClient client = await listener.AcceptTcpClientAsync(cancellationToken);
    using NetworkStream stream = client.GetStream();
    byte[] requestBuffer = new byte[4096];
    int requestLength = 0;
    while (requestLength < requestBuffer.Length)
    {
        int read = await stream.ReadAsync(
            requestBuffer.AsMemory(requestLength),
            cancellationToken);
        if (read == 0)
        {
            break;
        }
        requestLength += read;
        if (Encoding.ASCII.GetString(requestBuffer, 0, requestLength).Contains("\r\n\r\n"))
        {
            break;
        }
    }

    const string body = "event: message\ndata: from-csharp\n\n";
    string response =
        "HTTP/1.1 200 OK\r\n"
        + "Content-Type: text/event-stream\r\n"
        + $"Content-Length: {Encoding.UTF8.GetByteCount(body)}\r\n"
        + "Connection: close\r\n\r\n"
        + body;
    await stream.WriteAsync(Encoding.UTF8.GetBytes(response), cancellationToken);
}
