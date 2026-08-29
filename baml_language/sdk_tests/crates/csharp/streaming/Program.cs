using System.Diagnostics;
using System.Net;
using System.Net.Sockets;
using System.Reflection;
using System.Runtime.CompilerServices;
using System.Text;

using Baml;
using CsharpStreaming;

if (!args.Contains("--native-child", StringComparer.Ordinal))
{
    string counterFile = Path.GetTempFileName();
    await using var replay = new ReplayServer(counterFile);
    try
    {
        var start = new ProcessStartInfo
        {
            FileName = Environment.ProcessPath
                ?? throw new InvalidOperationException("C# fixture process path is unavailable"),
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
        };
        start.ArgumentList.Add("--native-child");
        start.Environment["BAML_CSHARP_REPLAY_BASE_URL"] = replay.BaseUrl;
        start.Environment["BAML_CSHARP_REPLAY_API_KEY"] = "local-replay-key";
        start.Environment["BAML_CSHARP_REPLAY_COUNT_FILE"] = counterFile;
        using Process child = Process.Start(start)
            ?? throw new InvalidOperationException("failed to start native stream child");
        Task<string> stdoutTask = child.StandardOutput.ReadToEndAsync();
        Task<string> stderrTask = child.StandardError.ReadToEndAsync();
        await child.WaitForExitAsync().ConfigureAwait(false);
        string stdout = await stdoutTask.ConfigureAwait(false);
        string stderr = await stderrTask.ConfigureAwait(false);
        Console.Out.Write(stdout);
        Console.Error.Write(stderr);
        Require(child.ExitCode == 0, $"native stream child exited with {child.ExitCode}");
        return;
    }
    finally
    {
        File.Delete(counterFile);
        File.Delete(counterFile + ".next");
        File.Delete(counterFile + ".auth");
        File.Delete(counterFile + ".auth.next");
        File.Delete(counterFile + ".dispose-ready");
        File.Delete(counterFile + ".ordered-partials.1");
        File.Delete(counterFile + ".ordered-partials.2");
        File.Delete(counterFile + ".structured-partials.1");
        File.Delete(counterFile + ".structured-partials.2");
    }
}

const string ExpectedFinal = "alpha beta gamma delta";
using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(20));
string requestCountFile = Environment.GetEnvironmentVariable("BAML_CSHARP_REPLAY_COUNT_FILE")
    ?? throw new InvalidOperationException("native child request-count file is missing");

const string PngBase64 = "iVBORw0KGgo=";
BamlImage image = BamlImage.FromBase64(PngBase64, "image/png");
BamlFunctionSpec<string> mediaSpec = Functions.InspectMediaSpec(image);
Require(
    mediaSpec.Name().Contains("InspectMedia", StringComparison.Ordinal),
    "FunctionSpec.name did not preserve the authored function identity");
Require(
    mediaSpec.ClientId().Length != 0,
    "FunctionSpec.client_id returned an empty client identity");
Require(
    mediaSpec.OutputType().Descriptor.Kind == BamlTypeDescriptorKind.String,
    "FunctionSpec reflected output type changed");
IReadOnlyDictionary<string, BamlValue> boundArguments = mediaSpec.Arguments();
Require(
    boundArguments.Count == 1
        && boundArguments["photo"].As<BamlImage>().TryGetBytes(
            out ReadOnlyMemory<byte> argumentBytes,
            out string? argumentMediaType)
        && Convert.ToBase64String(argumentBytes.Span) == PngBase64
        && argumentMediaType == "image/png",
    "FunctionSpec.arguments did not preserve its bound media value");
Require(
    mediaSpec.Parse("\"parsed\"") == "parsed",
    "FunctionSpec.parse did not decode through the final output codec");
Require(
    mediaSpec.Tools().Kind == BamlValueKind.Class,
    "FunctionSpec.tools did not return its canonical toolbox value");

BamlPrompt prompt = mediaSpec.Prompt();
string promptText = prompt.Text();
Require(
    prompt.Text() == promptText
        && promptText.Contains("Describe this image:", StringComparison.Ordinal),
    "portable Prompt.text was not repeatable");
