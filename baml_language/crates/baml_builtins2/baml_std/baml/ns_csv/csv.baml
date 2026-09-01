// BEP-060: CSV standard library.
//
// Streaming `Reader` / `Writer` handles built around `baml.iter.Iterator`
// (BEP-051), with typed decode (`rows<T>`, `decode<T>`, `read<T>`) using the
// same column-matching rules family as `baml.json` (BEP-038) and structured,
// positional `Error` throws.
//
// The streaming reader is the only parser; every one-shot is a wrapper over
// it. File IO goes through `baml.fs` handles: the reader pumps chunks via
// `baml.io.Read` from thin BAML method bodies, while all tokenization,
// decoding, and encoding is native.

/// Classifies a `Error` for portable handling.
enum ErrorKind {
    /// Invalid configuration, or an operation the handle does not support.
    Options,
    /// Unterminated or stray quote in a record.
    Quote,
    /// Record width differs from the expected width (see `ReaderOptions.ragged`).
    FieldCount,
    /// Invalid UTF-8 in a record (`encoding = "utf8"`).
    Encoding,
    /// Missing, duplicate, or unusable header for the requested operation.
    Header,
    /// A cell could not convert to the requested type.
    Decode,
    /// A value not representable under the writer options.
    Encode,
    /// `decode_one` on an input with zero data records.
    NotFound,
    /// `decode_one` / `decode_optional` on an input with extra records.
    TooManyRows,
    /// Operation on a closed reader or writer.
    Closed,
}

/// A structured CSV error with positional diagnostics.
///
/// `kind` is for portable handling; `line` / `record` / `field` / `column`
/// make "row 41,283 has 7 fields, expected 8" diagnostics cheap.
class Error {
    kind: ErrorKind,
    message: string,
    /// 1-based line in the source where the offending record starts.
    line: int?,
    /// 0-based data-record index (header excluded).
    record: int?,
    /// 0-based field index within the record.
    field: int?,
    /// Header name, when known.
    column: string?,
    /// `FieldCount`: expected width.
    expected: int?,
    /// `FieldCount`: actual width.
    found: int?,
}

/// A position in a CSV source.
class Position {
    /// 0-based byte offset into the source.
    byte: int,
    /// 1-based line number.
    line: int,
    /// 0-based data-record index (header excluded).
    record: int,
}

/// Options for CSV readers. All fields are optional; `null` means default.
///
/// `delimiter`, `quote`, `escape`, and `comment` must be single ASCII bytes
/// and mutually distinct; violations throw `Error { kind: Options }` at
/// construction.
class ReaderOptions {
    /// Field separator. Exactly one ASCII byte. Default `","` (`"\t"` for TSV).
    delimiter: string?,
    /// Quote character. Exactly one ASCII byte. Default `"\""`.
    quote: string?,
    /// `false` treats quote bytes as ordinary data. Default `true`.
    quoting: bool?,
    /// Escape byte for quotes. `null` (default) means RFC 4180 `""` doubling;
    /// setting it switches to escape-character dialects (`"\\"` for MySQL-style
    /// dumps).
    escape: string?,
    /// First record is the header. Default `true`.
    has_header: bool?,
    /// Supply (`has_header = false`) or replace (`has_header = true`; the
    /// file's header row is consumed) column names.
    headers: string[]?,
    /// Records whose first byte (outside quotes) matches are skipped. Off by
    /// default — `#` is valid data.
    comment: string?,
    /// Strip leading/trailing ASCII space and tab from header and/or unquoted
    /// field values. Default `"none"`.
    trim: ("none" | "headers" | "fields" | "all")?,
    /// Raw lines dropped before parsing begins (preamble junk, Excel `sep=;`
    /// lines, a Markdown code fence). Default `0`.
    skip_lines: int?,
    /// Drop blank records. Default `true`.
    skip_blank_records: bool?,
    /// Ragged-record policy. `"strict"` (default): width mismatch throws
    /// `FieldCount`. `"pad"`: short records padded with empty cells (long ones
    /// still throw). `"truncate"`: long records cut to the expected width
    /// (short ones still throw).
    ragged: ("strict" | "pad" | "truncate")?,
    /// Unquoted cell texts (after trim) treated as null cells. Default `[]`.
    null_values: string[]?,
    /// `"utf8"` (default) throws `Encoding` on invalid bytes; `"utf8-lossy"`
    /// substitutes U+FFFD.
    encoding: ("utf8" | "utf8-lossy")?,
    /// A leading UTF-8 BOM is stripped by default so it never corrupts the
    /// first header name.
    bom: ("strip" | "keep")?,
    /// Per-record error policy. `"throw"` (default): `next()` throws a
    /// `Error` for a malformed record and the reader resumes at the next
    /// record. `"skip"`: record-scoped errors are counted and skipped.
    on_error: ("throw" | "skip")?,
    /// Observer invoked with each skipped record's `Error` when
    /// `on_error = "skip"`. Constant-memory alternative to `skipped()`.
    on_skip: ((Error) -> null throws never)?,
    /// How many skipped `Error` values `skipped()` retains. Default 1000.
    /// `0` disables retention. `skipped_count()` stays exact regardless.
    max_skipped: int?,
    /// Stop after this many data records. Default unlimited.
    limit: int?,
}

