//! `BAML_GC_STATS=1` has a native program write one `baml-gc-stats {json}`
//! line on stderr when it ends, returning or throwing, and nothing else
//! changes: without the variable, or with it `0`, the program behaves as it
//! always did, and with it, it prints the same and exits the same.

use baml_db::testing::{assert_no_diagnostic_errors, setup_test_db};

mod common;
use common::{WAYS, build, command, finished};

const SOURCE: &str = r#"
function collects_seven() -> int {
  let kept: int[] = [];
  for (let i = 0; i < 7; i += 1) {
    kept.push(i);
    baml.sys.collect_garbage();
  }
  kept.length()
}

function collects_three_then_throws() -> int {
  for (let i = 0; i < 3; i += 1) {
    baml.sys.collect_garbage();
  }
  throw "done"
}
"#;

const PREFIX: &str = "baml-gc-stats ";

/// The cases: the entry function, and how many collections it forces.
const CASES: [(&str, u64); 2] = [("collects_seven", 7), ("collects_three_then_throws", 3)];

/// The value of the field `name` of the line, a whole number.
fn field(line: &str, name: &str) -> u64 {
    let key = format!("\"{name}\":");
    let start = line
        .find(&key)
        .unwrap_or_else(|| panic!("no `{name}` in {line}"))
        + key.len();
    let digits: String = line[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits
        .parse()
        .unwrap_or_else(|_| panic!("`{name}` is not a number in {line}"))
}

#[test]
fn the_line_is_written_only_when_asked_and_counts_every_collection() {
    let db = setup_test_db(SOURCE);
    assert_no_diagnostic_errors(&db);
    let directory = tempfile::tempdir().expect("a temporary directory");

    for (function, collections) in CASES {
        let built = build(&db, function, directory.path())
            .unwrap_or_else(|error| panic!("`{function}` {error}"));
        for way in &WAYS {
            let plain = finished(&mut command(&built, way));
            let off = finished(command(&built, way).env("BAML_GC_STATS", "0"));
            let on = finished(command(&built, way).env("BAML_GC_STATS", "1"));
            let text = |bytes: &[u8]| String::from_utf8_lossy(bytes).into_owned();
            let what = format!("`{function}` {}", way.name);

            assert!(
                !text(&plain.stderr).contains(PREFIX),
                "{what}: a line unasked"
            );
            assert_eq!(
                (off.status.code(), &off.stdout, &off.stderr),
                (plain.status.code(), &plain.stdout, &plain.stderr),
                "{what}: `BAML_GC_STATS=0` is not off",
            );

            // On: the same exit and stdout, and stderr with one line more,
            // the last.
            assert_eq!(on.status.code(), plain.status.code(), "{what}: exit");
            assert_eq!(text(&on.stdout), text(&plain.stdout), "{what}: stdout");
            let stderr = text(&on.stderr);
            let (before, line) = stderr
                .trim_end_matches('\n')
                .rsplit_once('\n')
                .map_or(("", stderr.trim_end_matches('\n')), |(before, last)| {
                    (before, last)
                });
            assert!(
                line.starts_with(PREFIX),
                "{what}: no line last in {stderr:?}"
            );
            assert_eq!(
                before.trim_end_matches('\n'),
                text(&plain.stderr).trim_end_matches('\n'),
                "{what}: stderr otherwise"
            );
            let json = &line[PREFIX.len()..];
            assert!(
                json.starts_with('{') && json.ends_with("]}"),
                "{what}: {line}"
            );
            let pauses = json.split_once("\"pause_ns\":[").expect("the pauses").1;
            let listed = pauses
                .trim_end_matches("]}")
                .split(',')
                .filter(|pause| !pause.is_empty());

            // Collecting on every allocation collects more than the
            // program asks for; otherwise the program's calls are all.
            let counted = field(line, "collections");
            if way.variables.contains(&"BAML_GC_STRESS") {
                assert!(counted >= collections, "{what}: {line}");
            } else {
                assert_eq!(counted, collections, "{what}: {line}");
            }
            assert_eq!(listed.count() as u64, counted, "{what}: {line}");
            assert!(
                field(line, "pause_ns_max") <= field(line, "pause_ns_total"),
                "{what}"
            );
            assert!(field(line, "heap_capacity_peak") > 0, "{what}: {line}");
            for name in [
                "bytes_copied",
                "owned_dropped_by_collections",
                "owned_drop_ns_total",
                "owned_dropped_at_exit",
            ] {
                field(line, name);
            }
        }
    }
}
