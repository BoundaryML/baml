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

#[test]
fn audit_unrelated_iter_method_can_spawn_with_receiver() {
    assert_locality(
        r#"
interface Launchable {
    function iter(self) -> int throws never;
}
implements Launchable for int[] {
    function iter(self) -> int {
        let child = spawn { self.push(1) };
        0
    }
}
function main() -> int {
    let xs = [1];
    let result = Launchable.iter(xs);
    xs.push(2);
    result
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn reassigned_iterator_parameter_calls_escape() {
    let source = r#"
function consume(it: baml.iter.Iterator<Item = int, Error = never>) -> int {
    let before = it.next();
    let xs = [1];
    it = xs.iter();
    let after = it.next();
    0
}
"#;
    let (locals, _) = needs(source, "consume");
    // The incoming iterator may run user code and retain its receiver. Its
    // single body assignment must not make either next call an array step.
    for name in ["it", "xs"] {
        assert_eq!(
            locals.iter().find(|(n, _)| n == name).unwrap().1,
            escape::NEVER,
            "{name} must escape through the opaque iterator calls",
        );
    }
}

/// `(local name, thread-local)` for each named local of `function`, in
/// declaration order. A shadowed name reports its first declaration.
fn classify(source: &str, function: &str) -> Vec<(String, bool)> {
    let mut db = make_db();
    let file = db.file("test.baml", source);
    classify_file(&db, file, function)
}

fn classify_file(
    db: &baml_db::ProjectDatabase,
    file: baml_base::SourceFile,
    function: &str,
) -> Vec<(String, bool)> {
    classify_file_at_opt(db, file, function, OptLevel::Two)
}

fn classify_file_at_opt(
    db: &baml_db::ProjectDatabase,
    file: baml_base::SourceFile,
    function: &str,
    opt: OptLevel,
) -> Vec<(String, bool)> {
    assert_no_diagnostic_errors(db);
    let loc = *file_functions(db, file)
        .iter()
        .find(|&&loc| function_data(db, loc).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let mir = lower_function(db, loc, opt)
        .as_ref()
        .unwrap_or_else(|error| panic!("MIR lowering failed: {error}"));
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("`{function}` must lower to bytecode")
    };
    let facts = escape::analyze(db, file.source_root(db), body, mir.arity, opt);
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

/// For each named local of `function`, the parameters it is thread-local
/// given (`EscapeFacts::object_needs`); and for each call to a source
/// function, in block order, its arguments' needs.
fn needs(source: &str, function: &str) -> (Vec<(String, u64)>, Vec<Vec<u64>>) {
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
    let facts = escape::analyze(&db, file.source_root(&db), body, mir.arity, OptLevel::Two);
    let mut seen = std::collections::HashSet::new();
    let locals = body
        .locals
        .iter()
        .enumerate()
        .filter_map(|(index, decl)| {
            let name = decl.name.as_ref()?.to_string();
            seen.insert(name.clone()).then(|| {
                let place = baml_compiler2_mir::Place::Local(baml_compiler2_mir::Local(index));
                (name, facts.object_needs(&place))
            })
        })
        .collect();
    let calls = (0..body.blocks.len())
        .filter_map(|block| facts.call_arg_needs(baml_compiler2_mir::BlockId(block)))
        .map(<[u64]>::to_vec)
        .collect();
    (locals, calls)
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
fn source_dependency_calls_are_opaque() {
    let mut db = make_db();
    db.dependency("dep");
    db.file(
        "<builtin>/dep/lib.baml",
        r#"
function fresh() -> int[] { [1] }
function read(xs: int[]) -> int { xs[0] }
"#,
    );
    let file = db.file(
        "test.baml",
        r#"
function recurse(xs: int[], n: int) -> int {
    if (n > 0) { recurse(xs, n - 1) } else { dep.read(xs) }
}
function main() -> int {
    let made = dep.fresh();
    let passed = [2];
    let recursive = [3];
    dep.read(passed) + recurse(recursive, 1) + made[0] + passed[0] + recursive[0]
}
"#,
    );
    let actual = classify_file(&db, file, "main");
    for name in ["made", "passed", "recursive"] {
        assert!(
            actual
                .iter()
                .any(|(local, is_local)| local == name && !is_local),
            "dependency source must not supply escape summaries: {actual:?}"
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
/// A callee that hands its argument to another task: a leak however precise
/// the analysis of builtins becomes.
const LEAK: &str =
    "function keep(xs: int[]) -> int { let child = spawn { xs.length() }; await child }";

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
function drain<T>(it: T) -> int { let child = spawn { [it].length() }; await child }
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
fn a_recursive_callee_that_only_reads_keeps_arguments_thread_local() {
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
        &[("xs", true)],
    );
}

#[test]
fn a_recursive_callee_that_spawns_at_its_base_case_shares_arguments() {
    assert_locality(
        r#"
function walk(xs: int[], n: int) -> int {
  if (n == 0) {
    let child = spawn { xs.length() };
    await child
  } else {
    walk(xs, n - 1)
  }
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
fn mutually_recursive_callees_share_what_either_leaks() {
    assert_locality(
        r#"
function even(xs: int[], n: int) -> int {
  if (n == 0) { xs[0] } else { odd(xs, n - 1) }
}
function odd(xs: int[], n: int) -> int {
  if (n == 0) {
    let child = spawn { xs.length() };
    await child
  } else {
    even(xs, n - 1)
  }
}
function main() -> int {
  let xs = [1];
  let ys = [2];
  even(xs, 3) + ys[0]
}
"#,
        "main",
        &[("xs", false), ("ys", true)],
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

    use baml_compiler2_hir::loc::{DeclRef, FunctionLoc};
    use baml_compiler2_mir::{
        Constant, MirFunction, Operand, Terminator,
        escape::{AccessKind, NEVER},
    };

    #[derive(Default)]
    struct Tally {
        sites: BTreeMap<AccessKind, (usize, usize)>,
        functions: Vec<(String, usize, usize)>,
        reasons: BTreeMap<String, usize>,
        /// Shared today, thread-local given some parameters.
        conditional: usize,
        /// Of those, accesses some in-package call chain can make local.
        realizable: usize,
        /// Of the rest: in a function with no static caller in scope.
        no_caller: usize,
    }

    /// A body's accesses and its static calls, for the locality-mask estimate.
    struct Body<'db> {
        /// `None` for a lambda: its callers are indirect, so its mask is 0.
        loc: Option<FunctionLoc<'db>>,
        name: String,
        accesses: Vec<escape::Access>,
        /// Per static call: the callee and each value argument's needs.
        calls: Vec<(FunctionLoc<'db>, Vec<u64>)>,
    }

    impl Tally {
        fn collect<'db>(
            db: &'db baml_db::ProjectDatabase,
            loc: Option<FunctionLoc<'db>>,
            root: baml_base::SourceRoot,
            mir: &MirFunction<'db>,
            name: String,
            out: &mut Vec<Body<'db>>,
        ) {
            if let MirFunctionKind::Bytecode(body) = &mir.kind {
                let facts = escape::analyze(db, root, body, mir.arity, OptLevel::Two);
                let calls = body
                    .blocks
                    .iter()
                    .filter_map(|block| match &block.terminator {
                        Some(Terminator::Call {
                            has_trace: false,
                            argument_layout,
                            callee:
                                Operand::Constant(
                                    Constant::Function(DeclRef::Source(callee))
                                    | Constant::GenericFunction {
                                        func: DeclRef::Source(callee),
                                        ..
                                    },
                                ),
                            args,
                            ntypeargs,
                            ..
                        }) if argument_layout
                            .as_ref()
                            .is_none_or(|layout| layout.0.iter().all(Option::is_none)) =>
                        {
                            let values = args.get(*ntypeargs..).unwrap_or_default();
                            Some((
                                *callee,
                                values.iter().map(|arg| facts.operand_needs(arg)).collect(),
                            ))
                        }
                        _ => None,
                    })
                    .collect();
                out.push(Body {
                    loc,
                    name: name.clone(),
                    accesses: escape::coverage(db, root, body, mir.arity, OptLevel::Two),
                    calls,
                });
            }
            for (index, lambda) in mir.lambdas.iter().enumerate() {
                Self::collect(
                    db,
                    None,
                    root,
                    lambda,
                    format!("{name}::<lambda {index}>"),
                    out,
                );
            }
        }

        fn files(
            &mut self,
            db: &baml_db::ProjectDatabase,
            files: &[baml_base::SourceFile],
            label: &str,
        ) {
            let mut bodies = Vec::new();
            for &file in files {
                for &loc in file_functions(db, file) {
                    if let Ok(mir) = lower_function(db, loc, OptLevel::Two) {
                        let name =
                            baml_compiler2_mir::function_display_name(db, DeclRef::Source(loc));
                        Self::collect(db, Some(loc), file.source_root(db), mir, name, &mut bodies);
                    }
                }
            }
            // The parameters some in-package call chain passes a thread-local
            // argument: least fixed point, seeded by unconditional arguments.
            let mut passed: std::collections::HashMap<FunctionLoc<'_>, u64> =
                std::collections::HashMap::new();
            loop {
                let mut changed = false;
                for body in &bodies {
                    let have = body
                        .loc
                        .map_or(0, |loc| passed.get(&loc).copied().unwrap_or(0));
                    for (callee, args) in &body.calls {
                        for (i, &needs) in args.iter().enumerate().take(64) {
                            if needs != NEVER && needs & !have == 0 {
                                let bits = passed.entry(*callee).or_default();
                                if *bits & (1 << i) == 0 {
                                    *bits |= 1 << i;
                                    changed = true;
                                }
                            }
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            let called: std::collections::HashSet<FunctionLoc<'_>> = bodies
                .iter()
                .flat_map(|body| body.calls.iter().map(|(callee, _)| *callee))
                .collect();
            for body in bodies {
                let total = body.accesses.len();
                let local = body.accesses.iter().filter(|a| a.thread_local).count();
                let name = format!("{label}::{}", body.name);
                if total > 0 {
                    self.functions.push((name, local, total));
                }
                let have = body
                    .loc
                    .map_or(0, |loc| passed.get(&loc).copied().unwrap_or(0));
                let has_caller = body.loc.is_some_and(|loc| called.contains(&loc));
                for access in body.accesses {
                    let (total, local) = self.sites.entry(access.kind).or_default();
                    *total += 1;
                    *local += usize::from(access.thread_local);
                    if !access.thread_local && access.needs != NEVER {
                        self.conditional += 1;
                        let realizable = access.needs & !have == 0;
                        self.realizable += usize::from(realizable);
                        self.no_caller += usize::from(!realizable && !has_caller);
                    }
                    if let Some(reason) = access.shared_because {
                        *self.reasons.entry(reason).or_default() += 1;
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
            println!(
                "  locality mask: +{} conditional, +{} realizable ({} of the rest have no caller) -> {}/{total} ({}%)",
                self.conditional,
                self.realizable,
                self.no_caller,
                local + self.realizable,
                percent(local + self.realizable, total)
            );
            for (kind, (total, local)) in &self.sites {
                println!(
                    "  {kind:<16?} {local:>6}/{total:<6} {:>3}%",
                    percent(*local, *total)
                );
            }
            let mut reasons: Vec<_> = self.reasons.iter().collect();
            reasons.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let mut functions: Vec<_> = self.functions.iter().collect();
            functions.sort_by(|a, b| (a.1 * b.2).cmp(&(b.1 * a.2)).then(a.0.cmp(&b.0)));
            println!("  15 lowest-coverage functions (unconditional local/total):");
            for (name, local, total) in functions.into_iter().take(15) {
                println!("  {local:>6}/{total:<6} {name}");
            }
            println!("  shared because:");
            for (reason, count) in reasons.iter().take(25) {
                println!("  {count:>6}  {reason}");
            }
        }
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
            let name = format!(
                "{}/{}",
                group.file_name().unwrap().to_string_lossy(),
                dir.file_name().unwrap().to_string_lossy()
            );
            let mut tally = Tally::default();
            tally.files(&db, &files, &name);
            all.files(&db, &files, &name);
            let input: Vec<_> = sources
                .iter()
                .map(|(name, text)| (name.as_str(), text.as_str()))
                .collect();
            let program = baml_db::testing::compile_multi_file_with_prefix(
                baml_test_support::prefix(OptLevel::Two),
                &input,
                OptLevel::Two,
            );
            let bytecode = crate::engine::display_user_functions(&program);
            let mut local_ops = 0;
            let mut locked_ops = 0;
            for line in bytecode.lines() {
                let tokens: Vec<_> = line.split_whitespace().collect();
                for token in tokens {
                    match token {
                        "load_field_local"
                        | "store_field_local"
                        | "load_array_element_local"
                        | "store_array_element_local"
                        | "container_len_local"
                        | "call_local" => local_ops += 1,
                        "load_field"
                        | "store_field"
                        | "load_array_element"
                        | "store_array_element"
                        | "container_len"
                        | "call" => locked_ops += 1,
                        _ => {}
                    }
                }
            }
            per_workload.push((name, tally.totals(), local_ops, locked_ops));
        }
    }
    all.print("speedtest workloads");
    println!("  per workload:");
    for (name, (total, local), local_ops, locked_ops) in &per_workload {
        println!(
            "  {:>3}%  {local:>4}/{total:<4}  {name}  O2 opcodes local={local_ops} locked={locked_ops}",
            percent(*local, *total)
        );
    }

    let mut db = make_db();
    let probes: Vec<_> = baml_files(&manifest.join("rust_diff/baml_src"))
        .iter()
        .map(|(name, text)| db.file(name, text))
        .collect();
    let mut tally = Tally::default();
    tally.files(&db, &probes, "rust_differential");
    tally.print("rust_differential probes");

    // Discover tracked programs, avoiding build products and other worktrees.
    let repo = manifest.join("../../..");
    let listing = std::process::Command::new("git")
        .args(["ls-files", "-z", "--", "*.baml"])
        .current_dir(&repo)
        .output()
        .expect("list repository examples");
    assert!(listing.status.success());
    let mut projects: BTreeMap<std::path::PathBuf, Vec<std::path::PathBuf>> = BTreeMap::new();
    for name in listing.stdout.split(|&b| b == 0).filter(|s| !s.is_empty()) {
        let path = std::path::PathBuf::from(std::str::from_utf8(name).unwrap());
        let example = path.components().any(|c| {
            matches!(
                c.as_os_str().to_str(),
                Some("examples" | "example" | "_examples")
            )
        });
        if !example && !path.starts_with("baml_language/wrapper/baml_src") {
            continue;
        }
        let parent = path
            .ancestors()
            .find(|p| p.file_name().is_some_and(|n| n == "baml_src"))
            .unwrap_or(&path)
            .to_path_buf();
        projects.entry(parent).or_default().push(path);
    }
    let mut examples = Tally::default();
    for (root, paths) in projects {
        let mut db = make_db();
        let files: Vec<_> = paths
            .iter()
            .map(|path| {
                let text = std::fs::read_to_string(repo.join(path)).unwrap();
                db.file(
                    path.strip_prefix(if root.extension().is_some_and(|ext| ext == "baml") {
                        root.parent().unwrap()
                    } else {
                        &root
                    })
                    .unwrap()
                    .to_str()
                    .unwrap(),
                    &text,
                )
            })
            .collect();
        let diagnostics = baml_db::check::ProjectDiagnostics::collect(&db);
        if baml_db::CheckedWorld::check(&diagnostics, files[0].source_root(&db)).is_err() {
            println!(
                "  examples: unverified {} ({} files; current compiler rejects project)",
                root.display(),
                paths.len()
            );
            continue;
        }
        println!("  examples: {} ({} files)", root.display(), paths.len());
        examples.files(&db, &files, &root.display().to_string());
    }
    examples.print("repository examples and baml wrapper");

    let roots = baml_compiler2_hir::package::lang_roots(&db);
    let mut tally = Tally::default();
    for package in baml_base::LangPackage::ALL {
        if let Some(root) = roots.get(package) {
            tally.files(&db, root.files(&db), &format!("{package:?}"));
        }
    }
    tally.print("standard library");
}

fn percent(part: usize, whole: usize) -> usize {
    (part * 100).checked_div(whole).unwrap_or(0)
}

/// The `.baml` files directly in `dir`, by name.
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

/// Every `.baml` file under `dir`, by path relative to it.
fn baml_tree(dir: &std::path::Path) -> Vec<(String, String)> {
    let mut files: Vec<_> = walkdir::WalkDir::new(dir)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.path().extension().is_some_and(|ext| ext == "baml"))
        .map(|entry| {
            (
                entry
                    .path()
                    .strip_prefix(dir)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
                std::fs::read_to_string(entry.path()).unwrap(),
            )
        })
        .collect();
    files.sort();
    files
}

/// The speedtest workload directories, as `category/slug`, in order.
fn speedtest_workloads() -> (std::path::PathBuf, Vec<String>) {
    let workloads =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/speedtest/workloads");
    let mut names: Vec<_> = walkdir::WalkDir::new(&workloads)
        .min_depth(2)
        .max_depth(2)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_dir())
        .filter(|entry| !baml_files(entry.path()).is_empty())
        .map(|entry| {
            entry
                .path()
                .strip_prefix(&workloads)
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    (workloads, names)
}

/// Allocation sites placed on the fiber stack by the Rust backend, and the
/// first cause every other one stays in the heap.
#[derive(Default)]
struct PlacementTally {
    placed: usize,
    total: usize,
    /// Bodies with at least one site.
    bodies: usize,
    buckets: std::collections::BTreeMap<&'static str, usize>,
    causes: std::collections::BTreeMap<String, usize>,
}

impl PlacementTally {
    fn add(&mut self, body: &baml_db::rust_backend::FunctionPlacement) {
        if body.sites.is_empty() {
            return;
        }
        self.bodies += 1;
        for site in &body.sites {
            self.total += 1;
            match site {
                Ok(_) => self.placed += 1,
                Err(refusal) => {
                    *self.buckets.entry(refusal.bucket()).or_default() += 1;
                    *self.causes.entry(refusal.to_string()).or_default() += 1;
                }
            }
        }
    }

    fn print(&self, title: &str, started: std::time::Instant) {
        println!(
            "\n## {title}: {}/{} allocation sites placed ({}%) in {} bodies ({:.0?})",
            self.placed,
            self.total,
            percent(self.placed, self.total),
            self.bodies,
            started.elapsed()
        );
        let mut buckets: Vec<_> = self.buckets.iter().collect();
        buckets.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        for (bucket, count) in buckets {
            println!("  {count:>6}  {bucket}");
        }
        let mut causes: Vec<_> = self
            .causes
            .iter()
            .filter(|(cause, _)| cause.contains(" to ") || cause.starts_with("not compiled"))
            .collect();
        causes.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
        if !causes.is_empty() {
            println!("  by callee or rejection:");
            for (cause, count) in causes.into_iter().take(10) {
                println!("  {count:>6}  {cause}");
            }
        }
    }

    fn absorb(&mut self, other: &Self) {
        self.placed += other.placed;
        self.total += other.total;
        self.bodies += other.bodies;
        for (bucket, count) in &other.buckets {
            *self.buckets.entry(bucket).or_default() += count;
        }
        for (cause, count) in &other.causes {
            *self.causes.entry(cause.clone()).or_default() += count;
        }
    }

    #[track_caller]
    fn pin(&self, corpus: &str, placed: usize, total: usize) {
        assert_eq!(
            (self.placed, self.total),
            (placed, total),
            "{corpus}: stack placement changed (placed/total); the buckets above say where \
             (run with --nocapture). Update the pin when the change is intended."
        );
    }
}

/// The placement of a project's bodies (`baml_compiler2_rust::placement_coverage`
/// at O2): the user package's into `user`, the standard library's into
/// `stdlib`. `None` when the project does not compile.
fn measure_placement(
    files: &[(String, String)],
    user: &mut PlacementTally,
    stdlib: Option<&mut PlacementTally>,
) -> Option<()> {
    use baml_db::rust_backend::{declared_functions, placement_coverage};
    let opt = OptLevel::Two;
    let input: Vec<_> = files
        .iter()
        .map(|(name, text)| (name.as_str(), text.as_str()))
        .collect();
    let db =
        baml_db::testing::setup_multi_file_db_with_prefix(baml_test_support::prefix(opt), &input);
    let root = db.workspace_root().unwrap();
    let program = baml_db::CheckedWorld::collect(&db, root)
        .ok()?
        .compile(opt)
        .ok()?;
    let roots = baml_compiler2_hir::package::lang_roots(&db);
    let lang: Vec<_> = baml_base::LangPackage::ALL
        .iter()
        .filter_map(|package| roots.get(*package))
        .collect();
    // Every function of the user package as an entry compiles the user
    // package and what it reaches; the standard library tally needs it all.
    let entries = stdlib
        .is_none()
        .then(|| declared_functions(&db, root, &program));
    let bodies =
        placement_coverage(&db, root, &program, opt, entries.as_deref()).expect("coverage runs");
    for body in &bodies {
        if !lang.contains(&body.root) {
            user.add(body);
        }
    }
    if let Some(stdlib) = stdlib {
        for body in &bodies {
            if lang.contains(&body.root) {
                stdlib.add(body);
            }
        }
    }
    Some(())
}

/// The speedtest workloads with a site on the stack, each pinned as
/// placed/total allocation sites. A workload not listed places nothing; the
/// report below says why.
const PLACED_WORKLOADS: &[(&str, usize, usize)] = &[
    ("classes/method-call-100k", 1, 1),
    ("classes/method-chain-100k", 1, 1),
    ("compute/class-instances-100k", 1, 1),
    ("compute/json-ndjson-events", 1, 7),
    ("compute/json-request-body", 1, 28),
    ("compute/json-tree-path", 2, 7),
    ("compute/json-typed-orders", 1, 6),
    ("compute/mixed-ops-5k", 1, 2),
    ("compute/nested-field-access-500k", 3, 3),
    ("compute/wide-nested-class-create-50k", 1, 8),
    ("interfaces/to-string-interpolation-10k", 1, 5),
    ("memory/heap-calls", 1, 9),
    ("memory/splay-2k", 1, 11),
    ("programs/agent-loop-300", 1, 11),
    ("programs/json-parser-15", 1, 19),
];

/// Stack placement by the Rust backend, pinned so a regression fails CI:
/// every speedtest workload that places a site ([`PLACED_WORKLOADS`]), the
/// rust_diff probes and the standard library. The whole corpus is
/// [`placement_coverage_report`]. A deliberate change updates the pins; the
/// buckets print with `cargo test -p baml_tests --lib placement_coverage -- --nocapture`.
#[test]
fn placement_coverage() {
    let (workloads, _) = speedtest_workloads();
    let started = std::time::Instant::now();
    let mut speedtest = PlacementTally::default();
    for &(name, placed, total) in PLACED_WORKLOADS {
        let mut tally = PlacementTally::default();
        measure_placement(&baml_files(&workloads.join(name)), &mut tally, None)
            .unwrap_or_else(|| panic!("speedtest workload {name} does not compile"));
        println!("  {:>3}/{:<3} {name}", tally.placed, tally.total);
        tally.pin(name, placed, total);
        speedtest.absorb(&tally);
    }
    speedtest.print("speedtest workloads with a placed site", started);

    let started = std::time::Instant::now();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut probes = PlacementTally::default();
    let mut stdlib = PlacementTally::default();
    measure_placement(
        &baml_files(&manifest.join("rust_diff/baml_src")),
        &mut probes,
        Some(&mut stdlib),
    )
    .expect("the rust_diff probes compile");
    probes.print("rust_diff probes", started);
    stdlib.print("standard library", started);
    probes.pin("rust_diff probes", 8, 169);
    stdlib.pin("standard library", 15, 1340);
}

/// [`placement_coverage`] over every corpus: all speedtest workloads, the
/// rust_diff probes, the standard library and `baml_tests/baml_src`, with
/// the per-corpus totals pinned. Minutes long (one world check and compile
/// per project), so not in CI.
///
/// `cargo test -p baml_tests --lib placement_coverage_report -- --ignored --nocapture`
#[test]
#[ignore = "minutes long; the CI pin is placement_coverage"]
fn placement_coverage_report() {
    let (workloads, names) = speedtest_workloads();
    let started = std::time::Instant::now();
    let mut speedtest = PlacementTally::default();
    for name in &names {
        let mut tally = PlacementTally::default();
        measure_placement(&baml_files(&workloads.join(name)), &mut tally, None)
            .unwrap_or_else(|| panic!("speedtest workload {name} does not compile"));
        if tally.placed > 0 {
            println!("  {:>3}/{:<3} {name}", tally.placed, tally.total);
        }
        speedtest.absorb(&tally);
    }
    speedtest.print(&format!("speedtest workloads ({})", names.len()), started);

    let started = std::time::Instant::now();
    let manifest = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut probes = PlacementTally::default();
    let mut stdlib = PlacementTally::default();
    measure_placement(
        &baml_files(&manifest.join("rust_diff/baml_src")),
        &mut probes,
        Some(&mut stdlib),
    )
    .expect("the rust_diff probes compile");
    probes.print("rust_diff probes", started);
    stdlib.print("standard library", started);

    let started = std::time::Instant::now();
    let mut corpus = PlacementTally::default();
    let mut projects: Vec<_> = std::fs::read_dir(manifest.join("baml_src"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.is_dir()
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("ns_")
        })
        .collect();
    projects.sort();
    let (mut compiled, mut rejected) = (0, 0);
    for project in &projects {
        let sources = baml_tree(project);
        if sources.is_empty() {
            continue;
        }
        match measure_placement(&sources, &mut corpus, None) {
            Some(()) => compiled += 1,
            None => rejected += 1,
        }
    }
    corpus.print(
        &format!("baml_tests/baml_src ({compiled} projects; {rejected} do not compile)"),
        started,
    );

    speedtest.pin("speedtest workloads", 18, 227);
    probes.pin("rust_diff probes", 8, 169);
    stdlib.pin("standard library", 15, 1340);
    corpus.pin("baml_tests/baml_src", 504, 6102);
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
fn fresh_results_of_declared_builtins_are_thread_local() {
    assert_locality(
        r#"
function main(s: string) -> int {
  let parts = s.split(",");
  let cps = s.to_code_points();
  let xs = [1, 2];
  let ys = xs.reverse();
  parts.length() + cps[0] + ys[0] + xs[0]
}
"#,
        "main",
        &[("parts", true), ("cps", true), ("xs", true), ("ys", true)],
    );
}

#[test]
fn inserting_into_a_shared_array_shares_the_item() {
    assert_locality(
        r#"
function main(shared: int[][]) -> int {
  let xs = [1];
  let ys = [2];
  let zs = [3];
  shared.insert(xs, 0);
  shared.unshift(ys);
  let local: int[][] = [];
  local.insert(zs, 0);
  xs[0] + ys[0] + zs[0] + local.length()
}
"#,
        "main",
        &[("xs", false), ("ys", false), ("zs", true), ("local", true)],
    );
}

const MASKED: &str = r#"
function total(xs: int[], ys: int[]) -> int { xs[0] + ys[0] }
function forward(xs: int[], shared: int[][]) -> int {
  let mine = [1];
  shared.push(mine);
  total(xs, [2])
}
function leaky(xs: int[]) -> int {
  let child = spawn { xs.length() };
  await child
}
function main(outside: int[][]) -> int {
  let xs = [1];
  forward(xs, outside) + leaky([3])
}
"#;

#[test]
fn a_parameter_is_thread_local_given_its_own_bit() {
    let (locals, _) = needs(MASKED, "forward");
    let of = |name: &str| locals.iter().find(|(n, _)| n == name).unwrap().1;
    assert_eq!(of("xs"), 0b01);
    assert_eq!(of("shared"), 0b10);
    // Stored into `shared`'s contents: as thread-local as `shared` is.
    assert_eq!(of("mine"), 0b10);
}

#[test]
fn a_parameter_the_body_leaks_is_never_thread_local() {
    let (locals, _) = needs(MASKED, "leaky");
    assert_eq!(
        locals.iter().find(|(n, _)| n == "xs").unwrap().1,
        escape::NEVER
    );
}

#[test]
fn callers_pass_what_they_prove_about_each_argument() {
    // `total(xs, [2])`: `xs` given the caller's first bit, a fresh array always.
    let (_, calls) = needs(MASKED, "forward");
    assert_eq!(calls, vec![vec![0b01, 0]]);
    // `forward(xs, outside)`: a fresh array always, a parameter given its bit.
    // `leaky([3])` passes no mask at all: the callee leaks its argument.
    let (_, calls) = needs(MASKED, "main");
    assert_eq!(calls, vec![vec![0, 0b1]]);
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
        "load_array_element_local",
        "container_len_local",
        "array_iter_next_local",
    ] {
        assert!(build.contains(local), "`build` lacks {local}:\n{build}");
    }
    let walk = body("walk");
    assert!(walk.contains("array_iter_next "), "{walk}");
    assert!(
        !walk.contains("_local"),
        "a parameter's array keeps its locks:\n{walk}"
    );
}

#[test]
fn bytecode_takes_the_lock_free_paths_for_thread_local_maps() {
    let program = crate::engine::compile_source_with_opt(
        r#"
function build(n: int) -> int {
  let m: map<string, int> = {};
  m.set("a", n);
  m["b"] = n;
  let c = m.get_or_insert("c", 1);
  let total = m["a"] + c + m.length() + m.keys().length();
  if (m.has("b")) { m.delete("b"); }
  total + (m.get("a") ?? 0)
}
function share(m: map<string, int>) -> int {
  m.set("a", 1);
  m["a"]
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
        "call_local baml.Map.set\n",
        "call_local baml.Map.get_or_insert\n",
        "call_local baml.Map.index\n",
        "call_local baml.Map.keys\n",
        "call_local baml.Map.has\n",
        "call_local baml.Map.delete\n",
        "call_local baml.Map.get\n",
        "container_len_local\n",
    ] {
        assert!(build.contains(local), "`build` lacks {local}:\n{build}");
    }
    let share = body("share");
    assert!(
        !share.contains("_local"),
        "a parameter's map keeps its locks:\n{share}"
    );
}

#[test]
fn bytecode_takes_relaxed_field_paths_for_thread_local_instances() {
    let program = crate::engine::compile_source_with_opt(
        r#"
class Counter {
  n: int
}
function build(k: int) -> int {
  let c = Counter { n: 0 };
  let i = 0;
  while (i < k) { c.n = c.n + i; i += 1; }
  c.n
}
function bump(c: Counter) -> int {
  c.n = c.n + 1;
  c.n
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
    for local in ["load_field_local .n", "store_field_local .n"] {
        assert!(build.contains(local), "`build` lacks {local}:\n{build}");
    }
    assert!(
        !build.contains("load_field .n") && !build.contains("store_field .n"),
        "`build` keeps an ordered field access:\n{build}"
    );
    let bump = body("bump");
    assert!(
        !bump.contains("_local"),
        "a parameter's instance keeps ordered fields:\n{bump}"
    );
}

#[test]
fn builtin_effects_match_native_signatures() {
    use escape::{EFFECTS, Returns};
    let (builtins, _, _) = baml_builtins2_codegen::extract_native_builtins_for("baml").unwrap();
    for (path, effect) in EFFECTS {
        let builtin = builtins
            .iter()
            .find(|b| b.path == *path)
            .unwrap_or_else(|| panic!("missing {path}"));
        assert_eq!(1 + builtin.params.len(), effect.arity, "{path}");
        for &index in effect.keeps {
            assert!(index < effect.arity, "{path}: keeps {index}");
        }
        if let Returns::Fresh(indices) = effect.returns {
            for &index in indices {
                assert!(index < effect.arity, "{path}: Fresh {index}");
            }
        }
    }
}

#[test]
fn bytecode_keeps_container_and_field_locks_at_o0() {
    let source = r#"
class Box { items: int[] }
function main() -> int {
  let box = Box { items: [1] };
  box.items = [2];
  box.items[0] = 3;
  box.items.push(4);
  let xs = [1, 2];
  for (let x in xs) { box.items.push(x); }
  let map = {"x": box.items[0]};
  map.set("y", 5);
  box.items.length() + map["y"]
}
"#;
    let bytecode = crate::engine::display_user_functions(&crate::engine::compile_source_with_opt(
        source,
        OptLevel::Zero,
    ));
    for opcode in [
        "load_field_local",
        "store_field_local",
        "load_array_element_local",
        "store_array_element_local",
        "container_len_local",
        "call_local",
        "array_iter_next",
    ] {
        assert!(
            !bytecode.contains(opcode),
            "O0 emitted {opcode}:\n{bytecode}"
        );
    }
}

#[test]
fn callbacks_share_elements_without_sharing_the_receiver() {
    assert_locality(
        r#"
class Item { value: int }
function main() -> int {
  let item = Item { value: 1 };
  let items = [item];
  let mapped = items.map((x: Item) -> Item { x });
  mapped[0].value + items.length()
}
"#,
        "main",
        &[("item", false), ("items", true), ("mapped", true)],
    );
}

#[test]
fn string_conversion_overrides_share_elements_without_sharing_the_receiver() {
    assert_locality(
        r#"
class Item { value: int }
function main() -> int {
  let item = Item { value: 1 };
  let items = [item];
  let text = items.join(",");
  items.length() + text.length()
}
"#,
        "main",
        &[("item", false), ("items", true)],
    );
}

#[test]
fn observed_array_iterator_keeps_its_object() {
    let source = r#"
function observe(it: baml.iter.Iterator<Item = int, Error = never>) -> void {
  it.next();
}
function observed(xs: int[]) -> int {
  let it = xs.iter();
  observe(it);
  let total = 0;
  for (let x in it) { total += x; }
  total
}
"#;
    let mut db = make_db();
    let file = db.file("test.baml", source);
    assert_no_diagnostic_errors(&db);
    let loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == "observed")
        .unwrap();
    let mir = lower_function(&db, loc, OptLevel::Two).as_ref().unwrap();
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("expected bytecode")
    };
    assert!(
        escape::array_walks(&db, body, mir.arity)
            .iter()
            .all(|walk| !walk)
    );
    let bytecode = crate::engine::display_user_functions(&crate::engine::compile_source_with_opt(
        source,
        OptLevel::Two,
    ));
    assert!(!bytecode.contains("array_iter_next"), "{bytecode}");
    assert!(bytecode.contains("virtual_call"), "{bytecode}");
}

#[test]
fn awaited_objects_are_shared() {
    assert_locality(
        r#"
function main() -> int {
    let child = spawn { [42] };
    let xs = await child;
    xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn global_objects_are_shared() {
    assert_locality(
        r#"
const SHARED = [42];
function main() -> int {
    let xs = SHARED;
    xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn sysop_results_are_shared() {
    assert_locality(
        r#"
function main() -> bool {
    let glob = baml.glob.new("*");
    glob.matches("file")
}
"#,
        "main",
        &[("glob", false)],
    );
}

#[test]
fn an_uncaptured_alias_of_a_captured_value_is_shared() {
    assert_locality(
        r#"
function main() -> int {
    let original = [1];
    let captured = original;
    let read = () -> int { captured[0] };
    original[0] + read()
}
"#,
        "main",
        &[("original", false)],
    );
}

#[test]
fn an_explicit_spawn_plan_is_shared() {
    assert_locality(
        r#"
function main() -> int {
    let plan = baml.spawn.Plan<int, never> {
        body: () -> int throws never { 1 },
        name: null,
        limits: [],
        cancel_tokens: [],
        root: false,
    };
    let child = baml.spawn.__spawn(plan);
    await child
}
"#,
        "main",
        &[("plan", false)],
    );
}

#[test]
fn sysop_arguments_are_shared() {
    // A sys-op receives its object arguments as heap handles.
    assert_locality(
        r#"
function main() -> int {
    let options = baml.glob.ScanOptions {
        cwd: null,
        dot: null,
        absolute: null,
        follow_symlinks: null,
        throw_error_on_broken_symlink: null,
        only_files: null,
    };
    let fresh = baml.glob.ScanOptions {
        cwd: null,
        dot: null,
        absolute: null,
        follow_symlinks: null,
        throw_error_on_broken_symlink: null,
        only_files: null,
    };
    options.dot = true;
    fresh.dot = true;
    let paths = baml.glob.new("*").scan(options);
    paths.length()
}
"#,
        "main",
        &[("options", false), ("fresh", true)],
    );
}

#[test]
fn fields_of_a_finalized_instance_are_shared() {
    // The collector runs `cleanup` in a VM of its own once the instance is
    // garbage, while `main` may still hold the array.
    assert_locality(
        r#"
class Resource {
    log: int[],
    function cleanup(self) -> void throws never { self.log.push(0); }
}
class Plain {
    log: int[],
}
function main() -> int {
    let held = [1];
    let resource = Resource { log: held };
    let kept = [2];
    let plain = Plain { log: kept };
    resource.log.push(3);
    plain.log.push(4);
    held[0] + kept[0]
}
"#,
        "main",
        &[
            ("held", false),
            ("kept", true),
            ("resource", true),
            ("plain", true),
        ],
    );
}

#[test]
fn fields_of_a_finalized_instance_built_in_a_callee_are_shared() {
    assert_locality(
        r#"
class Resource {
    log: int[],
    function cleanup(self) -> void throws never { self.log.push(0); }
}
function abandon(log: int[]) -> void throws never {
    let resource = Resource { log };
    resource.log.push(-1);
}
function main() -> int {
    let log: int[] = [];
    abandon(log);
    log.push(1);
    log[0]
}
"#,
        "main",
        &[("log", false)],
    );
}

#[test]
fn dependency_classes_may_finalize() {
    // Whether a dependency's class has a finalizer must not depend on whether
    // its source is mounted: assume it may.
    let mut db = make_db();
    db.dependency("dep");
    db.file(
        "<builtin>/dep/lib.baml",
        r#"
class Box {
    items: int[],
}
"#,
    );
    let file = db.file(
        "test.baml",
        r#"
function main() -> int {
    let items = [1];
    let boxed = dep.Box { items };
    items.push(2);
    boxed.items[0]
}
"#,
    );
    let actual = classify_file(&db, file, "main");
    assert!(
        actual
            .iter()
            .any(|(local, is_local)| local == "items" && !is_local),
        "a dependency class may have a finalizer: {actual:?}"
    );
}

#[test]
fn review_late_publication_through_returned_alias() {
    assert_locality(
        r#"
function identity(xs: int[]) -> int[] { xs }
function publish(xs: int[]) -> void { let child = spawn { xs.push(9) }; }
function main() -> int {
    let xs = [1];
    xs.push(2);
    let alias = identity(xs);
    publish(alias);
    xs[0]
}
"#,
        "main",
        &[("xs", false), ("alias", false)],
    );
}

#[test]
fn review_map_callback_returns_shared_array() {
    assert_locality(
        r#"
function main(shared: int[]) -> int {
    let mapped = [1].map((n: int) -> int[] { shared });
    let item = mapped[0];
    item.push(2);
    item[0]
}
"#,
        "main",
        &[("mapped", true), ("item", false)],
    );
}

#[test]
fn review_defer_publishes_array_after_local_access() {
    assert_locality(
        r#"
function publish(xs: int[]) -> void { let child = spawn { xs.push(9) }; }
function main() -> int {
    let xs = [1];
    defer { publish(xs); }
    xs.push(2);
    xs[0]
}
"#,
        "main",
        &[("xs", false)],
    );
}

#[test]
fn review_nested_container_mask_requires_shared_contents() {
    let (locals, calls) = needs(
        r#"
function read(nested: int[][]) -> int { nested[0][0] }
function main(shared: int[]) -> int {
    let nested = [shared];
    read(nested)
}
"#,
        "main",
    );
    assert_eq!(
        locals.iter().find(|(name, _)| name == "nested").unwrap().1,
        0
    );
    assert!(calls.iter().any(|args| args == &[1]), "{calls:?}");
}

#[test]
fn review_parameters_beyond_locality_mask_are_shared() {
    let params = (0..65)
        .map(|i| format!("p{i}: int[]"))
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!("function main({params}) -> int {{ p64[0] }}");
    let (locals, _) = needs(&source, "main");
    assert_eq!(
        locals.iter().find(|(name, _)| name == "p63").unwrap().1,
        1u64 << 63
    );
    assert_eq!(
        locals.iter().find(|(name, _)| name == "p64").unwrap().1,
        escape::NEVER
    );
}

#[test]
fn review_summary_invalidates_when_callee_starts_spawning() {
    use salsa::Setter;
    let mut db = make_db();
    let source = r#"
function helper(xs: int[]) -> int { xs.length() }
function main() -> int { let xs = [1]; helper(xs); xs[0] }
"#;
    let file = db.file("test.baml", source);
    for opt in [OptLevel::One, OptLevel::Two] {
        let locals = classify_file_at_opt(&db, file, "main", opt);
        assert!(locals.iter().find(|(name, _)| name == "xs").unwrap().1);
    }
    file.set_text(&mut db).to(source.replace(
        "xs.length()",
        "let child = spawn { xs.push(9) }; xs.length()",
    ));
    for opt in [OptLevel::One, OptLevel::Two] {
        let locals = classify_file_at_opt(&db, file, "main", opt);
        assert!(!locals.iter().find(|(name, _)| name == "xs").unwrap().1);
    }
}

// ---------------------------------------------------------------------------
// Frame verdict: may an allocation outlive the frame that built it?
// ---------------------------------------------------------------------------

use baml_compiler2_mir::escape::FrameEscape;

/// Run `check` on the frame facts of `function`, with every allocation site
/// labeled: by the local it is assigned to when that local is named, else
/// by the int constant its first field or element is built from. `compiled`
/// is the backend's answer for every source callee.
fn with_frame_facts<R>(
    source: &str,
    function: &str,
    compiled: bool,
    check: impl FnOnce(&escape::EscapeFacts, &[(String, escape::Site)]) -> R,
) -> R {
    use baml_compiler2_mir::{Constant, Operand, Place, Rvalue, StatementKind};
    let mut db = make_db();
    let file = db.file("test.baml", source);
    assert_no_diagnostic_errors(&db);
    let loc = *file_functions(&db, file)
        .iter()
        .find(|&&loc| function_data(&db, loc).name.as_str() == function)
        .unwrap_or_else(|| panic!("no function `{function}`"));
    let opt = OptLevel::Two;
    let mir = lower_function(&db, loc, opt)
        .as_ref()
        .unwrap_or_else(|error| panic!("MIR lowering failed: {error}"));
    let MirFunctionKind::Bytecode(body) = &mir.kind else {
        panic!("`{function}` must lower to bytecode")
    };
    let facts = escape::analyze_for(&db, file.source_root(&db), body, mir.arity, opt, &|_| {
        compiled
    });
    let sites: Vec<(String, escape::Site)> = facts
        .sites()
        .map(|(site, block, statement)| {
            let StatementKind::Assign { destination, value } =
                &body.blocks[block.0].statements[statement].kind
            else {
                panic!("site at {block:?}/{statement} is not an assignment")
            };
            let named = match destination {
                Place::Local(local) => body.locals[local.0].name.as_ref().map(|n| n.to_string()),
                _ => None,
            };
            let first = match value {
                Rvalue::Aggregate { fields, .. } => fields.first(),
                Rvalue::Array(_, elements) => elements.first(),
                _ => None,
            };
            let label = named
                .or_else(|| match first {
                    Some(Operand::Constant(Constant::Int(v))) => Some(v.to_string()),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("site at {block:?}/{statement} has no label"));
            (label, site)
        })
        .collect();
    check(&facts, &sites)
}

/// Why each labeled site of `function` may outlive the frame, in statement
/// order: `None` when it stays.
fn frame_verdicts(
    source: &str,
    function: &str,
    compiled: bool,
) -> Vec<(String, Option<FrameEscape>)> {
    with_frame_facts(source, function, compiled, |facts, sites| {
        sites
            .iter()
            .map(|(label, site)| (label.clone(), facts.site_escape(*site).cloned()))
            .collect()
    })
}

#[track_caller]
fn assert_frame(
    source: &str,
    function: &str,
    compiled: bool,
    expected: &[(&str, Option<FrameEscape>)],
) {
    let actual = frame_verdicts(source, function, compiled);
    for (label, escape) in expected {
        let found = actual
            .iter()
            .find(|(actual, _)| actual == label)
            .unwrap_or_else(|| panic!("no site `{label}` in {actual:?}"));
        assert_eq!(&found.1, escape, "site `{label}`; all: {actual:?}");
    }
}

const ITEM: &str = "class Item { value: int }";
const HOLDER: &str = "class Holder { tag: int, item: Item }";

#[test]
fn frame_local_allocation_stays() {
    assert_frame(
        &format!(
            r#"
{ITEM}
function main() -> int {{
    let item = Item {{ value: 1 }};
    item.value = 2;
    let xs = [3];
    xs.push(item.value);
    item.value + xs.length()
}}
"#
        ),
        "main",
        true,
        &[("item", None), ("xs", None)],
    );
}

#[test]
fn frame_returned_allocation_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
function main() -> Item {{
    let item = Item {{ value: 1 }};
    item
}}
"#
        ),
        "main",
        true,
        &[("item", Some(FrameEscape::Returned))],
    );
}

#[test]
fn frame_stored_into_a_parameter_field_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
function main(h: Holder) -> int {{
    let item = Item {{ value: 1 }};
    h.item = item;
    0
}}
"#
        ),
        "main",
        true,
        &[("item", Some(FrameEscape::Parameter))],
    );
}

#[test]
fn frame_stored_into_a_local_object_stays() {
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
function main() -> int {{
    let h = Holder {{ tag: 3, item: Item {{ value: 2 }} }};
    let item = Item {{ value: 1 }};
    h.item = item;
    h.item.value + h.tag
}}
"#
        ),
        "main",
        true,
        &[("h", None), ("item", None), ("2", None)],
    );
}

const CALLEES: &str = r#"
function read(x: Item) -> int { x.value }
function alias(x: Item) -> Item { x }
function retain(h: Holder, x: Item) -> int { h.item = x; 0 }
"#;

#[test]
fn frame_passed_to_a_non_keeping_callee_stays() {
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
{CALLEES}
function main() -> int {{
    let item = Item {{ value: 1 }};
    read(item)
}}
"#
        ),
        "main",
        true,
        &[("item", None)],
    );
}

#[test]
fn frame_passed_to_a_returning_callee_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
{CALLEES}
function main() -> int {{
    let item = Item {{ value: 1 }};
    let copy = alias(item);
    copy.value
}}
"#
        ),
        "main",
        true,
        &[("item", Some(FrameEscape::KeepingCall("user.alias".into())))],
    );
}

#[test]
fn frame_passed_to_a_storing_callee_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
{CALLEES}
function main() -> int {{
    let h = Holder {{ tag: 3, item: Item {{ value: 2 }} }};
    let item = Item {{ value: 1 }};
    retain(h, item)
}}
"#
        ),
        "main",
        true,
        &[
            ("h", None),
            ("item", Some(FrameEscape::KeepingCall("user.retain".into()))),
        ],
    );
}

#[test]
fn frame_interpreted_callee_keeps_its_arguments() {
    // The backend declines `read`: the interpreter runs it, which may keep
    // what it is passed, whatever the summary says.
    assert_frame(
        &format!(
            r#"
{ITEM}
{HOLDER}
{CALLEES}
function main() -> int {{
    let item = Item {{ value: 1 }};
    read(item)
}}
"#
        ),
        "main",
        false,
        &[("item", Some(FrameEscape::KeepingCall("user.read".into())))],
    );
}

#[test]
fn frame_passed_to_a_modeled_native_stays() {
    // `concat` is modeled with no `keeps`: it reads both arrays.
    assert_frame(
        r#"
function main() -> int {
    let xs = [1];
    let ys = [2];
    let zs = xs.concat(ys);
    zs.length() + xs.length() + ys.length()
}
"#,
        "main",
        true,
        &[("xs", None), ("ys", None)],
    );
}

#[test]
fn frame_kept_by_a_modeled_native_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
function main() -> int {{
    let xs = [Item {{ value: 1 }}];
    let item = Item {{ value: 2 }};
    xs.push(item);
    xs.length()
}}
"#
        ),
        "main",
        true,
        &[
            ("xs", None),
            ("item", Some(FrameEscape::Native("baml.Array.push".into()))),
        ],
    );
}

#[test]
fn frame_passed_to_an_unmodeled_native_escapes() {
    // `includes` is a VM builtin the effect table does not list.
    assert_frame(
        r#"
function main() -> bool {
    let inner = [1];
    let xs = [inner];
    let probe = [2];
    xs.includes(probe)
}
"#,
        "main",
        true,
        &[(
            "probe",
            Some(FrameEscape::Native("baml.Array.includes".into())),
        )],
    );
}

#[test]
fn frame_captured_by_a_closure_escapes() {
    assert_frame(
        &format!(
            r#"
{ITEM}
function main() -> int {{
    let item = Item {{ value: 1 }};
    let read = () -> int {{ item.value }};
    read()
}}
"#
        ),
        "main",
        true,
        // The captured local is a cell; the instance lands in a temporary.
        &[("1", Some(FrameEscape::Captured))],
    );
}

#[test]
fn frame_thrown_allocation_escapes() {
    assert_frame(
        r#"
function main() -> int {
    let xs = [1];
    if (xs[0] > 0) { throw xs; }
    0
}
"#,
        "main",
        true,
        &[("xs", Some(FrameEscape::Thrown))],
    );
}

#[test]
fn frame_finalizable_instance_escapes() {
    assert_frame(
        r#"
class Resource {
    value: int,
    function cleanup(self) -> void throws never {}
}
function main() -> int {
    let resource = Resource { value: 1 };
    resource.value
}
"#,
        "main",
        true,
        &[("resource", Some(FrameEscape::Finalizer))],
    );
}

#[test]
fn frame_parameters_never_stay() {
    with_frame_facts(
        &format!(
            r#"
{ITEM}
function main(item: Item) -> int {{
    item = Item {{ value: 1 }};
    item.value
}}
"#
        ),
        "main",
        true,
        |facts, sites| {
            assert_eq!(
                facts.local_escape(baml_compiler2_mir::Local(1)),
                Some(&FrameEscape::Parameter)
            );
            let (_, site) = &sites[0];
            assert_eq!(facts.site_escape(*site), Some(&FrameEscape::Parameter));
            assert!(!facts.site_stays_in_frame(*site));
        },
    );
}

#[test]
fn frame_generic_instances_never_stay() {
    assert_frame(
        r#"
class Box<T> { value: T }
function main() -> int {
    let b = Box<int> { value: 1 };
    b.value
}
"#,
        "main",
        true,
        &[("b", Some(FrameEscape::Generic))],
    );
}

#[test]
fn frame_sites_assigned_to_one_local_share_an_alias_group() {
    with_frame_facts(
        &format!(
            r#"
{ITEM}
function main(flip: bool) -> int {{
    let item = Item {{ value: 1 }};
    if (flip) {{ item = Item {{ value: 2 }}; }}
    let other = Item {{ value: 3 }};
    item.value + other.value
}}
"#
        ),
        "main",
        true,
        |facts, sites| {
            let site = |label: &str| {
                sites
                    .iter()
                    .find(|(l, _)| l == label)
                    .unwrap_or_else(|| panic!("no site `{label}` in {sites:?}"))
                    .1
            };
            let items: Vec<escape::Site> = sites
                .iter()
                .filter(|(l, _)| l == "item")
                .map(|(_, site)| *site)
                .collect();
            let [first, second] = items[..] else {
                panic!("two sites assign `item`: {sites:?}")
            };
            let other = site("other");
            assert_eq!(facts.alias_group(first), facts.alias_group(second));
            assert_ne!(facts.alias_group(first), facts.alias_group(other));
            assert!(facts.local_group(baml_compiler2_mir::Local(0)).is_some());
            for site in [first, second, other] {
                assert!(facts.site_stays_in_frame(site), "{site:?}");
            }
        },
    );
}

#[test]
fn frame_may_reach_follows_field_stores() {
    with_frame_facts(
        &format!(
            r#"
{ITEM}
{HOLDER}
function main() -> int {{
    let inner = Item {{ value: 1 }};
    let h = Holder {{ tag: 2, item: inner }};
    let apart = Item {{ value: 3 }};
    h.item.value + apart.value
}}
"#
        ),
        "main",
        true,
        |facts, sites| {
            let site = |label: &str| {
                sites
                    .iter()
                    .find(|(l, _)| l == label)
                    .unwrap_or_else(|| panic!("no site `{label}` in {sites:?}"))
                    .1
            };
            let (inner, h, apart) = (site("inner"), site("h"), site("apart"));
            assert!(facts.may_reach(h, inner));
            assert!(facts.may_reach(h, h));
            assert!(!facts.may_reach(inner, h));
            assert!(!facts.may_reach(h, apart));
            let contents = facts.contents_group(facts.alias_group(h)).unwrap();
            assert_eq!(contents, facts.alias_group(inner));
            assert!(facts.group_may_reach(contents, facts.alias_group(inner)));
        },
    );
}
