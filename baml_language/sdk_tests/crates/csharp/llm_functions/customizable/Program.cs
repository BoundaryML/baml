using Baml;
using System.Diagnostics;
using IpsumFunctions = BamlSdk.Ipsum.Functions;
using LoremFunctions = BamlSdk.Lorem.Functions;
using ReplayFunctions = BamlSdk.Replay.Functions;

if (args is ["--replay-server", var childRecording, var childAddressFile])
{
    try
    {
        _ = ReplayFunctions.ReplayServeUntilShutdown(childRecording, childAddressFile);
    }
    catch (BamlException error)
    {
        Console.Error.WriteLine($"replay server failed: {error.ClassName}: {error.Value}");
        Console.Error.WriteLine(string.Join(Environment.NewLine, error.BamlTrace));
        throw;
    }

    return;
}

if (args is ["--stream-client", var streamKind])
{
    await RunStreamTests(streamKind);
    return;
}

if (args is ["--build-request-client"])
{
    await RunBuildRequestTests();
    return;
}

await RunBuildRequestProcess();
await RunReplay("replay_extract_string", "string");
await RunReplay("replay_extract_doc", "doc");

static async Task RunBuildRequestProcess()
{
    var start = CreateSelfStartInfo("--build-request-client");
    start.Environment["OPENAI_API_KEY"] = "sk-openai-csharp-test";
    start.Environment["ANTHROPIC_API_KEY"] = "sk-anthropic-csharp-test";
    using var child = Process.Start(start)
        ?? throw new InvalidOperationException("Could not start the BAML build-request client process.");
    await child.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(30));
    if (child.ExitCode != 0)
    {
        throw new InvalidOperationException($"The BAML build-request client exited with code {child.ExitCode}.");
    }
}

static async Task RunBuildRequestTests()
{
    var openAi = LoremFunctions.ExtractResumeBuildRequest(
        "csharp-build-request-marker",
        BamlClient.FromShorthand("openai/gpt-4o-mini"));
    if (!string.Equals(openAi.Method, "POST", StringComparison.Ordinal)
        || !HeaderEquals(openAi, "authorization", "Bearer sk-openai-csharp-test")
        || !openAi.Body.Contains("csharp-build-request-marker", StringComparison.Ordinal))
    {
        throw new InvalidOperationException(
            $"The OpenAI build-request companion returned an invalid request: method={openAi.Method}; url={openAi.Url}; headers={string.Join(", ", openAi.Headers.Select(static header => $"{header.Key}={header.Value}"))}; body={openAi.Body}");
    }

    var anthropic = await IpsumFunctions.ClassifySentimentBuildRequestAsync("csharp-anthropic-marker");
    if (!string.Equals(anthropic.Method, "POST", StringComparison.Ordinal)
        || !HeaderEquals(anthropic, "x-api-key", "sk-anthropic-csharp-test")
        || !anthropic.Body.Contains("csharp-anthropic-marker", StringComparison.Ordinal))
    {
        throw new InvalidOperationException(
            $"The Anthropic build-request companion returned an invalid request: method={anthropic.Method}; url={anthropic.Url}; headers={string.Join(", ", anthropic.Headers.Select(static header => $"{header.Key}={header.Value}"))}; body={anthropic.Body}");
    }

    Console.WriteLine("C# build-request integration passed.");
}

static bool HeaderEquals(BamlHttpRequest request, string name, string expected) => request.Headers
    .Any(header => string.Equals(header.Key, name, StringComparison.OrdinalIgnoreCase)
        && string.Equals(header.Value, expected, StringComparison.Ordinal));

