//! Package identity is the source root, never a name.
//!
//! These pin what the compiler guarantees once a type head carries its
//! package as the root itself: same-named declarations in two packages are
//! two types with two runtime identities; a package names only what its
//! edges reach, under the names the edges give; and the boundary that
//! spells roots for the wire refuses a program whose spelling table is not
//! injective. Every scenario needs more than one package in one database,
//! which only the harness can build, so they live here rather than in the
//! BAML corpus.

use baml_base::{Dependency, Name, SourceRoot, SourceRootKind};
use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, Severity};
use baml_compiler2_emit::{LoweringError, generate_project_bytecode};
use baml_compiler2_hir::package::{SpellingCollision, spelling, spelling_within};
use baml_db::{ProjectDatabase, SourceRootSpec, collect_compiler2_diagnostics};
use baml_tests::engine::TestDbExt;
use bex_vm_types::Object;

/// MIR reads a wire name from the package reading it, never by searching the
/// database for something spelled that way.
///
/// The wire spells "my own package" the same way for every package, and an
/// unnamed package spells as the default, so a reverse lookup keyed on the
/// spelling answers with whichever unnamed root was created first. With two
/// unnamed projects in one database that is silently the wrong one, and the
/// head MIR gets back belongs to the other project.
#[test]
fn mir_reads_a_local_wire_name_as_the_package_being_lowered() {
    let (mut db, first) = workspace_db();
    let second = db
        .add_source_root(SourceRootSpec::new(
            "/package-identity-second",
            SourceRootKind::Workspace,
        ))
        .expect("a second unnamed workspace root is a valid root");

    let table = spelling(&db);
    assert_eq!(
        table.of(first),
        table.of(second),
        "two unnamed packages spell the same"
    );
    assert_eq!(
        table.root(table.of(second)),
        Some(first),
        "the spelling alone cannot tell them apart: it answers with the first"
    );

    // The crossing MIR lowers through, built for each package in turn.
    let aliases = baml_type::ResolvedAliases::default();
    let crossing = |viewpoint| baml_compiler2_mir::RuntimeLowering {
        aliases: &aliases,
        spelling: table,
        db: &db,
        viewpoint,
    };
    let point = baml_type::TypeName::local(Name::new("Point"));
    assert_eq!(
        crossing(second).decl(&point).map(|decl| decl.root()),
        Some(second),
        "the second package reads its own `Point`, not the first's"
    );
    assert_eq!(
        crossing(first).decl(&point).map(|decl| decl.root()),
        Some(first),
        "and the first still reads its own"
    );
}

/// A database with the stdlib and an unnamed workspace root.
fn workspace_db() -> (ProjectDatabase, SourceRoot) {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(std::path::Path::new("/package-identity"));
    (db, workspace)
}

