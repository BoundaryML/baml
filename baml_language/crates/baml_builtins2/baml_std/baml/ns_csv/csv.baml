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

/// Classifies an `Error` for portable handling.
///
/// Match on this rather than on `Error.message`, which is prose and not
/// stable. The split that matters is `Options` and `Header` against the
/// record-scoped kinds below them: those two are mistakes in the *setup*,
/// which no amount of retrying rows will fix, so `on_error = "skip"` never
/// suppresses them.
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

/// A CSV error with positional diagnostics.
///
/// `kind` is for portable handling. `line`, `record`, `field`, and `column`
/// make "row 41,283 has 7 fields, expected 8" cheap to report.
///
/// Every positional field is optional, because not every error has a position:
/// a bad option is caught before parsing starts, and `NotFound` /
/// `TooManyRows` are about the input as a whole. They narrow as the failure
/// does — `line` and `record` whenever a record is in hand, `field` and
/// `column` when one cell is at fault, `expected` and `found` only for
/// `FieldCount`.
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
///
/// The three coordinates disagree on purpose: `byte` is where to seek, `line`
/// is what a text editor shows, and `record` is what the data means. A quoted
/// field spanning newlines makes them diverge, which is exactly when a bare
/// line number would mislead.
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
/// (never CR or LF) and mutually distinct; violations throw
/// `Error { kind: Options }` at construction, as do negative `skip_lines`,
/// `max_skipped`, or `limit`. Validating up front means a misconfiguration
/// surfaces from `reader` / `open`, not a million records in.
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
///
/// As with `ReaderOptions`, `delimiter`, `quote`, and `escape` must be single
/// ASCII bytes (never CR or LF) and mutually distinct; violations throw
/// `Error { kind: Options }` when the writer is constructed.
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

/// A cell value for raw record writes.
///
/// Write-side only. `write_record` and `stringify_records` take these; reading
/// hands back cell *text* (`Record.fields`) or a decoded `T`. CSV carries no
/// type information to recover on the way in, so the two directions have no
/// reason to share a type. A `null` writes as `WriterOptions.null_value`, `""`
/// by default.
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
/// Where Python's `csv.DictReader` hands you a finished dict of strings, a
/// `Record` converts on demand: `get<int>("amount")` reads one cell and leaves
/// the others untouched. `to_map()` is there when you do want the whole dict.
///
/// Records are snapshots and stay valid after the reader advances. Each one
/// carries the headers in force when it was read, so name-based access keeps
/// working on a record you held onto.
///
/// The cell-decodable types, shared by `get`, `get_at`, and `decode`, are
/// `string`, `int`, `bigint`, `float`, `bool`, any enum (matched against
/// variant names), `baml.time.Instant`, `baml.time.PlainDate`,
/// `baml.time.PlainDateTime`, and optionals of those. Any other `T` is a
/// configuration mistake, not a data problem, and throws
/// `Error { kind: Options }`.
///
/// A "null cell" is an unquoted cell that is empty or matches
/// `ReaderOptions.null_values`; quoting a cell always makes it data. The one
/// asymmetry: an empty cell read as `T = string` is `""`, not `null`, since an
/// empty string is a perfectly good string and CSV cannot tell the two apart.
class Record {
    _handle: $rust_type,

    /// Converts the cell under header `column` to `T`.
    ///
    /// # Returns
    ///
    /// `null` for a column name absent from the headers, a missing cell, or a
    /// null cell.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Decode }` when a cell exists but cannot convert to `T`.
    /// - `Error { kind: Header }` when the name is duplicated in the header, or
    ///   when name-based access is used with no headers at all — use `get_at`
    ///   for positional access.
    /// - `Error { kind: Options }` when `T` is not cell-decodable.
    function get<T>(self, column: string) -> T? throws Error {
        _cell_by_name<T>(self, column)
    }