static async Task RunStreamTests(string kind)
{
    if (kind == "doc")
    {
        await using var stream = await LoremFunctions.StreamE2eExtractDocStreamAsync(
            "ignored-by-replay-server");
        var partials = 0;
        while (true)
        {
            var next = await stream.NextAsync();
            if (next.IsT1)
            {
                break;
            }

            partials++;
        }

        if (partials < 10)
        {
            throw new InvalidOperationException($"Expected at least 10 class partials, received {partials}.");
        }

        var final = await stream.FinalAsync();
        if (string.IsNullOrWhiteSpace(final.Title))
        {
            throw new InvalidOperationException("The final class stream value had an empty title.");
        }

        Console.WriteLine("C# class stream integration passed.");
        return;
    }

    if (kind != "string")
    {
        throw new ArgumentException($"Unknown stream test kind {kind}.", nameof(kind));
    }

    var prompt = await LoremFunctions.StreamE2eExtractRenderPromptAsync("prompt-marker");
    using (var clone = prompt.Clone())
    {
        prompt.Dispose();
        var text = await clone.TextAsync();
        var messages = clone.Messages();
        if (!text.Contains("prompt-marker", StringComparison.Ordinal)
            || messages.Count == 0
            || !messages.Any(static message => message.Content.Contains("prompt-marker", StringComparison.Ordinal)))
        {
            throw new InvalidOperationException("The rendered prompt AST did not preserve its text and messages.");
        }

        if (!string.Equals(text, clone.ToString(), StringComparison.Ordinal))
        {
            throw new InvalidOperationException("BamlPromptAst.ToString() did not use the readable prompt rendering.");
        }
    }

    try
    {
        _ = prompt.Text();
        throw new InvalidOperationException("A disposed prompt AST remained usable.");
    }
    catch (ObjectDisposedException)
    {
    }

    using (var syncStream = LoremFunctions.StreamE2eExtractStream("ignored-by-replay-server"))
    {
        var partials = 0;
        while (!syncStream.Next().IsT1)
        {
            partials++;
        }

        if (partials < 10 || string.IsNullOrWhiteSpace(syncStream.Final()))
        {
            throw new InvalidOperationException("The synchronous string stream did not produce its expected values.");
        }
    }

    await using (var stream = await CreateStringStream())
    {
        var firstPull = stream.NextAsync();
        var secondPull = stream.NextAsync();
        var initial = await Task.WhenAll(firstPull, secondPull);
        if (initial.Any(static next => next.IsT1))
        {
            throw new InvalidOperationException("The string stream terminated before its first two partials.");
        }

        var partials = initial.Length;
        while (true)
        {
            var next = await stream.NextAsync();
            if (next.IsT1)
            {
                break;
            }

            partials++;
            if (partials >= 10_000)
            {
                throw new InvalidOperationException("The string stream did not terminate.");
            }
        }

        if (partials < 10)
        {
            throw new InvalidOperationException($"Expected at least 10 partials, received {partials}.");
        }

        var firstFinal = await stream.FinalAsync();
        var secondFinal = await stream.FinalAsync();
        if (!string.Equals(firstFinal, secondFinal, StringComparison.Ordinal))
        {
            throw new InvalidOperationException("Repeated Stream.final calls returned different values.");
        }
    }

    await using (var cancellable = await CreateStringStream())
    {
        using var cancellation = new CancellationTokenSource();
        cancellation.Cancel();
        try
        {
            _ = await cancellable.NextAsync(cancellation.Token);
            throw new InvalidOperationException("A pre-canceled stream pull unexpectedly completed.");
        }
        catch (OperationCanceledException)
        {
        }

        if ((await cancellable.NextAsync()).IsT1)
        {
            throw new InvalidOperationException("A pre-canceled pull consumed the stream.");
        }
    }

    await using (var enumerable = await CreateStringStream())
    {
        var partials = 0;
        await foreach (var _ in enumerable)
        {
            partials++;
        }

        if (partials < 10)
        {
            throw new InvalidOperationException($"Expected at least 10 enumerated partials, received {partials}.");
        }

        _ = await enumerable.FinalAsync();
        try
        {
            _ = enumerable.GetAsyncEnumerator();
            throw new InvalidOperationException("The BAML stream allowed a second enumeration.");
        }
        catch (InvalidOperationException error) when (error.Message.Contains("only once", StringComparison.Ordinal))
        {
        }
    }

    var early = await CreateStringStream();
    await using (var enumerator = early.GetAsyncEnumerator())
    {
        _ = await enumerator.MoveNextAsync();
    }

    try
    {
        _ = await early.FinalAsync();
        throw new InvalidOperationException("Early enumerator disposal did not dispose the BAML stream.");
    }
    catch (ObjectDisposedException)
    {
    }

    Console.WriteLine("C# stream integration passed.");
}

