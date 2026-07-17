using Baml;
using BamlSdk;

var original = Functions.NewCsvBuffer();
using var writer = original.Clone();
original.Dispose();

try
{
    _ = original.Text();
    throw new InvalidOperationException("A disposed BAML CSV writer remained usable.");
}
catch (ObjectDisposedException)
{
}

writer.WriteHeader(["name", "count"]);
await writer.WriteRecordAsync(["Ada", 37L]);

using var roundTripped = Functions.RoundTripCsvWriter(writer);
await roundTripped.WriteRowAsync(new CsvProbeRow { Name = "Grace", Count = 41 });
roundTripped.Flush();

const string expected = "name,count\nAda,37\nGrace,41\n";
if (writer.RecordsWritten() != 2
    || await roundTripped.RecordsWrittenAsync() != 2
    || writer.Text() != expected
    || await roundTripped.TextAsync() != expected)
{
    throw new InvalidOperationException("BAML CSV writer state did not survive clone and encode-back ownership.");
}

await roundTripped.CloseAsync();
try
{
    writer.WriteRecord(["late", 1L]);
    throw new InvalidOperationException("A closed BAML CSV writer accepted another record.");
}
catch (BamlError error) when (error.ClassName == "baml.csv.CsvError")
{
}

var configuredOptions = new BamlCsvWriterOptions(
    delimiter: ";",
    terminator: "crlf",
    writeHeader: false,
    headers: ["left", "right"],
    nullValue: "NULL");
var returnedOptions = Functions.RoundTripCsvWriterOptions(configuredOptions);
if (returnedOptions.Delimiter != ";"
    || returnedOptions.Terminator != "crlf"
    || returnedOptions.WriteHeader != false
    || !returnedOptions.Headers!.SequenceEqual(["left", "right"])
    || returnedOptions.NullValue != "NULL")
{
    throw new InvalidOperationException("BAML CSV writer options did not round trip exactly.");
}

using (var configuredWriter = Functions.NewCsvBufferWithOptions(configuredOptions))
{
    configuredWriter.WriteRecord(["value", null]);
    if (configuredWriter.Text() != "value;NULL\r\n")
    {
        throw new InvalidOperationException("BAML CSV writer options were not applied.");
    }
}

var readerOptions = new BamlCsvReaderOptions(
    delimiter: ";",
    hasHeader: false,
    headers: ["name", "count"],
    trim: "fields",
    skipLines: 1,
    skipBlankRecords: true,
    ragged: "strict",
    nullValues: ["NULL"],
    encoding: "utf8",
    bom: "strip",
    onError: "throw",
    maxSkipped: 5,
    limit: 1);
var returnedReaderOptions = Functions.RoundTripCsvReaderOptions(readerOptions);
if (returnedReaderOptions.Delimiter != ";"
    || returnedReaderOptions.HasHeader != false
    || !returnedReaderOptions.Headers!.SequenceEqual(["name", "count"])
    || returnedReaderOptions.Trim != "fields"
    || returnedReaderOptions.SkipLines != 1
    || returnedReaderOptions.Ragged != "strict"
    || !returnedReaderOptions.NullValues!.SequenceEqual(["NULL"])
    || returnedReaderOptions.Limit != 1)
{
    throw new InvalidOperationException("BAML CSV reader options did not round trip exactly.");
}

using (var configuredReader = Functions.NewCsvReaderWithOptions(
           "preamble\n  Ada ; NULL \nGrace;42\n",
           readerOptions))
{
    var item = configuredReader.Next();
    using var record = item.AsT0;
    if (record.Get<string>("name").Value != "Ada"
        || !record.Get<long>("count").IsNull
        || !ReferenceEquals(configuredReader.Next().AsT1, BamlIteratorDone.Instance))
    {
        throw new InvalidOperationException("BAML CSV reader options were not applied.");
    }
}

var originalReader = Functions.NewCsvReader(expected);
using var reader = originalReader.Clone();
originalReader.Dispose();

if (!reader.Headers()!.SequenceEqual(["name", "count"]))
{
    throw new InvalidOperationException("BAML CSV reader returned invalid headers.");
}

using var roundTrippedReader = Functions.RoundTripCsvReader(reader);
var firstItem = await roundTrippedReader.NextAsync();
using var first = firstItem.AsT0;
if (first.Length() != 2
    || !first.Fields().SequenceEqual(["Ada", "37"])
    || first.Get<string>("name").Value != "Ada"
    || first.GetAt<long>(1).Value != 37
    || first.Position().Line != 2)
{
    throw new InvalidOperationException("BAML CSV record accessors returned invalid data.");
}

using var roundTrippedRecord = Functions.RoundTripCsvRecord(first);
var decodedRow = await roundTrippedRecord.DecodeAsync<CsvProbeRow>();
if (decodedRow.Name != "Ada"
    || decodedRow.Count != 37
    || roundTrippedRecord.ToMap()["count"] != "37")
{
    throw new InvalidOperationException("BAML CSV typed record decoding returned invalid data.");
}

var secondItem = reader.Next();
using var second = secondItem.AsT0;
if (second.Get<string>("name").Value != "Grace")
{
    throw new InvalidOperationException("BAML CSV reader clones did not share one cursor.");
}

var done = roundTrippedReader.Next();
if (!ReferenceEquals(done.AsT1, BamlIteratorDone.Instance)
    || reader.SkippedCount() != 0
    || reader.Skipped().Count != 0
    || reader.Position().Record != 2)
{
    throw new InvalidOperationException("BAML CSV reader completion state was invalid.");
}

reader.Close();
try
{
    _ = roundTrippedReader.Next();
    throw new InvalidOperationException("A closed BAML CSV reader remained readable.");
}
catch (BamlError error) when (error.ClassName == "baml.csv.CsvError")
{
}

Console.WriteLine("C# BAML CSV integration passed.");
