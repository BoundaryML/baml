//! Error identity from real recordings: every raise is distinct, origins are
//! linked only when the VM proved them, and failed calls are exactly the
//! retained calls each raise unwound.
mod support;

use std::{collections::HashMap, path::Path, sync::Arc, time::Duration};

use baml_query_btel::Index;
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder, TelemetryRecording};
use btel_recorder::RecordingConfig;
use serde_json::Value as Json;
use support::*;
use sys_native::SysOpsExt;

const SOURCE: &str = r#"
class Failure { code int }
function Leaf(n: int) -> int { n + 1 }
function Thrower(n: int) -> int throws Failure { throw Failure { code: n } }
function Middle(n: int) -> int throws Failure { Thrower(n) + 1 }
function Top(n: int) -> int throws Failure { Middle(n) + 1 }
function Same() -> int throws string { throw "same" }
function caught_here(n: int) -> int {
    let v = { if (n > 0) { throw Failure { code: n } }; n } catch (e) { Failure => 0 };
    v
}
function propagated(n: int) -> int { Top(n) catch (e) { Failure => 0 } }
function equal_values(n: int) -> int {
    let a = Same() catch (e) { _ => 1 };
    let b = Same() catch (e) { _ => 2 };
    a + b
}
function rethrown(n: int) -> int {
    let r = { Middle(n) catch (e) { Failure => { throw e } } } catch (outer) { Failure => 3 };
    r
}
function nested(n: int) -> int {
    let dup = Failure { code: n };
    let r = {
        { throw dup } catch (a) {
            _ => { { throw dup } catch (b) { _ => { throw a } } }
        }
    } catch (outer) { _ => 3 };
    r
}
function while_handling(n: int) -> int {
    let r = { { throw "first" } catch (e) { _ => { throw "second" } } } catch (x) { _ => 4 };
    r
}
function Deferred(n: int) -> int throws Failure {
    defer { Leaf(1) }
    Thrower(n)
}
function deferred(n: int) -> int { Deferred(n) catch (e) { Failure => 5 } }
function Convert(n: int) -> int throws baml.errors.UnknownError | string {
    Thrower(n) catch (e) { _ => { throw baml.errors.UnknownError.from<string>(e) } }
}
function converted(n: int) -> int { Convert(n) catch (e) { _ => 6 } }
function awaited(n: int) -> int {
    let c1 = spawn { Middle(n) };
    let c2 = spawn { Same() };
    let c3 = spawn { Same() };
    let x = (await c1) catch (e) { Failure => 1 };
    let y = (await c1) catch (e) { Failure => 2 };
    let z = (await c2) catch (e) { _ => 3 };
    let w = (await c3) catch (e) { _ => 4 };
    x + y + z + w
}
function boundaries(n: int) -> int {
    let j = baml.json.parse("{") catch (e) { _ => null };
    let d = (10 / (n - n)) catch (e) { baml.panics.DivisionByZero => 7 };
    let f = baml.fs.read("/definitely/not/here") catch (e) { _ => "" };
    d
}
function cancelled(n: int) -> int {
    let c = spawn { baml.sys.sleep(baml.time.Duration.from_nanoseconds(5000000000n)); 1 };
    c.cancel();
    (await c) catch (e) { baml.panics.Cancelled => 8 }
}
function crash(n: int) -> int { Top(n) }
"#;

const RETAINED: &[&str] = &["Thrower", "Middle", "Top", "Same", "Deferred", "Convert"];

#[derive(Debug, Clone, PartialEq)]
struct Raise {
    id: String,
    kind: String,
    occurrence: Option<String>,
    origin: String,
    via: Option<String>,
    candidates: Option<i64>,
    previous: Option<String>,
    fqn: String,
    result: String,
    handler: Option<String>,
    failed: i64,
    site: String,
}

fn opt(value: &Json) -> Option<String> {
    value.as_str().map(str::to_owned)
}