/// Options for CSV writers. All fields are optional; `null` means default.
class WriterOptions {
    /// Field separator. Exactly one ASCII byte. Default `","`.
    delimiter: string?,
    /// Quote character. Exactly one ASCII byte. Default `"\""`.
    quote: string?,
    /// `"minimal"` (default): quote only fields containing the delimiter,
    /// quote, CR, or LF. `"all"`: quote every field. `"never"`: quote nothing;
    /// a field that would require quoting throws `Encode`.
    quote_style: ("minimal" | "all" | "never")?,
    /// `null` (default) means `""` doubling inside quoted fields.
    escape: string?,
    /// Record terminator. Default `"lf"`; `"crlf"` for strict RFC 4180 / Excel.
    terminator: ("lf" | "crlf")?,
    /// Automatically emit a header before the first `write_row<T>`.
    /// Default `true`.
    write_header: bool?,
    /// Override header names for typed writes (positional).
    headers: string[]?,
    /// How `null` serializes. Default `""`.
    null_value: string?,
    /// Emit a UTF-8 BOM (Excel's UTF-8 detection). Default `false`.
    bom: bool?,
    /// CSV-injection guard: prefix `'` to cells starting with `=`, `+`, `-`,
    /// `@`, tab, CR, or LF (including full-width variants). Mutates the data,
    /// which is why it is opt-in. Default `false`.
    sanitize_formulas: bool?,
}

/// The cell value model for raw record writes.
type Value = null | bool | int | bigint | float | string;

/// (internal) Marker: the reader needs more input bytes before it can make
/// progress. Do not use directly.
class _NeedData {
}

/// (internal) Marker: a record was skipped under `on_error = "skip"`.
/// Do not use directly.
class _Skip {
    error: Error,
}

/// (internal) Marker: resolved header names (`null` when there are none).
/// Do not use directly.
class _Headers {
    names: string[]?,
}

/// One raw CSV record. Yielded by iterating a `Reader`.
///
/// Records have snapshot semantics: they remain valid after the reader
/// advances. Cell access is lazy — `get<int>("amount")` converts one cell
/// without touching the others.
class Record {
    _handle: $rust_type,

    /// Converts the cell under header `column` to `T`.
    ///
    /// Returns `null` for a column name absent from the headers, a missing
    /// cell, or a null cell. Throws `Error { kind: Decode }` when a cell
    /// exists but cannot convert to `T`, and `Error { kind: Header }` when
    /// the name is duplicated in the header or when name-based access is used
    /// with no headers at all.
    function get<T>(self, column: string) -> T? throws Error {
        _cell_by_name<T>(self, column)
    }

    /// Converts the cell at `index` to `T`. Returns `null` for a missing or
    /// null cell; throws `Error { kind: Decode }` on conversion failure.
    function get_at<T>(self, index: int) -> T? throws Error {
        _cell_by_index<T>(self, index)
    }

    /// All cells as raw text, verbatim (no `null_values` mapping).
    //baml:mut_vm
    function fields(self) -> string[] throws never {
        $rust_function
    }

