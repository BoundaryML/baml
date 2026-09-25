//! Landing-note lifetimes: a rethrow or conversion must never be proven from
//! a note whose handler already finished or from another invocation, and a
//! finished handler must not blur a live one. Real engine recordings.
//!
//! The first two tests reproduced review findings before the fix: a false
//! proven conversion and a lost rethrow link.
mod support;

use baml_query_btel::Index;
use serde_json::Value as Json;
use support::*;

const SOURCE: &str = r#"
function Word() -> string { "same" }
function Same() -> int throws string { throw Word() }

function sequential(n: int) -> int {
    let a = Same() catch (e) { _ => 1 };
    let b = { Same() catch (e2) { _ => { throw e2 } } } catch (outer) { _ => 2 };
    a + b
}

// A finished handler still holds the interned "same" in its slot. An
// unrelated `Word()` result, never raised or caught here, is converted.
function convert_unrelated(n: int) -> int {
    let a = Same() catch (e) { _ => 1 };
    let x = Word();
    let r = { throw baml.errors.UnknownError.from<int>(x) } catch (outer) { _ => 7 };
    a + r
}

// The handler is still running and its slot holds the interned "same", but
// the converted `x` comes from a separate `Word()` call, not from `e`.
function convert_in_live_catch(n: int) -> int {
    let r = {
        Same() catch (e) { _ => { let x = Word(); throw baml.errors.UnknownError.from<int>(x) } }
    } catch (outer) { _ => 9 };
    r
}

// Control: the same conversion with no earlier catch in the function.
function convert_control(n: int) -> int {
    let x = Word();
    let r = { throw baml.errors.UnknownError.from<int>(x) } catch (outer) { _ => 7 };
    r
}

function Twice(flag: bool) -> int throws string {
    if (flag) {
        let a = { throw Word() } catch (e) { _ => 1 };
        a
    } else {
        let s = Word();
        throw s
    }
}
function twice(n: int) -> int {
    let first = Twice(true);
    let second = Twice(false) catch (e) { _ => 4 };
    first + second
}

function Converts(flag: bool) -> int throws baml.errors.UnknownError | string | int {
    if (flag) {
        let a = { throw Word() } catch (e) { _ => 1 };
        a
    } else {
        let s = Word();
        throw baml.errors.UnknownError.from<int>(s)
    }
}
function converts(n: int) -> int {
    let first = Converts(true);
    let second = Converts(false) catch (e) { _ => 4 };
    first + second
}
"#;

const ENTRIES: &[&str] = &[
    "sequential",
    "convert_unrelated",
    "convert_in_live_catch",
    "convert_control",
    "twice",
    "converts",
];

#[derive(Debug)]
struct Raise {
    id: String,
    kind: String,
    origin: String,
    via: Option<String>,
    occurrence: Option<String>,
    site: String,
}

