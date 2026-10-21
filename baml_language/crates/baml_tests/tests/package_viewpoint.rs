//! Names live on edges: the executable records each package's edge table,
//! and every host-supplied name resolves from the root's viewpoint — the
//! root by its own name, a dependency by the name the root reaches it under.

use std::sync::Arc;

use baml_base::{Dependency, LangPackage, Name};
use baml_db::{OptLevel, ProjectDatabase, SourceRootKind, SourceRootSpec, compile_program};
use baml_tests::engine::TestDbExt;
use bex_engine::{BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder};
use bex_vm_types::{
    Program,
    types::{EdgeKind, PackageKind},
};
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
        root_package.edges.iter().any(|edge| {
            edge.name.as_str() == "baml"
                && program.packages[edge.target as usize].kind
                    == PackageKind::Language(LangPackage::Baml)
        }),
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

/// A function's display signature — what `baml run --list` and a host's
/// function listing show — is spelled from its OWN package's root namespace:
/// a type of that namespace bare, one of another namespace `root.`-qualified,
/// never from the declaring file's namespace (which would leave a bare name
/// a reader at the root cannot resolve).
#[test]
fn function_metadata_is_spelled_from_the_package_root() {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(std::path::Path::new("/viewpoint/metadata"));
    db.file("/viewpoint/metadata/main.baml", "class Foo { x int }\n");
    db.file(
        "/viewpoint/metadata/ns_util/main.baml",
        "class Bar { y int }\nfunction make(f: root.Foo) -> Bar { Bar { y: f.x } }\n",
    );
    let program = compile_program(&db, workspace, OptLevel::Two).expect("compiles");
    let make = program
        .objects
        .iter()
        .find_map(|object| match object {
            bex_vm_types::Object::Function(function)
                if function
                    .identity
                    .declared_name()
                    .map(baml_base::Name::as_str)
                    == Some("make") =>
            {
                Some(function)
            }
            _ => None,
        })
        .expect("`make` is pooled");
    assert_eq!(make.display_param_types, vec!["Foo".to_string()]);
    assert_eq!(make.display_return_type, "root.util.Bar");
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
        Some(("lib", EdgeKind::Dependency))
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

/// A `cleanup` finalizer is reached through the instance's own class, so it
/// runs whatever the host can or cannot name: `lib`'s class (the root reaches
/// the package as `gadgets`, never as `lib`) and `core`'s (which the root
/// reaches only through `lib`, so no host name spells it at all) are both
/// finalized when their instances are collected.
///
/// Rust-side because only the harness can build a world of several packages.
#[tokio::test]
async fn a_finalizer_runs_for_a_class_no_host_name_reaches() {
    let mut db = ProjectDatabase::new();
    let workspace = db.workspace(std::path::Path::new("/viewpoint/finalizers"));
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
    db.file(
        "<builtin>/core/main.baml",
        r#"
class Deep {
  log string[]
  function cleanup(self) -> void {
    self.log.push("core")
  }
}
function make(log: string[]) -> Deep { Deep { log: log } }
"#,
    );
    db.file(
        "<builtin>/lib/main.baml",
        r#"
class Near {
  log string[]
  function cleanup(self) -> void {
    self.log.push("lib")
  }
}
function make(log: string[]) -> Near { Near { log: log } }
function touch(log: string[]) -> void {
  let near = make(log);
  let deep = core.make(log);
}
"#,
    );
    db.file(
        "/viewpoint/finalizers/main.baml",
        r#"
function make_log() -> string[] { [] }
function touch(log: string[]) -> void { gadgets.touch(log) }
function read_log(log: string[]) -> string[] { log }
"#,
    );
    let program = compile_program(&db, workspace, OptLevel::Two).expect("the world compiles");
    let engine = engine(program);
    let call = |name: &'static str, args: Vec<BexExternalValue>, copy_objects: bool| {
        let engine = Arc::clone(&engine);
        async move {
            engine
                .call_function(
                    name,
                    args,
                    FunctionCallContextBuilder::new(sys_types::CallId::next()).build(),
                    copy_objects,
                )
                .await
                .unwrap_or_else(|error| panic!("`{name}` runs: {error}"))
        }
    };

    // The log is rooted by the test's handle; the two instances `touch`
    // allocates alias it and are garbage once the call returns.
    let log = call("make_log", Vec::new(), false).await;
    call("touch", vec![log.clone()], false).await;
    engine
        .collect_garbage(bex_heap::CollectionLevel::Major)
        .await;

    let BexExternalValue::Array { items, .. } = call("read_log", vec![log], true).await else {
        panic!("the log is an array");
    };
    let mut cleaned: Vec<String> = items
        .iter()
        .map(|item| match item {
            BexExternalValue::String(text) => text.to_string(),
            other => panic!("the log holds strings, got {other:?}"),
        })
        .collect();
    cleaned.sort();
    assert_eq!(cleaned, ["core", "lib"]);
}
