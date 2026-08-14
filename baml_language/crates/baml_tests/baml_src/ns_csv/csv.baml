// Unified tests for the `baml.csv` standard library (BEP-060).
//
// Logic lives in top-level functions (test-block locals are boxed by the
// closure capture — see the ns_closures/closures.baml header) and `test`
// blocks assert on the returned scalars.
//
// Error tests use the `{ <body>; baml.sys.panic("…") } catch (e) { <Class> =>
// … }` pattern (cf. ns_time/time.baml), with `catch` attached to a whole
// block at a `let` binding or in function-trailing position.
//
// HERMETICITY: file-IO tests use unique paths under /tmp (prefixed with
// baml_csv_<testname>) and remove the files they create before returning.

// ─── Helpers ────────────────────────────────────────────────────────────────

function kind_name(k: baml.csv.CsvErrorKind) -> string {
    match (k) {
        baml.csv.CsvErrorKind.Options => "Options",
        baml.csv.CsvErrorKind.Quote => "Quote",
        baml.csv.CsvErrorKind.FieldCount => "FieldCount",
        baml.csv.CsvErrorKind.Encoding => "Encoding",
        baml.csv.CsvErrorKind.Header => "Header",
        baml.csv.CsvErrorKind.Decode => "Decode",
        baml.csv.CsvErrorKind.Encode => "Encode",
        baml.csv.CsvErrorKind.NotFound => "NotFound",
        baml.csv.CsvErrorKind.TooManyRows => "TooManyRows",
        baml.csv.CsvErrorKind.Closed => "Closed",
    }
}

function join(parts: string[], sep: string) -> string {
    let out = "";
    let first = true;
    for (let p in parts) {
        if (first) {
            out = p;
            first = false;
        } else {
            out = out + sep + p;
        }
    }
    out
}

/// Render `string[][]` as "a|b;c|d" for compact assertions.
function grid(rows: string[][]) -> string {
    let out: string[] = [];
    for (let r in rows) {
        out.push(join(r, "|"));
    }
    join(out, ";")
}

function int_or(v: int?, fallback: string) -> string {
    match (v) {
        null => fallback,
        let i: int => baml.json.stringify(i),
    }
}

// ─── Dialect: line endings, quoting, trailing newline ──────────────────────

function csv_parse_line_endings() -> string {
    let opts = baml.csv.ReaderOptions { has_header: false };
    let crlf = grid(baml.csv.parse("a,b\r\n1,2\r\n", options = opts));
    let bare_cr = grid(baml.csv.parse("a,b\r1,2", options = opts));
    let lf = grid(baml.csv.parse("a,b\n1,2", options = opts));
    crlf + " / " + bare_cr + " / " + lf
}

test "csv_parse_line_endings" {
    assert.equal(csv_parse_line_endings(), "a|b;1|2 / a|b;1|2 / a|b;1|2")
}

function csv_parse_quotes() -> string {
    // Embedded delimiter, raw newline, and `""` doubling inside quotes.
    let rows = baml.csv.parse("\"x,y\",\"l1\nl2\",\"say \"\"hi\"\"\"", options = baml.csv.ReaderOptions { has_header: false });
    grid(rows)
}

test "csv_parse_quotes" {
    assert.equal(csv_parse_quotes(), "x,y|l1\nl2|say \"hi\"")
}

function csv_parse_no_phantom_record() -> int {
    // Trailing newline is optional and never produces a phantom record.
    let with_nl = baml.csv.parse("a\n1\n").length();
    let without_nl = baml.csv.parse("a\n1").length();
    with_nl * 10 + without_nl
}

test "csv_parse_no_phantom_record" {
    assert.equal(csv_parse_no_phantom_record(), 11)
}

function csv_quoting_disabled() -> string {
    grid(baml.csv.parse("\"a\",b\n", options = baml.csv.ReaderOptions { has_header: false, quoting: false }))
}

test "csv_quoting_disabled" {
    assert.equal(csv_quoting_disabled(), "\"a\"|b")
}

function csv_escape_dialect() -> string {
    // escape: "\\" switches from `""` doubling to backslash escapes.
    grid(baml.csv.parse("\"x\\\"y\",2\n", options = baml.csv.ReaderOptions { has_header: false, escape: "\\" }))
}

test "csv_escape_dialect" {
    assert.equal(csv_escape_dialect(), "x\"y|2")
}

function csv_tsv() -> string {
    grid(baml.csv.parse("a\tb\n1\t2\n", options = baml.csv.ReaderOptions { has_header: false, delimiter: "\t" }))
}

test "csv_tsv" {
    assert.equal(csv_tsv(), "a|b;1|2")
}

// ─── External parser corpus: PapaParse and csv-spectrum ────────────────────
//
// These cases are adapted from:
// - PapaParse `tests/test-cases.js`
//   https://github.com/mholt/PapaParse/blob/master/tests/test-cases.js
// - csv-spectrum fixtures
//   https://github.com/max-mapper/csv-spectrum
//
// Keep this section focused on portable CSV edge cases that match BAML's
// documented behavior. PapaParse-specific APIs and intentional divergences
// (delimiter guessing, dynamic typing, duplicate-header renaming, browser
// callbacks, etc.) belong outside this conformance block.

function csv_papaparse_quote_boundaries() -> string {
    let opts = baml.csv.ReaderOptions { has_header: false };

    // PapaParse: "Quoted field with escaped quotes at boundaries".
    let boundaries = grid(baml.csv.parse("A,\"\"\"B\"\"\",C", options = opts));

    // PapaParse: "Quoted field with quotes around delimiter".
    let around_delimiter = grid(baml.csv.parse("A,\"\"\",\"\"\",C", options = opts));

    // PapaParse: "Quoted field with 5 quotes in a row and a delimiter in there, too".
    let dense_quotes = grid(baml.csv.parse("\"1\",\"cnonce=\"\"\"\",nc=\"\"\"\"\",\"2\"", options = opts));

    boundaries + " / " + around_delimiter + " / " + dense_quotes
}

test "csv_papaparse_quote_boundaries" {
    assert.equal(
        csv_papaparse_quote_boundaries(),
        "A|\"B\"|C / A|\",\"|C / 1|cnonce=\"\",nc=\"\"|2",
    )
}