/// Every entry once, in one recording; each returns an int.
async fn record() -> (tempfile::TempDir, Vec<i64>, Index) {
    let project = tempfile::tempdir().unwrap();
    let calls: Vec<(&str, i64)> = ENTRIES.iter().map(|name| (*name, 1)).collect();
    let results = record_program(project.path(), SOURCE, &[], &calls).await;
    let values = results
        .into_iter()
        .map(|result| match result.expect("entry returns") {
            bex_engine::BexExternalValue::Int(value) => value,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    let index = index(project.path());
    (project, values, index)
}

fn result_of(values: &[i64], entry: &str) -> i64 {
    values[ENTRIES.iter().position(|name| *name == entry).unwrap()]
}

/// Raises of one entry's execution, in raise order.
fn raises(index: &mut Index, entry: &str) -> Vec<Raise> {
    let opt = |value: &Json| value.as_str().map(str::to_owned);
    sql(
        index,
        &format!(
            "SELECT e.raise_id, e.kind, e.origin_state, e.origin_via, e.occurrence_id,
               e.site_start, e.site_end
             FROM error_raises e JOIN threads t ON t.thread_id = e.thread_id
             JOIN executions x ON x.execution_id = t.execution_id
             WHERE x.entry_fqn = 'user.{entry}' ORDER BY e.raised_ticks"
        ),
    )
    .rows
    .iter()
    .map(|row| {
        let site = match (row[5].as_u64(), row[6].as_u64()) {
            (Some(a), Some(b)) => {
                SOURCE[usize::try_from(a).unwrap()..usize::try_from(b).unwrap()].to_owned()
            }
            _ => "<none>".to_owned(),
        };
        Raise {
            id: row[0].as_str().unwrap().to_owned(),
            kind: row[1].as_str().unwrap().to_owned(),
            origin: row[2].as_str().unwrap().to_owned(),
            via: opt(&row[3]),
            occurrence: opt(&row[4]),
            site,
        }
    })
    .collect()
}

/// A value obtained independently and never raised or caught must not be
/// linked to an earlier occurrence just because a finished handler's slot
/// still holds an equal (here: the same interned) value.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conversion_of_an_unrelated_equal_value_is_not_proven() {
    let (_project, values, mut index) = record().await;
    assert_eq!(result_of(&values, "convert_unrelated"), 8);
    let raises = raises(&mut index, "convert_unrelated");
    assert_eq!(raises.len(), 2, "{raises:#?}");
    assert_eq!(raises[0].origin, "fresh");
    let converted = &raises[1];
    assert_eq!(
        converted.site,
        "throw baml.errors.UnknownError.from<int>(x)"
    );
    assert_ne!(
        converted.occurrence.as_deref(),
        Some(raises[0].id.as_str()),
        "the conversion of an unrelated value was attributed to a finished handler's occurrence: {converted:#?}"
    );
    assert_eq!(
        (converted.origin.as_str(), converted.via.as_deref()),
        ("unresolved", Some("normalization")),
        "{converted:#?}"
    );
}

/// A live handler's slot holding an equal value proves nothing about where a
/// converted value came from: here it is a separate `Word()` result.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conversion_inside_a_live_catch_of_an_unrelated_equal_value_is_not_proven() {
    let (_project, values, mut index) = record().await;
    assert_eq!(result_of(&values, "convert_in_live_catch"), 9);
    let raises = raises(&mut index, "convert_in_live_catch");
    assert_eq!(raises.len(), 2, "{raises:#?}");
    assert_eq!(raises[0].origin, "fresh");
    let converted = &raises[1];
    assert_eq!(
        converted.site,
        "throw baml.errors.UnknownError.from<int>(x)"
    );
    assert_ne!(
        converted.occurrence.as_deref(),
        Some(raises[0].id.as_str()),
        "the conversion of an unrelated value was attributed to the live catch's occurrence: {converted:#?}"
    );
    assert_eq!(
        (converted.origin.as_str(), converted.via.as_deref()),
        ("unresolved", Some("normalization")),
        "{converted:#?}"
    );
}

/// A handler that already finished must not compete with the live one: the
/// rethrow of the second catch's binding is that catch's occurrence.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_finished_handler_does_not_make_a_later_rethrow_ambiguous() {
    let (_project, values, mut index) = record().await;
    assert_eq!(result_of(&values, "sequential"), 3);
    let raises = raises(&mut index, "sequential");
    assert_eq!(raises.len(), 3, "{raises:#?}");
    assert!(raises[..2].iter().all(|raise| raise.origin == "fresh"));
    let rethrow = &raises[2];
    assert_eq!(
        (rethrow.kind.as_str(), rethrow.site.as_str()),
        ("rethrow", "throw e2")
    );
    assert_eq!(
        (rethrow.origin.as_str(), rethrow.occurrence.as_deref()),
        ("proven", Some(raises[1].id.as_str())),
        "{rethrow:#?}"
    );
}

/// The same function called twice at one frame depth: notes of the first
/// call must not prove anything in the second.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_second_invocation_never_inherits_the_first_ones_notes() {
    let (_project, values, mut index) = record().await;
    assert_eq!(result_of(&values, "twice"), 5);
    assert_eq!(result_of(&values, "converts"), 5);
    for entry in ["twice", "converts"] {
        let raises = raises(&mut index, entry);
        assert_eq!(raises.len(), 2, "{entry}: {raises:#?}");
        assert_eq!(raises[0].origin, "fresh");
        assert_ne!(
            raises[1].occurrence.as_deref(),
            Some(raises[0].id.as_str()),
            "{entry}: {raises:#?}"
        );
    }
    let converted = &raises(&mut index, "converts")[1];
    assert_eq!(
        (converted.origin.as_str(), converted.via.as_deref()),
        ("unresolved", Some("normalization"))
    );
    // Control: the same conversion in a run with no earlier catch.
    let control = raises(&mut index, "convert_control");
    assert_eq!(control.len(), 1);
    assert_eq!(control[0].origin, "fresh");
}