IReadOnlyList<BamlPromptMessage> firstMessages = prompt.Messages();
IReadOnlyList<BamlPromptMessage> secondMessages = prompt.Messages();
Require(
    firstMessages.Count == 1
        && secondMessages.Count == 1
        && firstMessages[0].Role == "user"
        && firstMessages[0].Parts.Count == 2
        && firstMessages[0].Parts[0].Kind == BamlValueKind.String
        && firstMessages[0].Content
            .Contains("Describe this image:", StringComparison.Ordinal),
    "portable Prompt.messages did not preserve its structural message");
BamlImage promptImage = firstMessages[0].Parts[1].As<BamlImage>();
Require(
    promptImage.TryGetBytes(
        out ReadOnlyMemory<byte> promptBytes,
        out string? promptMediaType)
        && Convert.ToBase64String(promptBytes.Span) == PngBase64
        && promptMediaType == "image/png"
        && secondMessages[0].Parts[1].As<BamlImage>().Equals(promptImage),
    "portable Prompt.messages consumed or changed its media part");
Require(
    mediaSpec.Prompt().Text() == promptText,
    "rendering a second Prompt consumed the FunctionSpec or first Prompt");

Baml.Http.Request request = mediaSpec.BuildRequest().As<Baml.Http.Request>();
Require(
    request.Method == "POST"
        && request.Body.Contains(
            "data:image/png;base64," + PngBase64,
            StringComparison.Ordinal)
        && mediaSpec.BuildRequest().As<Baml.Http.Request>().Body == request.Body,
    "FunctionSpec.build_request did not preserve the reusable prompt media");

int requestsBefore = ReplayRequestCount();
BamlStream<string?, string> finalOnly = Functions.DeterministicStream("final-only");
Require(
    ReplayRequestCount() == requestsBefore,
    "generated FunctionStream eagerly dispatched its native factory");
string finalOnlyResult = await finalOnly.GetFinalResponseAsync(timeout.Token)
    .ConfigureAwait(false);
Require(finalOnlyResult == ExpectedFinal, "native final-only stream result changed");
Require(
    ReplayRequestCount() == requestsBefore + 1,
    "native final-only stream dispatched the wrong request count");
// The client declaration names its env vars rather than reading them, so the
// api key only becomes real when the provider builds the request. Observing it
// on the wire is what proves that request-time resolution happened.
Require(
    ObservedAuthorization() == "Bearer local-replay-key",
    "streamed request did not carry the request-time resolved api key");

int requestsBeforeEarly = ReplayRequestCount();
BamlStream<string?, string> early = Functions.DeterministicStream("dispose-early");
Require(
    ReplayRequestCount() == requestsBeforeEarly,
    "second generated FunctionStream was not cold");
Task<string> earlyFinal = early.GetFinalResponseAsync();
await WaitForReplayRequestCountAsync(requestsBeforeEarly + 1).ConfigureAwait(false);
// Select disposal as the terminal outcome before releasing the held response.
ValueTask earlyDisposal = early.DisposeAsync();
File.WriteAllText(requestCountFile + ".dispose-ready", "ready");
await earlyDisposal.ConfigureAwait(false);
BamlOperationCanceledException disposed =
    await ExpectAsync<BamlOperationCanceledException>(earlyFinal).ConfigureAwait(false);
Require(
    disposed.Origin == BamlCancellationOrigin.StreamDisposed,
    "early stream disposal changed its cancellation origin");

int requestsBeforePartials = ReplayRequestCount();
BamlStream<string?, string> stream = Functions.DeterministicStream("ordered-partials");
Require(
    ReplayRequestCount() == requestsBeforePartials,
    "partial-consuming generated FunctionStream was not cold");

var partials = new List<string>();
await using (IAsyncEnumerator<string?> enumerator =
    stream.GetAsyncEnumerator(timeout.Token))
{
    while (await enumerator.MoveNextAsync().ConfigureAwait(false))
    {
        string? partial = enumerator.Current;
        if (partial is null)
        {
            continue;
        }

        if (partials.Count != 0)
        {
            Require(
                partial.Length > partials[^1].Length
                    && partial.StartsWith(partials[^1], StringComparison.Ordinal),
                "native stream partials were not strictly ordered accumulated prefixes");
        }

        partials.Add(partial);
        if (partials.Count <= 2)
        {
            PublishReplayProgress(".ordered-partials", partials.Count);
        }
    }
}

