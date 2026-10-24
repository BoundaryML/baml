//! Thread-locality classification (`baml_compiler2_mir::escape`).
//!
//! Each test lowers one function and checks, per named local, whether every
//! object it can hold is thread-local. A wrong `true` is a data race once a
//! backend drops the lock, so most cases pin down a way an object leaves.

use baml_compiler2_hir::item_data::{file_functions, function_data};
use baml_compiler2_mir::{MirFunctionKind, OptLevel, escape, lower_function};
use baml_db::testing::assert_no_diagnostic_errors;

use super::make_db;
use crate::engine::TestDbExt;

/// `(local name, thread-local)` for each named local of `function`, in
/// declaration order. A shadowed name reports its first declaration.
fn classify(source: &str, function: &str) -> Vec<(String, bool)> {
    let mut db = make_db();
    let file = db.file("test.baml", source);
    assert_no_diagnostic_errors(&db);
    let loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let mir = lower_function(&db, loc, OptLevel::Two)
        .as_ref()
        .unwrap_or_else(|error| panic!("MIR lowering failed: {error}"));
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("`{function}` must lower to bytecode")
    };
    let facts = escape::analyze(&db, body, mir.arity, OptLevel::Two);
    let mut seen = std::collections::HashSet::new();
    body.locals
        .iter()
        .enumerate()
        .filter_map(|(index, decl)| {
            let name = decl.name.as_ref()?.to_string();
            seen.insert(name.clone()).then(|| {
                (
                    name,
                    facts.local_is_thread_local(baml_compiler2_mir::Local(index)),
                )
            })
        })
        .collect()
}

#[track_caller]
fn assert_locality(source: &str, function: &str, expected: &[(&str, bool)]) {
    let actual = classify(source, function);
    for (name, local) in expected {
        let found = actual
            .iter()
            .find(|(actual, _)| actual == name)
            .unwrap_or_else(|| panic!("no local `{name}` in {actual:?}"));
        assert_eq!(
            found.1, *local,
            "`{name}` thread-local should be {local}; all: {actual:?}"
        );
    }
}

#[test]
fn fresh_array_built_and_read_is_thread_local() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1, 2];
  xs.push(3);
  let total = 0;
  for (let x in xs) { total += x; }
  total + xs[0] + xs.length()
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn returning_an_object_keeps_it_thread_local() {
    assert_locality(
        r#"
function main() -> int[] {
  let xs = [1];
  xs[0] = 2;
  xs
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn parameters_are_shared() {
    assert_locality(
        r#"
function main(ys: int[]) -> int {
  let zs = ys;
  zs[0]
}
"#,
        "main",
        &[("ys", false), ("zs", false)],
    );
}

/// Leaks its argument: `reverse` is a builtin the analysis does not know.
const LEAK: &str = "function keep(xs: int[]) -> int { xs.reverse().length() }";

#[test]
fn passing_to_a_leaking_call_shares_every_alias() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function main() -> int {{
  let xs = [1];
  let alias = xs;
  keep(alias)
}}
"#
        ),
        "main",
        &[("xs", false), ("alias", false)],
    );
}

#[test]
fn storing_into_a_shared_object_shares_the_value() {
    assert_locality(
        r#"
class Box {
  items: int[]
}
function main(b: Box) -> int {
  let xs = [1];
  b.items = xs;
  xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn storing_into_a_fresh_object_keeps_both_thread_local() {
    assert_locality(
        r#"
class Box {
  items: int[]
}
function main() -> int {
  let xs = [1];
  let b = Box { items: xs };
  b.items[0] + xs[0]
}
"#,
        "main",
        &[("xs", true), ("b", true)],
    );
}

#[test]
fn leaking_a_loaded_element_shares_what_was_stored() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function main() -> int {{
  let inner = [1];
  let outer = [inner];
  let again = outer[0];
  keep(again)
}}
"#
        ),
        "main",
        &[("inner", false), ("again", false), ("outer", true)],
    );
}

