class CsvProbeRow {
  name string
  count int
}

function NewCsvBuffer() -> baml.csv.CsvWriter {
  baml.csv.buffer()
}

function NewCsvBufferWithOptions(options: baml.csv.WriterOptions) -> baml.csv.CsvWriter {
  baml.csv.buffer(options = options)
}

function RoundTripCsvWriterOptions(options: baml.csv.WriterOptions) -> baml.csv.WriterOptions {
  options
}

function RoundTripCsvWriter(writer: baml.csv.CsvWriter) -> baml.csv.CsvWriter {
  writer
}

function NewCsvReader(source: string) -> baml.csv.CsvReader {
  baml.csv.reader(source)
}

function NewCsvReaderWithOptions(source: string, options: baml.csv.ReaderOptions) -> baml.csv.CsvReader {
  baml.csv.reader(source, options = options)
}

function RoundTripCsvReaderOptions(options: baml.csv.ReaderOptions) -> baml.csv.ReaderOptions {
  options
}

function RoundTripCsvReader(reader: baml.csv.CsvReader) -> baml.csv.CsvReader {
  reader
}

function RoundTripCsvRecord(record: baml.csv.CsvRecord) -> baml.csv.CsvRecord {
  record
}