function csv_papaparse_empty_field_edges() -> string {
    let opts = baml.csv.ReaderOptions { has_header: false };

    // PapaParse: "Input is just the delimiter (2 empty fields)".
    let delimiter_only = grid(baml.csv.parse(",", options = opts));

    // PapaParse: "Multiple consecutive empty fields".
    let consecutive = grid(baml.csv.parse("a,b,,,c,d\n,,e,,,f", options = opts));

    // PapaParse: "Line starts with unquoted empty field".
    let starts_empty = grid(baml.csv.parse(",b,c\n\"d\",e,f", options = opts));

    // PapaParse: "Empty quoted field at EOF is empty".
    let quoted_empty_eof = grid(baml.csv.parse("a,b,\"\"\na,b,\"\"", options = opts));

    delimiter_only + " / " + consecutive + " / " + starts_empty + " / " + quoted_empty_eof
}

test "csv_papaparse_empty_field_edges" {
    assert.equal(
        csv_papaparse_empty_field_edges(),
        "| / a|b|||c|d;||e|||f / |b|c;d|e|f / a|b|;a|b|",
    )
}

function csv_papaparse_comment_runs() -> string {
    let opts = baml.csv.ReaderOptions { has_header: false, comment: "#" };

    // PapaParse: consecutive comments at beginning/middle/end and comment-only input.
    let at_beginning = grid(baml.csv.parse("#comment1\n#comment2\n#comment3\na,b,c", options = opts));
    let in_middle = grid(baml.csv.parse("a,b,c\n#comment1\n#comment2\nd,e,f", options = opts));
    let at_end = grid(baml.csv.parse("a,b,c\n#comment1\n#comment2", options = opts));
    let only_comments = baml.csv.parse("#comment1\n#comment2\n#comment3", options = opts).length();

    at_beginning + " / " + in_middle + " / " + at_end + " / only=" + baml.json.stringify(only_comments)
}

test "csv_papaparse_comment_runs" {
    assert.equal(csv_papaparse_comment_runs(), "a|b|c / a|b|c;d|e|f / a|b|c / only=0")
}

function csv_papaparse_line_endings_inside_quotes() -> string {
    let opts = baml.csv.ReaderOptions { has_header: false };

    // PapaParse: quoted fields with CRLF, CR, and LF inside the cell.
    let crlf = grid(baml.csv.parse("A,\"B\r\nB\",C", options = opts));
    let cr = grid(baml.csv.parse("A,\"B\rB\",C", options = opts));
    let lf = grid(baml.csv.parse("A,\"B\nB\",C", options = opts));

    crlf + " / " + cr + " / " + lf
}

test "csv_papaparse_line_endings_inside_quotes" {
    assert.equal(csv_papaparse_line_endings_inside_quotes(), "A|B\r\nB|C / A|B\rB|C / A|B\nB|C")
}

class SpectrumAB {
    a string
    b string
}

class SpectrumAddress {
    first string
    last string
    address string
    city string
    zip string
}

function csv_spectrum_core_fixtures() -> string {
    let comma = baml.csv.decode<SpectrumAddress>("first,last,address,city,zip\nJohn,Doe,120 any st.,\"Anytown, WW\",08123");
    let escaped = baml.csv.decode<SpectrumAB>("a,b\n1,\"ha \"\"ha\"\" ha\"\n3,4");
    let quotes_newlines = baml.csv.decode<SpectrumAB>("a,b\n1,\"ha \n\"\"ha\"\" \nha\"\n3,4");

    let city = match (comma.at(0)) {
        null => "missing",
        let row: SpectrumAddress => row.city + ":" + row.zip,
    };
    let escaped_b = match (escaped.at(0)) {
        null => "missing",
        let row: SpectrumAB => row.b,
    };
    let newline_b = match (quotes_newlines.at(0)) {
        null => "missing",
        let row: SpectrumAB => row.b,
    };

    city
        + " / " + baml.json.stringify(escaped.length()) + ":" + escaped_b
        + " / " + baml.json.stringify(quotes_newlines.length()) + ":" + newline_b
}

test "csv_spectrum_core_fixtures" {
    assert.equal(
        csv_spectrum_core_fixtures(),
        "Anytown, WW:08123 / 2:ha \"ha\" ha / 2:ha \n\"ha\" \nha",
    )
}

// ─── Dialect: BOM, comments, skip_lines, blank records, trim ───────────────

function csv_bom_stripped_by_default() -> string {
    // A real UTF-8 BOM (EF BB BF) via a byte-string source.
    let rows = baml.csv.parse(b"\xEF\xBB\xBFid\n7\n");
    let r = baml.csv.reader(b"\xEF\xBB\xBFid\n7\n");
    // `headers()` is `string[]?`; the empty fallback needs a `string[]` type so
    // the coalesced value is `string[]` (a bare `[]` is otherwise `_[]`).
    let h: string[] = r.headers() ?? [];
    grid(rows) + "/" + join(h, "|")
}

test "csv_bom_stripped_by_default" {
    assert.equal(csv_bom_stripped_by_default(), "7/id")
}

function csv_comment_lines() -> string {
    let with_comment = grid(baml.csv.parse("# note\n1,2\n# more\n3,4\n", options = baml.csv.ReaderOptions { has_header: false, comment: "#" }));
    // Off by default: `#` is data.
    let without = grid(baml.csv.parse("# note\n", options = baml.csv.ReaderOptions { has_header: false }));
    with_comment + " / " + without
}

test "csv_comment_lines" {
    assert.equal(csv_comment_lines(), "1|2;3|4 / # note")
}

function csv_skip_lines() -> string {
    grid(baml.csv.parse("junk preamble\n```csv\na,b\n1,2\n", options = baml.csv.ReaderOptions { skip_lines: 2 }))
}

test "csv_skip_lines" {
    assert.equal(csv_skip_lines(), "1|2")
}

function csv_blank_records() -> string {
    let skipped = baml.csv.parse("a\n\n1\n\n2\n", options = baml.csv.ReaderOptions { has_header: false }).length();
    let kept = baml.csv.parse("a\n\n1\n", options = baml.csv.ReaderOptions { has_header: false, skip_blank_records: false }).length();
    baml.json.stringify(skipped) + ":" + baml.json.stringify(kept)
}

test "csv_blank_records" {
    assert.equal(csv_blank_records(), "3:3")
}

