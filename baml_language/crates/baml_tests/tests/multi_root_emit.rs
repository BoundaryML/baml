//! A program is emitted for one root's world — the root and its dependency
//! closure — never for the whole database.
//!
//! One database may hold several workspace roots (the LSP serves every open
//! project from one process). Each is its own program: same-named
//! declarations in two roots never meet, one root's errors do not block
//! another's build, and each program's stdlib prefix is the same
//! user-independent slice. Every scenario needs two workspace roots in one
//! database, which only the harness can build, so they live here rather than
//! in the BAML corpus.

use std::sync::Arc;

use baml_base::SourceRoot;
use baml_compiler_diagnostics::Severity;
use baml_compiler2_emit::{LoweringError, generate_project_bytecode, generate_stdlib_program};
use baml_compiler2_hir::package::{spelling, spelling_within, world_roots};
use baml_db::{OptLevel, ProjectDatabase, SourceRootKind, SourceRootSpec};
use bex_engine::{BexEngine, BexExternalValue, FunctionCallContextBuilder};
use bex_vm_types::{Object, Program};
use sys_native::SysOpsExt;

/// A database with the stdlib and two unnamed workspace roots, each with
/// one file.
fn two_workspaces(first: &str, second: &str) -> (ProjectDatabase, SourceRoot, SourceRoot) {
    let mut db = ProjectDatabase::new();
    db.ensure_stdlib_sources();
    let mut add = |path: &str, source: &str| {
        let root = db
            .add_source_root(SourceRootSpec::new(path, SourceRootKind::Workspace))
            .expect("a second workspace root is a valid root");
        db.add_or_update_file_in(
            root,
            std::path::Path::new(&format!("{path}/main.baml")),
            source,
        );
        root
    };
    let a = add("/multi-root/a", first);
    let b = add("/multi-root/b", second);
    (db, a, b)
}

/// The class objects named `name` in `program`, as `(display name, type tag)`.
fn classes_named(program: &Program, name: &str) -> Vec<(String, bex_vm_types::type_tags::TypeTag)> {
    program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Class(class) if class.name.item_name().as_str() == name => {
                Some((class.name.display_name().to_string(), class.type_tag))
            }
            _ => None,
        })
        .collect()
}

/// Every object of `program`, serialized — objects carry no equality of
/// their own, so byte identity is the comparison.
fn object_bytes(program: &Program) -> Vec<Vec<u8>> {
    program
        .objects
        .iter()
        .map(|object| borsh::to_vec(object).expect("objects serialize"))
        .collect()
}

/// The user-defined function names in `program`.
fn function_names(program: &Program) -> Vec<String> {
    program
        .objects
        .iter()
        .filter_map(|object| match object {
            Object::Function(function) if function.name.starts_with("user.") => {
                Some(function.name.clone())
            }
            _ => None,
        })
        .collect()
}

async fn run_main(program: Program) -> BexExternalValue {
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine constructs"),
    );
    engine
        .call_function(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
        .expect("main runs")
}

#[test]
fn each_workspace_root_is_its_own_world() {
    let (db, a, b) = two_workspaces("class Point { x int }\n", "class Point { y string }\n");
    // Both roots spell themselves by the default; database-wide that is a
    // shared spelling, but neither program holds the other root.
    assert_eq!(spelling(&db).of(a), spelling(&db).of(b));
    assert!(!spelling(&db).collisions().is_empty());
    assert!(spelling_within(&db, a).collisions().is_empty());
    assert!(spelling_within(&db, b).collisions().is_empty());
    assert!(world_roots(&db, a).contains(&a) && !world_roots(&db, a).contains(&b));
    assert!(world_roots(&db, b).contains(&b) && !world_roots(&db, b).contains(&a));
    // Each world is the stdlib plus the one root, in table order.
    let stdlib: Vec<SourceRoot> = db
        .source_roots()
        .into_iter()
        .filter(|root| root.kind(&db) == SourceRootKind::Stdlib)
        .collect();
    assert_eq!(world_roots(&db, a)[..stdlib.len()], stdlib[..]);
    assert_eq!(world_roots(&db, a).last(), Some(&a));
}

#[test]
fn same_named_declarations_in_two_roots_emit_as_two_programs() {
    let (db, a, b) = two_workspaces("class Point { x int }\n", "class Point { y string }\n");
    let program_a = generate_project_bytecode(&db, a).expect("root a emits alone");
    let program_b = generate_project_bytecode(&db, b).expect("root b emits alone");
    // Exactly one `Point` per program, the workspace's own.
    let points_a = classes_named(&program_a, "Point");
    let points_b = classes_named(&program_b, "Point");
    let [(name_a, tag_a)] = points_a.as_slice() else {
        panic!("program a holds one Point: {points_a:?}");
    };
    let [(name_b, tag_b)] = points_b.as_slice() else {
        panic!("program b holds one Point: {points_b:?}");
    };
    assert_eq!(name_a, name_b, "each is its program's workspace package");
    // Tags are per program: content-addressed over the wire name, so the two
    // programs' `Point`s agree — they never share a VM.
    assert_eq!(tag_a, tag_b);
}

#[tokio::test]
async fn each_program_runs_its_own_root() {
    let (db, a, b) = two_workspaces(
        "function main() -> int throws never { 1 }\nfunction only_a() -> int throws never { 10 }\n",
        "function main() -> int throws never { 2 }\nfunction only_b() -> int throws never { 20 }\n",
    );
    let program_a = generate_project_bytecode(&db, a).expect("root a emits");
    let program_b = generate_project_bytecode(&db, b).expect("root b emits");
    assert_eq!(
        function_names(&program_a),
        vec!["user.main".to_string(), "user.only_a".to_string()]
    );
    assert_eq!(
        function_names(&program_b),
        vec!["user.main".to_string(), "user.only_b".to_string()]
    );
    assert_eq!(run_main(program_a).await, BexExternalValue::Int(1));
    assert_eq!(run_main(program_b).await, BexExternalValue::Int(2));
}

#[test]
fn one_roots_errors_do_not_block_another() {
    let (db, a, b) = two_workspaces(
        "function main() -> int throws never { 1 }\n",
        "function main() -> int throws never { \"not an int\" }\n",
    );
    assert!(db.get_bytecode(a).is_ok(), "root a is clean and builds");
    let Err(LoweringError::ProjectHasErrors { error_count }) = db.get_bytecode(b) else {
        panic!("root b has a type error and must not build");
    };
    assert_eq!(error_count, 1);
    // The error is reported against b's file only.
    let errors: Vec<_> = baml_db::collect_compiler2_diagnostics(&db)
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .collect();
    assert_eq!(errors.len(), 1);
    let file = errors[0]
        .primary_span()
        .expect("the error has a span")
        .file_id;
    assert!(
        b.files(&db).iter().any(|f| f.file_id(&db) == file),
        "the error belongs to root b"
    );
}

#[test]
fn every_world_shares_the_stdlib_prefix() {
    let (db, a, b) = two_workspaces(
        "function main() -> int throws never { 1 }\n",
        "class Other { z bool }\n",
    );
    let stdlib = generate_stdlib_program(&db, OptLevel::Two).expect("the stdlib emits");
    for root in [a, b] {
        let program = generate_project_bytecode(&db, root).expect("the root emits");
        let stdlib_objects = object_bytes(&stdlib);
        assert_eq!(
            object_bytes(&program)[..stdlib_objects.len()],
            stdlib_objects[..],
            "the stdlib is a user-independent prefix of every program"
        );
    }
}
