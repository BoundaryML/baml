//! Package identity is the source root, never a name.
//!
//! These pin what the compiler guarantees once a type head carries its
//! package as the root itself: same-named declarations in two packages are
//! two types with two runtime identities; a package names only what its
//! edges reach, under the names the edges give; and a program compiles
//! whatever it calls its packages — two that share a spelling, or one
//! reached under two names, are a rendering ambiguity the spelling table
//! reports, never a reason emit or the link cannot tell two declarations
//! apart. Every scenario needs more than one package in one database,
//! which only the harness can build, so they live here rather than in the
//! BAML corpus.

use baml_base::{Dependency, Name, SourceRoot, SourceRootKind};
use baml_compiler_diagnostics::{Diagnostic, DiagnosticId, Severity};
use baml_compiler2_emit::OptLevel;
use baml_compiler2_hir::package::{SpellingCollision, spelling, spelling_within};
use baml_db::{ProjectDatabase, SourceRootSpec, collect_compiler2_diagnostics, compile_program};
use baml_tests::engine::TestDbExt;
use bex_vm_types::Object;

/// MIR carries a head as its declaration's identity, never as a spelling.
///
/// An unnamed package spells as the default, so two unnamed projects in one
/// database spell the same and a lookup keyed on the spelling answers with
/// whichever root was created first. The crossing MIR lowers through never
/// makes that lookup: a head that enters as one package's declaration leaves
/// as that package's declaration, whatever the two are called.
#[test]
fn mir_keeps_a_head_as_its_declaration_whatever_the_package_is_called() {
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

    // The crossing MIR lowers through: the same for every package, because
    // nothing in it depends on who reads a head.
    let aliases = baml_type::ResolvedAliases::default();
    let crossing = baml_compiler2_mir::RuntimeLowering {
        aliases: &aliases,
        spelling: table,
        db: &db,
    };
    let point_of = |root| baml_type::DeclName::in_root(root, Vec::new(), Name::new("Point"));
    let lowered_root =
        |root| match crossing.convert(&baml_type::Ty::Class(point_of(root), Box::new([]))) {
            baml_compiler2_mir::RuntimeTy::Class(head, _) => head.root(),
            other => panic!("a class lowers to a class, got {other:?}"),
        };
    assert_eq!(
        lowered_root(second),
        second,
        "the second package's `Point` stays the second package's"
    );
    assert_eq!(
        lowered_root(first),
        first,
        "and the first's stays the first's"
    );
    assert_eq!(
        crossing.wire(&point_of(first)),
        crossing.wire(&point_of(second)),
        "while the two spell the same: a spelling is display, not identity"
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

    let program =
        compile_program(&db, workspace, OptLevel::Two).expect("two same-named classes emit");
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
    compile_program(&db, workspace, OptLevel::Two).expect("the workspace's world holds no clash");
}

#[test]
fn packages_that_share_a_spelling_stay_distinct_through_emit_and_link() {
    let (mut db, workspace) = workspace_db();
    // Two unnamed packages in ONE world spelled by the same edge name: the
    // workspace reaches `x` as `lib`, and `x` reaches `y` as `lib`. Each
    // dependent resolves its own edge, and every head is its declaration's
    // identity, so the program emits and links: a name lives on an edge.
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

    // The table reports the rendering ambiguity; the program is unaffected.
    let program = compile_program(&db, workspace, OptLevel::Two)
        .expect("a shared spelling is no obstacle to emit or link");
    let classes: Vec<_> = program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Class(class) if matches!(class.name.item_name().as_str(), "X" | "Y") => {
                Some((class.name.item_name().to_string(), class.type_tag))
            }
            _ => None,
        })
        .collect();
    let [(first, first_tag), (second, second_tag)] = classes.as_slice() else {
        panic!("expected exactly the classes `X` and `Y`, got {classes:?}");
    };
    assert_ne!(first, second, "both packages' classes are in the image");
    assert_ne!(first_tag, second_tag, "two declarations, two identities");
    // Each package reaches the next under ITS edge named `lib`.
    let lib_edges = program
        .packages
        .iter()
        .flat_map(|package| &package.edges)
        .filter(|edge| edge.name.as_str() == "lib")
        .map(|edge| edge.target)
        .collect::<Vec<_>>();
    assert_eq!(lib_edges.len(), 2, "two edges are named `lib`");
    assert_ne!(lib_edges[0], lib_edges[1], "and they reach two packages");
}