    /// Number of cells in this record.
    //baml:vm
    function length(self) -> int throws never {
        $rust_function
    }

    /// The position of the start of this record in the source.
    //baml:mut_vm
    function position(self) -> Position throws never {
        $rust_function
    }

    /// Applies the full typed-decode rules to this one record — useful for
    /// routing heterogeneous rows. Always throws on failure, regardless of
    /// the reader's `on_error` policy.
    function decode<T>(self) -> T throws Error {
        _decode_record<T>(self)
    }

    /// The record as a header-name → cell-text map. Throws
    /// `Error { kind: Header }` when headers are unavailable or duplicated.
    //baml:mut_vm
    function to_map(self) -> map<string, string> throws Error {
        $rust_function
    }
}

/// A streaming CSV reader. Create one with `baml.csv.open` or
/// `baml.csv.reader`.
///
/// `Reader` is a lazy iterator over `Record` values: a multi-gigabyte
/// file parses in constant memory, and every `baml.iter` adapter and default
/// method (`map`, `filter`, `collect`, ...) works on it. There is one
/// cursor; raw iteration, `rows<T>()`, and `headers()` all draw from the
/// same stream.
///
/// A thrown record error has already consumed the offending record: the
/// parser resynchronizes at the next record boundary and the next call to
/// `next()` continues. One corrupt cell costs exactly one record.
class Reader {
    _handle: $rust_type,
    _file: root.fs.File?,
    _on_skip: ((Error) -> null throws never)?,
    _owns_file: bool,

    implements root.iter.Iterable {
        type Item = root.csv.Record
        type Error = root.csv.Error | root.errors.Io

        function iter(
            self,
        ) -> root.iter.Iterator<Item = Record, Error = root.csv.Error | root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = root.csv.Record
        type Error = root.csv.Error | root.errors.Io

        function next(self) -> Record | root.iter.Done throws Error | root.errors.Io {
            while (true) {
                match (self._poll()) {
                    let rec: Record => {
                        return rec;
                    },
                    let d: root.iter.Done => {
                        return d;
                    },
                    let s: _Skip => {
                        match (self._on_skip) {
                            null => null,
                            let cb: (Error) -> null throws never => cb(s.error),
                        };
                    },
                    _NeedData => {
                        self._read_chunk();
                    },
                }
            }
            root.sys.panic("unreachable: Reader.next loop ended")
        }
    }

    /// The column names: the `headers` option when set, the file's header row
    /// when `has_header = true` (consumed on first call), and `null` otherwise.
    ///
    /// Parse errors in the header record (`Quote`, `Encoding`) surface from
    /// this call (or from whichever call reads the header first), and they
    /// exhaust the reader: without a trustworthy header every subsequent
    /// name-to-column mapping would be silently wrong, so the stream ends
    /// rather than mis-mapping a million rows.
    function headers(self) -> string[]? throws Error | root.errors.Io {
        while (true) {
            match (self._poll_headers()) {
                let h: _Headers => {
                    return h.names;
                },
                _NeedData => {
                    self._read_chunk();
                },
            }
        }
        root.sys.panic("unreachable: Reader.headers loop ended")
    }

    /// A typed iterator over the remaining records, decoding each into `T`.
    ///
    /// Validates eagerly: reads the header if necessary and checks that every
    /// non-optional field of `T` is satisfiable, throwing
    /// `Error { kind: Header }` at the call site rather than on the
    /// millionth row.
    function rows<T>(
        self,
    ) -> root.iter.Iterator<Item = T, Error = Error | root.errors.Io> throws Error | root.errors.Io {
        self.headers();
        _validate_columns<T>(self);
        Rows<T> { reader: self }
    }

    /// The first `max_skipped` (default 1,000) `Error` values skipped under
    /// `on_error = "skip"`.
    //baml:mut_vm
    function skipped(self) -> Error[] throws never {
        $rust_function
    }

    /// Exact count of records skipped under `on_error = "skip"`.
    //baml:vm
    function skipped_count(self) -> int throws never {
        $rust_function
    }

    /// The position of the start of the next unread record. Its `record`
    /// field counts all data records consumed so far, including skipped ones.
    //baml:mut_vm
    function position(self) -> Position throws never {
        $rust_function
    }