#[test]
fn pushing_onto_a_shared_array_shares_the_item() {
    assert_locality(
        r#"
function main(out: int[][]) -> int {
  let xs = [1];
  out.push(xs);
  xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn method_receivers_follow_the_method_summary() {
    assert_locality(
        r#"
class Point {
  x: int
  function get(self) -> int { self.x }
}
function main() -> int {
  let p = Point { x: 1 };
  let q = Point { x: 2 };
  q.x = 3;
  p.get() + q.x
}
"#,
        "main",
        &[("p", true), ("q", true)],
    );
}

#[test]
fn closures_share_what_they_capture() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1];
  let f = () -> int { xs[0] };
  f()
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn spawned_work_shares_its_plan() {
    assert_locality(
        r#"
function first(xs: int[]) -> int { xs[0] }
function main() -> int {
  let xs = [1];
  let ys = [2];
  let f = spawn { first(xs) };
  ys[0] + await f
}
"#,
        "main",
        &[("xs", false), ("ys", true)],
    );
}

#[test]
fn thrown_values_are_shared() {
    assert_locality(
        r#"
function main() -> int {
  let xs = [1];
  if (xs[0] > 0) { throw xs; }
  0
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn string_keyed_maps_stay_thread_local() {
    assert_locality(
        r#"
function main() -> int {
  let m = {"a": 1};
  m["b"] = 2;
  if (m.has("a")) { m["a"] } else { 0 }
}
"#,
        "main",
        &[("m", true)],
    );
}

#[test]
fn map_values_that_leak_share_the_map_contents() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function main() -> int {{
  let xs = [1];
  let m = {{"a": xs}};
  keep(m["a"])
}}
"#
        ),
        "main",
        &[("xs", false), ("m", true)],
    );
}

#[test]
fn an_iterator_that_leaks_shares_its_array() {
    assert_locality(
        r#"
function drain<T>(it: T) -> int { [it].reverse().length() }
function main() -> int {
  let xs = [1, 2];
  drain(xs.iter())
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn a_callee_that_only_reads_keeps_arguments_thread_local() {
    assert_locality(
        r#"
function first(xs: int[]) -> int { xs[0] + xs.length() }
function main() -> int {
  let xs = [1];
  first(xs) + xs[0]
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn a_fresh_return_value_is_thread_local_in_the_caller() {
    assert_locality(
        r#"
function make(n: int) -> int[] { [n, n + 1] }
function main() -> int {
  let xs = make(1);
  xs[0] = 3;
  xs[1]
}
"#,
        "main",
        &[("xs", true)],
    );
}

#[test]
fn a_returned_argument_aliases_it_in_the_caller() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function same(xs: int[]) -> int[] {{ xs }}
function main() -> int {{
  let xs = [1];
  let ys = same(xs);
  keep(ys)
}}
"#
        ),
        "main",
        &[("xs", false), ("ys", false)],
    );
}

#[test]
fn a_callee_that_stores_one_argument_in_another_links_them() {
    assert_locality(
        r#"
function put(out: int[][], xs: int[]) -> int { out.push(xs) }
function main(shared: int[][]) -> int {
  let xs = [1];
  let fresh: int[][] = [];
  let ys = [2];
  put(shared, xs);
  put(fresh, ys);
  xs[0] + ys[0]
}
"#,
        "main",
        &[("xs", false), ("fresh", true), ("ys", true)],
    );
}

#[test]
fn recursive_callees_are_opaque() {
    assert_locality(
        r#"
function walk(xs: int[], n: int) -> int {
  if (n == 0) { xs[0] } else { walk(xs, n - 1) }
}
function main() -> int {
  let xs = [1];
  walk(xs, 3)
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn leaks_compose_through_calls() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function forward(xs: int[]) -> int {{ keep(xs) }}
function main() -> int {{
  let xs = [1];
  forward(xs)
}}
"#
        ),
        "main",
        &[("xs", false)],
    );
}

/// Coverage of the classification over real programs: what share of
/// container and field accesses are thread-local, and why the rest are not.
///
/// `cargo test -p baml_tests --lib escape_coverage -- --ignored --nocapture`
#[test]
#[ignore = "report, not a check"]
fn escape_coverage() {
    use std::collections::BTreeMap;

    use baml_compiler2_mir::{MirFunction, escape::AccessKind};

    #[derive(Default)]
    struct Tally {
        sites: BTreeMap<AccessKind, (usize, usize)>,
        reasons: BTreeMap<String, usize>,
    }

    impl Tally {
        fn function(&mut self, db: &baml_db::ProjectDatabase, mir: &MirFunction<'_>) {
            if let MirFunctionKind::Bytecode(body) = &mir.kind {
                for access in escape::coverage(db, body, mir.arity, OptLevel::Two) {
                    let (total, local) = self.sites.entry(access.kind).or_default();
                    *total += 1;
                    *local += usize::from(access.thread_local);
                    if let Some(reason) = access.shared_because {
                        *self.reasons.entry(reason).or_default() += 1;
                    }
                }
            }
            for lambda in &mir.lambdas {
                self.function(db, lambda);
            }
        }

        fn files(&mut self, db: &baml_db::ProjectDatabase, files: &[baml_base::SourceFile]) {
            for &file in files {
                for &loc in file_functions(db, file) {
                    if let Ok(mir) = lower_function(db, loc, OptLevel::Two) {
                        self.function(db, mir);
                    }
                }
            }
        }

        fn totals(&self) -> (usize, usize) {
            self.sites
                .values()
                .fold((0, 0), |(t, l), (total, local)| (t + total, l + local))
        }

        fn print(&self, title: &str) {
            let (total, local) = self.totals();
            println!(
                "\n## {title}: {local}/{total} accesses thread-local ({}%)",
                percent(local, total)
            );
            for (kind, (total, local)) in &self.sites {
                println!(
                    "  {kind:<16?} {local:>6}/{total:<6} {:>3}%",
                    percent(*local, *total)
                );
            }
            let mut reasons: Vec<_> = self.reasons.iter().collect();
            reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            println!("  shared because:");
            for (reason, count) in reasons.iter().take(25) {
                println!("  {count:>6}  {reason}");
            }
        }
    }

    fn percent(part: usize, whole: usize) -> usize {
        (part * 100).checked_div(whole).unwrap_or(0)
    }

    fn baml_files(dir: &std::path::Path) -> Vec<(String, String)> {
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "baml"))
            .map(|path| {
                (
                    path.file_name().unwrap().to_string_lossy().into_owned(),
                    std::fs::read_to_string(&path).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let workloads = manifest.join("../../tools/speedtest/workloads");
    let mut all = Tally::default();
    let mut per_workload = Vec::new();
    let mut groups: Vec<_> = std::fs::read_dir(&workloads)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .collect();
    groups.sort();
    for group in groups {
        let mut dirs: Vec<_> = std::fs::read_dir(&group)
            .unwrap()
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .collect();
        dirs.sort();
        for dir in dirs {
            let sources = baml_files(&dir);
            if sources.is_empty() {
                continue;
            }
            let mut db = make_db();
            let files: Vec<_> = sources
                .iter()
                .map(|(name, text)| db.file(name, text))
                .collect();
            let mut tally = Tally::default();
            tally.files(&db, &files);
            all.files(&db, &files);
            let name = format!(
                "{}/{}",
                group.file_name().unwrap().to_string_lossy(),
                dir.file_name().unwrap().to_string_lossy()
            );
            per_workload.push((name, tally.totals()));
        }
    }
    all.print("speedtest workloads");
    println!("  per workload:");
    for (name, (total, local)) in &per_workload {
        println!(
            "  {:>3}%  {local:>4}/{total:<4}  {name}",
            percent(*local, *total)
        );
    }

    let mut db = make_db();
    let probes: Vec<_> = baml_files(&manifest.join("rust_diff/baml_src"))
        .iter()
        .map(|(name, text)| db.file(name, text))
        .collect();
    let mut tally = Tally::default();
    tally.files(&db, &probes);
    tally.print("rust_differential probes");

    let roots = baml_compiler2_hir::package::lang_roots(&db);
    let mut tally = Tally::default();
    for package in baml_base::LangPackage::ALL {
        if let Some(root) = roots.get(package) {
            tally.files(&db, root.files(&db));
        }
    }
    tally.print("standard library");
}

#[test]
fn immutable_values_do_not_share_their_neighbours() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function shout(s: string) -> int {{ [s].reverse().length() }}
class Named {{
  name: string
  items: int[]
}}
function main() -> int {{
  let n = Named {{ name: "a", items: [1] }};
  let name = n.name;
  shout(name) + shout(n.name) + n.items[0]
}}
"#
        ),
        "main",
        &[("n", true)],
    );
}

#[test]
fn builtin_calls_on_a_field_receiver_are_seen_through() {
    assert_locality(
        r#"
class Queue {
  items: int[]
}
function main() -> int {
  let q = Queue { items: [] };
  q.items.push(1);
  q.items.length() + q.items[0]
}
"#,
        "main",
        &[("q", true)],
    );
}

#[test]
fn map_keys_and_values_copy_the_entries() {
    assert_locality(
        &format!(
            r#"
{LEAK}
function main() -> int {{
  let xs = [1];
  let m = {{"a": xs}};
  let ks = m.keys();
  let vs = m.values();
  keep(vs[0]) + ks.length()
}}
"#
        ),
        "main",
        &[("xs", false), ("m", true), ("ks", true), ("vs", true)],
    );
}

#[test]
fn class_keyed_maps_stay_opaque() {
    assert_locality(
        r#"
class Key {
  k: int
}
function main(key: Key) -> int {
  let m: map<Key, int[]> = {};
  let xs = [1];
  m[key] = xs;
  xs[0]
}
"#,
        "main",
        &[("xs", false), ("m", false)],
    );
}

#[test]
fn comparing_with_null_runs_no_user_code() {
    assert_locality(
        r#"
class Node {
  next: Node?
  value: int
}
function main() -> int {
  let n = Node { next: null, value: 1 };
  let m = Node { next: null, value: 2 };
  let o = Node { next: null, value: 3 };
  if (o.next == null && n != m) { n.value + o.value } else { 0 }
}
"#,
        "main",
        &[("n", false), ("m", false), ("o", true)],
    );
}

#[test]
fn bytecode_takes_the_lock_free_paths_for_thread_local_arrays() {
    let program = crate::engine::compile_source_with_opt(
        r#"
function build(n: int) -> int {
  let xs: int[] = [];
  let i = 0;
  while (i < n) { xs.push(i); i += 1; }
  let total = 0;
  for (let x in xs) { total += x; }
  total + xs[0] + xs.length()
}
function walk(xs: int[]) -> int {
  let total = 0;
  for (let x in xs) { total += x; }
  xs.push(total);
  total + xs[0]
}
"#,
        OptLevel::Two,
    );
    let bytecode = crate::engine::display_user_functions(&program);
    let body = |name: &str| {
        bytecode
            .split(&format!("function {name}("))
            .nth(1)
            .and_then(|rest| rest.split("\nfunction ").next())
            .unwrap_or_else(|| panic!("no `{name}` in:\n{bytecode}"))
    };
    let build = body("build");
    for local in [
        "call_local baml.Array.push",
        "array_iter_next_local",
        "load_array_element_local",
        "container_len_local",
    ] {
        assert!(build.contains(local), "`build` lacks {local}:\n{build}");
    }
    let walk = body("walk");
    assert!(
        !walk.contains("_local"),
        "a parameter's array keeps its locks:\n{walk}"
    );
    assert!(
        walk.contains("array_iter_next "),
        "`walk` still walks by index:\n{walk}"
    );
}