string final = await stream.GetFinalResponseAsync(timeout.Token).ConfigureAwait(false);
Require(final == ExpectedFinal, "native stream final result changed");
Require(
    partials.Count >= 2
        && partials.All(partial => final.StartsWith(partial, StringComparison.Ordinal)),
    "native replay did not expose multiple ordered partials before the final result");
Require(
    ReplayRequestCount() == requestsBeforePartials + 1,
    "native stream dispatched the wrong request count");

AssertGeneratedStructuredPropertyShapes();
int requestsBeforeStructured = ReplayRequestCount();
BamlStream<StreamEnvelopeStream?, StreamEnvelope> structured =
    Functions.StructuredStream("stream-attributes");
Require(
    ReplayRequestCount() == requestsBeforeStructured,
    "structured generated FunctionStream was not cold");

var structuredPartials = new List<StreamEnvelopeStream>();
await foreach (StreamEnvelopeStream? partial in
    structured.WithCancellation(timeout.Token).ConfigureAwait(false))
{
    if (partial is not null)
    {
        structuredPartials.Add(partial);
        if (structuredPartials.Count <= 2)
        {
            PublishReplayProgress(".structured-partials", structuredPartials.Count);
        }
    }
}

StreamEnvelope structuredFinal = await structured.GetFinalResponseAsync(timeout.Token)
    .ConfigureAwait(false);
string structuredTransitions = string.Join(
    ", ",
    structuredPartials.Select(partial =>
        $"state={partial.State ?? "<null>"}:done={partial.Done ?? "<null>"}"));
Require(
    structuredFinal.Defaulted == "default"
        && structuredFinal.Done == "final"
        && structuredFinal.DoneRequired == "sealed"
        && structuredFinal.Required == "must"
        && structuredFinal.State == "progress",
    "structured native stream final result changed");
Require(
    structuredPartials.Count >= 2
        && structuredPartials[0].Done == "fin"
        && structuredPartials[^1].Done == "final",
    $"structured replay did not expose all stream-attribute transitions: {structuredTransitions}");
Require(
    structuredPartials.All(partial =>
        partial.Required == "must"
        && partial.DoneRequired == "sealed"
        && partial.State == "progress"
        && partial.Defaulted == "default"
        && (partial.Done is null
            || "final".StartsWith(partial.Done, StringComparison.Ordinal))),
    "structured partial field values did not follow their generated shapes");
Require(
    structuredPartials.Any(partial => partial.Done == "fin"),
    "@stream.done did not expose its incomplete value");
Require(
    structuredPartials.Any(partial => partial.Done == "final"),
    "@stream.done did not appear after its value completed");
Require(
    ReplayRequestCount() == requestsBeforeStructured + 1,
    "structured native stream dispatched the wrong request count");

Console.WriteLine("csharp_streaming_request=ok");

int ReplayRequestCount()
{
    using var file = new FileStream(
        requestCountFile,
        FileMode.Open,
        FileAccess.Read,
        FileShare.ReadWrite | FileShare.Delete);
    using var reader = new StreamReader(file, Encoding.UTF8);
    return int.Parse(
        reader.ReadToEnd(),
        System.Globalization.CultureInfo.InvariantCulture);
}

string ObservedAuthorization()
{
    using var file = new FileStream(
        requestCountFile + ".auth",
        FileMode.Open,
        FileAccess.Read,
        FileShare.ReadWrite | FileShare.Delete);
    using var reader = new StreamReader(file, Encoding.UTF8);
    return reader.ReadToEnd();
}

void PublishReplayProgress(string suffix, int value)
{
    File.WriteAllText($"{requestCountFile}{suffix}.{value}", "ready");
}

async Task WaitForReplayRequestCountAsync(int expected)
{
    while (ReplayRequestCount() < expected)
    {
        timeout.Token.ThrowIfCancellationRequested();
        await Task.Delay(10, timeout.Token).ConfigureAwait(false);
    }
}