/// Every error diagnostic in the database, stdlib excluded.
fn errors(db: &ProjectDatabase) -> Vec<Diagnostic> {
    collect_compiler2_diagnostics(db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect()
}

fn assert_clean(db: &ProjectDatabase) {
    let errors = errors(db);
    assert!(errors.is_empty(), "unexpected errors: {errors:#?}");
}

/// Whether a diagnostic's message or any of its annotations mentions `needle`.
fn mentions(diagnostic: &Diagnostic, needle: &str) -> bool {
    diagnostic.message.contains(needle)
        || diagnostic.annotations.iter().any(|annotation| {
            annotation
                .message
                .as_deref()
                .is_some_and(|m| m.contains(needle))
        })
}

/// Every error in the database is a `id` mentioning `needle` (one per
/// occurrence), and there is at least one.
fn only_errors(db: &ProjectDatabase, id: DiagnosticId, needle: &str) -> Vec<Diagnostic> {
    let errors = errors(db);
    assert!(
        !errors.is_empty(),
        "expected a {id:?} mentioning `{needle}`"
    );
    assert!(
        errors
            .iter()
            .all(|diagnostic| diagnostic.id == id && mentions(diagnostic, needle)),
        "expected only {id:?} mentioning `{needle}`, got {errors:#?}"
    );
    errors
}

#[test]
fn same_named_declarations_in_two_packages_are_two_types() {
    let (mut db, _workspace) = workspace_db();
    db.dependency("lib");
    db.file(
        "<builtin>/lib/lib.baml",
        "class Point { x int }\nfunction make() -> Point throws never { Point { x: 1 } }\n",
    );
    db.file(
        "main.baml",
        "class Point { x int }\n\
         function take(p: Point) -> int throws never { p.x }\n\
         function fine() -> int throws never { let p: lib.Point = lib.make()\n p.x }\n\
         function crossed() -> int throws never { take(lib.make()) }\n",
    );
    // `fine` uses the dependency's type as its own; only `crossed` errs, and
    // from the workspace's viewpoint its own `Point` is spelled bare.
    let mismatch = only_errors(&db, DiagnosticId::TypeMismatch, "lib.Point");
    assert_eq!(mismatch.len(), 1);
    assert!(mentions(
        &mismatch[0],
        "expected `Point`, found `lib.Point`"
    ));
}

#[test]
fn emit_keeps_same_named_declarations_apart() {
    let (mut db, workspace) = workspace_db();
    db.dependency("lib");
    db.file("<builtin>/lib/lib.baml", "class Point { x int }\n");
    db.file("main.baml", "class Point { y string }\n");
    assert_clean(&db);

    let program = generate_project_bytecode(&db, workspace).expect("two same-named classes emit");
    let tags: Vec<_> = program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Class(class) if class.name.item_name().as_str() == "Point" => {
                Some((class.name.display_name(), class.type_tag))
            }
            _ => None,
        })
        .collect();
    let [(first_name, first_tag), (second_name, second_tag)] = tags.as_slice() else {
        panic!("expected exactly two `Point` classes, got {tags:?}");
    };
    assert_ne!(first_name, second_name, "each package spells its own Point");
    assert_ne!(first_tag, second_tag, "two declarations, two identities");
}

#[test]
fn unnamed_packages_collide_only_within_one_program() {
    let (mut db, workspace) = workspace_db();
    // Two unnamed packages nobody depends on: the database holds them (a
    // spelling is not an identity) and its table reports the clash — but a
    // program is emitted for one root's world, and neither is in the
    // workspace's, so the workspace's own table is injective and it emits.
    let a = db
        .add_source_root(SourceRootSpec::new(
            "<builtin>/a",
            SourceRootKind::Dependency,
        ))
        .expect("an unnamed dependency root is a valid root");
    let b = db
        .add_source_root(SourceRootSpec::new(
            "<builtin>/b",
            SourceRootKind::Dependency,
        ))
        .expect("a second unnamed dependency root is a valid root");
    db.file("<builtin>/a/a.baml", "class Shared { x int }\n");
    db.file("<builtin>/b/b.baml", "class Shared { y string }\n");
    db.file("main.baml", "class Shared { z bool }\n");
    assert_clean(&db);

    let table = spelling(&db);
    assert_eq!(table.of(a), table.of(b));
    assert_eq!(table.of(a), table.of(workspace));
    let [SpellingCollision::SharedName { name, roots }] = table.collisions() else {
        panic!(
            "expected one shared-name collision, got {:?}",
            table.collisions()
        );
    };
    assert_eq!(name.as_str(), "user");
    assert_eq!(roots.len(), 3);
    assert!(spelling_within(&db, workspace).collisions().is_empty());
    generate_project_bytecode(&db, workspace).expect("the workspace's world holds no clash");
}

