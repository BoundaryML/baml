//! Link oracle: `borsh(link(emit_units(project))) ==
//! borsh(generate_project_bytecode(project))`.
//!
//! `emit_units` decomposes a full compile into per-file symbolic
//! `CompilationUnit`s; `link` folds them back into a `Program`. This oracle
//! proves the symbolic representation + linker reproduce the flat `Program`
//! byte-for-byte. `Program` has no `PartialEq`, so equality is proved on the
//! borsh byte vectors.

mod common;

use std::path::Path;

use baml_compiler2_emit::{
    CompileOptions, OptLevel, emit_units, generate_project_bytecode_with_opt,
};
use baml_db::ProjectDatabase;
use baml_tests::engine::TestDbExt;
use bex_vm_types::link::link;
use common::{A_BAML, B_BAML, C_BAML, assert_programs_byte_identical, build_db};

const ROOT: &str = "/link-oracle";

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

/// Assert `link(emit_units(project)) == generate_project_bytecode(project)` for
/// the project `build` produces. Two fresh databases are built (one per side) so
/// `emit_units` and the full compile share no salsa state.
fn assert_link_matches(label: &str, build: impl Fn() -> ProjectDatabase, emit_test_cases: bool) {
    let options = CompileOptions { emit_test_cases };

    let full = generate_project_bytecode_with_opt(&build(), &options, OptLevel::Two)
        .unwrap_or_else(|e| panic!("{label}: full compile: {e:?}"));

    let units = emit_units(&build(), &options, OptLevel::Two)
        .unwrap_or_else(|e| panic!("{label}: emit_units: {e:?}"));
    let linked = link(&units).unwrap_or_else(|e| panic!("{label}: link failed: {e}"));

    assert_programs_byte_identical(label, &full, &linked);
}

/// Simplest case: stdlib-only (empty user project) — just the builtin group.
#[test]
fn stdlib_only_links_byte_identical() {
    assert_link_matches("stdlib-only", || build_db(ROOT, &[]), false);
}

/// Single user file over the stdlib.
#[test]
fn single_file_links_byte_identical() {
    assert_link_matches(
        "single-file",
        || build_db(ROOT, &[("single.baml", SINGLE_BAML)]),
        false,
    );
}

/// The A/B/C multi-file fixture: cross-file class + function references
/// (Point defined in a, used in b/c; make_point/scale/label called across
/// files) exercise import resolution and pass-major placement.
#[test]
fn abc_fixture_links_byte_identical() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    assert_link_matches("abc-fixture", || build_db(ROOT, &files), false);
}

/// A file with a client-synthesized global exercises the `$init` synthesis path
/// (design §9 R2). A single-client package: `$init` calls one helper and stores
/// into the client slot.
#[test]
fn client_init_links_byte_identical() {
    const CLIENT_BAML: &str = r#"client<llm> TestClient {
  provider openai
  options {
    model "unused"
    api_key "unused"
  }
}

function shout() -> string {
  TestClient.name
}
"#;
    assert_link_matches(
        "client-init",
        || build_db(ROOT, &[("client.baml", CLIENT_BAML)]),
        false,
    );
}

/// Realistic project: the full `baml_src/` corpus (synthesized globals →
/// `$init`, per-file `$init_test` chainer, generic-function values, template
/// strings, tests). Exercises R1 (generic-fn interning), R2
/// (`$init`/`$init_test` tail synthesis), and R3 (pass-major placement)
/// together. Built from an on-disk directory (mirrors `emit_determinism`'s
/// discovery).
#[test]
fn baml_src_links_byte_identical() {
    use baml_db::discover_baml_files;

    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let sources: Vec<(std::path::PathBuf, String)> = discover_baml_files(&root)
        .into_iter()
        .map(|p| {
            let c = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
            (p, c)
        })
        .collect();
    assert!(!sources.is_empty(), "no .baml files under {root:?}");

    let build = || {
        let mut db = ProjectDatabase::new();
        db.workspace(&root);
        for (p, c) in &sources {
            db.file(p, c);
        }
        db
    };
    assert_link_matches("baml_src", build, true);
}