    /// Releases the reader. Idempotent. The underlying file handle is closed
    /// only when the reader owns it (created by `open`); a handle passed to
    /// `reader(file)` stays open — its lifecycle belongs to the caller.
    /// Subsequent `next()`, `headers()`, and `rows<T>()` calls throw
    /// `Error { kind: Closed }`; `skipped()`, `skipped_count()`, and
    /// `position()` remain valid. As with `baml.fs.File`, closing is optional:
    /// the GC reclaims unreferenced readers.
    function close(self) -> null throws root.errors.Io {
        self._mark_closed();
        match (self._file) {
            null => null,
            let f: root.fs.File => {
                if (self._owns_file) {
                    f.close();
                }
                null
            },
        }
    }

    /// (internal) Advance the native parser by one step.
    //baml:mut_vm
    function _poll(self) -> Record | _Skip | _NeedData | root.iter.Done throws Error {
        $rust_function
    }

    /// (internal) Resolve headers, or request more input.
    //baml:mut_vm
    function _poll_headers(self) -> _Headers | _NeedData throws Error {
        $rust_function
    }

    /// (internal) Append a chunk of source bytes to the parse buffer.
    //baml:vm
    function _feed(self, chunk: uint8array) -> null throws never {
        $rust_function
    }

    /// (internal) Mark the source as exhausted.
    //baml:vm
    function _feed_eof(self) -> null throws never {
        $rust_function
    }

    /// (internal) Mark the reader closed (idempotent).
    //baml:vm
    function _mark_closed(self) -> null throws never {
        $rust_function
    }

    /// (internal) Mark the iterator exhausted: subsequent polls return Done.
    //baml:vm
    function _mark_exhausted(self) -> null throws never {
        $rust_function
    }

    /// (internal) Pull the next chunk from the backing file, or mark EOF for
    /// in-memory sources. IO errors are stream-scoped and fatal: the reader is
    /// marked exhausted before the throw, so subsequent `next()` calls return
    /// `baml.iter.Done`. A closed handle arrives as an IO error because
    /// `baml.io.Read` has no separate closed-handle error channel.
    function _read_chunk(self) -> void throws root.errors.Io {
        let chunk = self._file?.read(65536) catch (e) {
            root.errors.Io => {
                self._mark_exhausted();
                throw e
            },
        };
        match (chunk) {
            null => {
                self._feed_eof();
            },
            let bytes: uint8array => {
                self._feed(bytes);
            },
        }
    }
}

/// A typed iterator over a `Reader`, decoding each record into `T`.
/// Create one with `Reader.rows<T>()`.
class Rows<T> {
    reader: Reader,

    implements root.iter.Iterable {
        type Item = T
        type Error = root.csv.Error | root.errors.Io

        function iter(
            self,
        ) -> root.iter.Iterator<Item = T, Error = root.csv.Error | root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = T
        type Error = root.csv.Error | root.errors.Io

        function next(self) -> T | root.iter.Done throws Error | root.errors.Io {
            while (true) {
                match (self.reader.next()) {
                    let d: root.iter.Done => {
                        return d;
                    },
                    let rec: Record => {
                        match (_try_decode<T>(rec, self.reader)) {
                            let s: _Skip => {
                                match (self.reader._on_skip) {
                                    null => null,
                                    let cb: (Error) -> null throws never => cb(s.error),
                                };
                            },
                            let v: T => {
                                return v;
                            },
                        }
                    },
                }
            }
            root.sys.panic("unreachable: Rows.next loop ended")
        }
    }
}

/// A streaming CSV writer. Create one with `baml.csv.create`,
/// `baml.csv.writer`, or `baml.csv.buffer`.
class Writer {
    _handle: $rust_type,
    _file: root.fs.File?,
    _owns_file: bool,

    /// Writes one raw record. Never triggers an automatic header.
    function write_record(self, record: Value[]) -> null throws Error | root.errors.Io {
        self._emit(self._encode_record(record))
    }