#[test]
fn a_shared_spelling_within_one_program_is_refused() {
    let (mut db, workspace) = workspace_db();
    // Two unnamed packages in ONE world spelled by the same edge name: the
    // workspace reaches `x` as `lib`, and `x` reaches `y` as `lib`. Each
    // dependent resolves its own edge, but on the wire both would be `lib`,
    // so the program cannot be emitted.
    let x = db
        .add_source_root(SourceRootSpec::new(
            "<builtin>/x",
            SourceRootKind::Dependency,
        ))
        .expect("unnamed dependency root");
    let y = db
        .add_source_root(SourceRootSpec::new(
            "<builtin>/y",
            SourceRootKind::Dependency,
        ))
        .expect("second unnamed dependency root");
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("lib"),
            root: x,
        },
    )
    .expect("edge from the workspace");
    db.add_dependency(
        x,
        Dependency {
            name: Name::new("lib"),
            root: y,
        },
    )
    .expect("edge from x");
    db.file("<builtin>/y/y.baml", "class Y { v int }\n");
    db.file(
        "<builtin>/x/x.baml",
        "class X { v int }\nfunction g() -> lib.Y throws never { lib.Y { v: 2 } }\n",
    );
    db.file(
        "main.baml",
        "function f() -> lib.X throws never { lib.X { v: 1 } }\n",
    );
    assert_clean(&db);

    let table = spelling_within(&db, workspace);
    assert_eq!(table.of(x), table.of(y));
    let [SpellingCollision::SharedName { name, roots }] = table.collisions() else {
        panic!(
            "expected one shared-name collision, got {:?}",
            table.collisions()
        );
    };
    assert_eq!(name.as_str(), "lib");
    assert_eq!(roots.len(), 2);

    match generate_project_bytecode(&db, workspace) {
        Err(LoweringError::Internal(message)) => assert!(
            message.contains("spelled `lib`"),
            "the refusal names the shared spelling: {message}"
        ),
        other => panic!("emit must refuse a non-injective spelling table, got {other:?}"),
    }
}

#[test]
fn an_unnamed_package_reached_under_two_names_is_a_collision() {
    let (mut db, workspace) = workspace_db();
    let shared = db
        .add_source_root(SourceRootSpec::new(
            "<builtin>/shared",
            SourceRootKind::Dependency,
        ))
        .expect("unnamed dependency root");
    db.file("<builtin>/shared/s.baml", "class S { x int }\n");
    let other = db.dependency("other");
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("first"),
            root: shared,
        },
    )
    .expect("edge from the workspace");
    db.add_dependency(
        other,
        Dependency {
            name: Name::new("second"),
            root: shared,
        },
    )
    .expect("edge from the other dependency");
    db.file(
        "main.baml",
        "function f() -> first.S throws never { first.S { x: 1 } }\n",
    );
    db.file(
        "<builtin>/other/o.baml",
        "function g() -> second.S throws never { second.S { x: 2 } }\n",
    );
    // Each dependent resolves the package under its own edge name.
    assert_clean(&db);

    let table = spelling(&db);
    let [SpellingCollision::ManyNames { root, names }] = table.collisions() else {
        panic!(
            "expected one many-names collision, got {:?}",
            table.collisions()
        );
    };
    assert_eq!(*root, shared);
    let mut names: Vec<&str> = names.iter().map(Name::as_str).collect();
    names.sort_unstable();
    assert_eq!(names, ["first", "second"]);
    assert!(
        matches!(
            generate_project_bytecode(&db, workspace),
            Err(LoweringError::Internal(_))
        ),
        "one root cannot be spelled two ways in one program"
    );
}

#[test]
fn a_package_names_only_what_its_edges_reach() {
    let (mut db, _workspace) = workspace_db();
    let lib = db.dependency("lib");
    db.file("<builtin>/lib/lib.baml", "class L { x int }\n");
    let other = db.dependency("other");
    db.file(
        "<builtin>/other/o.baml",
        "function g() -> lib.L throws never { lib.L { x: 2 } }\n",
    );
    // `other` has no edge to `lib`: the name resolves nothing there, however
    // many packages in the database are spelled `lib`.
    only_errors(&db, DiagnosticId::UnknownType, "lib.L");

    db.add_dependency(
        other,
        Dependency {
            name: Name::new("lib"),
            root: lib,
        },
    )
    .expect("edge other -> lib");
    assert_clean(&db);
}