/// Raises of one entry function's execution, in raise order.
fn raises(index: &mut Index, entry: &str) -> Vec<Raise> {
    let result = sql(
        index,
        &format!(
            "SELECT e.raise_id, e.kind, e.occurrence_id, e.origin_state, e.origin_via,
               e.origin_candidates, e.previous_raise_id, e.fqn, e.unwind_result, e.handler_fqn,
               e.failed_calls, e.site_start, e.site_end, e.site_state
             FROM error_raises e
             JOIN threads t ON t.thread_id = e.thread_id
             JOIN executions x ON x.execution_id = t.execution_id
             WHERE x.entry_fqn = 'user.{entry}' ORDER BY e.raised_ticks"
        ),
    );
    result
        .rows
        .iter()
        .map(|row| {
            assert_eq!(row[13], "resolved", "{row:?}");
            let (a, b) = (row[11].as_u64().unwrap(), row[12].as_u64().unwrap());
            Raise {
                id: row[0].as_str().unwrap().to_owned(),
                kind: row[1].as_str().unwrap().to_owned(),
                occurrence: opt(&row[2]),
                origin: row[3].as_str().unwrap().to_owned(),
                via: opt(&row[4]),
                candidates: row[5].as_i64(),
                previous: opt(&row[6]),
                fqn: row[7].as_str().unwrap().to_owned(),
                result: row[8].as_str().unwrap().to_owned(),
                handler: opt(&row[9]),
                failed: row[10].as_i64().unwrap(),
                site: SOURCE[usize::try_from(a).unwrap()..usize::try_from(b).unwrap()].to_owned(),
            }
        })
        .collect()
}

fn fresh(raise: &Raise) -> bool {
    raise.origin == "fresh" && raise.occurrence.as_deref() == Some(raise.id.as_str())
}

fn proven(raise: &Raise, origin: &Raise, via: &str) -> bool {
    raise.origin == "proven"
        && raise.occurrence.as_deref() == Some(origin.id.as_str())
        && raise.via.as_deref() == Some(via)
}

