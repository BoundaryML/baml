//! The per-package emitter's oracle: every package of a program emitted on
//! its own and linked (`baml_db::compile_program`) is byte-identical to the
//! flat whole-program emit of the same program.
//!
//! The flat emitter never fills the executable's three identity tables
//! (`Class.methods`, `ProgramPackage::{globals, init}`), so the linked image
//! is compared with those cleared, and each is asserted positively on its
//! own. Beyond identity: emission is deterministic, the parallel code pass
//! reproduces the serial one, and a package's unit is a pure function of its
//! own sources and its dependencies' interfaces — a stdlib package's unit is
//! the same bytes whatever user package sits above it.

mod common;

use std::path::Path;

use baml_compiler2_emit::{OptLevel, emit_package, generate_project_bytecode_with_opt};
use baml_db::{ProjectDatabase, compile_program};
use baml_tests::engine::TestDbExt;
use bex_vm_types::{Object, Program};
use common::{A_BAML, B_BAML, C_BAML, assert_programs_byte_identical, build_db};

const ROOT: &str = "/package-emit-oracle";

const SINGLE_BAML: &str = r#"class Vec2 {
  x int
  y int

  function len2(self) -> int {
    self.x * self.x + self.y * self.y
  }
}

enum Side {
  Left
  Right
}

function add(a: Vec2, b: Vec2) -> Vec2 {
  Vec2 { x: a.x + b.x, y: a.y + b.y }
}

function pick(s: Side) -> int {
  match (s) {
    Side.Left => 1,
    Side.Right => 2,
  }
}
"#;

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

const LEVELS: [OptLevel; 3] = [OptLevel::Zero, OptLevel::One, OptLevel::Two];

fn package(db: &ProjectDatabase) -> baml_db::SourceRoot {
    db.workspace_root()
        .unwrap_or_else(|| unreachable!("the fixture builder adds one workspace root"))
}

/// The three tables the flat emitter never fills, cleared so the rest of the
/// image can be compared byte for byte.
fn without_identity_tables(mut program: Program) -> Program {
    for object in program.objects.iter_mut() {
        if let Object::Class(class) = object {
            class.methods.clear();
        }
    }
    for package in &mut program.packages {
        package.globals.clear();
        package.init = None;
    }
    program
}

/// What the flat emitter leaves empty, the per-package emitter fills: every
/// class's inherent methods, every package's slot table, and a structural
/// `$init` wherever the rendered init order names one.
fn assert_identity_tables(label: &str, program: &Program) {
    for package in &program.packages {
        let rendered_init = if package.name.as_str() == "user" {
            "$init".to_string()
        } else {
            format!("{}.$init", package.name)
        };
        #[expect(
            deprecated,
            reason = "the rendered order is the oracle for the structural init"
        )]
        let has_rendered = program.package_init_order.contains(&rendered_init);
        assert_eq!(
            package.init.is_some(),
            has_rendered,
            "{label}: package `{}` init table disagrees with the rendered order",
            package.name
        );
        if let Some(init) = package.init {
            assert!(
                matches!(&program.objects[init], Object::Function(f) if f.name == "$init"),
                "{label}: package `{}` init is not its `$init`",
                package.name
            );
        }
        #[expect(
            deprecated,
            reason = "the rendered maps are the oracle for the slot table"
        )]
        let rendered_slots = program
            .function_global_indices
            .iter()
            .filter(|(name, _)| name.starts_with(&format!("{}.", package.name)))
            .count();
        assert!(
            package.globals.len() >= rendered_slots,
            "{label}: package `{}` slot table ({}) covers fewer callables than the rendered map \
             ({rendered_slots})",
            package.name,
            package.globals.len()
        );
    }
    let mut classes_with_methods = 0usize;
    for object in program.objects.iter() {
        let Object::Class(class) = object else {
            continue;
        };
        for (name, method) in &class.methods {
            let Object::Function(function) = &program.objects[method.function] else {
                panic!(
                    "{label}: class `{}` method `{name}` is not a function",
                    class.name
                )
            };
            assert!(
                function.name.ends_with(&format!(".{name}")),
                "{label}: class `{}` method `{name}` points at `{}`",
                class.name,
                function.name
            );
        }
        classes_with_methods += usize::from(!class.methods.is_empty());
    }
    assert!(
        classes_with_methods > 0,
        "{label}: no class carries an inherent method table"
    );
}

fn assert_program_matches_flat(label: &str, build: impl Fn() -> ProjectDatabase) {
    for opt in LEVELS {
        let label = format!("{label}@{opt:?}");
        let flat_db = build();
        let flat = generate_project_bytecode_with_opt(&flat_db, package(&flat_db), opt)
            .unwrap_or_else(|e| panic!("{label}: flat compile: {e:?}"));
        let db = build();
        let linked = compile_program(&db, package(&db), opt)
            .unwrap_or_else(|e| panic!("{label}: compile_program: {e}"));
        assert_identity_tables(&label, &linked);
        assert_programs_byte_identical(&label, &flat, &without_identity_tables(linked));
    }
}