#[test]
fn a_dependency_is_spelled_by_its_edge_name_not_its_own() {
    let (mut db, workspace) = workspace_db();
    let lib = db
        .add_source_root(
            SourceRootSpec::new("<builtin>/lib", SourceRootKind::Dependency)
                .named(Name::new("lib")),
        )
        .expect("named dependency root");
    db.file("<builtin>/lib/lib.baml", "class L { x int }\n");
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("util"),
            root: lib,
        },
    )
    .expect("edge workspace -> lib as `util`");

    db.file(
        "main.baml",
        "function f() -> util.L throws never { util.L { x: 1 } }\n",
    );
    assert_clean(&db);

    // The package's declared name is display metadata; only the edge name
    // resolves from the workspace.
    db.file(
        "main.baml",
        "function f() -> lib.L throws never { lib.L { x: 1 } }\n",
    );
    only_errors(&db, DiagnosticId::UnknownType, "lib.L");

    // Diagnostics spell the dependency the way this package writes it.
    db.file(
        "main.baml",
        "function f() -> int throws never { let l: util.L = util.L { x: 1 }\n l }\n",
    );
    let mismatch = only_errors(&db, DiagnosticId::TypeMismatch, "util.L");
    assert!(mentions(&mismatch[0], "expected `int`, found `util.L`"));
}

#[test]
fn a_transitive_dependency_is_named_by_provenance_but_not_spellable() {
    let (mut db, workspace) = workspace_db();
    let leaf = db
        .add_source_root(
            SourceRootSpec::new("<builtin>/leaf", SourceRootKind::Dependency)
                .named(Name::new("leaf")),
        )
        .expect("leaf root");
    db.file("<builtin>/leaf/leaf.baml", "class L { x int }\n");
    let mid = db.dependency("mid");
    db.add_dependency(
        mid,
        Dependency {
            name: Name::new("leaf"),
            root: leaf,
        },
    )
    .expect("edge mid -> leaf");
    db.file(
        "<builtin>/mid/mid.baml",
        "function get() -> leaf.L throws never { leaf.L { x: 1 } }\n",
    );
    // The workspace reaches `leaf` only through `mid`: a diagnostic can still
    // name its type (by the program's spelling of that package), but source
    // in the workspace has no way to write it.
    db.file(
        "main.baml",
        "function f() -> int throws never { mid.get() }\n",
    );
    let mismatch = only_errors(&db, DiagnosticId::TypeMismatch, "leaf.L");
    assert!(mentions(&mismatch[0], "expected `int`, found `leaf.L`"));

    let viewpoint = baml_compiler2_hir_ty::render::Viewpoint::user_facing(&db, workspace);
    let l = baml_type::DeclName::in_root(leaf, Vec::new(), Name::new("L"));
    assert_eq!(viewpoint.path(&l), "leaf.L");
    assert_eq!(
        viewpoint.source_path(&l),
        Err(baml_compiler2_hir_ty::render::Unspellable(l))
    );
    let m = baml_type::DeclName::in_root(mid, Vec::new(), Name::new("get"));
    assert_eq!(viewpoint.source_path(&m).as_deref(), Ok("mid.get"));
}

#[test]
fn impls_are_visible_only_within_the_dependency_closure() {
    let (mut db, _workspace) = workspace_db();
    let lib = db.dependency("lib");
    db.file(
        "<builtin>/lib/lib.baml",
        "interface Show { function show(self) -> string throws never }\n\
         implements Show for int { function show(self) -> string throws never { \"int\" } }\n",
    );
    let other = db.dependency("other");
    db.file(
        "<builtin>/other/o.baml",
        "function g() -> string throws never { (1).show() }\n",
    );
    // `other` has no edge to `lib`: the impl is not among what it can see,
    // so `int` has no `show` member there.
    let errors = errors(&db);
    assert!(
        errors.iter().any(|d| mentions(d, "show")),
        "expected a missing-member error mentioning `show`, got {errors:#?}"
    );

    db.add_dependency(
        other,
        Dependency {
            name: Name::new("lib"),
            root: lib,
        },
    )
    .expect("edge other -> lib");
    assert_clean(&db);
}