    /// Writes one typed row. Fields serialize in declaration order; if
    /// `write_header` is enabled (default) and no header has been written yet,
    /// the header derived from `T`'s field names (or `WriterOptions.headers`)
    /// is emitted first.
    function write_row<T>(self, row: T) -> null throws Error | root.errors.Io {
        self._emit(_encode_row<T>(self, row))
    }

    /// Writes many typed rows, as one atomic batch: on a thrown error nothing
    /// from the batch is emitted or counted. Otherwise equivalent to
    /// `write_row<T>` in a loop.
    function write_rows<T>(self, rows: T[]) -> null throws Error | root.errors.Io {
        self._emit(_encode_rows<T>(self, rows))
    }

    /// Writes an explicit header record. Throws `Error { kind: Header }`
    /// after data has been written.
    function write_header(self, names: string[]) -> null throws Error | root.errors.Io {
        self._emit(self._encode_header(names))
    }

    /// Number of data records written so far (headers excluded). Remains
    /// valid after `close()`.
    //baml:vm
    function records_written(self) -> int throws never {
        $rust_function
    }

    /// The accumulated CSV text. Valid only on `buffer()` writers; throws
    /// `Error { kind: Options }` otherwise.
    //baml:mut_vm
    function text(self) -> string throws Error {
        $rust_function
    }

    /// Flushes pending output. Writes through `baml.fs` flush eagerly, so this
    /// is an infallible no-op kept for symmetry with other writer APIs.
    function flush(self) -> null {
        null
    }

    /// Flushes and releases the writer. Idempotent. The underlying file
    /// handle is closed only when the writer owns it (created by `create`); a
    /// handle passed to `writer(file)` stays open — its lifecycle belongs to
    /// the caller. Writes after close throw `Error { kind: Closed }`;
    /// `records_written()` remains valid.
    function close(self) -> null throws root.errors.Io {
        self._mark_closed();
        match (self._file) {
            null => null,
            let f: root.fs.File => {
                if (self._owns_file) {
                    f.close();
                }
                null
            },
        }
    }

    /// (internal) Encode one raw record, updating writer state.
    //baml:mut_vm
    function _encode_record(self, record: Value[]) -> string throws Error {
        $rust_function
    }

    /// (internal) Encode an explicit header record, updating writer state.
    //baml:mut_vm
    function _encode_header(self, names: string[]) -> string throws Error {
        $rust_function
    }

    /// (internal) Total bytes encoded so far.
    //baml:vm
    function _bytes_written(self) -> int throws never {
        $rust_function
    }

    /// (internal) Mark the writer closed (idempotent).
    //baml:vm
    function _mark_closed(self) -> null throws never {
        $rust_function
    }

    /// (internal) Write encoded text to the backing file, if any. Buffer
    /// writers accumulate natively and receive `""` here.
    function _emit(self, out: string) -> null throws root.errors.Io {
        match (self._file) {
            null => null,
            let f: root.fs.File => {
                if (out.length() > 0) {
                    f.write(out);
                }
                null
            },
        }
    }
}

// ─── Reading ────────────────────────────────────────────────────────────────

/// A streaming reader over in-memory text/bytes or an open file handle.
///
/// A `string` or `uint8array` source is CSV content, parsed in place. A
/// `baml.fs.File` source is streamed from its current cursor to EOF; from
/// construction until the reader is exhausted or closed, the reader owns
/// that cursor.
function reader(
    source: string | uint8array | root.fs.File,
    options: ReaderOptions? = null,
) -> Reader throws Error {
    _reader(source, options, false)
}

/// Opens the file at `path` via `baml.fs` and streams it.
function open(
    path: string,
    options: ReaderOptions? = null,
) -> Reader throws Error | root.errors.Io {
    let f = baml.fs.open(path, "r") catch (_) {
        root.errors.InvalidArgument { message } => throw root.errors.Io { message: message },
    };
    _reader(f, options, true)
}

/// Eager typed read of a whole file: `open(path).rows<T>().collect()`.
///
/// If the file might not fit in memory, `open` it and stream instead — the
/// options are the same; the streaming spelling is a verb swap away.
function read<T>(
    path: string,
    options: ReaderOptions? = null,
) -> T[] throws Error | root.errors.Io {
    let r = open(path, options = options);
    let result = r.rows<T>().collect();
    r.close();
    result
}

