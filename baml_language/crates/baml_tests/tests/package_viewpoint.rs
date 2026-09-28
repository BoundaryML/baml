//! Names live on edges: the executable records each package's edge table,
//! and every host-supplied name resolves from the root's viewpoint — the
//! root by its own name, a dependency by the name the root reaches it under.

use std::sync::Arc;

use baml_base::{Dependency, Name};
use baml_db::{OptLevel, ProjectDatabase, SourceRootKind, SourceRootSpec, compile_program};
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_vm_types::{Program, types::EdgeKind};
use sys_native::SysOpsExt;

fn engine(program: Program) -> Arc<BexEngine> {
    Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("engine constructs"),
    )
}

async fn call(engine: &Arc<BexEngine>, name: &str) -> Result<BexExternalValue, EngineError> {
    engine
        .call_function(
            name,
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
            true,
        )
        .await
}

/// A root that declares nothing yet is still the program's root: it links,
/// carries the prelude on its edges, and the engine boots from its viewpoint.
#[tokio::test]
async fn a_root_without_files_is_still_the_program_root() {
    let mut db = ProjectDatabase::new();
    let root = db.workspace(std::path::Path::new("/viewpoint/empty"));
    let program = compile_program(&db, root, OptLevel::Two).expect("an empty root compiles");
    let root_package = &program.packages[program.root as usize];
    assert!(root_package.globals.is_empty() && root_package.classes.is_empty());
    assert!(
        root_package
            .edges
            .iter()
            .any(|edge| edge.name.as_str() == "baml" && edge.kind == EdgeKind::Prelude),
        "the root reaches the prelude: {:?}",
        root_package.edges
    );
    // Booting resolves the builtin error classes through the root's prelude
    // edges; a root with no test blocks collects nothing.
    let engine = engine(program);
    let registry = engine
        .collect_tests(
            sys_types::CallId::next(),
            sys_types::CancellationToken::default(),
        )
        .await
        .expect("collection runs");
    assert_eq!(registry, BexExternalValue::Null);
}

/// The root reaches package `lib` under the edge name `gadgets`, and `lib`
/// reaches `core`. A host names `lib` as the root does — `gadgets` — never by
/// `lib`'s own spelling, and `core`, which the root reaches only through
/// `lib`, has no host name at all while still being linked and callable
/// from `lib`.
#[tokio::test]
async fn a_host_names_a_dependency_as_the_root_reaches_it() {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(std::path::Path::new("/viewpoint/ws"));
    let add_dependency = |db: &mut ProjectDatabase, package: &str| {
        db.add_source_root(
            SourceRootSpec::new(format!("<builtin>/{package}"), SourceRootKind::Dependency)
                .named(Name::new(package)),
        )
        .expect("a dependency root")
    };
    let core = add_dependency(&mut db, "core");
    let lib = add_dependency(&mut db, "lib");
    db.add_dependency(
        lib,
        Dependency {
            name: Name::new("core"),
            root: core,
        },
    )
    .expect("lib depends on core");
    db.add_dependency(
        workspace,
        Dependency {
            name: Name::new("gadgets"),
            root: lib,
        },
    )
    .expect("the workspace reaches lib as gadgets");
    db.file("<builtin>/core/main.baml", "function g() -> int { 40 }\n");
    db.file(
        "<builtin>/lib/main.baml",
        "function f() -> int { core.g() + 2 }\n",
    );
    db.file(
        "/viewpoint/ws/main.baml",
        "function main() -> int { gadgets.f() }\n",
    );

    let program = compile_program(&db, workspace, OptLevel::Two).expect("the world compiles");
    let root = &program.packages[program.root as usize];
    let edge_to = |name: &str| {
        root.edges
            .iter()
            .find(|edge| edge.name.as_str() == name)
            .map(|edge| (&program.packages[edge.target as usize], edge.kind))
    };
    assert_eq!(
        edge_to("gadgets").map(|(package, kind)| (package.name.as_str(), kind)),
        Some(("lib", EdgeKind::Declared))
    );
    assert!(
        edge_to("lib").is_none(),
        "the root has no edge spelled `lib`"
    );
    assert!(
        edge_to("core").is_none(),
        "the root reaches `core` only through `lib`"
    );
    assert!(
        program
            .packages
            .iter()
            .any(|package| package.name.as_str() == "core"),
        "`core` is linked all the same"
    );

    let engine = engine(program);
    assert_eq!(
        call(&engine, "user.main").await.unwrap(),
        BexExternalValue::Int(42)
    );
    assert_eq!(
        call(&engine, "gadgets.f").await.unwrap(),
        BexExternalValue::Int(42)
    );
    assert!(matches!(
        call(&engine, "lib.f").await,
        Err(EngineError::FunctionNotFound { .. })
    ));
    assert!(matches!(
        call(&engine, "core.g").await,
        Err(EngineError::FunctionNotFound { .. })
    ));
}