static async Task RunReplay(string recordingName, string streamKind)
{
    var sdkTests = FindAncestor("sdk_tests");
    var recording = Path.Combine(
        sdkTests.FullName,
        "fixtures",
        "llm_functions",
        "recordings",
        $"{recordingName}.snap.sse");
    var addressFile = Path.Combine(
        Path.GetTempPath(),
        $"baml-csharp-stream-{Environment.ProcessId}-{streamKind}.addr");
    File.Delete(addressFile);

    using var server = StartSelf("--replay-server", recording, addressFile);
    var address = await WaitForAddress(addressFile, server);
    var clientStart = CreateSelfStartInfo("--stream-client", streamKind);
    clientStart.Environment["BAML_REPLAY_BASE_URL"] = $"http://{address}";
    clientStart.Environment["BAML_REPLAY_API_KEY"] = "replay-test-key";
    using var client = Process.Start(clientStart)
        ?? throw new InvalidOperationException("Could not start the BAML stream client process.");

    try
    {
        await client.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(60));
    }
    finally
    {
        try
        {
            using var http = new HttpClient { Timeout = TimeSpan.FromSeconds(5) };
            _ = await http.PostAsync($"http://{address}/__replay__/shutdown", null);
        }
        catch
        {
        }

        await server.WaitForExitAsync().WaitAsync(TimeSpan.FromSeconds(10));
        File.Delete(addressFile);
    }

    if (client.ExitCode != 0)
    {
        throw new InvalidOperationException($"The BAML stream client exited with code {client.ExitCode}.");
    }

    if (server.ExitCode != 0)
    {
        throw new InvalidOperationException($"The BAML replay server exited with code {server.ExitCode}.");
    }
}

static async Task<BamlStream<string?, string>> CreateStringStream()
{
    try
    {
        return await LoremFunctions.StreamE2eExtractStreamAsync("ignored-by-replay-server");
    }
    catch (BamlException error)
    {
        Console.Error.WriteLine($"stream creation failed: {error.ClassName}: {error.Value}");
        Console.Error.WriteLine(string.Join(Environment.NewLine, error.BamlTrace));
        throw;
    }
}

static Process StartSelf(params string[] arguments) => Process.Start(CreateSelfStartInfo(arguments))
    ?? throw new InvalidOperationException("Could not start the BAML replay server process.");

static ProcessStartInfo CreateSelfStartInfo(params string[] arguments)
{
    var start = new ProcessStartInfo(Environment.ProcessPath!) { UseShellExecute = false };
    foreach (var argument in arguments)
    {
        start.ArgumentList.Add(argument);
    }

    return start;
}

static DirectoryInfo FindAncestor(string name)
{
    for (var directory = new DirectoryInfo(AppContext.BaseDirectory);
         directory is not null;
         directory = directory.Parent)
    {
        if (string.Equals(directory.Name, name, StringComparison.Ordinal))
        {
            return directory;
        }
    }

    throw new DirectoryNotFoundException($"Could not find ancestor {name} from {AppContext.BaseDirectory}.");
}

static async Task<string> WaitForAddress(string path, Process server)
{
    var deadline = DateTime.UtcNow + TimeSpan.FromSeconds(10);
    while (DateTime.UtcNow < deadline)
    {
        if (server.HasExited)
        {
            throw new InvalidOperationException(
                $"The BAML replay server exited before binding with code {server.ExitCode}.");
        }

        if (File.Exists(path))
        {
            var address = (await File.ReadAllTextAsync(path)).Trim();
            if (address.Length != 0)
            {
                return address;
            }
        }

        await Task.Delay(20);
    }

    throw new TimeoutException("The BAML replay server did not bind within 10 seconds.");
}