async fn record(project: &Path) -> Index {
    let entries = [
        "caught_here",
        "propagated",
        "equal_values",
        "rethrown",
        "nested",
        "while_handling",
        "deferred",
        "converted",
        "awaited",
        "boundaries",
        "cancelled",
        "crash",
    ];
    let calls: Vec<(&str, i64)> = entries.iter().map(|name| (*name, 3)).collect();
    let results = record_program(project, SOURCE, RETAINED, &calls).await;
    let expected = [0, 0, 3, 3, 3, 4, 5, 6, 10, 7, 8];
    for (result, value) in results.iter().zip(expected) {
        assert_eq!(result, &Ok(BexExternalValue::Int(value)));
    }
    assert!(results[11].is_err(), "crash escapes");
    index(project)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn raises_are_distinct_and_only_proven_origins_are_linked() {
    let project = tempfile::tempdir().unwrap();
    let mut index = record(project.path()).await;

    // A throw caught in the function that threw it: a raise, no failed call.
    let here = raises(&mut index, "caught_here");
    assert_eq!(here.len(), 1);
    assert!(fresh(&here[0]));
    assert_eq!(here[0].site, "throw Failure { code: n }");
    assert_eq!(
        (
            here[0].result.as_str(),
            here[0].handler.as_deref(),
            here[0].failed
        ),
        ("caught", Some("user.caught_here"), 0)
    );

    // Propagation through three retained frames fails exactly those calls.
    let through = raises(&mut index, "propagated");
    assert_eq!(through.len(), 1);
    assert!(fresh(&through[0]));
    assert_eq!(through[0].fqn, "user.Thrower");
    assert_eq!(through[0].failed, 3);
    let failed = sql(
        &mut index,
        &format!(
            "SELECT fqn, role FROM error_call_links WHERE raise_id = '{}' ORDER BY fqn, role",
            through[0].id
        ),
    );
    let failed: Vec<(String, String)> = failed
        .rows
        .iter()
        .map(|r| (r[0].as_str().unwrap().into(), r[1].as_str().unwrap().into()))
        .collect();
    assert_eq!(
        failed,
        [
            ("user.Middle".into(), "unwound".into()),
            ("user.Thrower".into(), "raise_frame".into()),
            ("user.Thrower".into(), "unwound".into()),
            ("user.Top".into(), "unwound".into()),
        ]
    );

    // Two throws of the same interned value are two occurrences.
    let equal = raises(&mut index, "equal_values");
    assert_eq!(equal.len(), 2);
    assert!(equal.iter().all(fresh));
    assert_ne!(equal[0].occurrence, equal[1].occurrence);
    assert_eq!(equal[0].site, equal[1].site);

    // A rethrow of a caught error continues its occurrence.
    let again = raises(&mut index, "rethrown");
    assert_eq!(again.len(), 2);
    assert!(fresh(&again[0]));
    assert!(proven(&again[1], &again[0], "rethrow"));
    assert_eq!(again[1].kind, "rethrow");
    assert_eq!(again[1].previous.as_deref(), Some(again[0].id.as_str()));
    assert_eq!(again[1].site, "throw e");

    // The same object thrown twice into nested handlers: rethrowing the outer
    // binding cannot be told apart from the inner one, so it stays ambiguous.
    let nested = raises(&mut index, "nested");
    assert_eq!(nested.len(), 3);
    assert!(fresh(&nested[0]) && fresh(&nested[1]));
    assert_ne!(nested[0].occurrence, nested[1].occurrence);
    assert_eq!(
        (
            nested[2].origin.as_str(),
            nested[2].candidates,
            &nested[2].occurrence
        ),
        ("ambiguous", Some(2), &None)
    );
    assert_eq!(nested[2].site, "throw a");

    // A different error thrown while handling one is a new occurrence.
    let handling = raises(&mut index, "while_handling");
    assert_eq!(handling.len(), 2);
    assert!(handling.iter().all(fresh));
    assert_eq!(handling[1].site, "throw \"second\"");

    // A defer pad re-raises the in-flight error.
    let deferred = raises(&mut index, "deferred");
    assert_eq!(deferred.len(), 2);
    assert!(fresh(&deferred[0]));
    assert!(proven(&deferred[1], &deferred[0], "rethrow"));
    assert_eq!(deferred[1].fqn, "user.Deferred");
    assert_eq!((deferred[0].failed, deferred[1].failed), (1, 1));

    // UnknownError conversion: the VM does not record where the converted
    // value came from, so even `from(e)` is not linked. It keeps the
    // source's trace as weaker evidence.
    let converted = raises(&mut index, "converted");
    assert_eq!(converted.len(), 2);
    assert!(fresh(&converted[0]));
    assert_eq!(
        (
            converted[1].origin.as_str(),
            converted[1].via.as_deref(),
            converted[1].occurrence.as_deref()
        ),
        ("unresolved", Some("normalization"), None)
    );
    assert_eq!(converted[1].kind, "throw");
    assert_eq!(
        converted[1].site,
        "throw baml.errors.UnknownError.from<string>(e)"
    );
    let evidence = sql(
        &mut index,
        &format!(
            "SELECT unresolved_reason, inherited_trace_state FROM error_raises WHERE raise_id = '{}'",
            converted[1].id
        ),
    );
    assert_eq!(
        (evidence.rows[0][0].as_str(), evidence.rows[0][1].as_str()),
        (Some("source_not_recorded"), Some("complete"))
    );

    // Native, runtime and host failures at their BAML boundary.
    let edges = raises(&mut index, "boundaries");
    let edges: Vec<(&str, &str, bool)> = edges
        .iter()
        .map(|r| (r.kind.as_str(), r.site.as_str(), fresh(r)))
        .collect();
    assert_eq!(
        edges,
        [
            ("native_boundary", "baml.json.parse(\"{\")", true),
            ("runtime", "10 / (n - n)", true),
            (
                "host_boundary",
                "baml.fs.read(\"/definitely/not/here\")",
                true
            ),
        ]
    );

    // Cancellation observed at an await is its own occurrence there.
    let cancel = raises(&mut index, "cancelled");
    assert_eq!(cancel.len(), 1);
    assert_eq!(
        (cancel[0].kind.as_str(), cancel[0].site.as_str()),
        ("await_cancelled", "await c")
    );
    assert!(fresh(&cancel[0]));

    // An escaping error: the execution's failed calls and its occurrence.
    let crash = raises(&mut index, "crash");
    assert_eq!(crash.len(), 1);
    assert_eq!(crash[0].result, "unhandled");
    let occurrence = sql(
        &mut index,
        &format!(
            "SELECT raises, failed_calls, unhandled_raises, error_state, error['code']
             FROM error_occurrences WHERE occurrence_id = '{}'",
            crash[0].id
        ),
    );
    assert_eq!(
        occurrence.rows[0],
        [
            Json::from(1),
            Json::from(3),
            Json::from(1),
            Json::from("captured"),
            Json::from(3)
        ]
    );

    // error_calls keeps its meaning: retained failed calls, now linked.
    let unlinked = sql(
        &mut index,
        "SELECT COUNT(*) FROM error_calls WHERE error_link_state != 'linked'",
    );
    assert_eq!(unlinked.rows[0][0], 0);
    let distinct = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(DISTINCT error_occurrence_id) FROM error_calls
         WHERE execution_id IN (SELECT execution_id FROM executions WHERE entry_fqn = 'user.propagated')",
    );
    assert_eq!(distinct.rows[0], [Json::from(3), Json::from(1)]);
    assert!(sql(&mut index, "SELECT * FROM issues").rows.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn awaits_keep_the_childs_origin_and_equal_children_stay_distinct() {
    let project = tempfile::tempdir().unwrap();
    let mut index = record(project.path()).await;
    let all = raises(&mut index, "awaited");
    let (children, awaits): (Vec<_>, Vec<_>) = all.iter().partition(|r| r.kind == "throw");
    assert_eq!(children.len(), 3);
    assert_eq!(awaits.len(), 4);
    assert!(children.iter().all(|r| fresh(r) && r.result == "unhandled"));
    let by_id: HashMap<&str, &Raise> = children.iter().map(|r| (r.id.as_str(), *r)).collect();
    for raise in &awaits {
        assert_eq!(raise.kind, "await");
        assert_eq!(raise.origin, "proven");
        assert_eq!(raise.via.as_deref(), Some("await"));
        // The previous hop is the raise that escaped the child.
        let child = by_id[raise.previous.as_deref().unwrap()];
        assert_eq!(raise.occurrence.as_deref(), Some(child.id.as_str()));
        // The await's own site never replaces the child's throw site.
        assert!(raise.site.starts_with("await c"), "{raise:?}");
        assert!(child.site.starts_with("throw"), "{child:?}");
    }
    // Both awaits of c1 name the same child raise; c2 and c3 threw equal
    // values and stay two occurrences.
    assert_eq!(awaits[0].occurrence, awaits[1].occurrence);
    assert_eq!(awaits[0].site, "await c1");
    assert_eq!(awaits[1].site, "await c1");
    assert_ne!(awaits[2].occurrence, awaits[3].occurrence);
    let occurrence = sql(
        &mut index,
        &format!(
            "SELECT raises, failed_calls, unhandled_raises, fqn FROM error_occurrences
             WHERE occurrence_id = '{}'",
            awaits[0].occurrence.as_deref().unwrap()
        ),
    );
    assert_eq!(
        occurrence.rows[0],
        [
            Json::from(3),
            Json::from(2),
            Json::from(1),
            Json::from("user.Thrower")
        ]
    );
}

/// Many threads unwinding deep retained stacks at once: raises, links and
/// ends interleave across producers and chunk boundaries, and each thread's
/// failures stay with that thread's raise.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn interleaved_deep_unwinds_link_each_call_to_its_own_raise() {
    let project = tempfile::tempdir().unwrap();
    let source = r#"
        class Failure { code int }
        function Dive(n: int, depth: int) -> int throws Failure {
            if (depth == 0) { throw Failure { code: n } }
            Down(n, depth - 1) + 1
        }
        function Down(n: int, depth: int) -> int throws Failure { Dive(n, depth) }
        function main(n: int) -> int {
            let children = [
                spawn { Dive(1, 60) }, spawn { Dive(2, 60) }, spawn { Dive(3, 60) },
                spawn { Dive(4, 60) }, spawn { Dive(5, 60) }, spawn { Dive(6, 60) },
                spawn { Dive(7, 60) }, spawn { Dive(8, 60) },
            ];
            let total = 0;
            for (let child in children) {
                total = total + ((await child) catch (e) { Failure => 1 });
            }
            total
        }
    "#;
    let results = record_program(project.path(), source, &["Dive", "Down"], &[("main", 1)]).await;
    assert_eq!(results, vec![Ok(BexExternalValue::Int(8))]);
    let mut index = index(project.path());
    // 121 retained frames per child. Eight children's records share
    // 256-record chunks, so unwinds straddle chunk boundaries and interleave.
    let children = sql(
        &mut index,
        "SELECT e.raise_id, e.failed_calls, e.stack_depth, e.stack_state,
           (SELECT COUNT(*) FROM error_call_links l JOIN calls c ON c.call_id = l.call_id
            WHERE l.raise_id = e.raise_id AND c.thread_id != e.thread_id) AS foreign_links
         FROM error_raises e WHERE e.kind = 'throw'",
    );
    assert_eq!(children.rows.len(), 8);
    for row in &children.rows {
        assert_eq!(row[1], 121, "{row:?}");
        assert_eq!(row[3], "truncated");
        assert!(row[2].as_i64().unwrap() > 64);
        assert_eq!(row[4], 0, "a call linked to another thread's raise");
    }
    let awaits = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(DISTINCT occurrence_id) FROM error_raises
         WHERE kind = 'await' AND origin_state = 'proven'",
    );
    assert_eq!(awaits.rows[0], [Json::from(8), Json::from(8)]);
    let failed = sql(&mut index, "SELECT COUNT(*) FROM error_calls");
    let linked = sql(
        &mut index,
        "SELECT COUNT(*) FROM error_call_links WHERE role = 'unwound'",
    );
    assert_eq!(failed.rows, linked.rows);
    let frames = sql(
        &mut index,
        "SELECT COUNT(*), COUNT(DISTINCT raise_id) FROM error_frames
         WHERE raise_id IN (SELECT raise_id FROM error_raises WHERE kind = 'throw')",
    );
    assert_eq!(frames.rows[0], [Json::from(8 * 64), Json::from(8)]);
}

