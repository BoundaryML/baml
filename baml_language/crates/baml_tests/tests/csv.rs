//! Runtime tests for the `baml.csv` standard-library namespace.
//!
//! The bulk of the coverage lives in `baml_src/ns_csv/csv.baml` as pure-BAML
//! tests. This file holds quick end-to-end smoke tests that exercise the
//! stdlib through the full compile + execute pipeline with direct assertions
//! on the returned values.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn ok_string(s: &str) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::String(s.to_string().into()))
}

fn ok_int(i: i64) -> Result<BexExternalValue, String> {
    Ok(BexExternalValue::Int(i))
}

/// Untyped one-shot parse: dimensions and a quoted cell with comma + newline.
/// The header row is consumed (`has_header` defaults to `true`), so `parse`
/// returns data records only.
#[tokio::test]
async fn parse_quoted_cells() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let rows = baml.csv.parse("a,b\n1,\"x,\ny\"\n2,plain\n");
            baml.json.stringify(rows.length()) + ":" + rows[0][1]
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("2:x,\ny")
    );
}

/// Typed decode into a user class, with an optional null cell.
#[tokio::test]
async fn decode_typed_rows() {
    let output = baml_test!(
        r#"
        class Lead {
            company string
            score float?
        }
        function main() -> string {
            let leads = baml.csv.decode<Lead>("company,score\nAcme,0.5\nGlobex,\n");
            let second = match (leads.at(1)) {
                null => "missing",
                let l: Lead => match (l.score) {
                    null => l.company + ":null",
                    let s: float => l.company + ":" + baml.json.stringify(s),
                },
            };
            baml.json.stringify(leads.length()) + ":" + second
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("2:Globex:null")
    );
}

/// Regression: a user-package enum field must resolve when decoding typed rows.
/// `Target::Enum` once resolved through a package-eliding fqn lookup that only
/// worked for builtin enums, so a user enum decoded as "enum not found".
#[tokio::test]
async fn decode_user_enum_rows() {
    let output = baml_test!(
        r#"
        enum Color { Red Green Blue }
        class Row {
            name string
            color Color
        }
        function main() -> string {
            let rows = baml.csv.decode<Row>("name,color\nsky,Blue\ngrass,Green\n");
            let out = "";
            for (let r in rows) {
                let c = match (r.color) {
                    Color.Red => "Red",
                    Color.Green => "Green",
                    Color.Blue => "Blue",
                };
                out = out + r.name + "=" + c + ";";
            }
            out
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("sky=Blue;grass=Green;")
    );
}

/// Streaming reader over an in-memory source: iterator + get<T>.
#[tokio::test]
async fn reader_streaming_get() {
    let output = baml_test!(
        r#"
        function main() -> int {
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
    "#
    );
    assert_eq!(output.result.map_err(|e| format!("{e:?}")), ok_int(6));
}

/// Round-trip: stringify rows of a class, then decode them back.
#[tokio::test]
async fn stringify_round_trip() {
    let output = baml_test!(
        r#"
        class Verdict {
            id int
            label string
        }
        function main() -> string {
            let text = baml.csv.stringify<Verdict>([
                Verdict { id: 1, label: "ok, fine" },
                Verdict { id: 2, label: "bad" },
            ]);
            let back = baml.csv.decode<Verdict>(text);
            text + "|" + baml.json.stringify(back.length())
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("id,label\n1,\"ok, fine\"\n2,bad\n|2")
    );
}

/// A malformed record throws a positional CsvError and the reader resumes.
#[tokio::test]
async fn record_error_resumes() {
    let output = baml_test!(
        r#"
        function main() -> string {
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
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("1;err@3;bad=1")
    );
}

/// `on_error: "skip"` skips bad records and keeps diagnostics.
#[tokio::test]
async fn on_error_skip() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let r = baml.csv.reader("a,b\n1,2\n3\n4,5\n", options = baml.csv.ReaderOptions {
                on_error: "skip",
            });
            let count = 0;
            for (let rec in r) {
                count += 1;
            }
            baml.json.stringify(count) + ":" + baml.json.stringify(r.skipped_count())
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("2:1")
    );
}

/// Markdown rendering for prompt context.
#[tokio::test]
async fn to_markdown_table() {
    let output = baml_test!(
        r#"
        class Kpi {
            metric string
            q1 float
        }
        function main() -> string {
            baml.csv.to_markdown<Kpi>([Kpi { metric: "growth|rate", q1: 1.5 }])
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        ok_string("| metric | q1 |\n| --- | --- |\n| growth\\|rate | 1.5 |")
    );
}

/// Static typing probe: generic method returns must substitute at user sites.
#[tokio::test]
async fn typed_returns_probe() {
    let output = baml_test!(
        r#"
        class IntRow { n int }
        function main() -> string {
            let r = baml.csv.reader("name,n\nalice,1\nbob,2\n");
            let rec_name = {
                match (r.next()) {
                    baml.iter.Done => "none",
                    let rec: baml.csv.CsvRecord => rec.get<string>("name") ?? "<null>",
                }
            };
            let total = 0;
            for (let row in r.rows<IntRow>()) {
                total += row.n;
            }
            rec_name + ":" + baml.json.stringify(total)
        }
    "#
    );
    assert_eq!(
        output.result.map_err(|e| format!("{e:?}")),
        Ok(bex_engine::BexExternalValue::String(
            "alice:2".to_string().into()
        ))
    );
}