#[test]
fn stdlib_only_matches_flat() {
    assert_program_matches_flat("stdlib-only", || build_db(ROOT, &[]));
}

#[test]
fn single_file_matches_flat() {
    assert_program_matches_flat("single-file", || {
        build_db(ROOT, &[("single.baml", SINGLE_BAML)])
    });
}

#[test]
fn abc_fixture_matches_flat() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    assert_program_matches_flat("abc-fixture", || build_db(ROOT, &files));
}

#[test]
fn client_init_matches_flat() {
    assert_program_matches_flat("client-init", || {
        build_db(ROOT, &[("client.baml", CLIENT_BAML)])
    });
}

fn baml_src_sources() -> Vec<(std::path::PathBuf, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let sources: Vec<(std::path::PathBuf, String)> = baml_db::discover_baml_files(&root)
        .into_iter()
        .map(|p| {
            let c = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {p:?}: {e}"));
            (p, c)
        })
        .collect();
    assert!(!sources.is_empty(), "no .baml files under {root:?}");
    sources
}

fn baml_src_db(sources: &[(std::path::PathBuf, String)]) -> ProjectDatabase {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("baml_src");
    let mut db = ProjectDatabase::new();
    db.workspace(&root);
    for (p, c) in sources {
        db.file(p, c);
    }
    db
}

/// The whole corpus: generic-function values across files, `$init` and
/// `$init_test` tails, impl rules against stdlib interfaces, every match
/// shape.
#[test]
fn baml_src_matches_flat() {
    let sources = baml_src_sources();
    assert_program_matches_flat("baml_src", || baml_src_db(&sources));
}

/// One package's emitted bytes, under the program's spelling of it.
#[derive(PartialEq)]
struct EmittedBytes {
    name: String,
    unit: Vec<u8>,
    record: Vec<u8>,
    tail: Vec<u8>,
}

/// Every package of the program, emitted on its own.
fn emitted_bytes(db: &ProjectDatabase, opt: OptLevel) -> Vec<EmittedBytes> {
    let spelling = baml_compiler2_hir::package::spelling(db);
    baml_compiler2_hir::package::world_roots(db, package(db))
        .iter()
        .copied()
        .filter(|root| !root.files(db).is_empty())
        .map(|root| {
            let emitted = emit_package(db, root, opt).expect("emit_package");
            EmittedBytes {
                name: spelling.of(root).to_string(),
                unit: borsh::to_vec(&emitted.unit).expect("serialize unit"),
                record: borsh::to_vec(&emitted.record).expect("serialize record"),
                tail: borsh::to_vec(&emitted.tail).expect("serialize tail"),
            }
        })
        .collect()
}

/// Emission is deterministic: two databases from identical sources emit
/// identical bytes, package by package.
#[test]
fn baml_src_emission_is_deterministic() {
    let sources = baml_src_sources();
    let first = emitted_bytes(&baml_src_db(&sources), OptLevel::Two);
    let second = emitted_bytes(&baml_src_db(&sources), OptLevel::Two);
    assert!(first == second, "emit_package is nondeterministic");
}

/// The parallel code pass reproduces the serial one: import ordinals interned
/// per item and remapped at the merge land where the serial pass interns
/// them.
#[test]
fn parallel_program_is_byte_identical_to_serial() {
    let sources = baml_src_sources();
    let run_with = |threads: usize| {
        rayon::ThreadPoolBuilder::new()
            .num_threads(threads)
            .build()
            .expect("build rayon pool")
            .install(|| {
                let db = baml_src_db(&sources);
                let program =
                    compile_program(&db, package(&db), OptLevel::Two).expect("compile_program");
                borsh::to_vec(&program).expect("serialize program")
            })
    };
    let serial = run_with(1);
    let parallel = run_with(4);
    assert!(
        serial == parallel,
        "parallel emit diverges from serial: lengths {} vs {}",
        serial.len(),
        parallel.len()
    );
}

/// A package's unit is a pure function of its own sources and its
/// dependencies' interfaces: the stdlib's packages emit the same bytes under
/// two different user packages.
#[test]
fn stdlib_packages_are_independent_of_the_user_package() {
    let stdlib = |files: &[(&str, &str)]| {
        emitted_bytes(&build_db(ROOT, files), OptLevel::Two)
            .into_iter()
            .filter(|package| package.name != "user")
            .collect::<Vec<_>>()
    };
    let under_single = stdlib(&[("single.baml", SINGLE_BAML)]);
    let under_abc = stdlib(&[("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)]);
    assert!(!under_single.is_empty(), "the program has stdlib packages");
    assert!(
        under_single == under_abc,
        "stdlib packages emit differently under different user packages"
    );
}
