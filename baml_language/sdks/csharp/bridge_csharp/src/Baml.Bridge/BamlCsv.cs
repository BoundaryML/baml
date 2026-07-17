using Baml.Bridge;

namespace Baml;

public sealed class BamlIteratorDone
{
    private BamlIteratorDone()
    {
    }

    public static BamlIteratorDone Instance { get; } = new();
}

public sealed class BamlCsvPosition
{
    public BamlCsvPosition(long byteOffset, long line, long record)
    {
        ByteOffset = byteOffset;
        Line = line;
        Record = record;
    }

    public long ByteOffset { get; }

    public long Line { get; }

    public long Record { get; }
}

public sealed class BamlCsvWriterOptions
{
    public BamlCsvWriterOptions(
        string? delimiter = null,
        string? quote = null,
        string? quoteStyle = null,
        string? escape = null,
        string? terminator = null,
        bool? writeHeader = null,
        IReadOnlyList<string>? headers = null,
        string? nullValue = null,
        bool? bom = null,
        bool? sanitizeFormulas = null)
    {
        Delimiter = delimiter;
        Quote = quote;
        QuoteStyle = quoteStyle;
        Escape = escape;
        Terminator = terminator;
        WriteHeader = writeHeader;
        Headers = headers?.ToArray();
        NullValue = nullValue;
        Bom = bom;
        SanitizeFormulas = sanitizeFormulas;
    }

    public string? Delimiter { get; }

    public string? Quote { get; }

    public string? QuoteStyle { get; }

    public string? Escape { get; }

    public string? Terminator { get; }

    public bool? WriteHeader { get; }

    public IReadOnlyList<string>? Headers { get; }

    public string? NullValue { get; }

    public bool? Bom { get; }

    public bool? SanitizeFormulas { get; }
}

public sealed class BamlCsvReaderOptions
{
    public BamlCsvReaderOptions(
        string? delimiter = null,
        string? quote = null,
        bool? quoting = null,
        string? escape = null,
        bool? hasHeader = null,
        IReadOnlyList<string>? headers = null,
        string? comment = null,
        string? trim = null,
        long? skipLines = null,
        bool? skipBlankRecords = null,
        string? ragged = null,
        IReadOnlyList<string>? nullValues = null,
        string? encoding = null,
        string? bom = null,
        string? onError = null,
        Delegate? onSkip = null,
        long? maxSkipped = null,
        long? limit = null)
    {
        Delimiter = delimiter;
        Quote = quote;
        Quoting = quoting;
        Escape = escape;
        HasHeader = hasHeader;
        Headers = headers?.ToArray();
        Comment = comment;
        Trim = trim;
        SkipLines = skipLines;
        SkipBlankRecords = skipBlankRecords;
        Ragged = ragged;
        NullValues = nullValues?.ToArray();
        Encoding = encoding;
        Bom = bom;
        OnError = onError;
        OnSkip = onSkip;
        MaxSkipped = maxSkipped;
        Limit = limit;
    }

    public string? Delimiter { get; }

    public string? Quote { get; }

    public bool? Quoting { get; }

    public string? Escape { get; }

    public bool? HasHeader { get; }

    public IReadOnlyList<string>? Headers { get; }

    public string? Comment { get; }

    public string? Trim { get; }

    public long? SkipLines { get; }

    public bool? SkipBlankRecords { get; }

    public string? Ragged { get; }

    public IReadOnlyList<string>? NullValues { get; }

    public string? Encoding { get; }

    public string? Bom { get; }

    public string? OnError { get; }

    public Delegate? OnSkip { get; }

    public long? MaxSkipped { get; }

    public long? Limit { get; }
}

public sealed class BamlCsvRecord : IDisposable
{
    private NativeHandle? _handle;