/// Eager untyped parse of in-memory input: records as string arrays.
function parse(
    source: string | uint8array,
    options: ReaderOptions? = null,
) -> string[][] throws Error {
    {
        let r = reader(source, options = options);
        let out: string[][] = [];
        while (true) {
            match (r.next()) {
                root.iter.Done => {
                    break;
                },
                let rec: Record => {
                    out.push(rec.fields());
                },
            }
        }
        out
    } catch (_) {
        root.errors.Io { message } => {
            root.sys.panic("unreachable: in-memory CSV source raised Io: " + message)
        },
    }
}

/// Eager typed parse of in-memory input.
function decode<T>(
    source: string | uint8array,
    options: ReaderOptions? = null,
) -> T[] throws Error {
    {
        reader(source, options = options).rows<T>().collect()
    } catch (_) {
        root.errors.Io { message } => {
            root.sys.panic("unreachable: in-memory CSV source raised Io: " + message)
        },
    }
}

/// Typed parse of an input with zero or one data record. Extra records
/// throw `Error { kind: TooManyRows }`.
function decode_optional<T>(
    source: string | uint8array,
    options: ReaderOptions? = null,
) -> T? throws Error {
    let all = decode<T>(source, options = options);
    if (all.length() > 1) {
        throw _error(ErrorKind.TooManyRows, "expected at most one data record, found " + root.json.stringify(all.length()));
    }
    all.at(0)
}

/// Typed parse of an input with exactly one data record. Zero records throw
/// `Error { kind: NotFound }`; extra records throw
/// `Error { kind: TooManyRows }`.
///
/// Fits LLM output that was requested as exactly one row — with
/// `has_header: false` for a bare row (positional decode), or default
/// options when the model was asked to emit a header plus one data row.
function decode_one<T>(
    source: string | uint8array,
    options: ReaderOptions? = null,
) -> T throws Error {
    let all = decode<T>(source, options = options);
    if (all.length() == 0) {
        throw _error(ErrorKind.NotFound, "expected exactly one data record, found none");
    }
    if (all.length() > 1) {
        throw _error(ErrorKind.TooManyRows, "expected exactly one data record, found " + root.json.stringify(all.length()));
    }
    match (all.at(0)) {
        null => root.sys.panic("unreachable: decode_one lost its record"),
        let v: T => v,
    }
}

// ─── Writing ────────────────────────────────────────────────────────────────

/// A streaming writer to an open file handle. Emits from the file's current
/// cursor and never truncates; for fresh output files, use `create`.
function writer(file: root.fs.File, options: WriterOptions? = null) -> Writer throws Error {
    _writer(file, options, false)
}

/// Creates or truncates the file at `path` (auto-creating parent
/// directories, like `baml.fs.write`) and stream-writes to it.
function create(
    path: string,
    options: WriterOptions? = null,
) -> Writer throws Error | root.errors.Io {
    let f = baml.fs.open(path, "w") catch (_) {
        root.errors.InvalidArgument { message } => throw root.errors.Io { message: message },
    };
    _writer(f, options, true)
}

/// An in-memory writer; retrieve the accumulated CSV with `text()`.
function buffer(options: WriterOptions? = null) -> Writer throws Error {
    _buffer(options = options)
}

/// One-shot typed file write; returns bytes written, mirroring
/// `baml.fs.write` (creates or truncates).
function write<T>(
    path: string,
    rows: T[],
    options: WriterOptions? = null,
) -> int throws Error | root.errors.Io {
    let w = create(path, options = options);
    w.write_rows<T>(rows);
    w.close();
    w._bytes_written()
}

/// Serializes rows of `T` to CSV text.
function stringify<T>(rows: T[], options: WriterOptions? = null) -> string throws Error {
    {
        let w = buffer(options = options);
        w.write_rows<T>(rows);
        w.text()
    } catch (_) {
        root.errors.Io { message } => {
            root.sys.panic("unreachable: buffer writer raised Io: " + message)
        },
    }
}

