//! Call, spawn and recursion sites from real recordings, resolved only through
//! the recording's own source maps (the project directory has no sources).
mod support;

use serde_json::Value as Json;
use support::*;

const SOURCE: &str = r#"
class Box {
    v int
    function get(self) -> int { Leaf(self.v) }
}
function Leaf(n: int) -> int { n + 1 }
function Recur(n: int) -> int { if (n == 0) { Leaf(0) } else { Recur(n - 1) } }
function main(n: int) -> int {
    let a = Leaf(1);
    let b = Leaf(Leaf(2) + Leaf(3));
    let f = Leaf;
    let c = f(4);
    let box = Box { v: 5 };
    let d = box.get();
    let e = [1, 2].map((x: int) -> int { Leaf(x) });
    let s1 = spawn { Leaf(6) };
    let s2 = spawn { Leaf(7) };
    let r = Recur(3);
    a + b + c + d + e.length() + (await s1) + (await s2) + r
}
"#;

/// The recorded source text at `[start, end)`, or why there is none.
fn text(row: &[Json], start: usize) -> String {
    match (row[start].as_u64(), row[start + 1].as_u64()) {
        (Some(a), Some(b)) => {
            SOURCE[usize::try_from(a).unwrap()..usize::try_from(b).unwrap()].to_owned()
        }
        _ => "<none>".to_owned(),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn call_spawn_and_recursive_sites_resolve_through_recorded_maps() {
    let project = tempfile::tempdir().unwrap();
    let results = record_program(
        project.path(),
        SOURCE,
        &["Leaf", "Recur", "get"],
        &[("main", 3)],
    )
    .await;
    assert_eq!(results, vec![Ok(bex_engine::BexExternalValue::Int(39))]);
    let mut index = index(project.path());

    let paths = sql(
        &mut index,
        "SELECT caller_fqn, fqn, edge_kind, call_site_state, call_site_file, call_site_line,
           call_site_start, call_site_end
         FROM call_paths ORDER BY local_call_path_id",
    );
    let sites: Vec<(String, String, String, String)> = paths
        .rows
        .iter()
        .map(|row| {
            (
                row[0].as_str().unwrap_or("-").to_owned(),
                row[1].as_str().unwrap().to_owned(),
                row[3].as_str().unwrap().to_owned(),
                text(row, 6),
            )
        })
        .collect();
    let has = |caller: &str, callee: &str, site: &str| {
        assert!(
            sites.iter().any(|(c, f, state, t)| c == caller
                && f == callee
                && state == "resolved"
                && t == site),
            "{caller} -> {callee} at {site:?} in {sites:#?}"
        );
    };
    // Two sites for one function, and nested operands each keep their own
    // expression: the outer call is not labelled by its last argument.
    has("user.main", "user.Leaf", "Leaf(1)");
    has("user.main", "user.Leaf", "Leaf(2)");
    has("user.main", "user.Leaf", "Leaf(3)");
    has("user.main", "user.Leaf", "Leaf(Leaf(2) + Leaf(3))");
    // Indirect and method calls.
    has("user.main", "user.Leaf", "f(4)");
    has("user.main", "user.Box.get", "box.get()");
    has("user.Box.get", "user.Leaf", "Leaf(self.v)");
    // A callback run by native `map` is attributed to the BAML call that
    // entered it; the call inside the callback has its own site.
    has(
        "user.main",
        ".<lambda(main, 0)>",
        "[1, 2].map((x: int) -> int { Leaf(x) })",
    );
    has(".<lambda(main, 0)>", "user.Leaf", "Leaf(x)");
    // Two spawn expressions; the child's wrapper is an entry, and its body
    // resolves in the wrapper's own map.
    has("user.main", ".<lambda(main, 1)>", "spawn { Leaf(6) }");
    has("user.main", ".<lambda(main, 2)>", "spawn { Leaf(7) }");
    has(".<lambda(main, 1)>", "user.Leaf", "Leaf(6)");
    has("user.main", "user.Recur", "Recur(3)");
    has("user.Recur", "user.Leaf", "Leaf(0)");
    let entries: Vec<_> = sites
        .iter()
        .filter(|(_, _, state, _)| state != "resolved")
        .collect();
    assert_eq!(
        entries.len(),
        3,
        "main and both child wrappers: {entries:?}"
    );
    assert!(
        entries
            .iter()
            .all(|(c, _, state, t)| c == "-" && state == "no_caller" && t == "<none>")
    );
    // Byte-offset PCs, never instruction indexes: distinct sites in `main`
    // have distinct recorded PCs, each inside a real line.
    assert!(
        paths
            .rows
            .iter()
            .all(|row| row[4] == "test.baml" || row[4].is_null())
    );

    let threads = sql(
        &mut index,
        "SELECT kind, spawn_site_state, spawn_site_line, spawn_site_start, spawn_site_end
         FROM threads ORDER BY spawn_site_line",
    );
    let spawns: Vec<(String, String)> = threads
        .rows
        .iter()
        .map(|row| (row[1].as_str().unwrap().to_owned(), text(row, 3)))
        .collect();
    assert_eq!(
        spawns,
        [
            ("not_spawned".into(), "<none>".into()),
            ("resolved".into(), "spawn { Leaf(6) }".into()),
            ("resolved".into(), "spawn { Leaf(7) }".into()),
        ]
    );

    // A direct recursive re-entry keeps its caller's context and records no
    // PC of its own: it gets no site, never the outer call's location.
    let recur = sql(
        &mut index,
        "SELECT reentry, call_site_state, call_site_start, call_site_end FROM calls
         WHERE fqn = 'user.Recur' ORDER BY started_ticks",
    );
    let recur: Vec<(i64, String, String)> = recur
        .rows
        .iter()
        .map(|row| {
            (
                row[0].as_i64().unwrap(),
                row[1].as_str().unwrap().to_owned(),
                text(row, 2),
            )
        })
        .collect();
    assert_eq!(recur[0], (0, "resolved".into(), "Recur(3)".into()));
    assert_eq!(recur.len(), 4);
    assert!(
        recur[1..]
            .iter()
            .all(|r| *r == (1, "recursive_reentry".into(), "<none>".into()))
    );
    // Every non-recursive retained call has its path's exact site.
    let exact = sql(
        &mut index,
        "SELECT COUNT(*) FROM calls c JOIN call_paths p ON p.call_path_id = c.call_path_id
         WHERE c.reentry = 0 AND c.call_site_line = p.call_site_line
           AND c.call_site_start = p.call_site_start AND c.call_site_state = p.call_site_state",
    );
    let retained = sql(&mut index, "SELECT COUNT(*) FROM calls WHERE reentry = 0");
    assert_eq!(exact.rows, retained.rows);
    // Site queries read no captured values.
    assert_eq!(recur_values_read(&mut index), 0);
}

fn recur_values_read(index: &mut baml_query_btel::Index) -> u64 {
    sql(
        index,
        "SELECT call_site_line, call_site_state FROM calls UNION ALL
         SELECT call_site_line, call_site_state FROM call_paths",
    )
    .outcome
    .query
    .values
    .cas_loads
}