static async Task<TException> ExpectAsync<TException>(Task task)
    where TException : Exception
{
    try
    {
        await task.ConfigureAwait(false);
    }
    catch (TException error)
    {
        return error;
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

static void AssertGeneratedStructuredPropertyShapes()
{
    var nullability = new NullabilityInfoContext();
    PropertyInfo PartialProperty(string name) =>
        typeof(StreamEnvelopeStream).GetProperty(name)
            ?? throw new InvalidOperationException($"generated partial omitted {name}");
    PropertyInfo FinalProperty(string name) =>
        typeof(StreamEnvelope).GetProperty(name)
            ?? throw new InvalidOperationException($"generated final omitted {name}");

    PropertyInfo defaulted = PartialProperty(nameof(StreamEnvelopeStream.Defaulted));
    PropertyInfo done = PartialProperty(nameof(StreamEnvelopeStream.Done));
    PropertyInfo doneRequired = PartialProperty(nameof(StreamEnvelopeStream.DoneRequired));
    PropertyInfo required = PartialProperty(nameof(StreamEnvelopeStream.Required));
    PropertyInfo state = PartialProperty(nameof(StreamEnvelopeStream.State));
    Require(
        defaulted.PropertyType == typeof(string)
            && nullability.Create(defaulted).ReadState == NullabilityState.Nullable
            && !defaulted.IsDefined(typeof(RequiredMemberAttribute)),
        "default partial property shape changed");
    Require(
        done.PropertyType == typeof(string)
            && nullability.Create(done).ReadState == NullabilityState.Nullable
            && !done.IsDefined(typeof(RequiredMemberAttribute)),
        "@stream.done partial property shape changed");
    Require(
        doneRequired.PropertyType == typeof(string)
            && nullability.Create(doneRequired).ReadState == NullabilityState.Nullable
            && !doneRequired.IsDefined(typeof(RequiredMemberAttribute)),
        "@stream.done partial property shape changed");
    Require(
        required.PropertyType == typeof(string)
            && nullability.Create(required).ReadState == NullabilityState.Nullable
            && !required.IsDefined(typeof(RequiredMemberAttribute)),
        "ordinary required-field partial property shape changed");
    Require(
        state.PropertyType == typeof(string)
            && nullability.Create(state).ReadState == NullabilityState.Nullable
            && !state.IsDefined(typeof(RequiredMemberAttribute)),
        "ordinary state partial property shape changed");

    foreach (string name in new[]
    {
        nameof(StreamEnvelope.Defaulted),
        nameof(StreamEnvelope.Done),
        nameof(StreamEnvelope.DoneRequired),
        nameof(StreamEnvelope.Required),
        nameof(StreamEnvelope.State),
    })
    {
        PropertyInfo property = FinalProperty(name);
        Require(
            property.PropertyType == typeof(string)
                && nullability.Create(property).ReadState == NullabilityState.NotNull
                && property.IsDefined(typeof(RequiredMemberAttribute)),
            $"final property shape changed for {name}");
    }
}

internal sealed class ReplayServer : IAsyncDisposable
{
    // OpenAI Responses API SSE. The event layout is load-bearing: the pacing
    // logic in ServeRequestAsync waits on specific 1-based event indices, so a
    // leading no-text event keeps the text deltas at indices 2..5 (scalar) and
    // 2..7 (structured), matching the Chat Completions layout these recordings
    // replaced.
    private const string Recording = """
        data: {"type":"response.created","response":{"status":"in_progress","output":[]}}

        data: {"type":"response.output_text.delta","delta":"alpha"}

        data: {"type":"response.output_text.delta","delta":" beta"}

        data: {"type":"response.output_text.delta","delta":" gamma"}

        data: {"type":"response.output_text.delta","delta":" delta"}

        data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"alpha beta gamma delta"}]}],"usage":{"input_tokens":11,"output_tokens":4}}}

        """;

    private const string StructuredRecording = """
        data: {"type":"response.created","response":{"status":"in_progress","output":[]}}

        data: {"type":"response.output_text.delta","delta":"{\"required\":\"must\",\"done_required\":\"sealed\""}

        data: {"type":"response.output_text.delta","delta":",\"defaulted\":\"de"}

        data: {"type":"response.output_text.delta","delta":"fault\",\"state\":\"pro"}

        data: {"type":"response.output_text.delta","delta":"gress\""}

        data: {"type":"response.output_text.delta","delta":",\"done\":\"fin"}

        data: {"type":"response.output_text.delta","delta":"al\"}"}

        data: {"type":"response.completed","response":{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"{\"required\":\"must\",\"done_required\":\"sealed\",\"defaulted\":\"default\",\"state\":\"progress\",\"done\":\"final\"}"}]}],"usage":{"input_tokens":23,"output_tokens":31}}}

        """;

    private readonly CancellationTokenSource stopping = new();
    private readonly TcpListener listener = new(IPAddress.Loopback, 0);
    private readonly Task serveTask;
    private readonly string counterFile;
    private int requestCount;

    internal ReplayServer(string counterFile)
    {
        this.counterFile = counterFile;
        File.WriteAllText(counterFile, "0");
        listener.Start();
        var endpoint = (IPEndPoint)listener.LocalEndpoint;
        BaseUrl = $"http://127.0.0.1:{endpoint.Port}";
        serveTask = ServeAsync();
    }

    internal string BaseUrl { get; }

    public async ValueTask DisposeAsync()
    {
        stopping.Cancel();
        listener.Stop();
        try
        {
            await serveTask.ConfigureAwait(false);
        }
        catch (OperationCanceledException) when (stopping.IsCancellationRequested)
        {
        }
        finally
        {
            stopping.Dispose();
        }
    }

    private async Task ServeAsync()
    {
        while (!stopping.IsCancellationRequested)
        {
            TcpClient client;
            try
            {
                client = await listener.AcceptTcpClientAsync(stopping.Token).ConfigureAwait(false);
            }
            catch (SocketException) when (stopping.IsCancellationRequested)
            {
                return;
            }

            // Keep SSE frames independently observable on Windows instead of
            // letting Nagle coalesce the deliberately paced partial updates.
            client.NoDelay = true;
            using (client)
            {
                await ServeRequestAsync(client, stopping.Token).ConfigureAwait(false);
            }
        }
    }

    private async Task ServeRequestAsync(TcpClient client, CancellationToken cancellationToken)
    {
        NetworkStream stream = client.GetStream();
        using var reader = new StreamReader(
            stream,
            Encoding.ASCII,
            detectEncodingFromByteOrderMarks: false,
            bufferSize: 4096,
            leaveOpen: true);
        string requestLine = await reader.ReadLineAsync(cancellationToken).ConfigureAwait(false)
            ?? throw new EndOfStreamException("replay request omitted its request line");
        if (!requestLine.StartsWith("POST ", StringComparison.Ordinal))
        {
            throw new InvalidOperationException("replay server received a non-POST request");
        }
        int contentLength = 0;
        while (true)
        {
            string line = await reader.ReadLineAsync(cancellationToken).ConfigureAwait(false)
                ?? throw new EndOfStreamException("replay request headers ended unexpectedly");
            if (line.Length == 0)
            {
                break;
            }

            const string Prefix = "Content-Length:";
            if (line.StartsWith(Prefix, StringComparison.OrdinalIgnoreCase))
            {
                contentLength = int.Parse(
                    line.AsSpan(Prefix.Length).Trim(),
                    System.Globalization.CultureInfo.InvariantCulture);
            }

            const string AuthorizationPrefix = "Authorization:";
            if (line.StartsWith(AuthorizationPrefix, StringComparison.OrdinalIgnoreCase))
            {
                // Published for the child to assert: it is the only evidence
                // that the api key resolved from env when the request was built.
                string pendingAuthorization = counterFile + ".auth.next";
                File.WriteAllText(
                    pendingAuthorization,
                    line.AsSpan(AuthorizationPrefix.Length).Trim().ToString());
                File.Move(pendingAuthorization, counterFile + ".auth", overwrite: true);
            }
        }

        var body = new char[contentLength];
        int read = 0;
        while (read < body.Length)
        {
            int count = await reader.ReadAsync(body.AsMemory(read), cancellationToken)
                .ConfigureAwait(false);
            if (count == 0)
            {
                throw new EndOfStreamException("replay request body ended unexpectedly");
            }

            read += count;
        }

        byte[] headers = Encoding.ASCII.GetBytes(
            "HTTP/1.1 200 OK\r\n"
                + "Content-Type: text/event-stream\r\n"
                + "Transfer-Encoding: chunked\r\n"
                + "Connection: close\r\n\r\n");
        await stream.WriteAsync(headers, cancellationToken).ConfigureAwait(false);

        int currentRequest = Interlocked.Increment(ref requestCount);
        string pendingCounter = counterFile + ".next";
        File.WriteAllText(
            pendingCounter,
            currentRequest.ToString(System.Globalization.CultureInfo.InvariantCulture));
        File.Move(pendingCounter, counterFile, overwrite: true);

        // This dedicated replay fixture asserts the same exact dispatch order:
        // final-only, dispose-early, ordered scalar partials, then structured partials.
        bool holdForDisposal = currentRequest == 2;
        bool acknowledgeOrderedPartials = currentRequest == 3;
        bool acknowledgeStructuredPartials = currentRequest == 4;
        string recording = acknowledgeStructuredPartials
            ? StructuredRecording
            : Recording;

        try
        {
            if (holdForDisposal)
            {
                // Keep this response incomplete until the child has selected
                // stream disposal, so platform scheduling cannot let success win.
                string disposalSignal = counterFile + ".dispose-ready";
                while (!File.Exists(disposalSignal))
                {
                    await Task.Delay(10, cancellationToken).ConfigureAwait(false);
                }
                return;
            }

            int eventIndex = 0;
            // Raw string literals inherit the source checkout's line endings.
            // Normalize before splitting so Windows does not send one giant SSE event.
            string normalizedRecording = recording.ReplaceLineEndings("\n");
            foreach (string eventText in normalizedRecording.Split(
                "\n\n",
                StringSplitOptions.RemoveEmptyEntries))
            {
                eventIndex++;
                byte[] eventBytes = Encoding.UTF8.GetBytes(eventText.TrimStart() + "\n\n");
                byte[] prefix = Encoding.ASCII.GetBytes($"{eventBytes.Length:x}\r\n");
                await stream.WriteAsync(prefix, cancellationToken).ConfigureAwait(false);
                await stream.WriteAsync(eventBytes, cancellationToken).ConfigureAwait(false);
                await stream.WriteAsync("\r\n"u8.ToArray(), cancellationToken).ConfigureAwait(false);
                await stream.FlushAsync(cancellationToken).ConfigureAwait(false);

                if (acknowledgeOrderedPartials && eventIndex == 2)
                {
                    await WaitForProgressAsync(
                            ".ordered-partials",
                            expected: 1,
                            cancellationToken)
                        .ConfigureAwait(false);
                }
                else if (acknowledgeOrderedPartials && eventIndex == 3)
                {
                    await WaitForProgressAsync(
                            ".ordered-partials",
                            expected: 2,
                            cancellationToken)
                        .ConfigureAwait(false);
                }
                else if (acknowledgeStructuredPartials && eventIndex == 6)
                {
                    await WaitForProgressAsync(
                            ".structured-partials",
                            expected: 1,
                            cancellationToken)
                        .ConfigureAwait(false);
                }
                else if (acknowledgeStructuredPartials && eventIndex == 7)
                {
                    await WaitForProgressAsync(
                            ".structured-partials",
                            expected: 2,
                            cancellationToken)
                        .ConfigureAwait(false);
                }

                await Task.Delay(35, cancellationToken).ConfigureAwait(false);
            }

            await stream.WriteAsync("0\r\n\r\n"u8.ToArray(), cancellationToken)
                .ConfigureAwait(false);
            await stream.FlushAsync(cancellationToken).ConfigureAwait(false);
        }
        catch (IOException)
        {
            // Early stream disposal intentionally closes the replay connection.
        }
    }

    private async Task WaitForProgressAsync(
        string suffix,
        int expected,
        CancellationToken cancellationToken)
    {
        string progressFile = $"{counterFile}{suffix}.{expected}";
        while (!File.Exists(progressFile))
        {
            await Task.Delay(10, cancellationToken).ConfigureAwait(false);
        }
    }
}
