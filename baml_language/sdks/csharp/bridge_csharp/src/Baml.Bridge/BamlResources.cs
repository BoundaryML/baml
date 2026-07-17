using System.Collections.ObjectModel;
using Baml.Bridge;

namespace Baml;

public sealed class BamlHttpResponse : IDisposable
{
    private readonly IReadOnlyDictionary<string, string> _headers;
    private NativeHandle? _body;

    internal BamlHttpResponse(
        long statusCode,
        IReadOnlyDictionary<string, string> headers,
        string url,
        NativeHandle body)
    {
        StatusCode = statusCode;
        Url = url ?? throw new ArgumentNullException(nameof(url));
        _headers = CopyHeaders(headers);
        _body = body ?? throw new ArgumentNullException(nameof(body));
    }

    public long StatusCode { get; }

    public IReadOnlyDictionary<string, string> Headers => _headers;

    public string Url { get; }

    public bool Ok => StatusCode is >= 200 and < 300;

    public BamlHttpResponse Clone() => new(
        StatusCode,
        Headers,
        Url,
        GetBody().Clone("clone BamlHttpResponse"));

    public string Text() => TextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string> TextAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<string>(
            "baml.http.Response.text",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public byte[] Bytes() => BytesAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<byte[]> BytesAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<byte[]>(
            "baml.http.Response.bytes",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public void Dispose()
    {
        Interlocked.Exchange(ref _body, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetBody(), "clone BamlHttpResponse for BAML argument");

    private NativeHandle GetBody() => Volatile.Read(ref _body)
        ?? throw new ObjectDisposedException(GetType().FullName);

    private static IReadOnlyDictionary<string, string> CopyHeaders(
        IReadOnlyDictionary<string, string> headers)
    {
        ArgumentNullException.ThrowIfNull(headers);
        var copy = new Dictionary<string, string>(headers.Count, StringComparer.Ordinal);
        foreach (var (name, value) in headers)
        {
            ArgumentNullException.ThrowIfNull(name);
            ArgumentNullException.ThrowIfNull(value);
            copy.Add(name, value);
        }

        return new ReadOnlyDictionary<string, string>(copy);
    }
}

public sealed class BamlFile : IDisposable
{
    private NativeHandle? _handle;

    internal BamlFile(NativeHandle handle)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public BamlFile Clone() => new(GetHandle().Clone("clone BamlFile"));

    public string Text() => TextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string> TextAsync(CancellationToken cancellationToken = default) =>
        Call<string>("baml.fs.File.text", [], cancellationToken);

    public byte[] Bytes() => BytesAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<byte[]> BytesAsync(CancellationToken cancellationToken = default) =>
        Call<byte[]>("baml.fs.File.bytes", [], cancellationToken);

    public string Read(long count) => ReadAsync(count, CancellationToken.None).GetAwaiter().GetResult();

    public Task<string> ReadAsync(long count, CancellationToken cancellationToken = default) =>
        Call<string>("baml.fs.File.read", [("n", count)], cancellationToken);

    public byte[] ReadBytes(long count) =>
        ReadBytesAsync(count, CancellationToken.None).GetAwaiter().GetResult();

    public Task<byte[]> ReadBytesAsync(long count, CancellationToken cancellationToken = default) =>
        Call<byte[]>("baml.fs.File.read_bytes", [("n", count)], cancellationToken);

    public long SeekFrom(string whence, long offset) =>
        SeekFromAsync(whence, offset, CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> SeekFromAsync(
        string whence,
        long offset,
        CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.fs.File.seek_from",
            [("whence", whence ?? throw new ArgumentNullException(nameof(whence))), ("offset", offset)],
            cancellationToken);

    public long Write(string data) => WriteAsync(data, CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> WriteAsync(string data, CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.fs.File.write",
            [("data", data ?? throw new ArgumentNullException(nameof(data)))],
            cancellationToken);

    public long WriteBytes(byte[] data) =>
        WriteBytesAsync(data, CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> WriteBytesAsync(byte[] data, CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.fs.File.write_bytes",
            [("data", data ?? throw new ArgumentNullException(nameof(data)))],
            cancellationToken);

    public void Close() => CloseAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task CloseAsync(CancellationToken cancellationToken = default) =>
        _ = await Call<object?>("baml.fs.File.close", [], cancellationToken).ConfigureAwait(false);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlFile for BAML argument");

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        CancellationToken cancellationToken)
    {
        var allArguments = new (string Name, object? Value)[arguments.Count + 1];
        allArguments[0] = ("self", this);
        for (var index = 0; index < arguments.Count; index++)
        {
            allArguments[index + 1] = arguments[index];
        }

        return CallDispatcher.CallAsync<T>(
            functionName,
            allArguments,
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlSseStream : IDisposable
{
    private NativeHandle? _handle;

    internal BamlSseStream(string url, NativeHandle handle)
    {
        Url = url ?? throw new ArgumentNullException(nameof(url));
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public string Url { get; }

    public BamlSseStream Clone() => new(Url, GetHandle().Clone("clone BamlSseStream"));

    public string? Next() => NextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string?> NextAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<string?>(
            "baml.http.SseStream.next",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public void Close() => CloseAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task CloseAsync(CancellationToken cancellationToken = default) =>
        _ = await CallDispatcher.CallAsync<object?>(
                "baml.http.SseStream.close",
                [("self", this)],
                Array.Empty<(string Name, Type Type)>(),
                cancellationToken)
            .ConfigureAwait(false);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlSseStream for BAML argument");

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlGlobScanOptions
{
    public BamlGlobScanOptions(
        string? cwd = null,
        bool? dot = null,
        bool? absolute = null,
        bool? followSymlinks = null,
        bool? throwErrorOnBrokenSymlink = null,
        bool? onlyFiles = null)
    {
        Cwd = cwd;
        Dot = dot;
        Absolute = absolute;
        FollowSymlinks = followSymlinks;
        ThrowErrorOnBrokenSymlink = throwErrorOnBrokenSymlink;
        OnlyFiles = onlyFiles;
    }

    public string? Cwd { get; }

    public bool? Dot { get; }

    public bool? Absolute { get; }

    public bool? FollowSymlinks { get; }

    public bool? ThrowErrorOnBrokenSymlink { get; }

    public bool? OnlyFiles { get; }
}

public sealed class BamlGlob : IDisposable
{
    private NativeHandle? _handle;

    internal BamlGlob(NativeHandle handle)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public BamlGlob Clone() => new(GetHandle().Clone("clone BamlGlob"));

    public bool Matches(string path) =>
        MatchesAsync(path, CancellationToken.None).GetAwaiter().GetResult();

    public Task<bool> MatchesAsync(string path, CancellationToken cancellationToken = default) =>
        Call<bool>(
            "baml.glob.Glob.matches",
            [("path", path ?? throw new ArgumentNullException(nameof(path)))],
            cancellationToken);

    public List<string> Scan(string root) =>
        ScanAsync(root, CancellationToken.None).GetAwaiter().GetResult();

    public Task<List<string>> ScanAsync(
        string root,
        CancellationToken cancellationToken = default) =>
        ScanAsyncCore(root ?? throw new ArgumentNullException(nameof(root)), cancellationToken);

    public List<string> Scan(BamlGlobScanOptions options) =>
        ScanAsync(options, CancellationToken.None).GetAwaiter().GetResult();

    public Task<List<string>> ScanAsync(
        BamlGlobScanOptions options,
        CancellationToken cancellationToken = default) =>
        ScanAsyncCore(options ?? throw new ArgumentNullException(nameof(options)), cancellationToken);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlGlob for BAML argument");

    private Task<List<string>> ScanAsyncCore(object root, CancellationToken cancellationToken) =>
        Call<List<string>>("baml.glob.Glob.scan", [("root", root)], cancellationToken);

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        CancellationToken cancellationToken)
    {
        var allArguments = new (string Name, object? Value)[arguments.Count + 1];
        allArguments[0] = ("self", this);
        for (var index = 0; index < arguments.Count; index++)
        {
            allArguments[index + 1] = arguments[index];
        }

        return CallDispatcher.CallAsync<T>(
            functionName,
            allArguments,
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlCancelToken : IDisposable
{
    private NativeHandle? _handle;

    internal BamlCancelToken(NativeHandle handle)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public BamlCancelToken Clone() => new(GetHandle().Clone("clone BamlCancelToken"));

    public long Cancel() => CancelAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> CancelAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<long>(
            "baml.spawn.CancelToken.cancel",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public bool IsCancelled() =>
        IsCancelledAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<bool> IsCancelledAsync(CancellationToken cancellationToken = default) =>
        CallDispatcher.CallAsync<bool>(
            "baml.spawn.CancelToken.is_cancelled",
            [("self", this)],
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlCancelToken for BAML argument");

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlTaskGroup : IDisposable
{
    private NativeHandle? _handle;

    internal BamlTaskGroup(NativeHandle handle)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public BamlTaskGroup Clone() => new(GetHandle().Clone("clone BamlTaskGroup"));

    public long Cancel(bool? pending = true, bool? active = true) =>
        CancelAsync(pending, active, CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> CancelAsync(
        bool? pending = true,
        bool? active = true,
        CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.spawn.TaskGroup.cancel",
            [("pending", pending), ("active", active)],
            cancellationToken);

    public void SetLimit(long limit) =>
        SetLimitAsync(limit, CancellationToken.None).GetAwaiter().GetResult();

    public async Task SetLimitAsync(long limit, CancellationToken cancellationToken = default) =>
        _ = await Call<object?>(
                "baml.spawn.TaskGroup.set_limit",
                [("limit", limit)],
                cancellationToken)
            .ConfigureAwait(false);

    public long Limit() => LimitAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> LimitAsync(CancellationToken cancellationToken = default) =>
        Call<long>("baml.spawn.TaskGroup.limit", [], cancellationToken);

    public string? Name() => NameAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string?> NameAsync(CancellationToken cancellationToken = default) =>
        Call<string?>("baml.spawn.TaskGroup.name", [], cancellationToken);

    public long ActiveCount() => ActiveCountAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> ActiveCountAsync(CancellationToken cancellationToken = default) =>
        Call<long>("baml.spawn.TaskGroup.active_count", [], cancellationToken);

    public long QueuedCount() => QueuedCountAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> QueuedCountAsync(CancellationToken cancellationToken = default) =>
        Call<long>("baml.spawn.TaskGroup.queued_count", [], cancellationToken);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlTaskGroup for BAML argument");

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        CancellationToken cancellationToken)
    {
        var allArguments = new (string Name, object? Value)[arguments.Count + 1];
        allArguments[0] = ("self", this);
        for (var index = 0; index < arguments.Count; index++)
        {
            allArguments[index + 1] = arguments[index];
        }

        return CallDispatcher.CallAsync<T>(
            functionName,
            allArguments,
            Array.Empty<(string Name, Type Type)>(),
            cancellationToken);
    }

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

internal static class ResourceHandle
{
    internal static (ulong Key, int HandleType) CloneForWire(NativeHandle handle, string operation)
    {
        var clone = handle.Clone(operation);
        var key = clone.Key;
        var handleType = clone.HandleType;
        clone.SetHandleAsInvalid();
        clone.Dispose();
        return (key, handleType);
    }
}