#[test]
fn an_unnamed_package_reached_under_two_names_is_one_package() {
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
    // Two names for one package is a rendering ambiguity, not two packages:
    // the program holds the one `S`, reached by both dependents.
    let program = compile_program(&db, workspace, OptLevel::Two)
        .expect("a package reached under two names compiles");
    let shared_classes = program
        .objects
        .iter()
        .filter(|object| {
            matches!(object, Object::Class(class) if class.name.item_name().as_str() == "S")
        })
        .count();
    assert_eq!(shared_classes, 1, "one declaration, however it is reached");
    let targets = |edge_name: &str| {
        program
            .packages
            .iter()
            .flat_map(|package| &package.edges)
            .filter(|edge| edge.name.as_str() == edge_name)
            .map(|edge| edge.target)
            .collect::<Vec<_>>()
    };
    assert_eq!(targets("first"), targets("second"), "both edges reach it");
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

/// The function of `file` declared as `name`.
fn function_named<'db>(
    db: &'db ProjectDatabase,
    file: baml_base::SourceFile,
    name: &str,
) -> baml_compiler2_hir::loc::FunctionLoc<'db> {
    baml_compiler2_hir::item_data::file_functions(db, file)
        .iter()
        .copied()
        .find(|&function| {
            baml_compiler2_hir::item_data::function_data(db, function)
                .name
                .as_str()
                == name
        })
        .unwrap_or_else(|| panic!("no function `{name}` in the file"))
}