    /// Converts the cell at `index` to `T`, positionally.
    ///
    /// Works with or without headers. An out-of-range or negative `index`
    /// reads as a missing cell rather than an error.
    ///
    /// # Returns
    ///
    /// `null` for a missing or null cell.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Decode }` on conversion failure.
    /// - `Error { kind: Options }` when `T` is not cell-decodable.
    function get_at<T>(self, index: int) -> T? throws Error {
        _cell_by_index<T>(self, index)
    }

    /// All cells as raw text, verbatim.
    ///
    /// Quoting has already been undone, and `trim` applied to the unquoted
    /// cells it governs — quoting a cell preserves its whitespace.
    /// `null_values` is *not* applied, so a null cell still reads as whatever
    /// text spelled it. Padding and truncation under `ReaderOptions.ragged`
    /// are reflected here, so the length matches `length()`.
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
    /// routing heterogeneous rows.
    ///
    /// Always throws on failure, whatever the reader's `on_error` policy says.
    ///
    /// `T` must be a flat class. Each field is matched to a column by its
    /// declared name (no `@alias`), or positionally by declaration order when
    /// the reader has no headers. An optional field tolerates an absent
    /// column, a missing cell, and a null cell; a required one tolerates none
    /// of the three.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Decode }` when a cell will not convert, or when a
    ///   required field's cell is missing or null.
    /// - `Error { kind: Header }` when a required field has no matching
    ///   column, or its column name is duplicated.
    /// - `Error { kind: Options }` when `T` is not a class, or a field's type
    ///   is not cell-decodable.
    function decode<T>(self) -> T throws Error {
        _decode_record<T>(self)
    }

    /// The record as a header-name → cell-text map.
    ///
    /// Keyed in header order, with cell text verbatim as in `fields()`. A
    /// record shorter than the header contributes `""` for the missing
    /// columns, and cells past the end of the header are dropped.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Header }` when headers are unavailable or duplicated
    ///   — a duplicate name would silently lose a column.
    //baml:mut_vm
    function to_map(self) -> map<string, string> throws Error {
        $rust_function
    }
}

/// A streaming CSV reader. Create one with `baml.csv.open` or
/// `baml.csv.reader`.
///
/// A `Reader` is a lazy iterator over `Record` values, so a multi-gigabyte
/// file parses in constant memory and every `baml.iter` adapter and default
/// method (`map`, `filter`, `collect`, ...) works on it. There is one cursor:
/// raw iteration, `rows<T>()`, and `headers()` all draw from the same stream.
///
/// A thrown record error has already consumed the offending record. The parser
/// resynchronizes at the next record boundary, so the next call to `next()`
/// continues and one corrupt cell costs exactly one record.
///
/// `on_error` governs record-scoped failures for every read on this handle.
/// Under the default `"throw"` they propagate. Under `"skip"` they are
/// counted, handed to `ReaderOptions.on_skip`, and stepped over. `Options` and
/// `Header` failures are never record-scoped and throw either way — a header
/// that cannot serve `T` will not start serving it on the next row.
///
/// Every read throws `Error { kind: Closed }` after `close()`; `skipped()`,
/// `skipped_count()`, and `position()` stay valid. A `baml.errors.Io` failure
/// from the source is stream-scoped and final: the reader is marked exhausted
/// before the throw, so a caller that swallows it gets `baml.iter.Done` rather
/// than an infinite retry.
class Reader {
    _handle: $rust_type,
    _file: root.fs.File?,
    _on_skip: ((Error) -> null throws never)?,
    _owns_file: bool,

