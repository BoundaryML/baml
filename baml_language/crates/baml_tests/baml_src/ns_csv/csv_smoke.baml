//
// Helper functions use closures to box local values (see ns_closures/closures.baml).
// User types are prefixed `CsvSmoke` to avoid corpus-level collisions with
// ns_csv/csv.baml (which defines `IntRow`, `Verdict`, etc.).

// ─── Untyped one-shot parse ─────────────────────────────────────────────────

/// Untyped one-shot parse: dimensions and a quoted cell with comma + newline.
/// The header row is consumed (`has_header` defaults to `true`), so `parse`
/// returns data records only.
function csv_smoke_parse_quoted_cells() -> string {
    let rows = baml.csv.parse("a,b\n1,\"x,\ny\"\n2,plain\n");
    baml.json.stringify(rows.length()) + ":" + rows[0][1]
}

test "parse_quoted_cells" {
    assert.equal(csv_smoke_parse_quoted_cells(), "2:x,\ny")
}

// ─── Typed decode ───────────────────────────────────────────────────────────

class CsvSmokeLead {
    company string
    score float?
}

/// Typed decode into a user class, with an optional null cell.
function csv_smoke_decode_typed_rows() -> string {
    let leads = baml.csv.decode<CsvSmokeLead>("company,score\nAcme,0.5\nGlobex,\n");
    let second = match (leads.at(1)) {
        null => "missing",
        let l: CsvSmokeLead => match (l.score) {
            null => l.company + ":null",
            let s: float => l.company + ":" + baml.json.stringify(s),
        },
    };
    baml.json.stringify(leads.length()) + ":" + second
}

test "decode_typed_rows" {
    assert.equal(csv_smoke_decode_typed_rows(), "2:Globex:null")
}

enum CsvSmokeColor {
    Red
    Green
    Blue
}

class CsvSmokeEnumRow {
    name string
    color CsvSmokeColor
}

/// Regression: a user-package enum field must resolve when decoding typed rows.
/// `Target::Enum` once resolved through a package-eliding fqn lookup that only
/// worked for builtin enums, so a user enum decoded as "enum not found".
function csv_smoke_decode_user_enum_rows() -> string {
    let rows = baml.csv.decode<CsvSmokeEnumRow>("name,color\nsky,Blue\ngrass,Green\n");
    let out = "";
    for (let r in rows) {
        let c = match (r.color) {
            CsvSmokeColor.Red => "Red",
            CsvSmokeColor.Green => "Green",
            CsvSmokeColor.Blue => "Blue",
        };
        out = out + r.name + "=" + c + ";";
    }
    out
}

test "decode_user_enum_rows" {
    assert.equal(csv_smoke_decode_user_enum_rows(), "sky=Blue;grass=Green;")
}

// ─── Streaming reader ───────────────────────────────────────────────────────

/// Streaming reader over an in-memory source: iterator + get<T>.
function csv_smoke_reader_streaming_get() -> int {
    let r = baml.csv.reader("n\n1\n2\n3\n");
    let total = 0;
    for (let rec in r) {
        total += match (rec.get<int>("n")) {
            null => 0,
            let v: int => v,
        };
    }
    total
}

test "reader_streaming_get" {
    assert.equal(csv_smoke_reader_streaming_get(), 6)
}

// ─── Round-trip ─────────────────────────────────────────────────────────────

class CsvSmokeVerdict {
    id int
    label string
}

/// Round-trip: stringify rows of a class, then decode them back.
function csv_smoke_stringify_round_trip() -> string {
    let text = baml.csv.stringify<CsvSmokeVerdict>([
        CsvSmokeVerdict { id: 1, label: "ok, fine" },
        CsvSmokeVerdict { id: 2, label: "bad" },
    ]);
    let back = baml.csv.decode<CsvSmokeVerdict>(text);
    text + "|" + baml.json.stringify(back.length())
}

test "stringify_round_trip" {
    assert.equal(csv_smoke_stringify_round_trip(), "id,label\n1,\"ok, fine\"\n2,bad\n|2")
}

// ─── Per-record errors ──────────────────────────────────────────────────────

/// A malformed record throws a positional CsvError and the reader resumes.
function csv_smoke_record_error_resumes() -> string {
    let r = baml.csv.reader("a,b\n1,2\n\"oops,9\n3,4\n", options = baml.csv.ReaderOptions {
        ragged: "pad",
    });
    let seen = "";
    let bad = 0;
    while (true) {
        let step = {
            match (r.next()) {
                baml.iter.Done => "done",
                let rec: baml.csv.CsvRecord => rec.fields()[0],
            }
        } catch (_) {
            baml.csv.CsvError { line } => {
                bad += 1;
                "err@" + match (line) { null => "?", let l: int => baml.json.stringify(l) }
            },
        };
        if (step == "done") { break; }
        seen = seen + step + ";";
    }
    seen + "bad=" + baml.json.stringify(bad)
}

test "record_error_resumes" {
    assert.equal(csv_smoke_record_error_resumes(), "1;err@3;bad=1")
}

/// `on_error: "skip"` skips bad records and keeps diagnostics.
function csv_smoke_on_error_skip() -> string {
    let r = baml.csv.reader("a,b\n1,2\n3\n4,5\n", options = baml.csv.ReaderOptions {
        on_error: "skip",
    });
    let count = 0;
    for (let rec in r) {
        count += 1;
    }
    baml.json.stringify(count) + ":" + baml.json.stringify(r.skipped_count())
}

test "on_error_skip" {
    assert.equal(csv_smoke_on_error_skip(), "2:1")
}

// ─── Markdown rendering ─────────────────────────────────────────────────────

class CsvSmokeKpi {
    metric string
    q1 float
}

/// Markdown rendering for prompt context.
function csv_smoke_to_markdown_table() -> string {
    baml.csv.to_markdown<CsvSmokeKpi>([CsvSmokeKpi { metric: "growth|rate", q1: 1.5 }])
}

test "to_markdown_table" {
    assert.equal(
        csv_smoke_to_markdown_table(),
        "| metric | q1 |\n| --- | --- |\n| growth\\|rate | 1.5 |",
    )
}

// ─── Generic method returns ─────────────────────────────────────────────────

class CsvSmokeIntRow {
    n int
}

/// Static typing probe: generic method returns must substitute at user sites.
function csv_smoke_typed_returns_probe() -> string {
    let r = baml.csv.reader("name,n\nalice,1\nbob,2\n");
    let rec_name = {
        match (r.next()) {
            baml.iter.Done => "none",
            let rec: baml.csv.CsvRecord => rec.get<string>("name") ?? "<null>",
        }
    };
    let total = 0;
    for (let row in r.rows<CsvSmokeIntRow>()) {
        total += row.n;
    }
    rec_name + ":" + baml.json.stringify(total)
}

test "typed_returns_probe" {
    assert.equal(csv_smoke_typed_returns_probe(), "alice:2")
}
