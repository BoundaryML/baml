//! B-693 Stage 2 gate: `borsh(link(emit_units(project))) ==
//! borsh(generate_project_bytecode(project))`.
//!
//! `emit_units` decomposes a full compile into per-file symbolic
//! `CompilationUnit`s; `link` folds them back into a `Program`. This oracle
//! proves the symbolic representation + linker reproduce the flat `Program`
//! byte-for-byte, before any reuse is wired. `Program` has no `PartialEq`, so
//! equality is proved on the borsh byte vectors.

use std::path::Path;

use baml_compiler2_emit::{
    CompileOptions, OptLevel, emit_units, generate_project_bytecode_with_opt,
};
use baml_project::ProjectDatabase;
use bex_vm_types::link::link;

const ROOT: &str = "/link-oracle";

const A_BAML: &str = r#"class Point {
  x int
  y int
}

function make_point(x: int, y: int) -> Point {
  Point { x: x, y: y }
}

function origin() -> Point {
  make_point(0, 0)
}
"#;

const B_BAML: &str = r#"function scale(p: Point, factor: int) -> Point {
  let mul = (v: int) -> int { v * factor }
  Point { x: mul(p.x), y: mul(p.y) }
}

function magnitude_ish(p: Point) -> int {
  p.x * p.x + p.y * p.y
}

function label(p: Point) -> string {
  "point-label"
}
"#;

const C_BAML: &str = r#"function main() -> int {
  let banner = "start";
  baml.io.println(banner);
  let p = make_point(3, 4);
  let doubled = scale(p, 2);
  let tag = label(doubled);
  baml.io.println(tag);
  magnitude_ish(doubled)
}
"#;

const SINGLE_BAML: &str = r#"class Vec2 {
  x int
  y int
}

function add(a: Vec2, b: Vec2) -> Vec2 {
  Vec2 { x: a.x + b.x, y: a.y + b.y }
}

function greet(name: string) -> string {
  "hello"
}
"#;

fn build_db(files: &[(&str, &str)]) -> ProjectDatabase {
    let mut db = ProjectDatabase::new();
    db.set_project_root(Path::new(ROOT));
    for (name, content) in files {
        db.add_or_update_file(&Path::new(ROOT).join(name), content);
    }
    db
}

/// Assert the Stage 2 invariant for one project, printing a precise diagnostic
/// on mismatch (byte lengths + first-diff offset).
fn assert_link_matches(label: &str, files: &[(&str, &str)], emit_test_cases: bool) {
    let options = CompileOptions { emit_test_cases };

    let full = {
        let db = build_db(files);
        generate_project_bytecode_with_opt(&db, &options, OptLevel::Two).expect("full compile")
    };
    let full_bytes = borsh::to_vec(&full).expect("serialize full");

    let units = {
        let db = build_db(files);
        emit_units(&db, &options, OptLevel::Two).expect("emit_units")
    };
    let linked = link(&units).unwrap_or_else(|e| panic!("{label}: link failed: {e}"));
    let linked_bytes = borsh::to_vec(&linked).expect("serialize linked");

    if full_bytes != linked_bytes {
        let first_diff = full_bytes
            .iter()
            .zip(linked_bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| full_bytes.len().min(linked_bytes.len()));
        let ctx = |v: &[u8]| {
            let s = first_diff.saturating_sub(8);
            let end = (first_diff + 8).min(v.len());
            format!("{:02x?}", &v[s..end])
        };
        panic!(
            "{label}: link(emit_units) differs from full compile\n\
             lengths: full={} linked={}\n\
             first diff at byte {first_diff}\n\
             full   ..={}\n\
             linked ..={}",
            full_bytes.len(),
            linked_bytes.len(),
            ctx(&full_bytes),
            ctx(&linked_bytes),
        );
    }
}

/// Simplest case: stdlib-only (empty user project) — just the builtin group.
#[test]
fn stdlib_only_links_byte_identical() {
    assert_link_matches("stdlib-only", &[], false);
}

/// Single user file over the stdlib.
#[test]
fn single_file_links_byte_identical() {
    assert_link_matches("single-file", &[("single.baml", SINGLE_BAML)], false);
}

/// The A/B/C multi-file fixture: cross-file class + function references
/// (Point defined in a, used in b/c; make_point/scale/label called across
/// files) exercise import resolution and pass-major placement.
#[test]
fn abc_fixture_links_byte_identical() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    assert_link_matches("abc-fixture", &files, false);
}

/// A file with a top-level `let` (client-like) exercises the `$init` synthesis
/// path (design §9 R2). A single-let package: `$init` calls one helper and
/// stores into the let slot.
#[test]
fn let_init_links_byte_identical() {
    const LET_BAML: &str = r#"let greeting = "hi";

function shout() -> string {
  greeting
}
"#;
    assert_link_matches("let-init", &[("let.baml", LET_BAML)], false);
}

