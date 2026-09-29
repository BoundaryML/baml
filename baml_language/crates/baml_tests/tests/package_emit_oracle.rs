//! The per-package emitter's oracle: every package of a program emitted on
//! its own and linked (`baml_db::compile_program`) carries the executable's
//! three identity tables (`Class.methods`, `ProgramPackage::{globals, init}`),
//! each asserted positively against the view derived from them at load and
//! the ordinal init order. Beyond identity: emission is deterministic, the
//! parallel code pass reproduces the serial one, and a package's unit is a
//! pure function of its own sources and its dependencies' interfaces — a
//! stdlib package's unit is the same bytes whatever user package sits above
//! it.

mod common;

use std::path::Path;

use baml_compiler2_emit::{OptLevel, emit_package};
use baml_db::{ProjectDatabase, compile_program};
use baml_tests::engine::TestDbExt;
use bex_vm_types::{Object, Program};
use common::{A_BAML, B_BAML, C_BAML, build_db};

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

/// The linked image's identity tables, each checked against the rendered
/// view derived beside it: every class's inherent methods, every package's
/// slot table, and a structural `$init` wherever the rendered init order
/// names one.
fn assert_identity_tables(label: &str, program: &Program) {
    let callables = program.rendered_callables();
    for (ordinal, package) in program.packages.iter().enumerate() {
        let ordinal = u32::try_from(ordinal).expect("package ordinal fits in u32");
        assert_eq!(
            package.init.is_some(),
            program.init_order.contains(&ordinal),
            "{label}: package `{}` init table disagrees with the init order",
            package.name
        );
        if let Some(init) = package.init {
            assert!(
                matches!(&program.objects[init], Object::Function(f) if f.name == "$init"),
                "{label}: package `{}` init is not its `$init`",
                package.name
            );
        }
        let rendered_slots = callables
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

fn assert_program_links(label: &str, build: impl Fn() -> ProjectDatabase) {
    for opt in LEVELS {
        let label = format!("{label}@{opt:?}");
        let db = build();
        let linked = compile_program(&db, package(&db), opt)
            .unwrap_or_else(|e| panic!("{label}: compile_program: {e}"));
        assert_identity_tables(&label, &linked);
    }
}

#[test]
fn stdlib_only_links() {
    assert_program_links("stdlib-only", || build_db(ROOT, &[]));
}

#[test]
fn single_file_links() {
    assert_program_links("single-file", || {
        build_db(ROOT, &[("single.baml", SINGLE_BAML)])
    });
}

#[test]
fn abc_fixture_links() {
    let files = [("a.baml", A_BAML), ("b.baml", B_BAML), ("c.baml", C_BAML)];
    assert_program_links("abc-fixture", || build_db(ROOT, &files));
}

#[test]
fn client_init_links() {
    assert_program_links("client-init", || {
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
fn baml_src_links() {
    let sources = baml_src_sources();
    assert_program_links("baml_src", || baml_src_db(&sources));
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

// ── Interface-only fingerprints ──────────────────────────────────────────────
//
// A dependent's unit is a function of its dependencies' INTERFACES, never
// their bodies: the fingerprint it records is the interface payload's digest,
// so a body-only change in a dependency leaves every dependent's bytes and
// fingerprint alone (relink, never recompile), while a body change that alters
// what the interface says — an inferred `throws` clause — changes the digest,
// and a unit compiled against the old interface no longer links.

const FINGERPRINT_ROOT: &str = "/package-emit-oracle-fingerprint";

const LIBRARY_V1: &str = r#"
class Overflow {
    message string
}

function compute(x: int) -> int {
    x + 1
}
"#;

/// `compute` with another body: the same signature, the same inferred
/// `throws never`.
const LIBRARY_BODY_CHANGED: &str = r#"
class Overflow {
    message string
}

function compute(x: int) -> int {
    x + 2
}
"#;

/// `compute` with a body that throws: the declaration is untouched, but the
/// inferred `throws` clause — part of the exported interface — is now
/// `Overflow`.
const LIBRARY_THROWS_CHANGED: &str = r#"
class Overflow {
    message string
}

function compute(x: int) -> int {
    if (x > 100) {
        throw Overflow { message: "too big" }
    }
    x + 1
}
"#;

const FINGERPRINT_CONSUMER: &str = r#"
function main() -> int {
    app.compute(1)
}
"#;

/// A world of the consumer beside `library` as its `app` dependency.
fn fingerprint_world(library: &str) -> (ProjectDatabase, baml_db::SourceRoot) {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new(FINGERPRINT_ROOT));
    let app = db.dependency("app");
    db.file("<builtin>/app/lib.baml", library);
    db.file(
        Path::new(FINGERPRINT_ROOT).join("main.baml"),
        FINGERPRINT_CONSUMER,
    );
    baml_db::testing::assert_no_diagnostic_errors(&db);
    (db, app)
}

/// The consumer's fingerprint of `app`: the digest its unit's direct entry
/// for `app` records.
fn app_fingerprint(consumer: &baml_linker_types::EmittedPackage) -> [u8; 32] {
    consumer
        .unit
        .dependencies
        .iter()
        .find_map(|locator| match locator {
            baml_linker_types::Locator::Direct { edge, digest } if edge.as_str() == "app" => {
                Some(*digest)
            }
            baml_linker_types::Locator::Direct { .. }
            | baml_linker_types::Locator::Prelude { .. }
            | baml_linker_types::Locator::Transitive { .. } => None,
        })
        .expect("the consumer reaches `app` directly, fingerprinted")
}

#[test]
fn a_dependency_body_change_leaves_its_dependents_unit_and_fingerprint_unchanged() {
    let (before, app_before) = fingerprint_world(LIBRARY_V1);
    let (after, app_after) = fingerprint_world(LIBRARY_BODY_CHANGED);
    let digest = baml_compiler2_hir_ty::package_interface::interface_digest;
    assert_eq!(digest(&before, app_before), digest(&after, app_after));

    let consumer_before = emit_package(&before, package(&before), OptLevel::Two).expect("emit");
    let consumer_after = emit_package(&after, package(&after), OptLevel::Two).expect("emit");
    assert_eq!(
        app_fingerprint(&consumer_before),
        digest(&before, app_before)
    );
    assert_eq!(
        borsh::to_vec(&consumer_before).expect("serialize"),
        borsh::to_vec(&consumer_after).expect("serialize"),
        "a body-only change in `app` must leave the consumer's output untouched"
    );
    // The change was real: `app`'s own output moved.
    let app_unit = |db: &ProjectDatabase, app| {
        borsh::to_vec(&emit_package(db, app, OptLevel::Two).expect("emit").unit).expect("serialize")
    };
    assert_ne!(app_unit(&before, app_before), app_unit(&after, app_after));
}

/// The old consumer output, served for the new world's `user` root: what a
/// cache holding a unit compiled against the previous interface does.
struct ServedConsumer<'a>(&'a baml_linker_types::EmittedPackage);

impl baml_db::PackageCache for ServedConsumer<'_> {
    fn load(
        &self,
        db: &dyn baml_compiler2_hir::Db,
        root: baml_db::SourceRoot,
        _opt: OptLevel,
    ) -> Option<baml_linker_types::EmittedPackage> {
        (root.kind(db) == baml_base::SourceRootKind::Workspace).then(|| self.0.clone())
    }

    fn store(
        &self,
        _db: &dyn baml_compiler2_hir::Db,
        _root: baml_db::SourceRoot,
        _opt: OptLevel,
        _emitted: &baml_linker_types::EmittedPackage,
    ) {
    }
}

#[test]
fn a_dependency_body_change_that_alters_inferred_throws_changes_the_fingerprint() {
    let (before, app_before) = fingerprint_world(LIBRARY_V1);
    let (after, app_after) = fingerprint_world(LIBRARY_THROWS_CHANGED);
    let digest = baml_compiler2_hir_ty::package_interface::interface_digest;
    assert_ne!(digest(&before, app_before), digest(&after, app_after));

    let consumer_before = emit_package(&before, package(&before), OptLevel::Two).expect("emit");
    let consumer_after = emit_package(&after, package(&after), OptLevel::Two).expect("emit");
    assert_eq!(
        app_fingerprint(&consumer_before),
        digest(&before, app_before)
    );
    assert_eq!(app_fingerprint(&consumer_after), digest(&after, app_after));

    // Each world links on its own; the old consumer does not link into the
    // new world — the edge still locates `app`, the fingerprint refuses it.
    compile_program(&after, package(&after), OptLevel::Two).expect("the new world links");
    let error = baml_db::compile_program_with(
        &after,
        package(&after),
        OptLevel::Two,
        &ServedConsumer(&consumer_before),
    )
    .expect_err("a consumer of the old interface must not link");
    match error {
        baml_db::CompileProgramError::Link(baml_linker::LinkError::InterfaceMismatch {
            package,
            edge,
            expected,
            found,
        }) => {
            assert_eq!(package.as_str(), "user");
            assert_eq!(edge.as_str(), "app");
            assert_eq!(expected, digest(&before, app_before));
            assert_eq!(found, digest(&after, app_after));
        }
        other => panic!("expected an interface mismatch, got {other:?}"),
    }
}

// ── Merkle: a transitive change is a direct change ───────────────────────────
//
// A consumer bakes the LAYOUT of types it reaches only through a dependency's
// API (`app.make().size` indexes `core.Widget`'s field) with no import of
// them. The interface blob embeds the digest of every direct dependency, so
// `app`'s digest covers `core`: reordering `core.Widget`'s fields changes
// `app`'s digest with `app`'s source untouched, and the consumer compiled
// against the old layout is refused.

const CORE_V1: &str = r#"
class Widget {
    size int
    label string
}
"#;

const CORE_REORDERED: &str = r#"
class Widget {
    label string
    size int
}
"#;

const APP_OVER_CORE: &str = r#"
function make() -> core.Widget {
    core.Widget { size: 7, label: "w" }
}
"#;

const TRANSITIVE_CONSUMER: &str = r#"
function main() -> int {
    app.make().size
}
"#;

/// `user → app → core`, the workspace with no edge of its own to `core`.
fn transitive_world(core: &str) -> (ProjectDatabase, baml_db::SourceRoot) {
    let mut db = ProjectDatabase::new();
    db.workspace(Path::new(FINGERPRINT_ROOT));
    let app = db.dependency("app");
    let core_root = db
        .add_source_root(
            baml_db::SourceRootSpec::new("<builtin>/core", baml_base::SourceRootKind::Dependency)
                .named(baml_base::Name::new("core")),
        )
        .expect("the core root is added");
    db.add_dependency(
        app,
        baml_base::Dependency {
            name: baml_base::Name::new("core"),
            root: core_root,
        },
    )
    .expect("app reaches core");
    db.file("<builtin>/core/lib.baml", core);
    db.file("<builtin>/app/lib.baml", APP_OVER_CORE);
    db.file(
        Path::new(FINGERPRINT_ROOT).join("main.baml"),
        TRANSITIVE_CONSUMER,
    );
    baml_db::testing::assert_no_diagnostic_errors(&db);
    (db, app)
}

#[test]
fn a_transitive_layout_change_changes_the_direct_dependency_digest_and_refuses_the_stale_unit() {
    let (before, app_before) = transitive_world(CORE_V1);
    let (after, app_after) = transitive_world(CORE_REORDERED);
    let digest = baml_compiler2_hir_ty::package_interface::interface_digest;
    assert_ne!(
        digest(&before, app_before),
        digest(&after, app_after),
        "`app`'s interface embeds `core`'s digest"
    );

    let consumer_before = emit_package(&before, package(&before), OptLevel::Two).expect("emit");
    let consumer_after = emit_package(&after, package(&after), OptLevel::Two).expect("emit");
    assert_ne!(
        borsh::to_vec(&consumer_before.unit).expect("serialize"),
        borsh::to_vec(&consumer_after.unit).expect("serialize"),
        "the consumer baked `core.Widget`'s layout"
    );
    assert_eq!(
        app_fingerprint(&consumer_before),
        digest(&before, app_before)
    );

    compile_program(&after, package(&after), OptLevel::Two).expect("the new world links");
    let error = baml_db::compile_program_with(
        &after,
        package(&after),
        OptLevel::Two,
        &ServedConsumer(&consumer_before),
    )
    .expect_err("a consumer of the old layout must not link");
    match error {
        baml_db::CompileProgramError::Link(baml_linker::LinkError::InterfaceMismatch {
            package,
            edge,
            expected,
            found,
        }) => {
            assert_eq!(package.as_str(), "user");
            assert_eq!(edge.as_str(), "app");
            assert_eq!(expected, digest(&before, app_before));
            assert_eq!(found, digest(&after, app_after));
        }
        other => panic!("expected an interface mismatch on `app`, got {other:?}"),
    }
}