    internal BamlCsvRecord(NativeHandle handle)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
    }

    public BamlCsvRecord Clone() => new(GetHandle().Clone("clone BamlCsvRecord"));

    public BamlNullable<T> Get<T>(string column) =>
        GetAsync<T>(column, CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlNullable<T>> GetAsync<T>(
        string column,
        CancellationToken cancellationToken = default) =>
        Call<BamlNullable<T>>(
            "baml.csv.CsvRecord.get",
            [("column", column ?? throw new ArgumentNullException(nameof(column)))],
            [typeof(T)],
            cancellationToken);

    public BamlNullable<T> GetAt<T>(long index) =>
        GetAtAsync<T>(index, CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlNullable<T>> GetAtAsync<T>(
        long index,
        CancellationToken cancellationToken = default) =>
        Call<BamlNullable<T>>(
            "baml.csv.CsvRecord.get_at",
            [("index", index)],
            [typeof(T)],
            cancellationToken);

    public List<string> Fields() => FieldsAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<List<string>> FieldsAsync(CancellationToken cancellationToken = default) =>
        Call<List<string>>(
            "baml.csv.CsvRecord.fields",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public long Length() => LengthAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> LengthAsync(CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.csv.CsvRecord.length",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public BamlCsvPosition Position() =>
        PositionAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlCsvPosition> PositionAsync(CancellationToken cancellationToken = default) =>
        Call<BamlCsvPosition>(
            "baml.csv.CsvRecord.position",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public T Decode<T>() => DecodeAsync<T>(CancellationToken.None).GetAwaiter().GetResult();

    public Task<T> DecodeAsync<T>(CancellationToken cancellationToken = default) =>
        Call<T>("baml.csv.CsvRecord.decode", [], [typeof(T)], cancellationToken);

    public Dictionary<string, string> ToMap() =>
        ToMapAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<Dictionary<string, string>> ToMapAsync(
        CancellationToken cancellationToken = default) =>
        Call<Dictionary<string, string>>(
            "baml.csv.CsvRecord.to_map",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlCsvRecord for BAML argument");

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<Type> typeArguments,
        CancellationToken cancellationToken) =>
        BamlCsvCall.CallAsync<T>(
            functionName,
            this,
            arguments,
            typeArguments,
            cancellationToken);

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlCsvReader : IDisposable
{
    private NativeHandle? _handle;
    private BamlFile? _file;
    private BamlHandle? _onSkip;

    internal BamlCsvReader(
        NativeHandle handle,
        BamlFile? file,
        BamlHandle? onSkip,
        bool ownsFile)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
        _file = file;
        _onSkip = onSkip;
        OwnsFile = ownsFile;
    }

    internal BamlFile? BackingFile => Volatile.Read(ref _file);

    internal BamlHandle? OnSkip => Volatile.Read(ref _onSkip);

    internal bool OwnsFile { get; }

    public BamlCsvReader Clone()
    {
        var handle = GetHandle().Clone("clone BamlCsvReader");
        BamlFile? file = null;
        BamlHandle? onSkip = null;
        try
        {
            file = BackingFile?.Clone();
            onSkip = OnSkip?.Clone();
            return new BamlCsvReader(handle, file, onSkip, OwnsFile);
        }
        catch
        {
            handle.Dispose();
            file?.Dispose();
            onSkip?.Dispose();
            throw;
        }
    }

    public BamlUnion<BamlCsvRecord, BamlIteratorDone> Next() =>
        NextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlUnion<BamlCsvRecord, BamlIteratorDone>> NextAsync(
        CancellationToken cancellationToken = default) =>
        Call<BamlUnion<BamlCsvRecord, BamlIteratorDone>>(
            "baml.csv.CsvReader.root.iter.Iterator.next",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public List<string>? Headers() =>
        HeadersAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<List<string>?> HeadersAsync(CancellationToken cancellationToken = default) =>
        Call<List<string>?>(
            "baml.csv.CsvReader.headers",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public List<object?> Skipped() =>
        SkippedAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<List<object?>> SkippedAsync(CancellationToken cancellationToken = default) =>
        Call<List<object?>>(
            "baml.csv.CsvReader.skipped",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public long SkippedCount() =>
        SkippedCountAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> SkippedCountAsync(CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.csv.CsvReader.skipped_count",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public BamlCsvPosition Position() =>
        PositionAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<BamlCsvPosition> PositionAsync(CancellationToken cancellationToken = default) =>
        Call<BamlCsvPosition>(
            "baml.csv.CsvReader.position",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public void Close() => CloseAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task CloseAsync(CancellationToken cancellationToken = default)
    {
        _ = await Call<object?>(
                "baml.csv.CsvReader.close",
                [],
                Type.EmptyTypes,
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        Interlocked.Exchange(ref _file, null)?.Dispose();
        Interlocked.Exchange(ref _onSkip, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlCsvReader for BAML argument");

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<Type> typeArguments,
        CancellationToken cancellationToken) =>
        BamlCsvCall.CallAsync<T>(
            functionName,
            this,
            arguments,
            typeArguments,
            cancellationToken);

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

public sealed class BamlCsvWriter : IDisposable
{
    private NativeHandle? _handle;
    private BamlFile? _file;

    internal BamlCsvWriter(NativeHandle handle, BamlFile? file, bool ownsFile)
    {
        _handle = handle ?? throw new ArgumentNullException(nameof(handle));
        _file = file;
        OwnsFile = ownsFile;
    }

    internal BamlFile? BackingFile => Volatile.Read(ref _file);

    internal bool OwnsFile { get; }

    public BamlCsvWriter Clone()
    {
        var handle = GetHandle().Clone("clone BamlCsvWriter");
        try
        {
            return new BamlCsvWriter(handle, BackingFile?.Clone(), OwnsFile);
        }
        catch
        {
            handle.Dispose();
            throw;
        }
    }

    public void WriteRecord(IReadOnlyList<object?> record) =>
        WriteRecordAsync(record, CancellationToken.None).GetAwaiter().GetResult();

    public async Task WriteRecordAsync(
        IReadOnlyList<object?> record,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(record);
        _ = await Call<object?>(
                "baml.csv.CsvWriter.write_record",
                [("record", record.ToList())],
                Type.EmptyTypes,
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void WriteRow<T>(T row) =>
        WriteRowAsync(row, CancellationToken.None).GetAwaiter().GetResult();

    public async Task WriteRowAsync<T>(T row, CancellationToken cancellationToken = default)
    {
        _ = await Call<object?>(
                "baml.csv.CsvWriter.write_row",
                [("row", row)],
                [typeof(T)],
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void WriteRows<T>(IReadOnlyList<T> rows) =>
        WriteRowsAsync(rows, CancellationToken.None).GetAwaiter().GetResult();

    public async Task WriteRowsAsync<T>(
        IReadOnlyList<T> rows,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(rows);
        _ = await Call<object?>(
                "baml.csv.CsvWriter.write_rows",
                [("rows", rows.ToList())],
                [typeof(T)],
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void WriteHeader(IReadOnlyList<string> names) =>
        WriteHeaderAsync(names, CancellationToken.None).GetAwaiter().GetResult();

    public async Task WriteHeaderAsync(
        IReadOnlyList<string> names,
        CancellationToken cancellationToken = default)
    {
        ArgumentNullException.ThrowIfNull(names);
        _ = await Call<object?>(
                "baml.csv.CsvWriter.write_header",
                [("names", names.ToList())],
                Type.EmptyTypes,
                cancellationToken)
            .ConfigureAwait(false);
    }

    public long RecordsWritten() =>
        RecordsWrittenAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<long> RecordsWrittenAsync(CancellationToken cancellationToken = default) =>
        Call<long>(
            "baml.csv.CsvWriter.records_written",
            [],
            Type.EmptyTypes,
            cancellationToken);

    public string Text() => TextAsync(CancellationToken.None).GetAwaiter().GetResult();

    public Task<string> TextAsync(CancellationToken cancellationToken = default) =>
        Call<string>("baml.csv.CsvWriter.text", [], Type.EmptyTypes, cancellationToken);

    public void Flush() => FlushAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task FlushAsync(CancellationToken cancellationToken = default)
    {
        _ = await Call<object?>(
                "baml.csv.CsvWriter.flush",
                [],
                Type.EmptyTypes,
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void Close() => CloseAsync(CancellationToken.None).GetAwaiter().GetResult();

    public async Task CloseAsync(CancellationToken cancellationToken = default)
    {
        _ = await Call<object?>(
                "baml.csv.CsvWriter.close",
                [],
                Type.EmptyTypes,
                cancellationToken)
            .ConfigureAwait(false);
    }

    public void Dispose()
    {
        Interlocked.Exchange(ref _handle, null)?.Dispose();
        Interlocked.Exchange(ref _file, null)?.Dispose();
        GC.SuppressFinalize(this);
    }

    internal (ulong Key, int HandleType) CloneForWire() =>
        ResourceHandle.CloneForWire(GetHandle(), "clone BamlCsvWriter for BAML argument");

    private Task<T> Call<T>(
        string functionName,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<Type> typeArguments,
        CancellationToken cancellationToken) =>
        BamlCsvCall.CallAsync<T>(
            functionName,
            this,
            arguments,
            typeArguments,
            cancellationToken);

    private NativeHandle GetHandle() => Volatile.Read(ref _handle)
        ?? throw new ObjectDisposedException(GetType().FullName);
}

internal static class BamlCsvCall
{
    internal static Task<T> CallAsync<T>(
        string functionName,
        object receiver,
        IReadOnlyList<(string Name, object? Value)> arguments,
        IReadOnlyList<Type> typeArguments,
        CancellationToken cancellationToken)
    {
        var allArguments = new (string Name, object? Value)[arguments.Count + 1];
        allArguments[0] = ("self", receiver);
        for (var index = 0; index < arguments.Count; index++)
        {
            allArguments[index + 1] = arguments[index];
        }

        var bindings = typeArguments
            .Select((type, index) => (Name: $"T{index}", Type: type))
            .ToArray();
        if (bindings.Length == 1)
        {
            bindings[0] = ("T", bindings[0].Type);
        }

        return CallDispatcher.CallAsync<T>(
            functionName,
            allArguments,
            bindings,
            cancellationToken);
    }
}