/// Assert the Stage 2 invariant for an on-disk directory project (mirrors
/// `emit_determinism`'s discovery). Builds two fresh databases so `emit_units`
/// and the full compile share no salsa state.
fn assert_dir_project_links(label: &str, root: &Path, emit_test_cases: bool) {
    use baml_workspace::discover_baml_files;
    let sources: Vec<(std::path::PathBuf, String)> = discover_baml_files(root)
        .into_iter()
        .map(|p| {
            let c = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
            (p, c)
        })
        .collect();
    assert!(
        !sources.is_empty(),
        "{label}: no .baml files under {root:?}"
    );
    let options = CompileOptions { emit_test_cases };

    let build = || {
        let mut db = ProjectDatabase::new();
        db.set_project_root(root);
        for (p, c) in &sources {
            db.add_or_update_file(p, c);
        }
        db
    };

    let full = generate_project_bytecode_with_opt(&build(), &options, OptLevel::Two)
        .unwrap_or_else(|e| panic!("{label}: full compile: {e:?}"));
    let full_bytes = borsh::to_vec(&full).expect("serialize full");

    let units = emit_units(&build(), &options, OptLevel::Two)
        .unwrap_or_else(|e| panic!("{label}: emit_units: {e:?}"));
    let linked = link(&units).unwrap_or_else(|e| panic!("{label}: link failed: {e}"));
    let linked_bytes = borsh::to_vec(&linked).expect("serialize linked");

    if full_bytes != linked_bytes {
        let d = full_bytes
            .iter()
            .zip(linked_bytes.iter())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| full_bytes.len().min(linked_bytes.len()));
        panic!(
            "{label}: link(emit_units) differs from full compile: \
             lengths full={} linked={}, first diff at byte {d}",
            full_bytes.len(),
            linked_bytes.len(),
        );
    }
}

/// Realistic project: the full `baml_src/` corpus (top-level `let` → `$init`,
/// per-file `$init_test` chainer, generic-function values, template strings,
/// tests). Exercises R1 (generic-fn interning), R2 (`$init`/`$init_test` tail
/// synthesis), and R3 (pass-major placement) together.
#[test]
fn baml_src_links_byte_identical() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    assert_dir_project_links("baml_src", &root, true);
}

/// Phase 2b measurement (plan open question 4): report the per-user-file
/// interface-fragment blob size and the total fragment weight vs. the linked
/// bytecode `Program` blob, on a small real fixture and on the biggest
/// available project. Run explicitly:
///   cargo test -p baml_tests --test link_units_oracle -- \
///       --ignored --nocapture interface_fragment_size_report
#[test]
#[ignore = "measurement harness; run explicitly with --nocapture"]
fn interface_fragment_size_report() {
    use baml_workspace::discover_baml_files;

    // A small real multi-file fixture (the A/B/C cross-file corpus).
    let inline: Vec<(std::path::PathBuf, String)> =
        [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)]
            .into_iter()
            .map(|(n, c)| (Path::new(ROOT).join(n), c.to_string()))
            .collect();
    report_fragment_sizes("A/B/C fixture (small)", Path::new(ROOT), &inline);

    // The biggest available project: the full `baml_tests/baml_src` corpus.
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let dir_sources: Vec<(std::path::PathBuf, String)> = discover_baml_files(&root)
        .into_iter()
        .map(|p| {
            let c = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
            (p, c)
        })
        .collect();
    report_fragment_sizes("baml_tests/baml_src (biggest)", &root, &dir_sources);
}

#[allow(clippy::print_stderr, clippy::cast_precision_loss)]
fn report_fragment_sizes(label: &str, root: &Path, sources: &[(std::path::PathBuf, String)]) {
    let mut db = ProjectDatabase::new();
    db.set_project_root(root);
    for (p, c) in sources {
        db.add_or_update_file(p, c);
    }
    let options = CompileOptions {
        emit_test_cases: true,
    };
    let units = emit_units(&db, &options, OptLevel::Two).expect("emit_units");
    let program = link(&units).expect("link");

    // Per-user-file fragment sizes (units with a real, non-builtin source path).
    let mut per_file: Vec<(String, usize)> = units
        .iter()
        .filter(|u| !u.source_file.is_empty() && !u.source_file.starts_with("<builtin>/"))
        .map(|u| (u.source_file.clone(), u.interface_fragment.len()))
        .collect();
    per_file.sort_by_key(|(_, n)| std::cmp::Reverse(*n));
    let user_files = per_file.len();
    let fragment_total: usize = per_file.iter().map(|(_, n)| n).sum();
    let program_bytes = borsh::to_vec(&program).expect("serialize program").len();
    let image_bytes = borsh::to_vec(&bex_vm_types::LinkableImage { units })
        .expect("serialize image")
        .len();

    eprintln!("\n=== {label} ===");
    eprintln!("user files                 : {user_files}");
    eprintln!(
        "fragment blob total        : {fragment_total} bytes ({:.1} KiB)",
        fragment_total as f64 / 1024.0
    );
    if let Some(avg) = fragment_total.checked_div(user_files) {
        eprintln!(
            "fragment bytes / file      : avg {avg} , max {} , min {}",
            per_file.first().map_or(0, |(_, n)| *n),
            per_file.last().map_or(0, |(_, n)| *n),
        );
    }
    eprintln!(
        "linked Program blob        : {program_bytes} bytes ({:.1} KiB)",
        program_bytes as f64 / 1024.0
    );
    eprintln!(
        "LinkableImage blob (w/frag): {image_bytes} bytes ({:.1} KiB)",
        image_bytes as f64 / 1024.0
    );
    eprintln!(
        "fragment / Program ratio   : {:.1}%",
        100.0 * fragment_total as f64 / program_bytes.max(1) as f64
    );
    eprintln!("top 5 files by fragment size:");
    for (name, n) in per_file.iter().take(5) {
        eprintln!("  {n:>7} bytes  {name}");
    }
}