/// Landing notes hold stack slots and IDs, never heap pointers: a collection
/// that moves the caught error between its landing and its rethrow keeps the
/// rethrow proven.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_collection_between_catch_and_rethrow_keeps_the_origin() {
    let project = tempfile::tempdir().unwrap();
    let source = r#"
        class Failure { code int }
        function Thrower(n: int) -> int throws Failure { throw Failure { code: n } }
        function main(n: int) -> int {
            let r = {
                Thrower(n) catch (e) {
                    Failure => {
                        let junk = [];
                        let i = 0;
                        while (i < 2000) { junk.push(Failure { code: i }); i = i + 1; }
                        baml.sys.sleep(baml.time.Duration.from_nanoseconds(300000000n));
                        throw e
                    }
                }
            } catch (outer) { Failure => outer.code };
            r
        }
    "#;
    let program = baml_db::testing::compile_source(source);
    let engine = Arc::new(
        BexEngine::new_with_telemetry_recording(
            program,
            Arc::new(sys_native::SysOps::native()),
            vec![],
            None,
            btel_clock::ClockMode::Monotonic,
            TelemetryRecording::local_files(project.path(), RecordingConfig::default()),
        )
        .unwrap(),
    );
    let run = {
        let engine = Arc::clone(&engine);
        tokio::spawn(async move {
            engine
                .call_function(
                    "main",
                    vec![BexExternalValue::Int(9)],
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    true,
                )
                .await
        })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    let before = engine.heap_stats().runtime_objects;
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;
    assert!(engine.heap_stats().runtime_objects <= before);
    assert_eq!(run.await.unwrap().unwrap(), BexExternalValue::Int(9));
    tokio::time::timeout(Duration::from_secs(30), engine.shutdown())
        .await
        .unwrap();
    let mut index = index(project.path());
    let rows = sql(
        &mut index,
        "SELECT kind, origin_state, occurrence_id, raise_id FROM error_raises ORDER BY raised_ticks",
    );
    assert_eq!(rows.rows.len(), 2);
    assert_eq!(rows.rows[1][0], "rethrow");
    assert_eq!(rows.rows[1][1], "proven");
    assert_eq!(rows.rows[1][2], rows.rows[0][3]);
}