/// Serializes raw records to CSV text.
function stringify_records(
    records: Value[][],
    options: WriterOptions? = null,
) -> string throws Error {
    {
        let w = buffer(options = options);
        for (let rec in records) {
            w.write_record(rec);
        }
        w.text()
    } catch (_) {
        root.errors.Io { message } => {
            root.sys.panic("unreachable: buffer writer raised Io: " + message)
        },
    }
}

// ─── Prompt rendering ───────────────────────────────────────────────────────

/// Renders rows of `T` as a GitHub-style Markdown table for prompt context.
///
/// Headers come from `T`'s field names. `|` is escaped and embedded newlines
/// become spaces; non-finite floats render as `NaN` / `inf` (prompt text is
/// not meant to round-trip). Beyond `max_rows`, output is truncated with a
/// final `… (N more rows)` line.
function to_markdown<T>(rows: T[], max_rows: int = 100) -> string throws never {
    _to_markdown<T>(rows, max_rows = max_rows)
}

/// Renders untyped records as a GitHub-style Markdown table.
function to_markdown_records(
    records: string[][],
    headers: string[]? = null,
    max_rows: int = 100,
) -> string throws never {
    _to_markdown_records(records, headers = headers, max_rows = max_rows)
}

// ─── Internal natives ───────────────────────────────────────────────────────

/// (internal) Construct a reader handle. Do not call directly; use `reader`.
//baml:mut_vm
function _reader(
    source: string | uint8array | root.fs.File,
    options: ReaderOptions?,
    owns_file: bool,
) -> Reader throws Error {
    $rust_function
}

/// (internal) Construct a file-backed writer handle. Use `writer`.
//baml:mut_vm
function _writer(
    file: root.fs.File,
    options: WriterOptions?,
    owns_file: bool,
) -> Writer throws Error {
    $rust_function
}

/// (internal) Construct a buffer writer handle. Use `buffer`.
//baml:mut_vm
function _buffer(options: WriterOptions?) -> Writer throws Error {
    $rust_function
}

/// (internal) Check that every non-optional field of `T` is satisfiable by
/// the reader's columns. Use `Reader.rows<T>`.
//baml:mut_vm
function _validate_columns<T>(r: Reader) -> void throws Error {
    $rust_function
}

/// (internal) Decode one record into `T`, honoring the reader's `on_error`
/// policy: returns `_Skip` instead of throwing under `"skip"`.
//baml:mut_vm
function _try_decode<T>(rec: Record, r: Reader) -> T | _Skip throws Error {
    $rust_function
}

/// (internal) Decode one record into `T`, always throwing on failure. Use
/// `Record.decode<T>`.
//baml:mut_vm
function _decode_record<T>(rec: Record) -> T throws Error {
    $rust_function
}

/// (internal) Convert one cell by header name. Use `Record.get<T>`.
//baml:mut_vm
function _cell_by_name<T>(rec: Record, column: string) -> T? throws Error {
    $rust_function
}

/// (internal) Convert one cell by index. Use `Record.get_at<T>`.
//baml:mut_vm
function _cell_by_index<T>(rec: Record, index: int) -> T? throws Error {
    $rust_function
}

/// (internal) Encode one typed row (plus auto-header). Use
/// `Writer.write_row<T>`.
//baml:mut_vm
function _encode_row<T>(w: Writer, row: T) -> string throws Error {
    $rust_function
}

/// (internal) Encode many typed rows (plus auto-header). Use
/// `Writer.write_rows<T>`.
//baml:mut_vm
function _encode_rows<T>(w: Writer, rows: T[]) -> string throws Error {
    $rust_function
}

/// (internal) Render typed rows as a Markdown table. Use `to_markdown`.
//baml:mut_vm
function _to_markdown<T>(rows: T[], max_rows: int) -> string throws never {
    $rust_function
}

/// (internal) Render raw records as a Markdown table. Use
/// `to_markdown_records`.
//baml:mut_vm
function _to_markdown_records(
    records: string[][],
    headers: string[]?,
    max_rows: int,
) -> string throws never {
    $rust_function
}

/// (internal) Construct a `Error` with no positional fields.
function _error(kind: ErrorKind, message: string) -> Error throws never {
    Error {
        kind: kind,
        message: message,
        line: null,
        record: null,
        field: null,
        column: null,
        expected: null,
        found: null,
    }
}
