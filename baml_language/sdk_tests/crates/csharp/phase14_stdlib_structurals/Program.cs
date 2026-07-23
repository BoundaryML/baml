using Baml;
using Baml.Csv;
using Baml.Fs;
using Baml.Glob;
using Baml.Http;
using Baml.Iter;
using Baml.Net;
using Baml.Time;
using CsharpPhase14;
using UserFunctions = CsharpPhase14.Functions;

var scan = new ScanOptions
{
    Cwd = "./fixtures\0雪",
    Dot = true,
    Absolute = false,
    FollowSymlinks = null,
    ThrowErrorOnBrokenSymlink = true,
    OnlyFiles = false,
};
ScanOptions scanResult = UserFunctions.EchoScanOptions(scan);
Require(
    scanResult.Cwd == scan.Cwd
        && scanResult.Dot == scan.Dot
        && scanResult.Absolute == scan.Absolute
        && scanResult.FollowSymlinks is null
        && scanResult.ThrowErrorOnBrokenSymlink == scan.ThrowErrorOnBrokenSymlink
        && scanResult.OnlyFiles == scan.OnlyFiles,
    "glob scan options roundtrip changed");

var entry = new DirEntry
{
    Name = "name\0雪",
    IsDir = true,
    IsFile = false,
    IsSymlink = true,
};
DirEntry entryResult = await UserFunctions.EchoDirEntryAsync(entry);
Require(
    entryResult.Name == entry.Name
        && entryResult.IsDir == entry.IsDir
        && entryResult.IsFile == entry.IsFile
        && entryResult.IsSymlink == entry.IsSymlink,
    "directory entry roundtrip changed");

MkdirOptions mkdirResult = UserFunctions.EchoMkdirOptions(
    new MkdirOptions { Recursive = true });
Require(mkdirResult.Recursive, "mkdir options roundtrip changed");

byte[] payload = [0x00, 0x7f, 0x80, 0xff];
var datagram = new Datagram
{
    Data = payload,
    Addr = "[::1]:65535",
};
Datagram datagramResult = await UserFunctions.EchoDatagramAsync(datagram);
Array.Fill(payload, (byte)0xaa);
Require(
    datagramResult.Addr == datagram.Addr
        && datagramResult.Data.Span.SequenceEqual(
            new byte[] { 0x00, 0x7f, 0x80, 0xff }),
    "datagram bytes/address roundtrip changed");

var request = new Request
{
    Method = "POST",
    Url = "/resource?q=雪",
    Headers = new Dictionary<string, string>
    {
        ["content-type"] = "text/plain",
        ["x-nul"] = "a\0b",
    },
    Body = "payload\0雪",
};
Request requestResult = await UserFunctions.EchoRequestAsync(request);
Require(
    requestResult.Method == request.Method
        && requestResult.Url == request.Url
        && requestResult.Headers.SequenceEqual(request.Headers)
        && requestResult.Body == request.Body,
    "HTTP request structural roundtrip changed");

Duration negativeDuration = Duration.FromNanoseconds(-3_600_000_000_000L);
Duration absoluteDuration = await negativeDuration.AbsAsync();
Require(
    absoluteDuration.Nanoseconds == 3_600_000_000_000L
        && absoluteDuration.ToNanoseconds() == 3_600_000_000_000L
        && await absoluteDuration.ToMicrosecondsAsync() == 3_600_000_000L
        && absoluteDuration.ToMilliseconds() == 3_600_000L
        && await absoluteDuration.ToSecondsAsync() == 3_600L
        && absoluteDuration.ToMinutes() == 60L
        && await absoluteDuration.ToHoursAsync() == 1L,
    "Duration instance conversions changed");
Require(
    Duration.FromMicroseconds(2L).Nanoseconds == 2_000L
        && (await Duration.FromMillisecondsAsync(3L)).Nanoseconds == 3_000_000L
        && Duration.FromSeconds(4L).Nanoseconds == 4_000_000_000L
        && (await Duration.FromMinutesAsync(5L)).Nanoseconds == 300_000_000_000L
        && Duration.FromHours(6L).Nanoseconds == 21_600_000_000_000L,
    "Duration static constructors changed");
Duration durationResult = UserFunctions.EchoDuration(absoluteDuration);
Require(
    durationResult.Nanoseconds == absoluteDuration.Nanoseconds,
    "Duration structural roundtrip changed");

Done doneResult = await UserFunctions.EchoDoneAsync(new Done());
Require(doneResult is not null, "iter.Done structural roundtrip changed");

Require(
    UserFunctions.EchoCsvErrorKind(CsvErrorKind.FieldCount) == CsvErrorKind.FieldCount,
    "CSV error enum roundtrip changed");

var csvError = new CsvError
{
    Kind = CsvErrorKind.Decode,
    Message = "bad cell\0雪",
    Line = 41,
    Record = 39,
    Field = 2,
    Column = "amount",
    Expected = null,
    Found = null,
};
CsvError csvErrorResult = UserFunctions.EchoCsvError(csvError);
Require(
    csvErrorResult.Kind == csvError.Kind
        && csvErrorResult.Message == csvError.Message
        && csvErrorResult.Line == csvError.Line
        && csvErrorResult.Record == csvError.Record
        && csvErrorResult.Field == csvError.Field
        && csvErrorResult.Column == csvError.Column
        && csvErrorResult.Expected is null
        && csvErrorResult.Found is null,
    "CSV error roundtrip changed");

var position = new CsvPosition { Byte = 9, Line = 2, Record = 1 };
CsvPosition positionResult = await UserFunctions.EchoCsvPositionAsync(position);
Require(
    positionResult.Byte == position.Byte
        && positionResult.Line == position.Line
        && positionResult.Record == position.Record,
    "CSV position roundtrip changed");

var readerOptions = new ReaderOptions
{
    Delimiter = null,
    Quote = null,
    Quoting = null,
    Escape = null,
    HasHeader = null,
    Headers = null,
    Comment = null,
    Trim = BamlUnion<string, string, string, string>.FromT3("none"),
    SkipLines = null,
    SkipBlankRecords = null,
    Ragged = null,
    NullValues = null,
    Encoding = null,
    Bom = null,
    OnError = null,
    OnSkip = null,
    MaxSkipped = null,
    Limit = 17,
};
ReaderOptions readerResult = UserFunctions.EchoReaderOptions(readerOptions);
Require(
    readerResult.Delimiter is null
        && readerResult.Trim.HasValue
        && readerResult.Trim.Value.IsT3
        && readerResult.Trim.Value.AsT3 == "none"
        && readerResult.Ragged is null
        && readerResult.OnSkip is null
        && readerResult.Limit == 17,
    "CSV reader options roundtrip changed");

var writerOptions = new WriterOptions
{
    Delimiter = null,
    Quote = null,
    QuoteStyle = null,
    Escape = null,
    Terminator = null,
    WriteHeader = null,
    Headers = null,
    NullValue = "NULL\0雪",
    Bom = null,
    SanitizeFormulas = true,
};
WriterOptions writerResult = await UserFunctions.EchoWriterOptionsAsync(writerOptions);
Require(
    writerResult.QuoteStyle is null
        && writerResult.Terminator is null
        && writerResult.NullValue == writerOptions.NullValue
        && writerResult.SanitizeFormulas == true,
    "CSV writer options roundtrip changed");

Console.WriteLine("csharp_phase14_stdlib_structurals=ok");
return 0;

static void Require(bool condition, string message)
{
    if (!condition)
    {
        throw new InvalidOperationException(message);
    }
}