/// A cached throw seed locates every head by the edges that reach its
/// package from the seeded file's own, so the compile that reads it back
/// means the declarations the compile that wrote it meant — whatever the
/// program calls its packages.
///
/// Both dependencies declare the name `errs` and both declare a `Boom`, so
/// the two thrown types spell alike (`errs.Boom`) and neither package is
/// reached under the name it declares. Only the path tells them apart.
#[test]
fn a_cached_throw_seed_locates_its_heads_by_edge_path() {
    use std::collections::BTreeMap;

    use baml_compiler2_hir_ty::{
        callable::callable_throws,
        package_interface::export_callable_throws_fragment,
        throw_facts::{export_file_throw_facts, file_throw_facts},
    };
    use baml_type::{
        DeclName, PathName, QualifiedTypeName, Ty, throw_facts::FunctionThrowFacts, wire::EdgePath,
    };

    let (mut db, workspace) = workspace_db();
    let errs = |db: &mut ProjectDatabase, path: &str| {
        db.add_source_root(
            SourceRootSpec::new(path, SourceRootKind::Dependency).named(Name::new("errs")),
        )
        .expect("a dependency root named `errs`")
    };
    let far = errs(&mut db, "<builtin>/far");
    let near = errs(&mut db, "<builtin>/near");
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("near"),
            root: near,
        },
    )
    .expect("edge workspace -> near");
    db.add_dependency(
        near,
        Dependency {
            name: Name::new("inner"),
            root: far,
        },
    )
    .expect("edge near -> far as `inner`");
    db.file("<builtin>/far/far.baml", "class Boom { code int }\n");
    db.file(
        "<builtin>/near/near.baml",
        "class Boom { message string }\n\
         function fail_far() -> int throws inner.Boom { throw inner.Boom { code: 1 } }\n\
         function fail_near() -> int throws Boom { throw Boom { message: \"x\" } }\n",
    );
    let main = db.file(
        "main.baml",
        "class Local { v int }\n\
         function f() -> int { near.fail_far() }\n\
         function g() -> int { near.fail_near() }\n\
         function declared() -> int throws near.Boom { near.fail_near() }\n\
         function own() -> int { throw Local { v: 1 } }\n",
    );
    assert_clean(&db);

    let table = spelling(&db);
    assert_eq!(
        table.of(far),
        table.of(near),
        "the two packages spell alike"
    );

    let boom_of = |root| {
        Ty::Class(
            DeclName::in_root(root, Vec::new(), Name::new("Boom")),
            Box::new([]),
        )
    };
    let boom_at = |path: &[&str]| {
        Ty::Class(
            PathName::qualified(
                EdgePath(path.iter().copied().map(Name::new).collect()),
                Vec::new(),
                Name::new("Boom"),
            ),
            Box::new([]),
        )
    };
    let id_of = |db: &ProjectDatabase, name: &str| function_named(db, main, name).id(db).as_u32();
    let (f_id, g_id) = (id_of(&db, "f"), id_of(&db, "g"));
    let throws_of =
        |db: &ProjectDatabase, name: &str| callable_throws(db, function_named(db, main, name)).0;

    // Honest inference: each function throws the `Boom` of the package its
    // callee declared it in.
    assert_eq!(throws_of(&db, "f"), boom_of(far));
    assert_eq!(throws_of(&db, "g"), boom_of(near));

    // The cached form names each by the edges that reach it.
    let fragment = export_callable_throws_fragment(&db, main);
    assert_eq!(fragment.by_id[&f_id], boom_at(&["near", "inner"]));
    assert_eq!(fragment.by_id[&g_id], boom_at(&["near"]));

    // Read back, a seed is the declaration its path reaches. Each function is
    // seeded with the OTHER's throws, so the answer can only be the seed.
    let path = main.path(&db).display().to_string();
    db.set_seeded_callable_throws(BTreeMap::from([(
        path.clone(),
        BTreeMap::from([
            (f_id, boom_at(&["near"])),
            (g_id, boom_at(&["near", "inner"])),
        ]),
    )]));
    assert_eq!(throws_of(&db, "f"), boom_of(near));
    assert_eq!(throws_of(&db, "g"), boom_of(far));

    // A path that leaves the package's edges is not this compile's fact: the
    // function is inferred honestly.
    db.set_seeded_callable_throws(BTreeMap::from([(
        path.clone(),
        BTreeMap::from([(f_id, boom_at(&["near", "gone"]))]),
    )]));
    assert_eq!(throws_of(&db, "f"), boom_of(far));
    db.set_seeded_callable_throws(BTreeMap::new());

    // The per-file facts hold what a function throws in its own words — a
    // declared clause, a `throw` in its body — located the same way: a
    // dependency's by the edge, the file's own package's by the empty path.
    fn local<K>(key: K) -> QualifiedTypeName<K> {
        QualifiedTypeName::qualified(key, Vec::new(), Name::new("Local"))
    }
    fn direct_of<N: Clone + Ord>(
        facts: &[FunctionThrowFacts<N>],
        name: &str,
    ) -> std::collections::BTreeSet<Ty<N>> {
        facts
            .iter()
            .find(|fact| fact.key.as_str() == name)
            .unwrap_or_else(|| panic!("no throw fact for `{name}`"))
            .direct
            .clone()
    }
    let honest_facts = file_throw_facts(&db, main).0.clone();
    let mut cached_facts = export_file_throw_facts(&db, main);
    assert_eq!(
        direct_of(&cached_facts, "declared"),
        [boom_at(&["near"])].into()
    );
    assert_eq!(
        direct_of(&cached_facts, "own"),
        [Ty::Class(local(EdgePath::own()), Box::new([]))].into()
    );

    // Read back, the facts are the facts that were written.
    db.set_seeded_throw_facts(BTreeMap::from([(path.clone(), cached_facts.clone())]));
    assert_eq!(file_throw_facts(&db, main).0, honest_facts);

    // And only the seed decides them: with the two functions' throws
    // exchanged, each reads back as the other's declaration.
    let (declared, own) = (
        direct_of(&cached_facts, "declared"),
        direct_of(&cached_facts, "own"),
    );
    for fact in &mut cached_facts {
        match fact.key.as_str() {
            "declared" => fact.direct.clone_from(&own),
            "own" => fact.direct.clone_from(&declared),
            _ => {}
        }
    }
    db.set_seeded_throw_facts(BTreeMap::from([(path, cached_facts)]));
    let seeded = file_throw_facts(&db, main).0.clone();
    assert_eq!(
        direct_of(&seeded, "declared"),
        [Ty::Class(local(workspace), Box::new([]))].into()
    );
    assert_eq!(direct_of(&seeded, "own"), [boom_of(near)].into());
}