function csv_trim_all() -> string {
    let r = baml.csv.reader("  name , score \n  alice ,  9 \n", options = baml.csv.ReaderOptions { trim: "all" });
    let headers: string[] = r.headers() ?? [];
    let h = join(headers, "|");
    let row = match (r.next()) {
        baml.iter.Done => "none",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    h + "/" + row
}

test "csv_trim_all" {
    assert.equal(csv_trim_all(), "name|score/alice|9")
}

function csv_trim_preserves_quoted() -> string {
    grid(baml.csv.parse(" \" a \" ,b\n", options = baml.csv.ReaderOptions { has_header: false, trim: "fields" }))
}

test "csv_trim_preserves_quoted" {
    assert.equal(csv_trim_preserves_quoted(), " a |b")
}

// ─── Encoding ───────────────────────────────────────────────────────────────

function csv_encoding_strict_throws() -> string {
    {
        baml.csv.parse(b"a\n\xFF\n", options = baml.csv.ReaderOptions { has_header: false });
        baml.sys.panic("expected Encoding error")
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_encoding_strict_throws" {
    assert.equal(csv_encoding_strict_throws(), "Encoding")
}

function csv_encoding_lossy() -> int {
    let rows = baml.csv.parse(b"a,b\n\xFF,2\n", options = baml.csv.ReaderOptions { encoding: "utf8-lossy" });
    // The invalid byte became U+FFFD (one character).
    rows[0][0].length()
}

test "csv_encoding_lossy" {
    assert.equal(csv_encoding_lossy(), 1)
}

// ─── Null semantics ─────────────────────────────────────────────────────────

class NullProbe {
    a string
    s string
    so string?
    io int?
}

function csv_null_semantics_empty_unquoted() -> string {
    let rows = baml.csv.decode<NullProbe>("a,s,so,io\nx,,,\n");
    match (rows.at(0)) {
        null => "missing",
        let p: NullProbe => "s=" + p.s + " so=" + (p.so ?? "<null>") + " io=" + int_or(p.io, "<null>"),
    }
}

test "csv_null_semantics_empty_unquoted" {
    // empty unquoted: "" for string, null for optionals.
    assert.equal(csv_null_semantics_empty_unquoted(), "s= so=<null> io=<null>")
}

function csv_null_semantics_quoted_empty() -> string {
    // Quoted empty is always the empty string; for int? it is a Decode error.
    let ok = match (baml.csv.decode<NullProbe>("a,s,so,io\nx,\"\",\"\",1\n").at(0)) {
        null => "missing",
        let p: NullProbe => "so=" + (p.so ?? "<null>"),
    };
    let err = {
        baml.csv.decode<NullProbe>("a,s,so,io\nx,,,\"\"\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    ok + "/" + err
}

test "csv_null_semantics_quoted_empty" {
    assert.equal(csv_null_semantics_quoted_empty(), "so=/Decode")
}

function csv_null_values_option() -> string {
    let opts = baml.csv.ReaderOptions { null_values: ["N/A"] };
    let ok = match (baml.csv.decode<NullProbe>("a,s,so,io\nx,y,N/A,N/A\n", options = opts).at(0)) {
        null => "missing",
        let p: NullProbe => "so=" + (p.so ?? "<null>") + " io=" + int_or(p.io, "<null>"),
    };
    // Declaring "N/A" null and a field non-optional is a contradiction → Decode.
    let err = {
        baml.csv.decode<NullProbe>("a,s,so,io\nx,N/A,,\n", options = opts);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    // Quoted cells never match null_values.
    let quoted = match (baml.csv.decode<NullProbe>("a,s,so,io\nx,y,\"N/A\",\n", options = opts).at(0)) {
        null => "missing",
        let p: NullProbe => p.so ?? "<null>",
    };
    ok + "/" + err + "/" + quoted
}

test "csv_null_values_option" {
    assert.equal(csv_null_values_option(), "so=<null> io=<null>/Decode/N/A")
}

// ─── Typed decode ───────────────────────────────────────────────────────────

enum Verdict {
    Approve
    Reject
}

class FullRow {
    name string
    count int
    big bigint
    ratio float
    flag bool
    verdict Verdict
    at baml.time.Instant
}

function csv_decode_all_cell_types() -> string {
    let text = "name,count,big,ratio,flag,verdict,at\nx,+42,123456789012345678901234567890,2.5e-1,TRUE,Approve,2026-01-02T03:04:05Z\n";
    match (baml.csv.decode<FullRow>(text).at(0)) {
        null => "missing",
        let r: FullRow => {
            let parts: string[] = [];
            parts.push(r.name);
            parts.push(baml.json.stringify(r.count));
            parts.push(baml.json.stringify(r.big == 123456789012345678901234567890n));
            parts.push(baml.json.stringify(r.ratio));
            parts.push(baml.json.stringify(r.flag));
            parts.push(match (r.verdict) { Verdict.Approve => "Approve", Verdict.Reject => "Reject" });
            parts.push(r.at.to_string());
            join(parts, " ")
        },
    }
}

test "csv_decode_all_cell_types" {
    assert.equal(
        csv_decode_all_cell_types(),
        "x 42 true 0.25 true Approve 2026-01-02T03:04:05Z",
    )
}

class Pos {
    first string
    second int
}

function csv_decode_positional() -> string {
    // No header at all: fields decode positionally in declaration order.
    let rows = baml.csv.decode<Pos>("a,1\nb,2\n", options = baml.csv.ReaderOptions { has_header: false });
    match (rows.at(1)) {
        null => "missing",
        let p: Pos => p.first + baml.json.stringify(p.second),
    }
}

test "csv_decode_positional" {
    assert.equal(csv_decode_positional(), "b2")
}

function csv_decode_headers_override() -> string {
    // The renaming tool for ugly headers: file header consumed, names replaced.
    let renamed = baml.csv.decode<Pos>("Display Name,amount ($)\nx,7\n", options = baml.csv.ReaderOptions { headers: ["first", "second"] });
    let a = match (renamed.at(0)) {
        null => "missing",
        let p: Pos => p.first + baml.json.stringify(p.second),
    };
    // headers + has_header: false supplies names without consuming a row;
    // the override names columns positionally, so column 0 is `second`.
    let supplied = baml.csv.decode<Pos>("9,y\n", options = baml.csv.ReaderOptions { has_header: false, headers: ["second", "first"] });
    let b = match (supplied.at(0)) {
        null => "missing",
        let p: Pos => p.first + baml.json.stringify(p.second),
    };
    a + "/" + b
}

test "csv_decode_headers_override" {
    assert.equal(csv_decode_headers_override(), "x7/y9")
}

function csv_decode_missing_column_is_header_error() -> string {
    {
        baml.csv.decode<Pos>("first,other\nx,y\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind) + ":" + (e.column ?? "?"),
    }
}

test "csv_decode_missing_column_is_header_error" {
    assert.equal(csv_decode_missing_column_is_header_error(), "Header:second")
}

class OnlyFirst {
    first string
}

function csv_decode_extra_columns_ignored() -> string {
    match (baml.csv.decode<OnlyFirst>("first,extra1,extra2\nx,y,z\n").at(0)) {
        null => "missing",
        let p: OnlyFirst => p.first,
    }
}

test "csv_decode_extra_columns_ignored" {
    assert.equal(csv_decode_extra_columns_ignored(), "x")
}

function csv_duplicate_column_matched_is_header_error() -> string {
    let err = {
        baml.csv.decode<OnlyFirst>("first,first\nx,y\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    // Positional access is unaffected by duplicate names.
    let r = baml.csv.reader("first,first\nx,y\n");
    let positional = match (r.next()) {
        baml.iter.Done => "none",
        let rec: baml.csv.CsvRecord => rec.get_at<string>(1) ?? "<null>",
    };
    err + "/" + positional
}

test "csv_duplicate_column_matched_is_header_error" {
    assert.equal(csv_duplicate_column_matched_is_header_error(), "Header/y")
}

class IntRow {
    n int
}

function csv_decode_error_details() -> string {
    {
        baml.csv.decode<IntRow>("n\n1\noops\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind)
            + " line=" + int_or(e.line, "?")
            + " record=" + int_or(e.record, "?")
            + " column=" + (e.column ?? "?"),
    }
}

test "csv_decode_error_details" {
    assert.equal(csv_decode_error_details(), "Decode line=3 record=1 column=n")
}

function csv_decode_rejects_locale_numbers() -> string {
    let one = {
        baml.csv.decode<IntRow>("n\n\"1,000\"\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    let two = {
        baml.csv.decode<IntRow>("n\n 5\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    one + "/" + two
}

test "csv_decode_rejects_locale_numbers" {
    assert.equal(csv_decode_rejects_locale_numbers(), "Decode/Decode")
}

class FloatRow {
    x float
}

function csv_decode_rejects_nonfinite_floats() -> string {
    {
        baml.csv.decode<FloatRow>("x\nNaN\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_decode_rejects_nonfinite_floats" {
    assert.equal(csv_decode_rejects_nonfinite_floats(), "Decode")
}

// ─── Ragged records ─────────────────────────────────────────────────────────

function csv_ragged_strict() -> string {
    {
        baml.csv.parse("a,b\n1\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind)
            + " expected=" + int_or(e.expected, "?")
            + " found=" + int_or(e.found, "?"),
    }
}

test "csv_ragged_strict" {
    assert.equal(csv_ragged_strict(), "FieldCount expected=2 found=1")
}

function csv_ragged_pad() -> string {
    let opts = baml.csv.ReaderOptions { ragged: "pad" };
    let short = grid(baml.csv.parse("a,b\n1\n", options = opts));
    // Long records still throw under "pad".
    let long = {
        baml.csv.parse("a,b\n1,2,3\n", options = opts);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    short + "/" + long
}

test "csv_ragged_pad" {
    assert.equal(csv_ragged_pad(), "1|/FieldCount")
}

function csv_ragged_truncate() -> string {
    let opts = baml.csv.ReaderOptions { ragged: "truncate" };
    let long = grid(baml.csv.parse("a,b\n1,2,3\n", options = opts));
    let short = {
        baml.csv.parse("a,b\n1\n", options = opts);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    long + "/" + short
}

test "csv_ragged_truncate" {
    assert.equal(csv_ragged_truncate(), "1|2/FieldCount")
}

// ─── Per-record errors: resumability and skip mode ──────────────────────────

function csv_reader_resumes_after_throw() -> string {
    let r = baml.csv.reader("a,b\n1,2\n3\n4,5\n");
    let seen: string[] = [];
    let bad = 0;
    while (true) {
        let step = {
            match (r.next()) {
                baml.iter.Done => "done",
                let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
            }
        } catch (e) {
            baml.csv.CsvError => {
                bad += 1;
                "<" + kind_name(e.kind) + ">"
            },
        };
        if (step == "done") { break; }
        seen.push(step);
    }
    join(seen, ";") + " bad=" + baml.json.stringify(bad)
}

test "csv_reader_resumes_after_throw" {
    // One corrupt record costs exactly one record.
    assert.equal(csv_reader_resumes_after_throw(), "1|2;<FieldCount>;4|5 bad=1")
}

function csv_unterminated_quote_exhausts() -> string {
    let r = baml.csv.reader("a\n\"open\n1\n", options = baml.csv.ReaderOptions { has_header: false });
    let first = match (r.next()) {
        baml.iter.Done => "done",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    let second = {
        match (r.next()) {
            baml.iter.Done => "done",
            let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
        }
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    // An unterminated quote legally consumes the rest of the input.
    let third = match (r.next()) {
        baml.iter.Done => "done",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    first + "/" + second + "/" + third
}

test "csv_unterminated_quote_exhausts" {
    assert.equal(csv_unterminated_quote_exhausts(), "a/Quote/done")
}

function csv_skip_mode_diagnostics() -> string {
    let r = baml.csv.reader("a,b\n1,2\nbad\n3,4\nworse\n", options = baml.csv.ReaderOptions {
        on_error: "skip",
        max_skipped: 1,
    });
    let count = 0;
    for (let rec in r) {
        count += 1;
    }
    let first_line = match (r.skipped().at(0)) {
        null => "?",
        let e: baml.csv.CsvError => int_or(e.line, "?"),
    };
    baml.json.stringify(count)
        + " skipped=" + baml.json.stringify(r.skipped_count())
        + " retained=" + baml.json.stringify(r.skipped().length())
        + " first_line=" + first_line
}

test "csv_skip_mode_diagnostics" {
    // skipped_count stays exact while retention is capped by max_skipped.
    assert.equal(csv_skip_mode_diagnostics(), "2 skipped=2 retained=1 first_line=3")
}

function csv_on_skip_observer() -> string {
    let log: string[] = [];
    let r = baml.csv.reader("a,b\n1\n2,3\n", options = baml.csv.ReaderOptions {
        on_error: "skip",
        on_skip: (e: baml.csv.CsvError) -> null {
            log.push(kind_name(e.kind));
            null
        },
    });
    let count = 0;
    for (let rec in r) {
        count += 1;
    }
    baml.json.stringify(count) + ":" + join(log, ",")
}

test "csv_on_skip_observer" {
    assert.equal(csv_on_skip_observer(), "1:FieldCount")
}

function csv_skip_mode_typed_decode() -> string {
    // Undecodable cells are skippable record-scoped errors for rows<T>.
    let r = baml.csv.reader("n\n1\noops\n3\n", options = baml.csv.ReaderOptions { on_error: "skip" });
    let total = 0;
    for (let row in r.rows<IntRow>()) {
        total += row.n;
    }
    baml.json.stringify(total) + ":" + baml.json.stringify(r.skipped_count())
}

test "csv_skip_mode_typed_decode" {
    assert.equal(csv_skip_mode_typed_decode(), "4:1")
}

// ─── Cardinality one-shots ──────────────────────────────────────────────────

function csv_decode_optional_cases() -> string {
    let zero = match (baml.csv.decode_optional<IntRow>("n\n")) {
        null => "null",
        let r: IntRow => baml.json.stringify(r.n),
    };
    let one = match (baml.csv.decode_optional<IntRow>("n\n7\n")) {
        null => "null",
        let r: IntRow => baml.json.stringify(r.n),
    };
    let two = {
        baml.csv.decode_optional<IntRow>("n\n1\n2\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    zero + "/" + one + "/" + two
}

test "csv_decode_optional_cases" {
    assert.equal(csv_decode_optional_cases(), "null/7/TooManyRows")
}

function csv_decode_one_cases() -> string {
    let zero = {
        baml.csv.decode_one<IntRow>("n\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    let one = baml.json.stringify(baml.csv.decode_one<IntRow>("n\n7\n").n);
    // A bare row, positionally decoded: the LLM single-row case.
    let bare = baml.json.stringify(baml.csv.decode_one<IntRow>("9\n", options = baml.csv.ReaderOptions { has_header: false }).n);
    let two = {
        baml.csv.decode_one<IntRow>("n\n1\n2\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    zero + "/" + one + "/" + bare + "/" + two
}

test "csv_decode_one_cases" {
    assert.equal(csv_decode_one_cases(), "NotFound/7/9/TooManyRows")
}

// ─── CsvRecord API ──────────────────────────────────────────────────────────

function csv_record_api() -> string {
    let r = baml.csv.reader("name,score\nalice,9\n");
    match (r.next()) {
        baml.iter.Done => "none",
        let rec: baml.csv.CsvRecord => {
            let parts: string[] = [];
            parts.push(baml.json.stringify(rec.length()));
            parts.push(join(rec.fields(), "|"));
            parts.push(rec.get<string>("name") ?? "<null>");
            parts.push(int_or(rec.get<int>("score"), "<null>"));
            parts.push(rec.get<string>("absent") ?? "<absent-null>");
            parts.push(rec.get_at<string>(99) ?? "<oob-null>");
            let m = rec.to_map();
            parts.push(m["name"]);
            join(parts, " ")
        },
    }
}

test "csv_record_api" {
    assert.equal(csv_record_api(), "2 alice|9 alice 9 <absent-null> <oob-null> alice")
}

function csv_record_get_decode_error() -> string {
    let r = baml.csv.reader("n\noops\n");
    {
        match (r.next()) {
            baml.iter.Done => "none",
            let rec: baml.csv.CsvRecord => {
                rec.get<int>("n");
                "no-throw"
            },
        }
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind) + ":" + (e.column ?? "?"),
    }
}

test "csv_record_get_decode_error" {
    assert.equal(csv_record_get_decode_error(), "Decode:n")
}

function csv_record_get_without_headers() -> string {
    let r = baml.csv.reader("1,2\n", options = baml.csv.ReaderOptions { has_header: false });
    {
        match (r.next()) {
            baml.iter.Done => "none",
            let rec: baml.csv.CsvRecord => {
                rec.get<int>("n");
                "no-throw"
            },
        }
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_record_get_without_headers" {
    assert.equal(csv_record_get_without_headers(), "Header")
}

function csv_record_decode_single() -> string {
    // Per-record typed decode, useful for routing heterogeneous rows.
    let r = baml.csv.reader("n\n5\n");
    match (r.next()) {
        baml.iter.Done => "none",
        let rec: baml.csv.CsvRecord => baml.json.stringify(rec.decode<IntRow>().n),
    }
}

test "csv_record_decode_single" {
    assert.equal(csv_record_decode_single(), "5")
}

function csv_record_snapshot_outlives_reader() -> string {
    // Records have snapshot semantics: still valid after the reader advances.
    let r = baml.csv.reader("a\nfirst\nsecond\n");
    let kept: baml.csv.CsvRecord[] = [];
    for (let rec in r) {
        kept.push(rec);
    }
    let parts: string[] = [];
    for (let rec in kept) {
        parts.push(rec.fields()[0]);
    }
    join(parts, ",")
}

test "csv_record_snapshot_outlives_reader" {
    assert.equal(csv_record_snapshot_outlives_reader(), "first,second")
}

// ─── CsvReader API ──────────────────────────────────────────────────────────

function csv_headers_variants() -> string {
    let with_header = join(baml.csv.reader("a,b\n1,2\n").headers() ?? ["<null>"], "|");
    let no_header = match (baml.csv.reader("1,2\n", options = baml.csv.ReaderOptions { has_header: false }).headers()) {
        null => "<null>",
        let h: string[] => join(h, "|"),
    };
    let empty_input = match (baml.csv.reader("").headers()) {
        null => "<null>",
        let h: string[] => join(h, "|"),
    };
    with_header + " / " + no_header + " / " + empty_input
}

test "csv_headers_variants" {
    assert.equal(csv_headers_variants(), "a|b / <null> / <null>")
}

function csv_reader_limit() -> int {
    baml.csv.parse("a\n1\n2\n3\n", options = baml.csv.ReaderOptions { limit: 2 }).length()
}

test "csv_reader_limit" {
    assert.equal(csv_reader_limit(), 2)
}

function csv_reader_position() -> string {
    let r = baml.csv.reader("a,b\n1,2\n3,4\n");
    r.headers();
    let p1 = r.position();
    r.next();
    let p2 = r.position();
    "byte=" + baml.json.stringify(p1.byte) + " line=" + baml.json.stringify(p1.line) + " record=" + baml.json.stringify(p1.record)
        + " then byte=" + baml.json.stringify(p2.byte) + " line=" + baml.json.stringify(p2.line) + " record=" + baml.json.stringify(p2.record)
}

test "csv_reader_position" {
    assert.equal(
        csv_reader_position(),
        "byte=4 line=2 record=0 then byte=8 line=3 record=1",
    )
}

function csv_reader_close() -> string {
    let r = baml.csv.reader("a\n1\n");
    r.next();
    r.close();
    r.close(); // idempotent
    let err = {
        r.next();
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    // Diagnostic accessors remain valid after close.
    err + ":" + baml.json.stringify(r.skipped_count()) + ":" + baml.json.stringify(r.position().record)
}

test "csv_reader_close" {
    assert.equal(csv_reader_close(), "Closed:0:1")
}

function csv_rows_validates_eagerly() -> string {
    // rows<T>() throws Header at the call site, before any row is read.
    let r = baml.csv.reader("other\nx\n");
    {
        r.rows<IntRow>();
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_rows_validates_eagerly" {
    assert.equal(csv_rows_validates_eagerly(), "Header")
}

class Inner {
    x string
}

class Nested {
    inner Inner
}

function csv_rows_rejects_nested_class() -> string {
    let r = baml.csv.reader("inner\nx\n");
    {
        r.rows<Nested>();
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_rows_rejects_nested_class" {
    // CSV is flat; nested classes are rejected when rows<T> validates.
    assert.equal(csv_rows_rejects_nested_class(), "Options")
}

function csv_iterator_adapters() -> int {
    // BEP-051 default methods work on the typed iterator.
    let r = baml.csv.reader("n\n1\n2\n3\n4\n");
    let doubled = r.rows<IntRow>().map((row: IntRow) -> int { row.n * 2 }).collect();
    let total = 0;
    for (let v in doubled) {
        total += v;
    }
    total
}

test "csv_iterator_adapters" {
    assert.equal(csv_iterator_adapters(), 20)
}

// ─── Options validation ─────────────────────────────────────────────────────

function csv_options_validation() -> string {
    let multi = {
        baml.csv.reader("a\n", options = baml.csv.ReaderOptions { delimiter: "ab" });
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    let clash = {
        baml.csv.reader("a\n", options = baml.csv.ReaderOptions { delimiter: "\"" });
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    multi + "/" + clash
}

test "csv_options_validation" {
    assert.equal(csv_options_validation(), "Options/Options")
}

// ─── Writing ────────────────────────────────────────────────────────────────

class OutRow {
    id int
    note string?
}

class Pos2 {
    a int
    b int
}

function csv_writer_buffer_basics() -> string {
    let w = baml.csv.buffer();
    w.write_rows<OutRow>([
        OutRow { id: 1, note: "needs, quoting" },
        OutRow { id: 2, note: null },
    ]);
    w.text() + "#" + baml.json.stringify(w.records_written())
}

test "csv_writer_buffer_basics" {
    assert.equal(csv_writer_buffer_basics(), "id,note\n1,\"needs, quoting\"\n2,\n#2")
}

function csv_writer_quote_styles() -> string {
    let all = baml.csv.stringify<OutRow>([OutRow { id: 1, note: "x" }], options = baml.csv.WriterOptions { quote_style: "all" });
    let never_err = {
        baml.csv.stringify<OutRow>([OutRow { id: 1, note: "a,b" }], options = baml.csv.WriterOptions { quote_style: "never" });
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    all + "#" + never_err
}

test "csv_writer_quote_styles" {
    assert.equal(csv_writer_quote_styles(), "\"id\",\"note\"\n\"1\",\"x\"\n#Encode")
}

function csv_writer_crlf_and_null_value() -> string {
    baml.csv.stringify<OutRow>([OutRow { id: 1, note: null }], options = baml.csv.WriterOptions {
        terminator: "crlf",
        null_value: "NULL",
        write_header: false,
    })
}

test "csv_writer_crlf_and_null_value" {
    assert.equal(csv_writer_crlf_and_null_value(), "1,NULL\r\n")
}

function csv_writer_headers_override() -> string {
    baml.csv.stringify<OutRow>([OutRow { id: 1, note: "x" }], options = baml.csv.WriterOptions {
        headers: ["ID", "Note"],
    })
}

test "csv_writer_headers_override" {
    assert.equal(csv_writer_headers_override(), "ID,Note\n1,x\n")
}

function csv_writer_explicit_header() -> string {
    let w = baml.csv.buffer();
    w.write_header(["a", "b"]);
    w.write_row<Pos2>(Pos2 { a: 1, b: 2 }); // no auto header after explicit one
    let after_data = {
        w.write_header(["x"]);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    w.text() + "#" + after_data
}

test "csv_writer_explicit_header" {
    assert.equal(csv_writer_explicit_header(), "a,b\n1,2\n#Header")
}

function csv_writer_write_record_raw() -> string {
    let w = baml.csv.buffer(options = baml.csv.WriterOptions { write_header: false });
    let record: baml.csv.CsvValue[] = [null, true, 7, 12345678901234567890123n, 2.5, "txt"];
    w.write_record(record);
    w.text()
}

test "csv_writer_write_record_raw" {
    // write_record never triggers an automatic header.
    assert.equal(csv_writer_write_record_raw(), ",true,7,12345678901234567890123,2.5,txt\n")
}

function csv_writer_float_canonical() -> string {
    let w = baml.csv.buffer(options = baml.csv.WriterOptions { write_header: false });
    let record: baml.csv.CsvValue[] = [2.0, 0.1];
    w.write_record(record);
    w.text()
}

test "csv_writer_float_canonical" {
    assert.equal(csv_writer_float_canonical(), "2.0,0.1\n")
}

function csv_writer_nonfinite_float_throws() -> string {
    let w = baml.csv.buffer(options = baml.csv.WriterOptions { write_header: false });
    let record: baml.csv.CsvValue[] = [float.nan()];
    {
        w.write_record(record);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    }
}

test "csv_writer_nonfinite_float_throws" {
    // A cell the reader would reject is never written.
    assert.equal(csv_writer_nonfinite_float_throws(), "Encode")
}

function csv_writer_sanitize_formulas() -> string {
    let w = baml.csv.buffer(options = baml.csv.WriterOptions { write_header: false, sanitize_formulas: true });
    let record: baml.csv.CsvValue[] = ["=SUM(A1)", "safe"];
    w.write_record(record);
    w.text()
}

test "csv_writer_sanitize_formulas" {
    assert.equal(csv_writer_sanitize_formulas(), "'=SUM(A1),safe\n")
}

function csv_writer_bom() -> string {
    let w = baml.csv.buffer(options = baml.csv.WriterOptions { write_header: false, bom: true });
    let record: baml.csv.CsvValue[] = ["x"];
    w.write_record(record);
    let out = w.text();
    // BOM + "x" + "\n" = 3 chars; the BOM strips on a default read-back.
    baml.json.stringify(out.length())
        + ":" + out.char_at(1)
        + ":" + grid(baml.csv.parse(out, options = baml.csv.ReaderOptions { has_header: false }))
}

test "csv_writer_bom" {
    assert.equal(csv_writer_bom(), "3:x:x")
}

function csv_writer_text_on_file_writer_throws() -> string {
    let path = `/tmp/baml_csv_text_on_file_${baml.time.Instant.now().to_timestamp_milliseconds()}.csv`;
    let w = baml.csv.create(path);
    let err = {
        w.text();
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    w.close();
    baml.fs.remove(path);
    err
}

test "csv_writer_text_on_file_writer_throws" {
    assert.equal(csv_writer_text_on_file_writer_throws(), "Options")
}

function csv_writer_closed_throws() -> string {
    let w = baml.csv.buffer();
    w.write_row<Pos2>(Pos2 { a: 1, b: 2 });
    w.close();
    w.close(); // idempotent
    let err = {
        w.write_row<Pos2>(Pos2 { a: 3, b: 4 });
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    // records_written remains valid after close.
    err + ":" + baml.json.stringify(w.records_written())
}

test "csv_writer_closed_throws" {
    assert.equal(csv_writer_closed_throws(), "Closed:1")
}

function csv_stringify_records_roundtrip() -> string {
    let text = baml.csv.stringify_records([["a", "b"], ["1", "x\ny"]]);
    let back = baml.csv.parse(text, options = baml.csv.ReaderOptions { has_header: false });
    text + "#" + grid(back)
}

test "csv_stringify_records_roundtrip" {
    assert.equal(csv_stringify_records_roundtrip(), "a,b\n1,\"x\ny\"\n#a|b;1|x\ny")
}

// ─── File IO ────────────────────────────────────────────────────────────────

function csv_file_roundtrip() -> string {
    let path = `/tmp/baml_csv_file_roundtrip_${baml.time.Instant.now().to_timestamp_milliseconds()}.csv`;
    let bytes = baml.csv.write<OutRow>(path, [
        OutRow { id: 1, note: "hello" },
        OutRow { id: 2, note: null },
    ]);
    let back = baml.csv.read<OutRow>(path);
    let first_note = match (back.at(0)) {
        null => "missing",
        let r: OutRow => r.note ?? "<null>",
    };
    baml.fs.remove(path);
    baml.json.stringify(bytes > 0) + ":" + baml.json.stringify(back.length()) + ":" + first_note
}

test "csv_file_roundtrip" {
    assert.equal(csv_file_roundtrip(), "true:2:hello")
}

function csv_open_streams_file() -> string {
    let path = `/tmp/baml_csv_open_streams_${baml.time.Instant.now().to_timestamp_milliseconds()}.csv`;
    baml.fs.write(path, "n\n10\n20\n30\n");
    let r = baml.csv.open(path);
    let total = 0;
    for (let row in r.rows<IntRow>()) {
        total += row.n;
    }
    r.close();
    baml.fs.remove(path);
    baml.json.stringify(total)
}

test "csv_open_streams_file" {
    assert.equal(csv_open_streams_file(), "60")
}

function csv_create_then_fs_read() -> string {
    let path = `/tmp/baml_csv_create_read_${baml.time.Instant.now().to_timestamp_milliseconds()}.csv`;
    let w = baml.csv.create(path);
    w.write_header(["k", "v"]);
    let record: baml.csv.CsvValue[] = ["x", 1];
    w.write_record(record);
    w.close();
    let content = baml.fs.read(path);
    baml.fs.remove(path);
    content
}

test "csv_create_then_fs_read" {
    assert.equal(csv_create_then_fs_read(), "k,v\nx,1\n")
}

// ─── Markdown rendering ─────────────────────────────────────────────────────

class MdRow {
    metric string
    value float
}

function csv_to_markdown_truncation() -> string {
    baml.csv.to_markdown<MdRow>(
        [
            MdRow { metric: "a", value: 1.0 },
            MdRow { metric: "b", value: 2.0 },
            MdRow { metric: "c", value: 3.0 },
        ],
        max_rows = 2,
    )
}

test "csv_to_markdown_truncation" {
    assert.equal(
        csv_to_markdown_truncation(),
        "| metric | value |\n| --- | --- |\n| a | 1.0 |\n| b | 2.0 |\n… (1 more rows)",
    )
}

function csv_to_markdown_nonfinite() -> string {
    baml.csv.to_markdown<MdRow>([MdRow { metric: "nan", value: float.nan() }])
}

test "csv_to_markdown_nonfinite" {
    // Prompt text is not meant to round-trip: non-finite floats render as text.
    assert.equal(
        csv_to_markdown_nonfinite(),
        "| metric | value |\n| --- | --- |\n| nan | NaN |",
    )
}

function csv_to_markdown_records_with_headers() -> string {
    baml.csv.to_markdown_records([["1", "x"]], headers = ["id", "name"])
}

test "csv_to_markdown_records_with_headers" {
    assert.equal(
        csv_to_markdown_records_with_headers(),
        "| id | name |\n| --- | --- |\n| 1 | x |",
    )
}

function csv_to_markdown_records_default_headers() -> string {
    baml.csv.to_markdown_records([["1", "x"]])
}

test "csv_to_markdown_records_default_headers" {
    assert.equal(
        csv_to_markdown_records_default_headers(),
        "| col1 | col2 |\n| --- | --- |\n| 1 | x |",
    )
}

// ─── The Excel profile (documented recipe) ──────────────────────────────────

function csv_excel_profile() -> string {
    let rows = baml.csv.parse("a;b\n1;2\n", options = baml.csv.ReaderOptions { delimiter: ";", encoding: "utf8-lossy" });
    let out = baml.csv.stringify_records([["x"]], options = baml.csv.WriterOptions {
        terminator: "crlf",
        bom: true,
        sanitize_formulas: true,
    });
    // BOM + "x" + "\r" + "\n" = 4 chars.
    grid(rows) + "#" + baml.json.stringify(out.length()) + ":" + out.char_at(1) + ":" + out.char_at(2)
}

test "csv_excel_profile" {
    assert.equal(csv_excel_profile(), "1|2#4:x:\r")
}

// ─── Review-fix regressions ─────────────────────────────────────────────────

function csv_malformed_header_exhausts_reader() -> string {
    // A corrupt header must NOT silently install the first data row as the
    // header; the reader is exhausted after the throw.
    let r = baml.csv.reader("\"h1\"x,h2\na,b\nc,d\n");
    let first = {
        match (r.next()) {
            baml.iter.Done => "done",
            let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
        }
    } catch (e) {
        baml.csv.CsvError => "<" + kind_name(e.kind) + ">",
    };
    let second = match (r.next()) {
        baml.iter.Done => "done",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    first + "/" + second
}

test "csv_malformed_header_exhausts_reader" {
    assert.equal(csv_malformed_header_exhausts_reader(), "<Quote>/done")
}

function csv_malformed_header_failure_is_terminal_for_headers() -> string {
    // After the malformed header is consumed and thrown from next(), headers()
    // must not reinterpret the first data row ("a,b") as column names.
    let r = baml.csv.reader("\"h1\"x,h2\na,b\nc,d\n");
    let first = {
        match (r.next()) {
            baml.iter.Done => "done",
            let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
        }
    } catch (e) {
        baml.csv.CsvError => "<" + kind_name(e.kind) + ">",
    };
    let headers_after = {
        match (r.headers()) {
            null => "<null>",
            let h: string[] => join(h, "|"),
        }
    } catch (e) {
        baml.csv.CsvError => "<" + kind_name(e.kind) + ">",
    };
    let next_after = match (r.next()) {
        baml.iter.Done => "done",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    first + "/" + headers_after + "/" + next_after
}

test "csv_malformed_header_failure_is_terminal_for_headers" {
    assert.equal(csv_malformed_header_failure_is_terminal_for_headers(), "<Quote>/<Quote>/done")
}

function csv_header_encoding_failure_is_terminal_for_headers() -> string {
    // Header UTF-8 failures also consume the header record. A later headers()
    // call must not promote the first data row to headers.
    let r = baml.csv.reader(b"\xff,h2\na,b\nc,d\n");
    let first = {
        match (r.next()) {
            baml.iter.Done => "done",
            let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
        }
    } catch (e) {
        baml.csv.CsvError => "<" + kind_name(e.kind) + ">",
    };
    let headers_after = {
        match (r.headers()) {
            null => "<null>",
            let h: string[] => join(h, "|"),
        }
    } catch (e) {
        baml.csv.CsvError => "<" + kind_name(e.kind) + ">",
    };
    let next_after = match (r.next()) {
        baml.iter.Done => "done",
        let rec: baml.csv.CsvRecord => join(rec.fields(), "|"),
    };
    first + "/" + headers_after + "/" + next_after
}

test "csv_header_encoding_failure_is_terminal_for_headers" {
    assert.equal(csv_header_encoding_failure_is_terminal_for_headers(), "<Encoding>/<Encoding>/done")
}

function csv_writer_state_survives_encode_error() -> string {
    // A caught Encode error must leave the writer untouched: the auto header
    // still goes out with the next successful row, and nothing phantom is
    // counted.
    let w = baml.csv.buffer();
    let caught = {
        w.write_row<MdRow>(MdRow { metric: "bad", value: float.nan() });
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    w.write_row<MdRow>(MdRow { metric: "ok", value: 1.5 });
    caught + "#" + w.text() + "#" + baml.json.stringify(w.records_written())
}

test "csv_writer_state_survives_encode_error" {
    assert.equal(
        csv_writer_state_survives_encode_error(),
        "Encode#metric,value\nok,1.5\n#1",
    )
}

function csv_writer_bom_survives_encode_error() -> string {
    // The pending BOM must not be consumed by a line that was never emitted.
    let w = baml.csv.buffer(options = baml.csv.WriterOptions {
        write_header: false,
        bom: true,
        quote_style: "never",
    });
    let bad: baml.csv.CsvValue[] = ["a,b"];
    let caught = {
        w.write_record(bad);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    let good: baml.csv.CsvValue[] = ["x"];
    w.write_record(good);
    let out = w.text();
    // BOM + "x" + "\n" = 3 chars.
    caught + "#" + baml.json.stringify(out.length()) + ":" + out.char_at(1)
}

test "csv_writer_bom_survives_encode_error" {
    assert.equal(csv_writer_bom_survives_encode_error(), "Encode#3:x")
}

function csv_write_rows_batch_is_atomic() -> string {
    // write_rows is one atomic batch: a mid-batch failure emits and counts
    // nothing.
    let w = baml.csv.buffer();
    let caught = {
        w.write_rows<MdRow>([
            MdRow { metric: "good", value: 1.0 },
            MdRow { metric: "bad", value: float.nan() },
        ]);
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    caught + "#" + w.text() + "#" + baml.json.stringify(w.records_written())
}

test "csv_write_rows_batch_is_atomic" {
    assert.equal(csv_write_rows_batch_is_atomic(), "Encode##0")
}

function csv_caller_owned_handles_stay_open() -> string {
    // Handles passed to writer(file)/reader(file) belong to the caller:
    // closing the CSV handle must not close them. open()/create() handles
    // are owned and do get closed.
    let path = `/tmp/baml_csv_caller_owned_${baml.time.Instant.now().to_timestamp_milliseconds()}.csv`;
    let wf = baml.fs.open(path, "w");
    let w = baml.csv.writer(wf);
    let record: baml.csv.CsvValue[] = ["x", 1];
    w.write_record(record);
    w.close();
    // The caller's handle is still usable after w.close().
    let appended = wf.write("tail\n");
    wf.close();
    let content = baml.fs.read(path);
    baml.fs.remove(path);
    baml.json.stringify(appended > 0) + "#" + content
}

test "csv_caller_owned_handles_stay_open" {
    assert.equal(csv_caller_owned_handles_stay_open(), "true#x,1\ntail\n")
}

class DatedRow {
    day baml.time.PlainDate
    at baml.time.PlainDateTime
}

function csv_plaindate_cells_roundtrip() -> string {
    // PlainDate / PlainDateTime have shipped textual contracts and decode /
    // encode as CSV cells.
    let rows = baml.csv.decode<DatedRow>("day,at\n2026-06-11,2026-06-11T08:30:00\n");
    let rendered = match (rows.at(0)) {
        null => "missing",
        let r: DatedRow => r.day.to_string() + " " + r.at.to_string(),
    };
    let back = baml.csv.stringify<DatedRow>(rows);
    let bad = {
        baml.csv.decode<DatedRow>("day,at\nnot-a-date,2026-06-11T08:30:00\n");
        "no-throw"
    } catch (e) {
        baml.csv.CsvError => kind_name(e.kind),
    };
    rendered + "#" + back + "#" + bad
}

test "csv_plaindate_cells_roundtrip" {
    assert.equal(
        csv_plaindate_cells_roundtrip(),
        "2026-06-11 2026-06-11T08:30:00#day,at\n2026-06-11,2026-06-11T08:30:00\n#Decode",
    )
}