    implements root.iter.Iterable {
        type Item = root.csv.Record
        type Error = root.csv.Error | root.errors.Io

        /// Returns `self`: a `Reader` is its own iterator, so a reader can be
        /// consumed exactly once. There is only one cursor to hand out, and a
        /// second traversal would resume mid-file rather than restart.
        function iter(
            self,
        ) -> root.iter.Iterator<Item = Record, Error = root.csv.Error | root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = root.csv.Record
        type Error = root.csv.Error | root.errors.Io

        /// Yields the next raw `Record`, pumping input from the source as the
        /// parser needs it.
        ///
        /// A skipped record is not a result, so the loop runs until one
        /// survives or the stream ends. Under `"throw"` the error propagates
        /// instead — but the record has already been consumed, so calling
        /// `next()` again resumes cleanly at the following record.
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
    /// Idempotent. The header is resolved once and cached, so calling this
    /// before iterating costs nothing and consumes no data record. An empty
    /// input has no header row, and reads as the `headers` option or `null`.
    ///
    /// A malformed header record surfaces from whichever call reads the header
    /// first, and it exhausts the reader rather than resynchronizing: without
    /// a trustworthy header every later name-to-column mapping would be
    /// silently wrong. `on_error` does not apply, because there is no next
    /// header to resume at.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Quote }` or `Error { kind: Encoding }` if the header
    ///   record itself is malformed. Every later call rethrows the same error.
    /// - `baml.errors.Io` if the source cannot be read.
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
    /// Validates eagerly. It reads the header if necessary and checks that
    /// every non-optional field of `T` is satisfiable, so an unusable `T`
    /// throws `Error { kind: Header }` at the call site rather than on the
    /// millionth row.
    ///
    /// The decode rules are `Record.decode<T>`'s, under this reader's
    /// `on_error` policy. "Remaining" is literal: this shares the reader's one
    /// cursor, so records already pulled by `next()` are gone.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Header }` if a required field of `T` has no column, or
    ///   its column name is duplicated.
    /// - `Error { kind: Options }` if `T` is not a flat class of
    ///   cell-decodable fields.
    /// - Anything `headers()` throws, since it is called first.
    function rows<T>(
        self,
    ) -> root.iter.Iterator<Item = T, Error = Error | root.errors.Io> throws Error | root.errors.Io {
        self.headers();
        _validate_columns<T>(self);
        Rows<T> { reader: self }
    }

    /// The first `max_skipped` (default 1,000) `Error` values skipped under
    /// `on_error = "skip"`.
    ///
    /// Retention is bounded so a pathological file cannot turn a
    /// constant-memory stream into an unbounded error log. It keeps the first
    /// errors rather than the last, because the first ones usually explain the
    /// rest. Use `ReaderOptions.on_skip` to observe every skip, and
    /// `skipped_count()` for the true total.
    //baml:mut_vm
    function skipped(self) -> Error[] throws never {
        $rust_function
    }

    /// Exact count of records skipped under `on_error = "skip"`.
    ///
    /// Unaffected by `max_skipped` — the counter is kept even once retention
    /// stops.
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

    /// Releases the reader. Idempotent.
    ///
    /// The underlying file handle is closed only when the reader owns it
    /// (created by `open`); a handle passed to `reader(file)` stays open,
    /// since its lifecycle belongs to the caller. Closing is optional, as with
    /// `baml.fs.File` — the GC reclaims unreferenced readers.
    ///
    /// # Throws
    ///
    /// - `baml.errors.Io` if closing an owned file handle fails. The reader is
    ///   already marked closed by then, so a retry will not reopen it.
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
    /// in-memory sources.
    ///
    /// The reader is marked exhausted before an IO error is thrown, so
    /// subsequent `next()` calls return `baml.iter.Done`. A closed handle
    /// arrives as an IO error, because `baml.io.Read` has no separate
    /// closed-handle error channel.
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
///
/// A view, not a copy: it shares the reader's single cursor, so mixing it with
/// `Reader.next()` interleaves rather than duplicating, and dropping it leaves
/// the reader wherever it stopped. `Reader.rows<T>()` has already validated
/// the columns by the time you hold one.
class Rows<T> {
    reader: Reader,

    implements root.iter.Iterable {
        type Item = T
        type Error = root.csv.Error | root.errors.Io

        /// Returns `self`, for the same one-cursor reason `Reader` is its own
        /// iterator.
        function iter(
            self,
        ) -> root.iter.Iterator<Item = T, Error = root.csv.Error | root.errors.Io> throws never {
            self
        }
    }

    implements root.iter.Iterator {
        type Item = T
        type Error = root.csv.Error | root.errors.Io

        /// Yields the next record decoded into `T`.
        ///
        /// Two skip layers stack here under `on_error = "skip"`: the reader
        /// drops records that will not *parse*, and this drops records that
        /// parse but will not *decode*. Both honor `ReaderOptions.on_skip`, and
        /// both count toward `skipped_count()`.
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
///
/// Every write is atomic in its own output: a record is encoded in full before
/// any of it reaches the file, and writer state (record count, header flag,
/// pending BOM) only advances once that succeeds. A rejected row therefore
/// leaves nothing half-written and no phantom header.
///
/// The cell-encodable types are `string`, `int`, `bigint`, finite `float`,
/// `bool`, `null`, any enum (written as its variant name),
/// `baml.time.Instant`, `baml.time.PlainDate`, and `baml.time.PlainDateTime`.
/// A nested class or list is not a cell — serialize it yourself with
/// `baml.json.to_string` and write the string.
///
/// Everything that emits output shares three failure modes. A cell that would
/// need quoting under `quote_style: "never"`, or a non-finite float, throws
/// `Error { kind: Encode }`. A backing file that cannot be written throws
/// `baml.errors.Io`. A write after `close()` throws `Error { kind: Closed }`,
/// while `records_written()` stays valid.
class Writer {
    _handle: $rust_type,
    _file: root.fs.File?,
    _owns_file: bool,

    /// Writes one raw record. Never triggers an automatic header.
    ///
    /// The escape hatch from typed writing: cell count and content are
    /// whatever you pass, unchecked against any header already written.
    function write_record(self, record: Value[]) -> null throws Error | root.errors.Io {
        self._emit(self._encode_record(record))
    }

    /// Writes one typed row, fields in declaration order.
    ///
    /// If `write_header` is enabled (the default) and no header has been
    /// written yet, the header derived from `T`'s field names — or from
    /// `WriterOptions.headers` — is emitted first. Field names go out raw, as
    /// `@alias` is not consulted; the same BEP-038 rule `baml.json` follows.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Options }` if `T` is not a flat class, or a field's
    ///   value is not cell-encodable.
    function write_row<T>(self, row: T) -> null throws Error | root.errors.Io {
        self._emit(_encode_row<T>(self, row))
    }

    /// Writes many typed rows as one atomic batch.
    ///
    /// On a thrown error nothing from the batch is emitted or counted;
    /// otherwise this is `write_row<T>` in a loop, and throws exactly what it
    /// throws. All-or-nothing is the point — a partial batch would leave a
    /// file that looks complete.
    function write_rows<T>(self, rows: T[]) -> null throws Error | root.errors.Io {
        self._emit(_encode_rows<T>(self, rows))
    }

    /// Writes an explicit header record.
    ///
    /// Use it to name columns that do not match `T`'s fields, or to give
    /// `write_record` output a header, which it never emits on its own.
    /// Writing one satisfies `write_header: true`, so a later `write_row<T>`
    /// will not add a second.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Header }` after data has been written — a header is
    ///   only a header on the first line.
    function write_header(self, names: string[]) -> null throws Error | root.errors.Io {
        self._emit(self._encode_header(names))
    }

    /// Number of data records written so far, headers excluded.
    //baml:vm
    function records_written(self) -> int throws never {
        $rust_function
    }

    /// The accumulated CSV text. Valid only on `buffer()` writers.
    ///
    /// Non-consuming: the buffer keeps everything, so calling it twice returns
    /// the same text, growing as more rows are written.
    ///
    /// # Throws
    ///
    /// - `Error { kind: Options }` on a file-backed writer, whose output has
    ///   already gone to the file and is not retained.
    //baml:mut_vm
    function text(self) -> string throws Error {
        $rust_function
    }

    /// Flushes pending output.
    ///
    /// Writes through `baml.fs` flush eagerly, so this is an infallible no-op
    /// kept for symmetry with other writer APIs.
    function flush(self) -> null {
        null
    }

    /// Flushes and releases the writer. Idempotent.
    ///
    /// The underlying file handle is closed only when the writer owns it
    /// (created by `create`); a handle passed to `writer(file)` stays open,
    /// since its lifecycle belongs to the caller.
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

/// A streaming reader over in-memory text or bytes, or an open file handle.
///
/// A `string` or `uint8array` source is CSV content, parsed in place. A
/// `baml.fs.File` source is streamed from its current cursor to EOF, and the
/// reader owns that cursor from construction until it is exhausted or closed.
///
/// The handle is not adopted: `close()` on the reader leaves the file open,
/// because you opened it and something else may still want it. Use `open` when
/// you want the reader to own the file instead.
///
/// # Throws
///
/// - `Error { kind: Options }` if `options` is invalid. Nothing is parsed
///   here — the first record is read lazily, on the first `next()`, `rows`,
///   or `headers`.
function reader(
    source: string | uint8array | root.fs.File,
    options: ReaderOptions? = null,
) -> Reader throws Error {
    _reader(source, options, false)
}

/// Opens the file at `path` through `baml.fs` and streams it.
///
/// The reader owns the resulting handle, so `close()` closes the file.
///
/// # Throws
///
/// - `Error { kind: Options }` if `options` is invalid.
/// - `baml.errors.Io` if `path` cannot be opened for reading.
function open(
    path: string,
    options: ReaderOptions? = null,
) -> Reader throws Error | root.errors.Io {
    let f = baml.fs.open(path, "r") catch (_) {
        root.errors.InvalidArgument { message } => throw root.errors.Io { message: message },
    };
    _reader(f, options, true)
}

/// Eager typed read of a whole file, equivalent to
/// `open(path).rows<T>().collect()`.
///
/// If the file might not fit in memory, `open` it and stream instead. The
/// options are identical; the streaming spelling is a verb swap away.
///
/// # Throws
///
/// - `Error` for anything `open` and `Reader.rows<T>` throw, and for any
///   `Decode` failure not suppressed by `on_error = "skip"`.
/// - `baml.errors.Io` if the file cannot be opened or read.
function read<T>(
    path: string,
    options: ReaderOptions? = null,
) -> T[] throws Error | root.errors.Io {
    let r = open(path, options = options);
    let result = r.rows<T>().collect();
    r.close();
    result
}

/// Eager untyped parse of in-memory input: every record as a `string[]`.
///
/// The array-of-arrays shape Node's `csv-parse` returns by default. Cells come
/// back as raw text, exactly as `Record.fields` gives them — `null_values` is
/// not applied, since there is no `null` in a `string[][]` to map onto. The
/// header row is consumed and not returned unless `has_header: false`.
///
/// # Throws
///
/// - `Error` for a malformed record, unless `on_error = "skip"`. The reader is
///   not returned, so its errors are unreachable afterwards; pass `on_skip` if
///   you need to see them.
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
///
/// The in-memory counterpart of `read<T>`, with `Record.decode<T>`'s rules.
/// The same one-shot caveat as `parse` applies: the reader is not returned, so
/// `skipped()` is unreachable and `on_skip` is the only way to observe skipped
/// rows.
///
/// # Throws
///
/// - `Error` for anything `reader` and `Reader.rows<T>` throw, and for any
///   `Decode` failure not suppressed by `on_error = "skip"`.
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

/// Typed parse of an input with zero or one data record.
///
/// The tolerant sibling of `decode_one<T>`: an empty input is `null` rather
/// than an error. It still reads the whole input, since finding a second
/// record is the only way to know there is not one.
///
/// # Throws
///
/// - `Error { kind: TooManyRows }` if the input holds more than one data
///   record.
/// - `Error` for anything `decode<T>` throws.
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

/// Typed parse of an input with exactly one data record.
///
/// Fits LLM output that was requested as exactly one row: `has_header: false`
/// for a bare row (positional decode), or the default options when the model
/// was asked to emit a header plus one data row.
///
/// # Throws
///
/// - `Error { kind: NotFound }` if the input holds no data record.
/// - `Error { kind: TooManyRows }` if it holds more than one.
/// - `Error` for anything `decode<T>` throws.
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

/// A streaming writer to an open file handle.
///
/// Emits from the file's current cursor and never truncates; use `create` for
/// a fresh output file. The handle is not adopted, so `close()` on the writer
/// leaves the file open. Appending to an existing CSV usually also wants
/// `write_header: false`, since the file already has one.
///
/// # Throws
///
/// - `Error { kind: Options }` if `options` is invalid.
function writer(file: root.fs.File, options: WriterOptions? = null) -> Writer throws Error {
    _writer(file, options, false)
}

/// Creates or truncates the file at `path` and stream-writes to it.
///
/// Parent directories are created as needed, as `baml.fs.write` does. The
/// writer owns the resulting handle, so `close()` closes the file.
///
/// # Throws
///
/// - `Error { kind: Options }` if `options` is invalid.
/// - `baml.errors.Io` if `path` cannot be opened for writing.
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
///
/// The only writer whose output stays reachable — file-backed writers hand
/// their bytes to `baml.fs` and keep nothing. Use it to build CSV
/// incrementally when `stringify<T>` cannot see all the rows at once.
///
/// # Throws
///
/// - `Error { kind: Options }` if `options` is invalid.
function buffer(options: WriterOptions? = null) -> Writer throws Error {
    _buffer(options = options)
}

/// One-shot typed file write, creating or truncating like `baml.fs.write`.
///
/// The rows go out as one atomic batch, so a value that will not encode leaves
/// the (already truncated) file empty rather than half written.
///
/// # Returns
///
/// Total bytes encoded, header and BOM included.
///
/// # Throws
///
/// - `Error` for anything `create` and `Writer.write_rows<T>` throw.
/// - `baml.errors.Io` if the file cannot be opened, written, or closed.
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
///
/// A header comes first, unless `WriterOptions.write_header` is `false`. The
/// names come off the first row's class, or off `WriterOptions.headers`, so an
/// empty `rows` produces `""` with no header — with no row there is nothing to
/// name the columns after.
///
/// # Throws
///
/// - `Error` for anything `buffer` and `Writer.write_rows<T>` throw.
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
///
/// No header is ever emitted: raw records carry no field names to derive one
/// from. Write the header row yourself as the first entry of `records` if you
/// want one. Rows are written one at a time, so a failure part-way leaves the
/// earlier rows encoded but discards the text along with the error.
///
/// # Throws
///
/// - `Error` for anything `buffer` and `Writer.write_record` throw.
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
/// become spaces; non-finite floats render as `NaN` / `inf`, since prompt text
/// is not meant to round-trip. Past `max_rows`, output is truncated with a
/// final `… (N more rows)` line whose count covers all of `rows`, so the model
/// is told what it is not seeing.
///
/// Never fails, where the CSV writers are strict, and for the same reason: a
/// prompt that renders "<unrepresentable>" in one cell is more useful than no
/// prompt. Where CSV would reject a nested value, this falls back to its JSON
/// form. A `T` that is not a class renders as a single `value` column.
function to_markdown<T>(rows: T[], max_rows: int = 100) -> string throws never {
    _to_markdown<T>(rows, max_rows = max_rows)
}

/// Renders untyped records as a GitHub-style Markdown table.
///
/// The `parse`-shaped counterpart of `to_markdown<T>`. Without `headers` the
/// columns are named `col1`, `col2`, ... out to the width of the widest
/// rendered record; supply the header row explicitly to do better. Escaping
/// and `… (N more rows)` truncation are as in `to_markdown<T>`.
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

/// (internal) Construct an `Error` with no positional fields — for the
/// input-scoped kinds (`NotFound`, `TooManyRows`) that no single record
/// caused.
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
